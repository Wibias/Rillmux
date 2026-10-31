pub(crate) fn ingest_pubsub(topic: &str, message: &Value) -> bool {
    if let Some(channel_id) = topic
        .strip_prefix("polls.")
        .map(str::trim)
        .filter(|id| !id.is_empty())
    {
        return ingest_poll(channel_id, message);
    }
    if let Some(channel_id) = topic
        .strip_prefix("predictions-channel-v1.")
        .map(str::trim)
        .filter(|id| !id.is_empty())
    {
        return ingest_prediction(channel_id, message);
    }
    false
}

fn ingest_poll(channel_id: &str, message: &Value) -> bool {
    let event = if message.get("type").and_then(Value::as_str) == Some("MESSAGE") {
        message
            .pointer("/data/message")
            .map(json_value)
            .unwrap_or_else(|| message.clone())
    } else {
        message.clone()
    };
    let event_type = event
        .get("type")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_ascii_uppercase();
    if matches!(
        event_type.as_str(),
        "POLL_COMPLETE" | "POLL_ARCHIVE" | "POLL_TERMINATE"
    ) {
        if let Ok(mut cache) = poll_cache().lock() {
            cache.remove(channel_id);
        }
        return true;
    }
    let poll_value = event.get("data").and_then(|data| data.get("poll"));
    let Some(poll) = poll_value
        .and_then(parse_poll_value)
        .or_else(|| parse_poll_value(&event))
    else {
        return false;
    };
    if poll.status != "ACTIVE" {
        if let Ok(mut cache) = poll_cache().lock() {
            cache.remove(channel_id);
        }
        return true;
    }
    if let Ok(mut cache) = poll_cache().lock() {
        cache.insert(channel_id.to_string(), poll);
    }
    true
}

fn ingest_prediction(channel_id: &str, message: &Value) -> bool {
    let event = if message.get("type").and_then(Value::as_str) == Some("MESSAGE") {
        message
            .pointer("/data/message")
            .map(json_value)
            .unwrap_or_else(|| message.clone())
    } else {
        message.clone()
    };
    let event_type = event
        .get("type")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_ascii_lowercase();
    if prediction_topic_ended(&event_type) {
        return clear_prediction(channel_id);
    }
    let payload = event
        .pointer("/data/event")
        .or_else(|| event.get("event"))
        .cloned()
        .unwrap_or(event);
    match parse_prediction_event(&payload) {
        Some(prediction) => {
            store_prediction(channel_id, prediction);
            true
        }
        None if prediction_payload_ended(&payload) => clear_prediction(channel_id),
        None => false,
    }
}

fn prediction_topic_ended(event_type: &str) -> bool {
    matches!(
        event_type,
        "event-complete"
            | "event-completed"
            | "event-cancel"
            | "event-cancelled"
            | "event-canceled"
    )
}

fn prediction_payload_ended(payload: &Value) -> bool {
    let status = str_field(payload, &["status"])
        .unwrap_or("")
        .to_ascii_uppercase();
    matches!(
        status.as_str(),
        "RESOLVED" | "RESOLVE_PENDING" | "CANCELED" | "CANCELLED" | "CANCEL_PENDING"
    )
}

fn clear_prediction(channel_id: &str) -> bool {
    if let Ok(mut cache) = prediction_cache().lock() {
        cache.remove(channel_id);
    }
    true
}

async fn fetch_viewable_poll(
    channel_login: &str,
    token: &str,
    client_version: &str,
) -> Option<ChannelPointsPoll> {
    for hash in VIEWABLE_POLL_HASHES {
        for login_key in ["login", "channelLogin"] {
            let payload = viewable_poll_payload(channel_login, hash, login_key);
            let Ok((status, body)) =
                post_web_gql(&payload, channel_login, token, client_version).await
            else {
                continue;
            };
            if !status.is_success() {
                continue;
            }
            if let Some(poll) = parse_viewable_poll_body(&body) {
                return Some(poll);
            }
        }
    }
    for query in VIEWABLE_POLL_QUERIES {
        let payload = viewable_poll_query_payload(channel_login, query);
        let Ok((status, body)) = post_web_gql(&payload, channel_login, token, client_version).await
        else {
            continue;
        };
        if !status.is_success() {
            continue;
        }
        if let Some(poll) = parse_viewable_poll_body(&body) {
            return Some(poll);
        }
    }
    None
}

