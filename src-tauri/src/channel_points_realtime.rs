//! Authenticated Twitch Hermes presence for Channel Points watch credit.
//!
//! Twitch accepts minute-watched Spade telemetry without necessarily crediting
//! Channel Points. The working browser-authenticated miner also maintains the
//! private viewer + channel PubSub topics over Hermes, so viewer presence is
//! established here before the existing minute-watched worker is started.

use std::collections::HashSet;
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
    } else if error.contains("socket") || error.contains("close frame") || error.contains("closed") {
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

pub async fn sync(
    enabled: bool,
    targets: &[crate::viewer_presence::ViewerPresenceTarget],
) -> Result<(), String> {
    if !enabled || targets.is_empty() {
        debug_credit(
            "hermes.sync.skip",
            &format!("enabled={enabled} target_count={}", targets.len()),
        );
        clear();
        return Ok(());
    }

    let web_auth = crate::twitch_web_auth::load_session()
        .map_err(|error| error.to_string())?
        .ok_or_else(|| "Twitch Website Authentication is not configured".to_string())?;
    let auth_session = crate::auth::get_session()
        .await
        .map_err(|error| error.to_string())?;
    let viewer_id = auth_session
        .user_id
        .ok_or_else(|| "log in with Twitch before enabling channel points".to_string())?;
    if !auth_session.logged_in || web_auth.user_id != viewer_id {
        debug_credit("hermes.sync.reject", "reason=account_mismatch");
        clear();
        return Err(
            "Twitch Website Authentication does not match the current Twitch account".into(),
        );
    }

    let mut channel_ids = targets
        .iter()
        .map(|target| target.channel_id.trim().to_string())
        .filter(|channel_id| {
            !channel_id.is_empty()
                && channel_id
                    .chars()
                    .all(|character| character.is_ascii_digit())
        })
        .collect::<Vec<_>>();
    channel_ids.sort();
    channel_ids.dedup();
    if channel_ids.is_empty() {
        debug_credit("hermes.sync.skip", "reason=no_valid_channels");
        clear();
        return Ok(());
    }

    let desired = DesiredPresence {
        token: web_auth.token,
        viewer_id,
        channel_ids,
    };
    let realtime = state();
    let (generation, changed) = {
        let mut current = realtime
            .desired
            .lock()
            .map_err(|_| "Channel Points realtime state is poisoned".to_string())?;
        if current.as_ref() == Some(&desired) {
            (realtime.generation.load(Ordering::Acquire), false)
        } else {
            *current = Some(desired.clone());
            realtime.ready.store(false, Ordering::Release);
            if let Ok(mut error) = realtime.last_error.lock() {
                *error = None;
            }
            (realtime.generation.fetch_add(1, Ordering::AcqRel) + 1, true)
        }
    };
    debug_credit(
        "hermes.sync.desired",
        &format!(
            "generation={generation} changed={changed} channel_count={} viewer={}",
            desired.channel_ids.len(),
            crate::diagnostics::redact_id(&desired.viewer_id)
        ),
    );

    ensure_supervisor();
    realtime.wake.notify_waiters();
    let result = wait_until_ready(generation).await;
    debug_credit(
        "hermes.sync.result",
        &format!("generation={generation} ready={}", result.is_ok()),
    );
    result
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

async fn wait_until_ready(generation: u64) -> Result<(), String> {
    let realtime = state();
    let deadline = tokio::time::Instant::now() + CONNECT_TIMEOUT;
    loop {
        if realtime.generation.load(Ordering::Acquire) != generation {
            debug_credit(
                "hermes.wait.cancelled",
                &format!("generation={generation} reason=reconfigured"),
            );
            return Err("Channel Points realtime presence was reconfigured".into());
        }
        if realtime.ready.load(Ordering::Acquire) {
            debug_credit("hermes.wait.ready", &format!("generation={generation}"));
            return Ok(());
        }
        if tokio::time::Instant::now() >= deadline {
            let detail = realtime
                .last_error
                .lock()
                .ok()
                .and_then(|error| error.clone())
                .unwrap_or_else(|| "waiting for Twitch realtime presence".to_string());
            debug_credit(
                "hermes.wait.timeout",
                &format!(
                    "generation={generation} reason={}",
                    hermes_error_class(&detail)
                ),
            );
            return Err(format!("Channel Points realtime is not ready: {detail}"));
        }
        tokio::select! {
            _ = realtime.changed.notified() => {}
            _ = tokio::time::sleep(Duration::from_millis(100)) => {}
        }
    }
}

async fn run_supervisor() {
    let realtime = state();
    let mut backoff = Duration::from_secs(1);

    loop {
        let (desired, generation) = snapshot();
        let Some(desired) = desired else {
            realtime.wake.notified().await;
            continue;
        };

        realtime.ready.store(false, Ordering::Release);
        realtime.changed.notify_waiters();
        debug_credit(
            "hermes.session.start",
            &format!(
                "generation={generation} channel_count={} backoff_ms={}",
                desired.channel_ids.len(),
                backoff.as_millis()
            ),
        );
        match run_session(&desired, generation).await {
            Ok(()) => {
                debug_credit(
                    "hermes.session.end",
                    &format!("generation={generation} reason=reconfigured"),
                );
                backoff = Duration::from_secs(1);
            }
            Err(error) => {
                let reason = hermes_error_class(&error);
                debug_credit(
                    "hermes.reconnect",
                    &format!(
                        "generation={generation} reason={reason} backoff_ms={}",
                        backoff.as_millis()
                    ),
                );
                mark_not_ready(generation, Some(error));
                tokio::select! {
                    _ = realtime.wake.notified() => {
                        backoff = Duration::from_secs(1);
                    }
                    _ = tokio::time::sleep(backoff) => {
                        backoff = (backoff * 2).min(MAX_BACKOFF);
                    }
                }
            }
        }
    }
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

async fn run_session(desired: &DesiredPresence, generation: u64) -> Result<(), String> {
    debug_credit(
        "hermes.connect.attempt",
        &format!(
            "generation={generation} channel_count={}",
            desired.channel_ids.len()
        ),
    );
    let mut request = format!(
        "{HERMES_URL_PREFIX}{}",
        crate::twitch_web_auth::WEB_CLIENT_ID
    )
    .into_client_request()
    .map_err(|error| format!("Hermes request: {error}"))?;
    request
        .headers_mut()
        .insert(ORIGIN, HeaderValue::from_static(TWITCH_ORIGIN));

    let (mut socket, _) = tokio::time::timeout(CONNECT_TIMEOUT, connect_async(request))
        .await
        .map_err(|_| "Hermes connection timed out".to_string())?
        .map_err(|error| format!("Hermes connect: {error}"))?;
    debug_credit("hermes.connect.ok", &format!("generation={generation}"));

    debug_credit(
        "hermes.auth.request",
        &format!(
            "generation={generation} viewer={}",
            crate::diagnostics::redact_id(&desired.viewer_id)
        ),
    );
    send_json(&mut socket, authenticate_request(&desired.token)).await?;
    wait_for_authentication(&mut socket).await?;
    debug_credit("hermes.auth.ack", &format!("generation={generation}"));

    let mut subscriptions = HashSet::new();
    let viewer_topic = format!("community-points-user-v1.{}", desired.viewer_id);
    subscriptions.insert(send_subscription(&mut socket, &viewer_topic).await?);
    for channel_id in &desired.channel_ids {
        let topic = format!("video-playback-by-id.{channel_id}");
        subscriptions.insert(send_subscription(&mut socket, &topic).await?);
    }
    debug_credit(
        "hermes.subscription.request",
        &format!(
            "generation={generation} viewer_topics=1 playback_topics={}",
            desired.channel_ids.len()
        ),
    );
    wait_for_subscriptions(&mut socket, &mut subscriptions).await?;
    debug_credit(
        "hermes.subscription.ack",
        &format!(
            "generation={generation} viewer_topics=1 playback_topics={}",
            desired.channel_ids.len()
        ),
    );

    if !generation_matches(generation) {
        debug_credit(
            "hermes.session.cancelled",
            &format!("generation={generation} phase=private_topics"),
        );
        let _ = socket.send(Message::Close(None)).await;
        return Ok(());
    }
    debug_poll(
        "poll.subscription.request",
        &format!(
            "generation={generation} channel_count={}",
            desired.channel_ids.len()
        ),
    );
    if let Err(error) = subscribe_poll_topics(&mut socket, &desired.channel_ids).await {
        debug_poll(
            "poll.subscription.fallback",
            &format!(
                "generation={generation} reason={}",
                hermes_error_class(&error)
            ),
        );
    } else {
        debug_poll(
            "poll.subscription.ack",
            &format!(
                "generation={generation} channel_count={}",
                desired.channel_ids.len()
            ),
        );
    }
    if !generation_matches(generation) {
        debug_credit(
            "hermes.session.cancelled",
            &format!("generation={generation} phase=poll_topics"),
        );
        let _ = socket.send(Message::Close(None)).await;
        return Ok(());
    }
    mark_ready(generation);

    let mut keepalive = tokio::time::interval(KEEPALIVE_INTERVAL);
    keepalive.tick().await;
    loop {
        tokio::select! {
            _ = state().wake.notified() => {
                if !generation_matches(generation) {
                    debug_credit(
                        "hermes.session.cancelled",
                        &format!("generation={generation} phase=active"),
                    );
                    let _ = socket.send(Message::Close(None)).await;
                    return Ok(());
                }
            }
            _ = keepalive.tick() => {
                socket
                    .send(Message::Ping(Vec::new().into()))
                    .await
                    .map_err(|error| format!("Hermes keepalive: {error}"))?;
            }
            message = socket.next() => {
                let Some(message) = message else {
                    return Err("Hermes socket closed".into());
                };
                match message.map_err(|error| format!("Hermes read: {error}"))? {
                    Message::Ping(payload) => {
                        socket
                            .send(Message::Pong(payload))
                            .await
                            .map_err(|error| format!("Hermes pong: {error}"))?;
                    }
                    Message::Close(_) => return Err("Hermes close frame".into()),
                    Message::Text(text) => {
                        if let Ok(value) = serde_json::from_str::<Value>(&text) {
                            if value.get("type").and_then(Value::as_str) == Some("reconnect") {
                                debug_credit(
                                    "hermes.reconnect.requested",
                                    &format!("generation={generation}"),
                                );
                                return Err("Hermes requested reconnect".into());
                            }
                            if let Some((topic, message)) = pubsub_topic_and_message(&value) {
                                let kind = topic_kind(&topic);
                                let changed = crate::channel_points::ingest_pubsub(&topic, &message);
                                if matches!(kind, "poll" | "prediction") {
                                    debug_poll(
                                        "poll.pubsub",
                                        &format!(
                                            "generation={generation} topic={kind} changed={changed}"
                                        ),
                                    );
                                } else {
                                    debug_credit(
                                        "hermes.pubsub",
                                        &format!(
                                            "generation={generation} topic={kind} changed={changed}"
                                        ),
                                    );
                                }
                                if changed {
                                    emit_frontend("channel-points-pubsub");
                                }
                            }
                        }
                    }
                    _ => {}
                }
            }
        }
    }
}

async fn subscribe_poll_topics(
    socket: &mut HermesSocket,
    channel_ids: &[String],
) -> Result<(), String> {
    let mut subscriptions = HashSet::new();
    for channel_id in channel_ids {
        let topic = format!("polls.{channel_id}");
        subscriptions.insert(send_subscription(socket, &topic).await?);
        let predictions = format!("predictions-channel-v1.{channel_id}");
        subscriptions.insert(send_subscription(socket, &predictions).await?);
    }

    tokio::time::timeout(
        Duration::from_secs(3),
        wait_for_subscriptions(socket, &mut subscriptions),
    )
    .await
    .map_err(|_| "Hermes poll/prediction subscription confirmation timed out".to_string())?
}

fn json_value(value: &Value) -> Value {
    match value {
        Value::String(text) => serde_json::from_str(text).unwrap_or_else(|_| value.clone()),
        other => other.clone(),
    }
}

fn pubsub_topic_and_message(value: &Value) -> Option<(String, Value)> {
    if value.get("type").and_then(Value::as_str) == Some("MESSAGE") {
        let topic = value.pointer("/data/topic")?.as_str()?.to_string();
        let message = value
            .pointer("/data/message")
            .map(json_value)
            .unwrap_or_else(|| value.clone());
        return Some((topic, message));
    }
    if let Some(pubsub) = value.pointer("/notification/pubsub") {
        return pubsub_topic_and_message(&json_value(pubsub)).or_else(|| {
            let topic = json_value(pubsub)
                .get("topic")
                .and_then(Value::as_str)
                .map(str::to_string)?;
            let message = json_value(pubsub)
                .get("message")
                .map(json_value)
                .unwrap_or_else(|| json_value(pubsub));
            Some((topic, message))
        });
    }
    if let Some(data) = value.pointer("/notification/data") {
        return pubsub_topic_and_message(data).or_else(|| {
            let topic = data.get("topic").and_then(Value::as_str)?.to_string();
            let message = data
                .get("message")
                .map(json_value)
                .unwrap_or_else(|| data.clone());
            Some((topic, message))
        });
    }
    None
}

async fn send_json(socket: &mut HermesSocket, value: Value) -> Result<(), String> {
    socket
        .send(Message::Text(value.to_string().into()))
        .await
        .map_err(|error| format!("Hermes write: {error}"))
}

async fn wait_for_authentication(socket: &mut HermesSocket) -> Result<(), String> {
    loop {
        let value = receive_json(socket).await?;
        if value.get("type").and_then(Value::as_str) != Some("authenticateResponse") {
            continue;
        }
        let response = value
            .get("authenticateResponse")
            .ok_or_else(|| "Hermes authentication response was malformed".to_string())?;
        if response.get("result").and_then(Value::as_str) == Some("ok") {
            return Ok(());
        }
        return Err(format!(
            "Hermes authentication failed: {}",
            response
                .get("error")
                .and_then(Value::as_str)
                .unwrap_or("unknown error")
        ));
    }
}

async fn send_subscription(socket: &mut HermesSocket, topic: &str) -> Result<String, String> {
    let subscription_id = random_id();
    send_json(socket, subscription_request(&subscription_id, topic)).await?;
    Ok(subscription_id)
}

async fn wait_for_subscriptions(
    socket: &mut HermesSocket,
    pending: &mut HashSet<String>,
) -> Result<(), String> {
    while !pending.is_empty() {
        let value = receive_json(socket).await?;
        if value.get("type").and_then(Value::as_str) != Some("subscribeResponse") {
            if let Some((topic, message)) = pubsub_topic_and_message(&value) {
                let kind = topic_kind(&topic);
                let changed = crate::channel_points::ingest_pubsub(&topic, &message);
                if matches!(kind, "poll" | "prediction") {
                    debug_poll(
                        "poll.pubsub.before_ready",
                        &format!("topic={kind} changed={changed}"),
                    );
                }
                if changed {
                    emit_frontend("channel-points-pubsub");
                }
            }
            continue;
        }
        let response = value
            .get("subscribeResponse")
            .ok_or_else(|| "Hermes subscription response was malformed".to_string())?;
        let subscription_id = response
            .get("subscription")
            .and_then(|subscription| subscription.get("id"))
            .and_then(Value::as_str)
            .ok_or_else(|| "Hermes subscription response was missing its id".to_string())?;
        if !pending.contains(subscription_id) {
            continue;
        }
        if response.get("result").and_then(Value::as_str) != Some("ok") {
            let error = response
                .get("error")
                .and_then(Value::as_str)
                .unwrap_or("unauthorized");
            let code = response
                .get("errorCode")
                .and_then(Value::as_str)
                .unwrap_or("unknown");
            return Err(format!("Hermes subscription failed: {error} ({code})"));
        }
        pending.remove(subscription_id);
        debug_credit(
            "hermes.subscription.ack.item",
            &format!("remaining={}", pending.len()),
        );
    }
    Ok(())
}

async fn receive_json(socket: &mut HermesSocket) -> Result<Value, String> {
    loop {
        let message = tokio::time::timeout(CONNECT_TIMEOUT, socket.next())
            .await
            .map_err(|_| "Hermes response timed out".to_string())?
            .ok_or_else(|| "Hermes socket closed".to_string())?
            .map_err(|error| format!("Hermes read: {error}"))?;
        match message {
            Message::Text(text) => {
                return serde_json::from_str(&text)
                    .map_err(|error| format!("Hermes JSON: {error}"));
            }
            Message::Ping(payload) => {
                socket
                    .send(Message::Pong(payload))
                    .await
                    .map_err(|error| format!("Hermes pong: {error}"))?;
            }
            Message::Close(_) => return Err("Hermes close frame".into()),
            _ => {}
        }
    }
}

fn authenticate_request(token: &str) -> Value {
    json!({
        "id": random_id(),
        "timestamp": timestamp(),
        "type": "authenticate",
        "authenticate": {
            "token": token
        }
    })
}

fn subscription_request(subscription_id: &str, topic: &str) -> Value {
    json!({
        "id": random_id(),
        "timestamp": timestamp(),
        "type": "subscribe",
        "subscribe": {
            "id": subscription_id,
            "type": "pubsub",
            "pubsub": {
                "topic": topic
            }
        }
    })
}

fn random_id() -> String {
    Uuid::new_v4()
        .simple()
        .to_string()
        .chars()
        .take(21)
        .collect()
}

fn timestamp() -> String {
    let duration = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();
    let seconds = duration.as_secs() as i64;
    let millis = duration.subsec_millis();
    let days = seconds.div_euclid(86_400);
    let seconds_of_day = seconds.rem_euclid(86_400);
    let (year, month, day) = civil_from_days(days);
    let hour = seconds_of_day / 3_600;
    let minute = (seconds_of_day % 3_600) / 60;
    let second = seconds_of_day % 60;
    format!("{year:04}-{month:02}-{day:02}T{hour:02}:{minute:02}:{second:02}.{millis:03}Z")
}

fn civil_from_days(days_since_epoch: i64) -> (i64, i64, i64) {
    let z = days_since_epoch + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let day_of_era = z - era * 146_097;
    let year_of_era =
        (day_of_era - day_of_era / 1_460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let mut year = year_of_era + era * 400;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let month_prime = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * month_prime + 2) / 5 + 1;
    let month = month_prime + if month_prime < 10 { 3 } else { -9 };
    if month <= 2 {
        year += 1;
    }
    (year, month, day)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn auth_request_uses_bare_token() {
        let request = authenticate_request("secret-token");
        assert_eq!(request["type"], "authenticate");
        assert_eq!(request["authenticate"]["token"], "secret-token");
        assert!(!request["authenticate"]["token"]
            .as_str()
            .unwrap()
            .starts_with("OAuth "));
    }

    #[test]
    fn builds_private_pubsub_subscription() {
        let request = subscription_request("sub-1", "community-points-user-v1.45537718");
        assert_eq!(request["type"], "subscribe");
        assert_eq!(request["subscribe"]["id"], "sub-1");
        assert_eq!(request["subscribe"]["type"], "pubsub");
        assert_eq!(
            request["subscribe"]["pubsub"]["topic"],
            "community-points-user-v1.45537718"
        );
    }

    #[test]
    fn unix_epoch_date_conversion_is_correct() {
        assert_eq!(civil_from_days(0), (1970, 1, 1));
        assert_eq!(civil_from_days(20_000), (2024, 10, 4));
    }

    #[test]
    fn extracts_poll_topic_from_hermes_notification() {
        let notification = json!({
            "type": "notification",
            "notification": {
                "pubsub": {
                    "type": "MESSAGE",
                    "data": {
                        "topic": "polls.123",
                        "message": "{\"type\":\"POLL_UPDATE\",\"data\":{\"poll\":{\"poll_id\":\"p1\"}}}"
                    }
                }
            }
        });
        let (topic, message) = pubsub_topic_and_message(&notification).expect("topic");
        assert_eq!(topic, "polls.123");
        assert_eq!(message["type"], "POLL_UPDATE");
        assert_eq!(message["data"]["poll"]["poll_id"], "p1");
    }

    #[test]
    fn classifies_private_topics_without_logging_ids() {
        assert_eq!(topic_kind("community-points-user-v1.123"), "viewer");
        assert_eq!(topic_kind("video-playback-by-id.456"), "playback");
        assert_eq!(topic_kind("polls.789"), "poll");
        assert_eq!(topic_kind("predictions-channel-v1.789"), "prediction");
    }

    #[test]
    fn classifies_hermes_errors_without_echoing_details() {
        assert_eq!(
            hermes_error_class("Hermes authentication failed"),
            "authentication"
        );
        assert_eq!(
            hermes_error_class("Hermes subscription failed"),
            "subscription"
        );
        assert_eq!(
            hermes_error_class("Hermes requested reconnect"),
            "reconnect"
        );
        assert_eq!(hermes_error_class("Hermes read: connection closed"), "socket");
    }
}
