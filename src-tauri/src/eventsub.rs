//! Twitch EventSub WebSocket — outgoing `channel.raid` for watched channels.

use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Duration;

use futures_util::stream::{SplitSink, SplitStream};
use futures_util::{SinkExt, StreamExt};
use serde::Deserialize;
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

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RaidOutgoing {
    pub from_channel: String,
    pub to_channel: String,
    pub to_user_id: String,
    pub viewers: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub remaining_seconds: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub kind: Option<String>,
}

struct EventSubState {
    enabled: bool,
    /// Lowercase logins the UI wants watched for outgoing raids.
    logins: HashSet<String>,
}

fn state() -> &'static Mutex<EventSubState> {
    static S: OnceLock<Mutex<EventSubState>> = OnceLock::new();
    S.get_or_init(|| {
        Mutex::new(EventSubState {
            enabled: true,
            logins: HashSet::new(),
        })
    })
}

fn wake() -> &'static Notify {
    static N: OnceLock<Notify> = OnceLock::new();
    N.get_or_init(Notify::new)
}

static STARTED: AtomicBool = AtomicBool::new(false);

pub fn init(app: AppHandle) {
    if STARTED.swap(true, Ordering::SeqCst) {
        return;
    }
    debug_raid("eventsub.supervisor.start", "started=true");
    tauri::async_runtime::spawn(async move {
        run_supervisor(app).await;
    });
}

/// Enable/disable + replace the watched login set (lowercase).
pub fn sync(enabled: bool, logins: Vec<String>) {
    let requested_count = logins.len();
    if let Ok(mut g) = state().lock() {
        g.enabled = enabled;
        g.logins = logins
            .into_iter()
            .map(|s| s.to_ascii_lowercase())
            .filter(|s| !s.is_empty())
            .collect();
        debug_raid(
            "eventsub.sync.native",
            &format!(
                "enabled={enabled} requested_count={requested_count} desired_count={}",
                g.logins.len()
            ),
        );
    }
    wake().notify_waiters();
}

fn desired_state() -> Result<(bool, HashSet<String>), String> {
    let g = state().lock().map_err(|error| error.to_string())?;
    Ok((g.enabled, g.logins.clone()))
}

async fn run_supervisor(app: AppHandle) {
    let mut backoff = Duration::from_secs(1);
    loop {
        let (enabled, logins) = desired_state().unwrap_or((false, HashSet::new()));
        if !enabled || logins.is_empty() {
            debug_raid(
                "eventsub.supervisor.idle",
                &format!("enabled={enabled} target_count={}", logins.len()),
            );
            wake().notified().await;
            continue;
        }
        debug_raid(
            "eventsub.session.start",
            &format!(
                "target_count={} backoff_ms={}",
                logins.len(),
                backoff.as_millis()
            ),
        );
        match run_session(app.clone(), logins).await {
            Ok(()) => {
                debug_raid("eventsub.session.end", "reason=reconfigured");
                backoff = Duration::from_secs(1);
            }
            Err(e) => {
                let reason = eventsub_error_class(&e);
                debug_raid(
                    "eventsub.session.error",
                    &format!("reason={reason} backoff_ms={}", backoff.as_millis()),
                );
                // EventSub can reach its HTTP subscription calls without a
                // session validation round trip. Rebuild the shared client so
                // an offline-start transport failure cannot poison every retry.
                crate::http::reset_shared_client();
                eprintln!("[eventsub] session ended: {e}");
                tokio::time::sleep(backoff).await;
                backoff = (backoff * 2).min(Duration::from_secs(60));
            }
        }
    }
}

async fn connect_eventsub(url: &str) -> Result<EventSubSocket, String> {
    let parsed = url::Url::parse(url).map_err(|error| format!("ws url: {error}"))?;
    if parsed.scheme() != "wss" || parsed.host_str() != Some("eventsub.wss.twitch.tv") {
        return Err("EventSub reconnect URL was not a Twitch WSS endpoint".into());
    }
    debug_raid(
        "eventsub.connect.attempt",
        &format!("handoff={}", url != WS_URL),
    );
    let (socket, _) = connect_async(url)
        .await
        .map_err(|error| format!("ws connect: {error}"))?;
    debug_raid("eventsub.connect.ok", &format!("handoff={}", url != WS_URL));
    Ok(socket)
}

