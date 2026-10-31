fn parse_context(body: &Value) -> Result<ContextState, ChannelPointsError> {
    if let Some(message) = gql_error_message(body) {
        return Err(ChannelPointsError::Message(message));
    }

    let channel = body
        .pointer("/data/community/channel")
        .ok_or_else(|| ChannelPointsError::Message("Channel Points are unavailable".into()))?;
    let channel_id = channel
        .get("id")
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| ChannelPointsError::Message("Twitch returned no channel id".into()))?
        .to_string();
    let community_points = channel.pointer("/self/communityPoints").ok_or_else(|| {
        ChannelPointsError::Message("Twitch returned no Channel Points state".into())
    })?;
    let balance = community_points
        .get("balance")
        .and_then(Value::as_u64)
        .ok_or_else(|| {
            ChannelPointsError::Message("Twitch returned no Channel Points balance".into())
        })?;
    let claim_id = community_points
        .pointer("/availableClaim/id")
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .map(str::to_string);

    Ok(ContextState {
        channel_id,
        balance,
        claim_id,
        poll: parse_active_poll(channel),
        rewards: parse_custom_rewards(channel),
    })
}

fn json_u64(value: &Value) -> Option<u64> {
    value
        .as_u64()
        .or_else(|| value.as_i64().and_then(|n| u64::try_from(n).ok()))
        .or_else(|| {
            value.as_f64().and_then(|n| {
                if n.is_finite() && n >= 0.0 {
                    Some(n as u64)
                } else {
                    None
                }
            })
        })
}

fn json_bool(value: &Value, keys: &[&str]) -> Option<bool> {
    keys.iter()
        .find_map(|key| value.get(*key).and_then(Value::as_bool))
}

fn json_nodes(value: &Value) -> Vec<&Value> {
    if let Some(items) = value.as_array() {
        return items.iter().collect();
    }
    value
        .get("edges")
        .and_then(Value::as_array)
        .map(|edges| edges.iter().filter_map(|edge| edge.get("node")).collect())
        .unwrap_or_default()
}

fn parse_custom_rewards(channel: &Value) -> Vec<ChannelPointsReward> {
    const PATHS: [&str; 5] = [
        "/communityPointsSettings/customRewards",
        "/communityPointsSettings/rewards",
        "/communityPointsSettings/customRewards/edges",
        "/redemptionSettings/customRewards",
        "/self/communityPoints/customRewards",
    ];
    PATHS
        .iter()
        .find_map(|path| {
            let found = channel.pointer(path)?;
            let nodes = if path.ends_with("/edges") {
                found
                    .as_array()?
                    .iter()
                    .filter_map(|edge| edge.get("node"))
                    .collect::<Vec<_>>()
            } else {
                json_nodes(found)
            };
            if nodes.is_empty() {
                None
            } else {
                Some(nodes)
            }
        })
        .into_iter()
        .flatten()
        .filter_map(parse_custom_reward)
        .collect()
}

fn parse_custom_reward(value: &Value) -> Option<ChannelPointsReward> {
    let id = value
        .get("id")
        .and_then(Value::as_str)
        .filter(|id| !id.is_empty())?
        .to_string();
    let title = value
        .get("title")
        .or_else(|| value.get("title"))
        .and_then(Value::as_str)
        .filter(|title| !title.is_empty())?
        .to_string();
    let cost = value
        .get("cost")
        .and_then(json_u64)
        .or_else(|| value.pointer("/cost/amount").and_then(json_u64))?;
    let image_url = value
        .pointer("/image/url")
        .or_else(|| value.pointer("/defaultImage/url"))
        .or_else(|| value.pointer("/image/url4x"))
        .and_then(Value::as_str)
        .filter(|url| !url.is_empty())
        .map(str::to_string);
    Some(ChannelPointsReward {
        id,
        title,
        cost,
        image_url,
        is_paused: json_bool(value, &["isPaused", "isPaused", "paused"]).unwrap_or(false),
        in_stock: json_bool(value, &["isInStock", "isInStock", "inStock"]).unwrap_or(true),
        is_enabled: json_bool(value, &["isEnabled", "isEnabled"]).unwrap_or(true),
        is_user_input_required: json_bool(value, &["isUserInputRequired", "isUserInputRequired"])
            .unwrap_or(false),
        prompt: value
            .get("prompt")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|prompt| !prompt.is_empty())
            .map(str::to_string),
        cooldown_seconds: value
            .get("cooldownSeconds")
            .and_then(json_u64)
            .or_else(|| value.get("cooldownSeconds").and_then(json_u64))
            .or_else(|| {
                value
                    .pointer("/self/redemptionCooldown/secondsRemaining")
                    .and_then(json_u64)
            })
            .unwrap_or(0),
    })
}

const REDEEM_REWARD_QUERY: &str = r#"mutation RedeemCommunityPointsCustomReward($input: RedeemCommunityPointsCustomRewardInput!) { redeemCommunityPointsCustomReward(input: $input) { error { code } } }"#;