fn parse_helix_polls(body: &Value) -> Option<ChannelPointsPoll> {
    body.get("data")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .find_map(|poll| {
            let mut mapped = poll.clone();
            if let Some(obj) = mapped.as_object_mut() {
                if let Some(enabled) = obj.remove("channel_points_voting_enabled") {
                    obj.insert("channelPointsVotingEnabled".into(), enabled);
                }
                if let Some(cost) = obj.remove("channel_points_per_vote") {
                    obj.insert("cost".into(), cost);
                }
            }
            parse_poll_value(&mapped).and_then(live_poll)
        })
}

fn str_field<'a>(value: &'a Value, keys: &[&str]) -> Option<&'a str> {
    keys.iter()
        .find_map(|key| value.get(*key).and_then(Value::as_str))
}

fn parse_prediction_event(event: &Value) -> Option<ChannelPointsPrediction> {
    let status = str_field(event, &["status"])
        .unwrap_or("ACTIVE")
        .to_ascii_uppercase();
    if status != "ACTIVE" && status != "LOCKED" {
        return None;
    }
    let id = str_field(event, &["id"]).filter(|value| !value.is_empty())?;
    let title = str_field(event, &["title", "prediction_title"])
        .filter(|value| !value.is_empty())
        .unwrap_or("Channel Points prediction")
        .to_string();
    let created_at = str_field(event, &["createdAt", "created_at"])
        .filter(|value| !value.is_empty())
        .map(str::to_string);
    let window_seconds = event
        .get("predictionWindowSeconds")
        .or_else(|| event.get("prediction_window_seconds"))
        .and_then(Value::as_u64);
    let predicted_outcome_id = event
        .pointer("/self/prediction/outcome/id")
        .or_else(|| event.pointer("/self/prediction/outcomeID"))
        .or_else(|| event.pointer("/self/prediction/outcomeId"))
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .map(str::to_string);
    let predicted_points = event
        .pointer("/self/prediction/points")
        .and_then(Value::as_u64);
    let outcomes = event
        .get("outcomes")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|outcome| {
            let id = str_field(outcome, &["id"]).filter(|value| !value.is_empty())?;
            let title = str_field(outcome, &["title"])
                .filter(|value| !value.is_empty())
                .unwrap_or("Outcome")
                .to_string();
            Some(ChannelPointsPredictionOutcome {
                id: id.to_string(),
                title,
                points: u64_field(outcome, "totalPoints").max(u64_field(outcome, "total_points")),
                users: u64_field(outcome, "totalUsers").max(u64_field(outcome, "total_users")),
            })
        })
        .collect::<Vec<_>>();
    if outcomes.len() < 2 {
        return None;
    }
    Some(ChannelPointsPrediction {
        id: id.to_string(),
        title,
        status,
        created_at,
        window_seconds,
        predicted_outcome_id,
        predicted_points,
        outcomes,
    })
}

fn parse_prediction(body: &Value) -> Option<ChannelPointsPrediction> {
    if gql_error_message(body).is_some() {
        return None;
    }
    [
        "/data/community/channel/activePredictionEvents",
        "/data/community/channel/lockedPredictionEvents",
        "/data/channel/activePredictionEvents",
        "/data/channel/lockedPredictionEvents",
        "/data/user/channel/activePredictionEvents",
        "/data/user/channel/lockedPredictionEvents",
        "/data/channel/predictionEvents",
    ]
    .into_iter()
    .find_map(|path| {
        body.pointer(path)
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .find_map(parse_prediction_event)
    })
}

