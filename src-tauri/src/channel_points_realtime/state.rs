// Shared Hermes presence state, diagnostics, and readiness bookkeeping.

use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use futures_util::{SinkExt, StreamExt};
use serde_json::{json, Value};
use tauri::{AppHandle, Emitter};
use tokio::net::TcpStream;
use tokio::sync::Notify;
use tokio_tungstenite::tungstenite::client::IntoClientRequest;
use tokio_tungstenite::tungstenite::http::header::{HeaderValue, ORIGIN};
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::{connect_async, MaybeTlsStream, WebSocketStream};
use uuid::Uuid;

const HERMES_URL_PREFIX: &str = "wss://hermes.twitch.tv/v1?clientId=";
const TWITCH_ORIGIN: &str = "https://www.twitch.tv";
const CONNECT_TIMEOUT: Duration = Duration::from_secs(12);
const KEEPALIVE_INTERVAL: Duration = Duration::from_secs(10);
const MAX_BACKOFF: Duration = Duration::from_secs(30);

type HermesSocket = WebSocketStream<MaybeTlsStream<TcpStream>>;

#[derive(Clone, Debug, PartialEq, Eq)]
struct DesiredPresence {
    token: String,
    viewer_id: String,
    channel_ids: Vec<String>,
    logins_by_id: HashMap<String, String>,
}

struct RealtimeState {
    desired: Mutex<Option<DesiredPresence>>,
    generation: AtomicU64,
    ready: AtomicBool,
    last_error: Mutex<Option<String>>,
    wake: Notify,
    changed: Notify,
    started: AtomicBool,
}

fn debug_credit(event: &str, fields: &str) {
    crate::diagnostics::log_event(
        crate::diagnostics::DebugCategory::PointsCredit,
        event,
        fields,
    );
}

fn debug_poll(event: &str, fields: &str) {
    crate::diagnostics::log_event(crate::diagnostics::DebugCategory::Polls, event, fields);
}

fn hermes_error_class(error: &str) -> &'static str {
    if error.contains("authentication") {
        "authentication"
    } else if error.contains("subscription") {
        "subscription"
    } else if error.contains("reconnect") {
        "reconnect"
    } else if error.contains("socket") || error.contains("close frame") || error.contains("closed")
    {
        "socket"
    } else if error.contains("connect") || error.contains("connection") {
        "connect"
    } else if error.contains("keepalive") || error.contains("pong") {
        "keepalive"
    } else if error.contains("timed out") {
        "timeout"
    } else if error.contains("read") || error.contains("JSON") {
        "read"
    } else {
        "other"
    }
}

fn topic_kind(topic: &str) -> &'static str {
    if topic.starts_with("community-points-user-v1.") {
        "viewer"
    } else if topic.starts_with("video-playback-by-id.") {
        "playback"
    } else if topic.starts_with("polls.") {
        "poll"
    } else if topic.starts_with("predictions-channel-v1.") {
        "prediction"
    } else if topic.starts_with("raid.") {
        "raid"
    } else {
        "other"
    }
}

fn state() -> &'static RealtimeState {
    static STATE: OnceLock<RealtimeState> = OnceLock::new();
    STATE.get_or_init(|| RealtimeState {
        desired: Mutex::new(None),
        generation: AtomicU64::new(0),
        ready: AtomicBool::new(false),
        last_error: Mutex::new(None),
        wake: Notify::new(),
        changed: Notify::new(),
        started: AtomicBool::new(false),
    })
}

static APP: OnceLock<AppHandle> = OnceLock::new();

pub fn init(app: AppHandle) {
    let _ = APP.set(app);
}

fn emit_frontend(event: &'static str) {
    if let Some(app) = APP.get() {
        let _ = app.emit(event, ());
    }
}

pub fn clear() {
    crate::channel_points::clear_poll_cache();
    let realtime = state();
    if let Ok(mut desired) = realtime.desired.lock() {
        *desired = None;
    }
    realtime.ready.store(false, Ordering::Release);
    let generation = realtime.generation.fetch_add(1, Ordering::AcqRel) + 1;
    debug_credit("hermes.clear", &format!("generation={generation}"));
    realtime.wake.notify_waiters();
    realtime.changed.notify_waiters();
}

fn ensure_supervisor() {
    let realtime = state();
    if realtime.started.swap(true, Ordering::AcqRel) {
        return;
    }
    debug_credit("hermes.supervisor.start", "started=true");
    tauri::async_runtime::spawn(async {
        run_supervisor().await;
    });
}
fn snapshot() -> (Option<DesiredPresence>, u64) {
    let realtime = state();
    let desired = realtime.desired.lock().ok().and_then(|value| value.clone());
    (desired, realtime.generation.load(Ordering::Acquire))
}

fn generation_matches(generation: u64) -> bool {
    state().generation.load(Ordering::Acquire) == generation
}

fn mark_ready(generation: u64) {
    let realtime = state();
    if generation_matches(generation) {
        if let Ok(mut error) = realtime.last_error.lock() {
            *error = None;
        }
        realtime.ready.store(true, Ordering::Release);
        debug_credit("hermes.ready", &format!("generation={generation}"));
        realtime.changed.notify_waiters();
        emit_frontend("viewer-presence-changed");
    }
}

fn mark_not_ready(generation: u64, error: Option<String>) {
    let realtime = state();
    if generation_matches(generation) {
        realtime.ready.store(false, Ordering::Release);
        let reason = error.as_deref().map(hermes_error_class).unwrap_or("none");
        debug_credit(
            "hermes.not_ready",
            &format!("generation={generation} reason={reason}"),
        );
        if let Ok(mut last_error) = realtime.last_error.lock() {
            *last_error = error;
        }
        realtime.changed.notify_waiters();
        emit_frontend("viewer-presence-changed");
    }
}
