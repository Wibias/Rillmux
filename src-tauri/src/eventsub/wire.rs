// EventSub wire types and raid notification parsing.

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