fn prediction_context_payload(channel_login: &str) -> Value {
    json!({
        "operationName": "ChannelPointsPredictionContext",
        "variables": {
            "count": 1,
            "channelLogin": channel_login
        },
        "extensions": {
            "persistedQuery": {
                "version": 1,
                "sha256Hash": CHANNEL_POINTS_PREDICTION_CONTEXT_HASH
            }
        }
    })
}

fn prediction_query_payload(channel_login: &str, query: &str) -> Value {
    json!({
        "operationName": "ViewablePredictions",
        "query": query,
        "variables": { "login": channel_login }
    })
}

fn make_prediction_input(
    event_id: &str,
    outcome_id: &str,
    points: u64,
    transaction_id: &str,
) -> Value {
    json!({
        "eventID": event_id,
        "outcomeID": outcome_id,
        "points": points,
        "transactionID": transaction_id
    })
}

fn make_prediction_payload(
    event_id: &str,
    outcome_id: &str,
    points: u64,
    transaction_id: &str,
) -> Value {
    json!({
        "operationName": "MakePrediction",
        "query": MAKE_PREDICTION_QUERY,
        "variables": {
            "input": make_prediction_input(event_id, outcome_id, points, transaction_id)
        }
    })
}

fn make_prediction_persisted_payload(
    event_id: &str,
    outcome_id: &str,
    points: u64,
    transaction_id: &str,
) -> Value {
    json!({
        "operationName": "MakePrediction",
        "variables": {
            "input": make_prediction_input(event_id, outcome_id, points, transaction_id)
        },
        "extensions": {
            "persistedQuery": {
                "version": 1,
                "sha256Hash": MAKE_PREDICTION_HASH
            }
        }
    })
}

fn make_prediction_closed(body: &Value) -> bool {
    matches!(
        body.pointer("/data/makePrediction/error/code")
            .and_then(Value::as_str),
        Some("EVENT_NOT_ACTIVE" | "NOT_FOUND")
    )
}

fn make_prediction_error(body: &Value) -> Option<String> {
    let code = body
        .pointer("/data/makePrediction/error/code")
        .and_then(Value::as_str)
        .filter(|code| !code.is_empty())?;
    Some(match code {
        "NOT_ENOUGH_POINTS" => "Not enough Channel Points to make that prediction".into(),
        "EVENT_NOT_ACTIVE" | "NOT_FOUND" => "This prediction is no longer accepting votes".into(),
        "MUST_ACCEPT_TOS" => "Accept Predictions terms on twitch.tv, then try again".into(),
        "MULTIPLE_OUTCOMES" => "You already predicted the other outcome".into(),
        "MAX_POINTS_PER_EVENT" => "That would go over the prediction point limit".into(),
        "FORBIDDEN" | "EVENT_MANAGER" => "This account cannot vote on this prediction".into(),
        "REGION_LOCKED" | "CATEGORY_REGION_LOCKED" => {
            "Predictions are not available in this region".into()
        }
        other => format!("Twitch rejected the prediction ({other})"),
    })
}

fn drop_cached_prediction(channel_login: &str) {
    let channel_id = last_contexts().lock().ok().and_then(|cache| {
        cache
            .get(channel_login)
            .map(|context| context.channel_id.clone())
    });
    if let Some(channel_id) = channel_id {
        let _ = clear_prediction(&channel_id);
    }
    if let Ok(mut misses) = prediction_gql_misses().lock() {
        misses.remove(channel_login);
    }
}

async fn fetch_prediction(
    channel_login: &str,
    token: &str,
    client_version: &str,
) -> Option<ChannelPointsPrediction> {
    let payload = prediction_context_payload(channel_login);
    if let Ok((status, body)) = post_web_gql(&payload, channel_login, token, client_version).await {
        if status.is_success() {
            if let Some(prediction) = parse_prediction(&body) {
                return Some(prediction);
            }
        }
    }

    for query in [
        PREDICTION_QUERY,
        PREDICTION_QUERY_USER,
        PREDICTION_QUERY_BARE,
    ] {
        let payload = prediction_query_payload(channel_login, query);
        let Ok((status, body)) = post_web_gql(&payload, channel_login, token, client_version).await
        else {
            continue;
        };
        if !status.is_success() {
            continue;
        }
        if let Some(prediction) = parse_prediction(&body) {
            return Some(prediction);
        }
    }
    None
}

