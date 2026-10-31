// Supervisor loop, connect/welcome handshake, and reconnect handoff.

async fn run_supervisor(app: AppHandle) {
    let mut backoff = Duration::from_secs(1);
    loop {
        let (enabled, logins) = desired_state().unwrap_or((false, HashSet::new()));
        if !enabled || logins.is_empty() {
            set_state(EventSubConnectionState::Idle);
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
        set_state(EventSubConnectionState::Connecting);
        match run_session(app.clone(), logins).await {
            Ok(()) => {
                debug_raid("eventsub.session.end", "reason=reconfigured");
                backoff = Duration::from_secs(1);
            }
            Err(e) => {
                let reason = eventsub_error_class(&e);
                set_error(reason);
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

fn map_eventsub_connect_result<T, E: std::fmt::Display>(
    result: Result<Result<T, E>, tokio::time::error::Elapsed>,
) -> Result<T, String> {
    match result {
        Ok(Ok(value)) => Ok(value),
        Ok(Err(error)) => Err(format!("ws connect: {error}")),
        Err(_) => Err("EventSub connection timed out".into()),
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
    let (socket, _) = map_eventsub_connect_result(
        tokio::time::timeout(CONNECT_TIMEOUT, connect_async(url)).await,
    )?;
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