async fn wait_for_welcome(
    write: &mut EventSubWrite,
    read: &mut EventSubRead,
) -> Result<WsSession, String> {
    let deadline = Instant::now() + WELCOME_TIMEOUT;
    loop {
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            return Err("EventSub welcome timed out".into());
        }
        let message = tokio::time::timeout(remaining, read.next())
            .await
            .map_err(|_| "EventSub welcome timed out".to_string())?
            .ok_or_else(|| "ws closed before welcome".to_string())?
            .map_err(|error| format!("ws read: {error}"))?;
        let text = match message {
            Message::Text(text) => text.to_string(),
            Message::Ping(payload) => {
                write
                    .send(Message::Pong(payload))
                    .await
                    .map_err(|error| format!("ws pong: {error}"))?;
                continue;
            }
            Message::Close(_) => return Err("ws close frame before welcome".into()),
            _ => continue,
        };
        let parsed: WsEnvelope =
            serde_json::from_str(&text).map_err(|error| format!("ws json: {error}"))?;
        if parsed.metadata.message_type != "session_welcome" {
            continue;
        }
        let session = parsed
            .payload
            .session
            .ok_or_else(|| "welcome missing session".to_string())?;
        debug_raid(
            "eventsub.welcome",
            &format!(
                "session={} keepalive_seconds={}",
                crate::diagnostics::redact_id(&session.id),
                session.keepalive_timeout_seconds.unwrap_or(10)
            ),
        );
        return Ok(session);
    }
}

async fn connect_and_welcome(
    url: &str,
) -> Result<(EventSubWrite, EventSubRead, WsSession), String> {
    let socket = connect_eventsub(url).await?;
    let (mut write, mut read) = socket.split();
    let session = wait_for_welcome(&mut write, &mut read).await?;
    Ok((write, read, session))
}

fn keepalive_duration(session: &WsSession) -> Duration {
    Duration::from_secs(session.keepalive_timeout_seconds.unwrap_or(10).max(1)) + KEEPALIVE_GRACE
}

fn emit_notification(app: &AppHandle, parsed: &WsEnvelope) {
    if let Some(raid) = parse_raid_notification(parsed) {
        debug_raid(
            "raid.received.native",
            &format!(
                "from={} to={} to_user={} viewers={:?}",
                raid.from_channel,
                raid.to_channel,
                crate::diagnostics::redact_id(&raid.to_user_id),
                raid.viewers
            ),
        );
        let _ = app.emit("raid-outgoing", raid);
    }
}

async fn reconnect_with_handoff(
    app: &AppHandle,
    write: &mut EventSubWrite,
    read: &mut EventSubRead,
    reconnect_url: &str,
) -> Result<(EventSubWrite, EventSubRead, WsSession), String> {
    debug_raid("eventsub.reconnect.handoff.start", "requested=true");
    let mut replacement = Box::pin(connect_and_welcome(reconnect_url));
    loop {
        tokio::select! {
            result = replacement.as_mut() => {
                if let Ok((_, _, session)) = &result {
                    debug_raid(
                        "eventsub.reconnect.handoff.ready",
                        &format!("session={}", crate::diagnostics::redact_id(&session.id)),
                    );
                }
                return result;
            },
            old_message = read.next() => {
                let Some(old_message) = old_message else {
                    return replacement.as_mut().await;
                };
                match old_message.map_err(|error| format!("ws read during reconnect: {error}"))? {
                    Message::Text(text) => {
                        let parsed: WsEnvelope = serde_json::from_str(&text)
                            .map_err(|error| format!("ws json during reconnect: {error}"))?;
                        if parsed.metadata.message_type == "notification" {
                            emit_notification(app, &parsed);
                        }
                    }
                    Message::Ping(payload) => {
                        let _ = write.send(Message::Pong(payload)).await;
                    }
                    Message::Close(_) => return replacement.as_mut().await,
                    _ => {}
                }
            }
        }
    }
}

