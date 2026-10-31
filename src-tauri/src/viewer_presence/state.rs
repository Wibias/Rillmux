// Presence types, shared worker state, and status/diagnostic reporting.

use reqwest::header::{ACCEPT, AUTHORIZATION, RANGE, REFERER, USER_AGENT};
use reqwest::{Response, StatusCode};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::error::Error as _;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tauri::{AppHandle, Emitter};
use thiserror::Error;
use url::Url;

use crate::http::shared_client;

const TWITCH_URL: &str = "https://www.twitch.tv";
const GQL_URL: &str = "https://gql.twitch.tv/gql";
const USHER_URL: &str = "https://usher.ttvnw.net/api/channel/hls/";
const PLAYBACK_ACCESS_TOKEN_HASH: &str =
    "ed230aa1e33e07eebb8928504583da78a5173989fadfb1ac94be06a04f3cdbe9";
const FALLBACK_CLIENT_VERSION: &str = "ef928475-9403-42f2-8a34-55784bd08e16";
const USER_AGENT_VALUE: &str =
    "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/140.0.0.0 Safari/537.36";
const MIN_SUCCESS_INTERVAL_SECS: u64 = 55;
const MAX_SUCCESS_INTERVAL_SECS: u64 = 70;
const MAX_WORKERS: usize = 8;

