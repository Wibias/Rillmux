// Hermes socket framing, authentication, and subscription confirmation.

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
    logins_by_id: &HashMap<String, String>,
) -> Result<(), String> {
    while !pending.is_empty() {
        let value = receive_json(socket).await?;
        if value.get("type").and_then(Value::as_str) != Some("subscribeResponse") {
            if let Some((topic, message)) = pubsub_topic_and_message(&value) {
                let kind = topic_kind(&topic);
                if kind == "raid" {
                    emit_raid_pubsub(&topic, &message, logins_by_id);
                }
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