async fn run_session(app: AppHandle, initial_logins: HashSet<String>) -> Result<(), String> {
    let creds = auth::credentials_for_api()
        .await
        .map_err(|e| format!("auth: {e}"))?;
    let token = creds.access_token;
    let client_id = creds.client_id;

    let (mut write, mut read, mut session) = connect_and_welcome(WS_URL).await?;
    let mut session_id = session.id.clone();
    let mut keepalive_deadline = Instant::now() + keepalive_duration(&session);
    let mut subs: HashMap<String, String> = HashMap::new();
    let mut desired = initial_logins;
    let id_cache: Arc<Mutex<HashMap<String, String>>> = Arc::new(Mutex::new(HashMap::new()));

    sync_subscriptions(
        &token,
        &client_id,
        &session_id,
        &desired,
        &mut subs,
        &id_cache,
    )
    .await?;
    debug_raid(
        "eventsub.subscription.ready",
        &format!(
            "desired_count={} active_count={}",
            desired.len(),
            subs.len()
        ),
    );

    loop {
        let keepalive_wait = tokio::time::sleep_until(keepalive_deadline);
        tokio::pin!(keepalive_wait);
        tokio::select! {
            _ = wake().notified() => {
                let (enabled, logins) = desired_state()?;
                debug_raid(
                    "eventsub.resync",
                    &format!("enabled={enabled} desired_count={}", logins.len()),
                );
                if !enabled || logins.is_empty() {
                    let _ = write.close().await;
                    return Ok(());
                }
                desired = logins;
                sync_subscriptions(
                    &token,
                    &client_id,
                    &session_id,
                    &desired,
                    &mut subs,
                    &id_cache,
                )
                .await?;
            }
            _ = &mut keepalive_wait => {
                return Err("EventSub keepalive deadline expired".into());
            }
            msg = read.next() => {
                let Some(msg) = msg else {
                    return Err("ws closed".into());
                };
                let msg = msg.map_err(|e| format!("ws read: {e}"))?;
                let text = match msg {
                    Message::Text(t) => t.to_string(),
                    Message::Ping(p) => {
                        let _ = write.send(Message::Pong(p)).await;
                        continue;
                    }
                    Message::Close(_) => return Err("ws close frame".into()),
                    _ => continue,
                };
                keepalive_deadline = Instant::now() + keepalive_duration(&session);
                let parsed: WsEnvelope = serde_json::from_str(&text)
                    .map_err(|e| format!("ws json: {e}"))?;
                match parsed.metadata.message_type.as_str() {
                    "session_welcome" => {}
                    "session_keepalive" => {}
                    "session_reconnect" => {
                        debug_raid(
                            "eventsub.reconnect.requested",
                            &format!("session={}", crate::diagnostics::redact_id(&session_id)),
                        );
                        let reconnect_url = parsed
                            .payload
                            .session
                            .as_ref()
                            .and_then(|value| value.reconnect_url.as_deref())
                            .ok_or_else(|| "session_reconnect missing reconnect_url".to_string())?;
                        let (new_write, new_read, new_session) = reconnect_with_handoff(
                            &app,
                            &mut write,
                            &mut read,
                            reconnect_url,
                        )
                        .await?;
                        let _ = write.close().await;
                        write = new_write;
                        read = new_read;
                        session = new_session;
                        session_id = session.id.clone();
                        keepalive_deadline = Instant::now() + keepalive_duration(&session);

                        // A settings update can occur while the replacement socket is being
                        // established. Re-read desired state after handoff because Notify's
                        // notify_waiters does not retain a permit for a future waiter.
                        let (enabled, logins) = desired_state()?;
                        if !enabled || logins.is_empty() {
                            let _ = write.close().await;
                            return Ok(());
                        }
                        desired = logins;
                        sync_subscriptions(
                            &token,
                            &client_id,
                            &session_id,
                            &desired,
                            &mut subs,
                            &id_cache,
                        )
                        .await?;
                        debug_raid(
                            "eventsub.reconnect.complete",
                            &format!(
                                "session={} desired_count={} active_count={}",
                                crate::diagnostics::redact_id(&session_id),
                                desired.len(),
                                subs.len()
                            ),
                        );
                    }
                    "notification" => emit_notification(&app, &parsed),
                    "revocation" => {
                        if let Some(sub) = parsed.payload.subscription.as_ref() {
                            let before = subs.len();
                            subs.retain(|_, id| id != &sub.id);
                            debug_raid(
                                "eventsub.subscription.revoked",
                                &format!(
                                    "subscription={} removed={} active_count={}",
                                    crate::diagnostics::redact_id(&sub.id),
                                    before != subs.len(),
                                    subs.len()
                                ),
                            );
                        }
                    }
                    _ => {}
                }
            }
        }
    }
}

