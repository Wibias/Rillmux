// Twitch EventSub WebSocket - outgoing channel.raid for watched channels.

use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Duration;

use futures_util::stream::{SplitSink, SplitStream};
use futures_util::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tauri::{AppHandle, Emitter};
use tokio::net::TcpStream;
use tokio::sync::Notify;
use tokio::time::Instant;
use tokio_tungstenite::{connect_async, tungstenite::Message, MaybeTlsStream, WebSocketStream};

use crate::auth;
use crate::http::shared_client;

const WS_URL: &str = "wss://eventsub.wss.twitch.tv/ws";
const HELIX_EVENTSUB: &str = "https://api.twitch.tv/helix/eventsub/subscriptions";
const WELCOME_TIMEOUT: Duration = Duration::from_secs(15);
const CONNECT_TIMEOUT: Duration = Duration::from_secs(12);
const KEEPALIVE_GRACE: Duration = Duration::from_secs(2);

type EventSubSocket = WebSocketStream<MaybeTlsStream<TcpStream>>;
type EventSubWrite = SplitSink<EventSubSocket, Message>;
type EventSubRead = SplitStream<EventSubSocket>;

fn debug_raid(event: &str, fields: &str) {
    crate::diagnostics::log_event(crate::diagnostics::DebugCategory::Raids, event, fields);
}

fn eventsub_error_class(error: &str) -> &'static str {
    let error = error.to_ascii_lowercase();
    if error.contains("auth") || error.contains("401") || error.contains("403") {
        "authentication"
    } else if error.contains("subscription") || error.contains("create sub") {
        "subscription"
    } else if error.contains("keepalive") {
        "keepalive"
    } else if error.contains("reconnect") {
        "reconnect"
    } else if error.contains("welcome") {
        "welcome"
    } else if error.contains("closed") || error.contains("close frame") {
        "socket"
    } else if error.contains("connect") || error.contains("connection") {
        "connect"
    } else if error.contains("timeout") || error.contains("timed out") {
        "timeout"
    } else if error.contains("json") || error.contains("read") {
        "read"
    } else {
        "other"
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum EventSubConnectionState {
    #[default]
    Idle,
    Connecting,
    Connected,
    Retrying,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EventSubStatus {
    pub enabled: bool,
    pub watched_channels: usize,
    pub state: EventSubConnectionState,
    pub last_error: Option<&'static str>,
}

#[derive(Default)]
struct RuntimeStatus {
    state: EventSubConnectionState,
    last_error: Option<&'static str>,
}

fn runtime_status() -> &'static Mutex<RuntimeStatus> {
    static S: OnceLock<Mutex<RuntimeStatus>> = OnceLock::new();
    S.get_or_init(|| Mutex::new(RuntimeStatus::default()))
}

fn set_state(state: EventSubConnectionState) {
    if let Ok(mut runtime) = runtime_status().lock() {
        runtime.state = state;
    }
}

fn set_error(reason: &'static str) {
    if let Ok(mut runtime) = runtime_status().lock() {
        runtime.state = EventSubConnectionState::Retrying;
        runtime.last_error = Some(reason);
    }
}

fn set_connected() {
    if let Ok(mut runtime) = runtime_status().lock() {
        runtime.state = EventSubConnectionState::Connected;
        runtime.last_error = None;
    }
}

fn sync_runtime(active: bool) {
    if let Ok(mut runtime) = runtime_status().lock() {
        if active {
            if runtime.state == EventSubConnectionState::Idle {
                runtime.state = EventSubConnectionState::Connecting;
            }
        } else {
            runtime.state = EventSubConnectionState::Idle;
            runtime.last_error = None;
        }
    }
}

pub fn status() -> Result<EventSubStatus, String> {
    let config = state().lock().map_err(|error| error.to_string())?;
    let runtime = runtime_status().lock().map_err(|error| error.to_string())?;
    Ok(EventSubStatus {
        enabled: config.enabled,
        watched_channels: config.logins.len(),
        state: runtime.state,
        last_error: runtime.last_error,
    })
}