fn redeem_reward_payload(channel_id: &str, reward_id: &str, text: Option<&str>) -> Value {
    let mut input = json!({
        "channelID": channel_id,
        "rewardID": reward_id,
    });
    if let Some(text) = text.map(str::trim).filter(|value| !value.is_empty()) {
        input["text"] = json!(text);
    }
    json!({
        "operationName": "RedeemCommunityPointsCustomReward",
        "query": REDEEM_REWARD_QUERY,
        "variables": { "input": input }
    })
}

fn u64_field(value: &Value, key: &str) -> u64 {
    value.get(key).and_then(Value::as_u64).unwrap_or(0)
}

fn parse_poll_value(poll: &Value) -> Option<ChannelPointsPoll> {
    let points_enabled = poll
        .get("channelPointsVotingEnabled")
        .and_then(Value::as_bool)
        .or_else(|| {
            poll.pointer("/settings/communityPointsVotes/isEnabled")
                .and_then(Value::as_bool)
        })
        .or_else(|| {
            poll.pointer("/settings/channelPointsVotes/isEnabled")
                .and_then(Value::as_bool)
        })
        .or_else(|| {
            poll.pointer("/settings/channel_points_votes/is_enabled")
                .and_then(Value::as_bool)
        });
    if points_enabled == Some(false) {
        return None;
    }
    let id = poll
        .get("id")
        .or_else(|| poll.get("poll_id"))
        .and_then(Value::as_str)
        .filter(|v| !v.is_empty())?;
    let status = poll
        .get("status")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_ascii_uppercase();
    if status != "ACTIVE" && status != "COMPLETED" {
        return None;
    }
    let title = poll
        .get("title")
        .or_else(|| poll.get("prompt"))
        .and_then(Value::as_str)
        .unwrap_or("Channel Points poll")
        .to_string();
    let remaining_seconds = poll
        .get("remainingSeconds")
        .or_else(|| poll.get("remainingDurationSeconds"))
        .or_else(|| poll.get("timeLeft"))
        .and_then(Value::as_u64)
        .or_else(|| {
            poll.get("remainingDurationMilliseconds")
                .or_else(|| poll.get("remaining_duration_milliseconds"))
                .and_then(Value::as_u64)
                .map(|ms| ms / 1000)
        });
    let cost = poll
        .get("cost")
        .or_else(|| poll.get("channelPointsPerVote"))
        .or_else(|| poll.pointer("/settings/cost"))
        .or_else(|| poll.pointer("/channelPointsVoting/amountPerVote"))
        .or_else(|| poll.pointer("/settings/communityPointsVotes/cost"))
        .or_else(|| poll.pointer("/settings/channelPointsVotes/cost"))
        .or_else(|| poll.pointer("/settings/channel_points_votes/cost"))
        .and_then(Value::as_u64)
        .unwrap_or(10);
    let voted_choice_id = poll
        .pointer("/self/choiceID")
        .or_else(|| poll.pointer("/self/choiceId"))
        .or_else(|| poll.pointer("/self/voter/choices/0/pollChoice/id"))
        .and_then(Value::as_str)
        .filter(|v| !v.is_empty())
        .map(str::to_string);
    let choices = poll
        .get("choices")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|choice| {
            let id = choice
                .get("id")
                .or_else(|| choice.get("choice_id"))
                .and_then(Value::as_str)
                .filter(|v| !v.is_empty())?;
            let title = choice
                .get("title")
                .and_then(Value::as_str)
                .unwrap_or("Choice")
                .to_string();
            let nested_votes = choice
                .pointer("/votes/total")
                .or_else(|| choice.pointer("/votes/communityPoints"))
                .or_else(|| choice.pointer("/votes/channelPoints"))
                .or_else(|| choice.pointer("/votes/channel_points"))
                .and_then(Value::as_u64)
                .unwrap_or(0);
            Some(ChannelPointsPollChoice {
                id: id.to_string(),
                title,
                votes: u64_field(choice, "votes")
                    .max(u64_field(choice, "totalVotes"))
                    .max(u64_field(choice, "channelPointsVotes"))
                    .max(nested_votes),
                points: u64_field(choice, "points").max(u64_field(choice, "totalPoints")),
                total_voters: u64_field(choice, "totalVoters"),
            })
        })
        .collect::<Vec<_>>();
    if choices.is_empty() {
        return None;
    }
    Some(ChannelPointsPoll {
        id: id.to_string(),
        title,
        status,
        remaining_seconds,
        cost,
        voted_choice_id,
        choices,
    })
}

fn live_poll(poll: ChannelPointsPoll) -> Option<ChannelPointsPoll> {
    if poll.status != "ACTIVE" {
        return None;
    }
    if poll.remaining_seconds == Some(0) {
        return None;
    }
    Some(poll)
}

