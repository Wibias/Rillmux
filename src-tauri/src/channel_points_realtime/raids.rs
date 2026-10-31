// Raid PubSub decoding and frontend raid emission.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RaidPubsubKind {
    Start,
    Go,
    Cancel,
}

fn emit_raid_pubsub(topic: &str, message: &Value, logins_by_id: &HashMap<String, String>) {
    let Some((kind, raid)) = parse_raid_pubsub(topic, message, logins_by_id) else {
        return;
    };
    crate::diagnostics::log_event(
        crate::diagnostics::DebugCategory::Raids,
        "raid.pubsub",
        &format!(
            "kind={:?} from={} to={} viewers={:?}",
            kind, raid.from_channel, raid.to_channel, raid.viewers
        ),
    );
    if let Some(app) = APP.get() {
        let event = match kind {
            RaidPubsubKind::Cancel => "raid-cancelled",
            RaidPubsubKind::Start | RaidPubsubKind::Go => "raid-outgoing",
        };
        let _ = app.emit(event, raid);
    }
}

fn parse_raid_pubsub(
    topic: &str,
    message: &Value,
    logins_by_id: &HashMap<String, String>,
) -> Option<(RaidPubsubKind, crate::eventsub::RaidOutgoing)> {
    let channel_id = topic.strip_prefix("raid.")?.trim();
    if channel_id.is_empty() {
        return None;
    }
    let from_channel = logins_by_id.get(channel_id)?.clone();
    let event = json_value(message);
    let kind = match event
        .get("type")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_ascii_lowercase()
        .as_str()
    {
        "raid_update_v2" => RaidPubsubKind::Start,
        "raid_go_v2" => RaidPubsubKind::Go,
        "raid_cancel_v2" => RaidPubsubKind::Cancel,
        _ => return None,
    };
    let raid = event.get("raid").unwrap_or(&event);
    let to_channel = raid
        .get("target_login")
        .and_then(Value::as_str)
        .filter(|login| !login.is_empty())?
        .to_ascii_lowercase();
    let to_user_id = raid
        .get("target_id")
        .and_then(Value::as_str)
        .filter(|id| !id.is_empty())?
        .to_string();
    let viewers = raid.get("viewer_count").and_then(Value::as_u64);
    let remaining_seconds = raid
        .get("force_raid_now_seconds")
        .and_then(Value::as_u64)
        .or(match kind {
            RaidPubsubKind::Start => Some(90),
            _ => None,
        });
    Some((
        kind,
        crate::eventsub::RaidOutgoing {
            from_channel,
            to_channel,
            to_user_id,
            viewers,
            remaining_seconds,
            kind: Some(
                match kind {
                    RaidPubsubKind::Start => "start",
                    RaidPubsubKind::Go => "go",
                    RaidPubsubKind::Cancel => "cancel",
                }
                .into(),
            ),
        },
    ))
}

async fn subscribe_poll_topics(
    socket: &mut HermesSocket,
    channel_ids: &[String],
    logins_by_id: &HashMap<String, String>,
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
        wait_for_subscriptions(socket, &mut subscriptions, logins_by_id),
    )
    .await
    .map_err(|_| "Hermes poll/prediction subscription confirmation timed out".to_string())?
}

async fn subscribe_raid_topics(
    socket: &mut HermesSocket,
    channel_ids: &[String],
    logins_by_id: &HashMap<String, String>,
) -> Result<(), String> {
    let mut subscriptions = HashSet::new();
    for channel_id in channel_ids {
        let raid = format!("raid.{channel_id}");
        subscriptions.insert(send_subscription(socket, &raid).await?);
    }

    tokio::time::timeout(
        Duration::from_secs(3),
        wait_for_subscriptions(socket, &mut subscriptions, logins_by_id),
    )
    .await
    .map_err(|_| "Hermes raid subscription confirmation timed out".to_string())?
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
