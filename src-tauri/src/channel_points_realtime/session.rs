// A single authenticated Hermes session: connect, subscribe, and dispatch.

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
    wait_for_subscriptions(&mut socket, &mut subscriptions, &desired.logins_by_id).await?;
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
    if let Err(error) =
        subscribe_poll_topics(&mut socket, &desired.channel_ids, &desired.logins_by_id).await
    {
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
    if let Err(error) =
        subscribe_raid_topics(&mut socket, &desired.channel_ids, &desired.logins_by_id).await
    {
        crate::diagnostics::log_event(
            crate::diagnostics::DebugCategory::Raids,
            "raid.subscription.fallback",
            &format!(
                "generation={generation} reason={}",
                hermes_error_class(&error)
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
                                if kind == "raid" {
                                    emit_raid_pubsub(&topic, &message, &desired.logins_by_id);
                                }
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