fn parse_active_poll(channel: &Value) -> Option<ChannelPointsPoll> {
    [
        "/activePoll",
        "/viewerPoll",
        "/viewablePoll",
        "/currentPoll",
        "/poll",
        "/polls/0",
        "/communityPointsSettings/activePoll",
        "/channel/currentPoll",
        "/channel/viewerPoll",
        "/channel/viewablePoll",
    ]
    .into_iter()
    .find_map(|path| channel.pointer(path))
    .or_else(|| channel.get("activePoll"))
    .and_then(parse_poll_value)
    .and_then(live_poll)
    .or_else(|| parse_poll_value(channel).and_then(live_poll))
}

fn parse_viewable_poll_body(body: &Value) -> Option<ChannelPointsPoll> {
    if gql_error_message(body).is_some() {
        return None;
    }
    [
        "/data/channel",
        "/data/user",
        "/data/user/channel",
        "/data/community/channel",
        "/data/channel/viewerPoll",
        "/data/channel/viewablePoll",
        "/data/channel/currentPoll",
        "/data/channel/poll",
    ]
    .into_iter()
    .find_map(|path| body.pointer(path).and_then(parse_active_poll))
}

fn viewable_poll_payload(channel_login: &str, hash: &str, login_key: &str) -> Value {
    json!({
        "operationName": "ChannelPollContext_GetViewablePoll",
        "variables": { login_key: channel_login },
        "extensions": {
            "persistedQuery": {
                "version": 1,
                "sha256Hash": hash
            }
        }
    })
}

fn viewable_poll_query_payload(channel_login: &str, query: &str) -> Value {
    json!({
        "operationName": "ViewableChannelPoll",
        "query": query,
        "variables": { "login": channel_login }
    })
}

fn poll_cache() -> &'static Mutex<HashMap<String, ChannelPointsPoll>> {
    static CACHE: OnceLock<Mutex<HashMap<String, ChannelPointsPoll>>> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

fn poll_gql_misses() -> &'static Mutex<HashMap<String, Instant>> {
    static CACHE: OnceLock<Mutex<HashMap<String, Instant>>> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

fn prediction_cache() -> &'static Mutex<HashMap<String, ChannelPointsPrediction>> {
    static CACHE: OnceLock<Mutex<HashMap<String, ChannelPointsPrediction>>> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

fn prediction_gql_misses() -> &'static Mutex<HashMap<String, Instant>> {
    static CACHE: OnceLock<Mutex<HashMap<String, Instant>>> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

fn should_skip_poll_gql(channel_login: &str) -> bool {
    poll_gql_misses()
        .lock()
        .ok()
        .and_then(|cache| cache.get(channel_login).copied())
        .is_some_and(|stored_at| stored_at.elapsed() < POLL_GQL_MISS_TTL)
}

fn mark_poll_gql_miss(channel_login: &str) {
    if let Ok(mut cache) = poll_gql_misses().lock() {
        cache.insert(channel_login.to_string(), Instant::now());
    }
}

pub(crate) fn clear_poll_cache() {
    if let Ok(mut cache) = poll_cache().lock() {
        cache.clear();
    }
    if let Ok(mut cache) = poll_gql_misses().lock() {
        cache.clear();
    }
    if let Ok(mut cache) = prediction_cache().lock() {
        cache.clear();
    }
    if let Ok(mut cache) = prediction_gql_misses().lock() {
        cache.clear();
    }
}

fn json_value(value: &Value) -> Value {
    match value {
        Value::String(text) => serde_json::from_str(text).unwrap_or_else(|_| value.clone()),
        other => other.clone(),
    }
}

pub(crate) fn cached_poll(channel_id: &str) -> Option<ChannelPointsPoll> {
    poll_cache()
        .lock()
        .ok()?
        .get(channel_id)
        .cloned()
        .and_then(live_poll)
}

pub(crate) fn cached_prediction(channel_id: &str) -> Option<ChannelPointsPrediction> {
    prediction_cache()
        .lock()
        .ok()?
        .get(channel_id)
        .cloned()
        .filter(|prediction| prediction.status == "ACTIVE" || prediction.status == "LOCKED")
}

fn store_prediction(channel_id: &str, prediction: ChannelPointsPrediction) {
    if let Ok(mut cache) = prediction_cache().lock() {
        cache.insert(channel_id.to_string(), prediction);
    }
}

fn should_skip_prediction_gql(channel_login: &str) -> bool {
    prediction_gql_misses()
        .lock()
        .ok()
        .and_then(|cache| cache.get(channel_login).copied())
        .is_some_and(|stored_at| stored_at.elapsed() < POLL_GQL_MISS_TTL)
}

fn mark_prediction_gql_miss(channel_login: &str) {
    if let Ok(mut cache) = prediction_gql_misses().lock() {
        cache.insert(channel_login.to_string(), Instant::now());
    }
}

