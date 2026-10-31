// A single EventSub session: subscribe, dispatch notifications, keepalive.

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
    set_connected();
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