#[derive(Debug, Error)]
pub enum ViewerPresenceError {
    #[error("{0}")]
    Message(String),
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ViewerPresenceTarget {
    pub session_id: String,
    pub channel_login: String,
    pub channel_id: String,
    pub broadcast_id: String,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ViewerPresenceWorkerStatus {
    pub session_id: String,
    pub channel_login: String,
    pub last_stage: String,
    pub last_http_status: Option<u16>,
    pub last_error: Option<String>,
    pub last_success_unix_ms: Option<u64>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ViewerPresenceStatus {
    pub enabled: bool,
    pub active_session_ids: Vec<String>,
    pub limited: bool,
    pub workers: Vec<ViewerPresenceWorkerStatus>,
}

struct Worker {
    target: ViewerPresenceTarget,
    token: String,
    cancel: Arc<AtomicBool>,
}

pub struct ViewerPresenceState {
    workers: Mutex<HashMap<String, Worker>>,
    diagnostics: Mutex<HashMap<String, ViewerPresenceWorkerStatus>>,
    sync_gate: tokio::sync::Mutex<()>,
    generation: AtomicU64,
    enabled: AtomicBool,
    limited: AtomicBool,
    device_id: String,
    client_session: String,
}

pub type SharedViewerPresence = Arc<ViewerPresenceState>;

static APP: OnceLock<AppHandle> = OnceLock::new();

pub fn init(app: AppHandle) {
    let _ = APP.set(app);
}

fn emit_presence_changed() {
    if let Some(app) = APP.get() {
        let _ = app.emit("viewer-presence-changed", ());
    }
}

impl ViewerPresenceState {
    pub fn new() -> Self {
        Self {
            workers: Mutex::new(HashMap::new()),
            diagnostics: Mutex::new(HashMap::new()),
            sync_gate: tokio::sync::Mutex::new(()),
            generation: AtomicU64::new(0),
            enabled: AtomicBool::new(false),
            limited: AtomicBool::new(false),
            device_id: crate::twitch_web_auth::device_id().to_string(),
            client_session: crate::twitch_web_auth::client_session_id().to_string(),
        }
    }
}

#[derive(Debug, Clone)]
struct RuntimeConfig {
    spade_url: Url,
    client_version: String,
}

#[derive(Debug)]
struct ProtocolError {
    stage: &'static str,
    status: Option<u16>,
    message: String,
}

impl ProtocolError {
    fn new(stage: &'static str, message: impl Into<String>) -> Self {
        Self {
            stage,
            status: None,
            message: message.into(),
        }
    }

    fn with_status(stage: &'static str, status: StatusCode, message: impl Into<String>) -> Self {
        Self {
            stage,
            status: Some(status.as_u16()),
            message: message.into(),
        }
    }
}

fn valid_login(login: &str) -> bool {
    !login.is_empty()
        && login.len() <= 25
        && login.chars().all(|character| {
            character.is_ascii_lowercase() || character.is_ascii_digit() || character == '_'
        })
}

fn normalize_target(mut target: ViewerPresenceTarget) -> Option<ViewerPresenceTarget> {
    target.session_id = target.session_id.trim().to_string();
    target.channel_login = target.channel_login.trim().to_ascii_lowercase();
    target.channel_id = target.channel_id.trim().to_string();
    target.broadcast_id = target.broadcast_id.trim().to_string();

    if target.session_id.is_empty()
        || target.session_id.len() > 128
        || !valid_login(&target.channel_login)
        || target.channel_id.is_empty()
        || target.channel_id.len() > 32
        || !target
            .channel_id
            .chars()
            .all(|character| character.is_ascii_digit())
        || target.broadcast_id.is_empty()
        || target.broadcast_id.len() > 128
    {
        return None;
    }
    Some(target)
}

pub(crate) fn select_targets(targets: Vec<ViewerPresenceTarget>) -> Vec<ViewerPresenceTarget> {
    let mut seen = HashSet::new();
    targets
        .into_iter()
        .filter_map(normalize_target)
        .filter(|target| seen.insert(target.session_id.clone()))
        .take(MAX_WORKERS)
        .collect()
}

fn cancel_workers(state: &ViewerPresenceState, clear_diagnostics: bool) {
    state.generation.fetch_add(1, Ordering::AcqRel);
    if let Ok(mut workers) = state.workers.lock() {
        for worker in workers.values() {
            worker.cancel.store(true, Ordering::Release);
        }
        workers.clear();
    }
    if clear_diagnostics {
        if let Ok(mut diagnostics) = state.diagnostics.lock() {
            diagnostics.clear();
        }
    }
}

pub fn cancel_all(state: &ViewerPresenceState) {
    state.enabled.store(false, Ordering::Release);
    state.limited.store(false, Ordering::Release);
    cancel_workers(state, true);
}

pub fn get_status(
    state: &ViewerPresenceState,
) -> Result<ViewerPresenceStatus, ViewerPresenceError> {
    let workers = state
        .workers
        .lock()
        .map_err(|_| ViewerPresenceError::Message("viewer presence state poisoned".into()))?;
    let mut active_session_ids = workers.keys().cloned().collect::<Vec<_>>();
    active_session_ids.sort();
    drop(workers);

    let diagnostics = state
        .diagnostics
        .lock()
        .map_err(|_| ViewerPresenceError::Message("viewer presence diagnostics poisoned".into()))?;
    let mut worker_statuses = diagnostics.values().cloned().collect::<Vec<_>>();
    worker_statuses.sort_by(|left, right| left.session_id.cmp(&right.session_id));

    Ok(ViewerPresenceStatus {
        enabled: state.enabled.load(Ordering::Acquire),
        active_session_ids,
        limited: state.limited.load(Ordering::Acquire),
        workers: worker_statuses,
    })
}

fn set_diagnostic(
    state: &ViewerPresenceState,
    target: &ViewerPresenceTarget,
    stage: &str,
    status: Option<u16>,
    error: Option<String>,
    success_unix_ms: Option<u64>,
) {
    let changed = if let Ok(mut diagnostics) = state.diagnostics.lock() {
        let previous_success = diagnostics
            .get(&target.session_id)
            .and_then(|current| current.last_success_unix_ms);
        let next = ViewerPresenceWorkerStatus {
            session_id: target.session_id.clone(),
            channel_login: target.channel_login.clone(),
            last_stage: stage.to_string(),
            last_http_status: status,
            last_error: error,
            last_success_unix_ms: success_unix_ms.or(previous_success),
        };
        let changed = diagnostics.get(&target.session_id) != Some(&next);
        diagnostics.insert(target.session_id.clone(), next);
        changed
    } else {
        false
    };
    if changed {
        emit_presence_changed();
    }
}