#[derive(Debug)]
enum CreateSubscriptionError {
    Auth(String),
    Other(String),
}

fn subscription_auth_rejected(status: u16) -> bool {
    matches!(status, 401 | 403)
}

async fn sync_subscriptions(
    token: &str,
    client_id: &str,
    session_id: &str,
    desired_logins: &HashSet<String>,
    subs: &mut HashMap<String, String>,
    id_cache: &Mutex<HashMap<String, String>>,
) -> Result<(), String> {
    let stale: Vec<String> = subs
        .keys()
        .filter(|l| !desired_logins.contains(*l))
        .cloned()
        .collect();
    debug_raid(
        "eventsub.subscription.sync",
        &format!(
            "session={} desired_count={} active_count={} stale_count={}",
            crate::diagnostics::redact_id(session_id),
            desired_logins.len(),
            subs.len(),
            stale.len()
        ),
    );
    for login in stale {
        if let Some(sub_id) = subs.remove(&login) {
            let result = delete_subscription(token, client_id, &sub_id).await;
            debug_raid(
                "eventsub.subscription.delete",
                &format!(
                    "login={login} subscription={} ok={}",
                    crate::diagnostics::redact_id(&sub_id),
                    result.is_ok()
                ),
            );
        }
    }
    for login in desired_logins {
        if subs.contains_key(login) {
            continue;
        }
        let user_id = resolve_user_id(token, client_id, login, id_cache).await?;
        debug_raid(
            "eventsub.subscription.create",
            &format!(
                "login={login} broadcaster={}",
                crate::diagnostics::redact_id(&user_id)
            ),
        );
        match create_raid_subscription(token, client_id, session_id, &user_id).await {
            Ok(sub_id) => {
                debug_raid(
                    "eventsub.subscription.created",
                    &format!(
                        "login={login} subscription={}",
                        crate::diagnostics::redact_id(&sub_id)
                    ),
                );
                subs.insert(login.clone(), sub_id);
            }
            Err(CreateSubscriptionError::Auth(error)) => {
                debug_raid(
                    "eventsub.subscription.failed",
                    &format!("login={login} reason={}", eventsub_error_class(&error)),
                );
                return Err(error);
            }
            Err(CreateSubscriptionError::Other(error)) => {
                debug_raid(
                    "eventsub.subscription.failed",
                    &format!("login={login} reason={}", eventsub_error_class(&error)),
                );
                eprintln!("[eventsub] subscribe {login}: {error}");
            }
        }
    }
    Ok(())
}