const CUSTOM_REWARDS_QUERIES: [&str; 2] = [
    r#"query ChannelCustomRewards($login: String!) { channel(name: $login) { communityPointsSettings { customRewards { id title prompt cost isPaused isEnabled isInStock isUserInputRequired cooldownSeconds image { url } defaultImage { url } } } } }"#,
    r#"query ChannelCustomRewards($login: String!) { user(login: $login) { channel { communityPointsSettings { customRewards { id title prompt cost isPaused isEnabled isInStock isUserInputRequired cooldownSeconds image { url } defaultImage { url } } } } } }"#,
];

fn custom_rewards_payload(channel_login: &str, query: &str) -> Value {
    json!({
        "operationName": "ChannelCustomRewards",
        "query": query,
        "variables": { "login": channel_login }
    })
}

fn parse_rewards_from_body(body: &Value) -> Vec<ChannelPointsReward> {
    const ROOTS: [&str; 3] = [
        "/data/community/channel",
        "/data/channel",
        "/data/user/channel",
    ];
    ROOTS
        .iter()
        .find_map(|path| {
            let channel = body.pointer(path)?;
            let rewards = parse_custom_rewards(channel);
            if rewards.is_empty() {
                None
            } else {
                Some(rewards)
            }
        })
        .unwrap_or_default()
}

async fn fetch_custom_rewards(
    channel_login: &str,
    token: &str,
    client_version: &str,
) -> Vec<ChannelPointsReward> {
    for query in CUSTOM_REWARDS_QUERIES {
        let payload = custom_rewards_payload(channel_login, query);
        let Ok((status, body)) = post_web_gql(&payload, channel_login, token, client_version).await
        else {
            continue;
        };
        if !status.is_success() {
            continue;
        }
        let rewards = parse_rewards_from_body(&body);
        if !rewards.is_empty() {
            return rewards;
        }
    }
    Vec::new()
}

async fn fetch_context(
    channel_login: &str,
    token: &str,
    client_version: &str,
) -> Result<ContextState, ChannelPointsError> {
    let mut last_error = None;
    for hash in CHANNEL_POINTS_CONTEXT_HASHES {
        let payload = context_payload(channel_login, hash);
        let (status, body) = post_web_gql(&payload, channel_login, token, client_version).await?;
        if !status.is_success() {
            last_error = Some(ChannelPointsError::Message(format!(
                "Twitch rejected ChannelPointsContext (HTTP {status})"
            )));
            continue;
        }
        match parse_context(&body) {
            Ok(context) => return Ok(context),
            Err(error) => last_error = Some(error),
        }
    }

    Err(last_error.unwrap_or_else(|| {
        ChannelPointsError::Message("Twitch ChannelPointsContext failed".into())
    }))
}

async fn claim_bonus(
    channel_login: &str,
    channel_id: &str,
    claim_id: &str,
    token: &str,
    client_version: &str,
) -> Result<u16, ChannelPointsError> {
    let payload = claim_payload(channel_id, claim_id);
    let (status, body) = post_tv_claim_gql(&payload, channel_login, token, client_version).await?;

    if !status.is_success() {
        return Err(ChannelPointsError::Message(format!(
            "Twitch rejected the Channel Points bonus claim (HTTP {status})"
        )));
    }
    if let Some(message) = gql_error_message(&body) {
        return Err(ChannelPointsError::Message(message));
    }
    if body.pointer("/data/claimCommunityPoints").is_none()
        || body
            .pointer("/data/claimCommunityPoints")
            .is_some_and(Value::is_null)
    {
        return Err(ChannelPointsError::Message(
            "Twitch did not apply the Channel Points bonus claim".into(),
        ));
    }

    Ok(status.as_u16())
}