async fn resolve_user_id(
    token: &str,
    client_id: &str,
    login: &str,
    cache: &Mutex<HashMap<String, String>>,
) -> Result<String, String> {
    if let Ok(g) = cache.lock() {
        if let Some(id) = g.get(login) {
            debug_raid("eventsub.user.cache", &format!("login={login} hit=true"));
            return Ok(id.clone());
        }
    }
    debug_raid("eventsub.user.lookup", &format!("login={login}"));
    let url = format!("https://api.twitch.tv/helix/users?login={login}");
    let res = shared_client()
        .get(&url)
        .header("Client-Id", client_id)
        .bearer_auth(token)
        .send()
        .await
        .map_err(|e| e.to_string())?;
    let status = res.status();
    debug_raid(
        "eventsub.user.lookup.result",
        &format!("login={login} http_status={}", status.as_u16()),
    );
    if !status.is_success() {
        let body = res.text().await.unwrap_or_default();
        return Err(format!("users lookup failed: {body}"));
    }
    let v: Value = res.json().await.map_err(|e| e.to_string())?;
    let id = v
        .pointer("/data/0/id")
        .and_then(|x| x.as_str())
        .ok_or_else(|| format!("no user id for {login}"))?
        .to_string();
    if let Ok(mut g) = cache.lock() {
        g.insert(login.to_string(), id.clone());
    }
    Ok(id)
}

async fn create_raid_subscription(
    token: &str,
    client_id: &str,
    session_id: &str,
    from_broadcaster_user_id: &str,
) -> Result<String, CreateSubscriptionError> {
    let body = json!({
        "type": "channel.raid",
        "version": "1",
        "condition": {
            "from_broadcaster_user_id": from_broadcaster_user_id
        },
        "transport": {
            "method": "websocket",
            "session_id": session_id
        }
    });
    let res = shared_client()
        .post(HELIX_EVENTSUB)
        .header("Client-Id", client_id)
        .bearer_auth(token)
        .json(&body)
        .send()
        .await
        .map_err(|error| CreateSubscriptionError::Other(error.to_string()))?;
    let status = res.status();
    debug_raid(
        "eventsub.subscription.http",
        &format!(
            "operation=create broadcaster={} http_status={}",
            crate::diagnostics::redact_id(from_broadcaster_user_id),
            status.as_u16()
        ),
    );
    let text = res.text().await.unwrap_or_default();
    if !status.is_success() {
        let message = format!("create sub {status}: {text}");
        if subscription_auth_rejected(status.as_u16()) {
            return Err(CreateSubscriptionError::Auth(message));
        }
        return Err(CreateSubscriptionError::Other(message));
    }
    let v: Value = serde_json::from_str(&text)
        .map_err(|error| CreateSubscriptionError::Other(error.to_string()))?;
    v.pointer("/data/0/id")
        .and_then(|x| x.as_str())
        .map(|s| s.to_string())
        .ok_or_else(|| CreateSubscriptionError::Other(format!("create sub missing id: {v}")))
}

async fn delete_subscription(token: &str, client_id: &str, id: &str) -> Result<(), String> {
    let url = format!("{HELIX_EVENTSUB}?id={id}");
    let res = shared_client()
        .delete(&url)
        .header("Client-Id", client_id)
        .bearer_auth(token)
        .send()
        .await
        .map_err(|e| e.to_string())?;
    let status = res.status();
    debug_raid(
        "eventsub.subscription.http",
        &format!(
            "operation=delete subscription={} http_status={}",
            crate::diagnostics::redact_id(id),
            status.as_u16()
        ),
    );
    if status.is_success() || status.as_u16() == 404 {
        Ok(())
    } else {
        Err(format!(
            "delete sub {}: {}",
            status,
            res.text().await.unwrap_or_default()
        ))
    }
}

#[derive(Debug, Deserialize)]
struct WsEnvelope {
    metadata: WsMeta,
    payload: WsPayload,
}

#[derive(Debug, Deserialize)]
struct WsMeta {
    message_type: String,
}

#[derive(Debug, Deserialize)]
struct WsPayload {
    #[serde(default)]
    session: Option<WsSession>,
    #[serde(default)]
    subscription: Option<WsSubscription>,
    #[serde(default)]
    event: Option<Value>,
}

#[derive(Debug, Deserialize)]
struct WsSession {
    id: String,
    #[serde(default)]
    keepalive_timeout_seconds: Option<u64>,
    #[serde(default)]
    reconnect_url: Option<String>,
}

#[derive(Debug, Deserialize)]
struct WsSubscription {
    id: String,
    #[serde(rename = "type")]
    kind: String,
}

fn parse_raid_notification(env: &WsEnvelope) -> Option<RaidOutgoing> {
    let sub = env.payload.subscription.as_ref()?;
    if sub.kind != "channel.raid" {
        return None;
    }
    let event = env.payload.event.as_ref()?;
    let from = event
        .get("from_broadcaster_user_login")?
        .as_str()?
        .to_ascii_lowercase();
    let to = event
        .get("to_broadcaster_user_login")?
        .as_str()?
        .to_ascii_lowercase();
    let to_user_id = event.get("to_broadcaster_user_id")?.as_str()?.to_string();
    let viewers = event.get("viewers").and_then(|v| v.as_u64());
    Some(RaidOutgoing {
        from_channel: from,
        to_channel: to,
        to_user_id,
        viewers,
        remaining_seconds: None,
        kind: Some("go".into()),
    })
}

#[cfg(test)]
fn parse_raid_notification_json(text: &str) -> Option<RaidOutgoing> {
    let env: WsEnvelope = serde_json::from_str(text).ok()?;
    if env.metadata.message_type != "notification" {
        return None;
    }
    parse_raid_notification(&env)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_channel_raid_notification() {
        let raw = r#"{
          "metadata": {
            "message_id": "1",
            "message_type": "notification",
            "message_timestamp": "2026-08-01T00:00:00Z",
            "subscription_type": "channel.raid",
            "subscription_version": "1"
          },
          "payload": {
            "subscription": {
              "id": "sub1",
              "status": "enabled",
              "type": "channel.raid",
              "version": "1",
              "condition": { "from_broadcaster_user_id": "111" },
              "transport": { "method": "websocket", "session_id": "s" },
              "created_at": "2026-08-01T00:00:00Z",
              "cost": 0
            },
            "event": {
              "from_broadcaster_user_id": "111",
              "from_broadcaster_user_login": "Alice",
              "from_broadcaster_user_name": "Alice",
              "to_broadcaster_user_id": "222",
              "to_broadcaster_user_login": "Bob",
              "to_broadcaster_user_name": "Bob",
              "viewers": 42
            }
          }
        }"#;
        let raid = parse_raid_notification_json(raw).expect("parse");
        assert_eq!(raid.from_channel, "alice");
        assert_eq!(raid.to_channel, "bob");
        assert_eq!(raid.to_user_id, "222");
        assert_eq!(raid.viewers, Some(42));
        assert_eq!(raid.kind.as_deref(), Some("go"));
    }

    #[test]
    fn identifies_subscription_auth_rejection() {
        assert!(subscription_auth_rejected(401));
        assert!(subscription_auth_rejected(403));
        assert!(!subscription_auth_rejected(429));
        assert!(!subscription_auth_rejected(500));
    }

    #[test]
    fn keepalive_uses_server_timeout_plus_small_grace() {
        let session = WsSession {
            id: "s".into(),
            keepalive_timeout_seconds: Some(10),
            reconnect_url: None,
        };
        assert_eq!(keepalive_duration(&session), Duration::from_secs(12));
    }

    #[test]
    fn classifies_eventsub_errors_without_echoing_details() {
        assert_eq!(
            eventsub_error_class("auth: token unavailable"),
            "authentication"
        );
        assert_eq!(eventsub_error_class("create sub 500"), "subscription");
        assert_eq!(
            eventsub_error_class("session_reconnect missing reconnect_url"),
            "reconnect"
        );
        assert_eq!(
            eventsub_error_class("EventSub welcome timed out"),
            "welcome"
        );
        assert_eq!(eventsub_error_class("ws read: connection closed"), "socket");
    }
}
