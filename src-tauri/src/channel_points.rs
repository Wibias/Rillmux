use reqwest::header::{ACCEPT, AUTHORIZATION, REFERER, USER_AGENT};
use serde::Serialize;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};
use thiserror::Error;

use crate::http::shared_client;
use crate::twitch_gql_operations::{
    CHANNEL_POINTS_CONTEXT_HASHES, CHANNEL_POINTS_PREDICTION_CONTEXT_HASH,
    CLAIM_COMMUNITY_POINTS_HASH, MAKE_PREDICTION_HASH, MAKE_PREDICTION_QUERY, PREDICTION_QUERY,
    PREDICTION_QUERY_BARE, PREDICTION_QUERY_USER, VIEWABLE_POLL_HASHES, VIEWABLE_POLL_QUERIES,
    VOTE_IN_POLL_QUERY, VOTE_POLL_QUERY,
};

const TWITCH_URL: &str = "https://www.twitch.tv";
const GQL_URL: &str = "https://gql.twitch.tv/gql";
const USER_AGENT_VALUE: &str =
    "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/140.0.0.0 Safari/537.36";
const FALLBACK_CLIENT_VERSION: &str = "ef928475-9403-42f2-8a34-55784bd08e16";
const CLIENT_VERSION_TTL: Duration = Duration::from_secs(30 * 60);
const POLL_GQL_MISS_TTL: Duration = Duration::from_secs(20);

#[derive(Debug, Error)]
pub enum ChannelPointsError {
    #[error("{0}")]
    Message(String),
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChannelPointsPollChoice {
    pub id: String,
    pub title: String,
    pub votes: u64,
    pub points: u64,
    pub total_voters: u64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChannelPointsPoll {
    pub id: String,
    pub title: String,
    pub status: String,
    pub remaining_seconds: Option<u64>,
    pub cost: u64,
    pub voted_choice_id: Option<String>,
    pub choices: Vec<ChannelPointsPollChoice>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChannelPointsPredictionOutcome {
    pub id: String,
    pub title: String,
    pub points: u64,
    pub users: u64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChannelPointsPrediction {
    pub id: String,
    pub title: String,
    pub status: String,
    pub created_at: Option<String>,
    pub window_seconds: Option<u64>,
    pub predicted_outcome_id: Option<String>,
    pub predicted_points: Option<u64>,
    pub outcomes: Vec<ChannelPointsPredictionOutcome>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChannelPointsReward {
    pub id: String,
    pub title: String,
    pub cost: u64,
    pub image_url: Option<String>,
    pub is_paused: bool,
    pub in_stock: bool,
    pub is_enabled: bool,
    pub is_user_input_required: bool,
    pub prompt: Option<String>,
    pub cooldown_seconds: u64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChannelPointsSnapshot {
    pub channel_login: String,
    pub balance: u64,
    pub bonus_available: bool,
    pub bonus_claimed: bool,
    pub claim_http_status: Option<u16>,
    pub claim_error: Option<String>,
    pub poll: Option<ChannelPointsPoll>,
    pub prediction: Option<ChannelPointsPrediction>,
    pub rewards: Vec<ChannelPointsReward>,
}

#[derive(Debug, Clone)]
struct ContextState {
    channel_id: String,
    balance: u64,
    claim_id: Option<String>,
    poll: Option<ChannelPointsPoll>,
    rewards: Vec<ChannelPointsReward>,
}

fn last_contexts() -> &'static Mutex<HashMap<String, ContextState>> {
    static CACHE: OnceLock<Mutex<HashMap<String, ContextState>>> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

fn remember_context(channel_login: &str, context: &ContextState) {
    if let Ok(mut cache) = last_contexts().lock() {
        cache.insert(channel_login.to_string(), context.clone());
    }
}

pub fn cached_snapshot(raw_channel_login: &str) -> Option<ChannelPointsSnapshot> {
    let channel_login = raw_channel_login.trim().to_ascii_lowercase();
    if !valid_login(&channel_login) {
        return None;
    }
    let context = last_contexts().lock().ok()?.get(&channel_login).cloned()?;
    Some(ChannelPointsSnapshot {
        channel_login,
        balance: context.balance,
        bonus_available: context.claim_id.is_some(),
        bonus_claimed: false,
        claim_http_status: None,
        claim_error: None,
        poll: cached_poll(&context.channel_id).or(context.poll),
        prediction: cached_prediction(&context.channel_id),
        rewards: context.rewards,
    })
}

fn client_version_cache() -> &'static Mutex<Option<(String, Instant)>> {
    static CACHE: OnceLock<Mutex<Option<(String, Instant)>>> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(None))
}

fn valid_login(login: &str) -> bool {
    !login.is_empty()
        && login.len() <= 25
        && login.chars().all(|character| {
            character.is_ascii_lowercase() || character.is_ascii_digit() || character == '_'
        })
}

async fn current_client_version() -> String {
    if let Ok(cache) = client_version_cache().lock() {
        if let Some((version, stored_at)) = cache.as_ref() {
            if stored_at.elapsed() < CLIENT_VERSION_TTL {
                return version.clone();
            }
        }
    }

    let discovered = match shared_client()
        .get(TWITCH_URL)
        .header(USER_AGENT, USER_AGENT_VALUE)
        .send()
        .await
    {
        Ok(response) if response.status().is_success() => match response.text().await {
            Ok(html) => crate::viewer_presence::extract_client_version(&html),
            Err(_) => None,
        },
        _ => None,
    }
    .unwrap_or_else(|| FALLBACK_CLIENT_VERSION.to_string());

    if let Ok(mut cache) = client_version_cache().lock() {
        *cache = Some((discovered.clone(), Instant::now()));
    }
    discovered
}

fn gql_error_message(body: &Value) -> Option<String> {
    body.get("errors")
        .and_then(Value::as_array)
        .and_then(|errors| {
            errors.iter().find_map(|error| {
                error
                    .get("message")
                    .and_then(Value::as_str)
                    .map(|message| message.chars().take(240).collect::<String>())
            })
        })
}

async fn decode_gql_response(
    response: reqwest::Response,
) -> Result<(reqwest::StatusCode, Value), ChannelPointsError> {
    let status = response.status();
    let body = response.json::<Value>().await.map_err(|_| {
        ChannelPointsError::Message(format!(
            "Twitch returned an invalid Channel Points response (HTTP {status})"
        ))
    })?;
    Ok((status, body))
}

async fn post_web_gql(
    payload: &Value,
    channel_login: &str,
    token: &str,
    client_version: &str,
) -> Result<(reqwest::StatusCode, Value), ChannelPointsError> {
    let response = shared_client()
        .post(GQL_URL)
        .header(USER_AGENT, USER_AGENT_VALUE)
        .header(ACCEPT, "application/json")
        .header(AUTHORIZATION, format!("OAuth {token}"))
        .header("Client-Id", crate::twitch_web_auth::WEB_CLIENT_ID)
        .header(
            "Client-Session-Id",
            crate::twitch_web_auth::client_session_id(),
        )
        .header("Client-Version", client_version)
        .header("X-Device-Id", crate::twitch_web_auth::device_id())
        .header(REFERER, format!("{TWITCH_URL}/{channel_login}"))
        .json(payload)
        .send()
        .await
        .map_err(|error| {
            if error.is_timeout() {
                ChannelPointsError::Message("Twitch Channel Points request timed out".into())
            } else if error.is_connect() {
                ChannelPointsError::Message("Twitch Channel Points connection failed".into())
            } else {
                ChannelPointsError::Message("Twitch Channel Points request failed".into())
            }
        })?;

    decode_gql_response(response).await
}

async fn post_tv_claim_gql(
    payload: &Value,
    channel_login: &str,
    token: &str,
    client_version: &str,
) -> Result<(reqwest::StatusCode, Value), ChannelPointsError> {
    let response = shared_client()
        .post(GQL_URL)
        .header(USER_AGENT, USER_AGENT_VALUE)
        .header(ACCEPT, "application/json")
        .header(AUTHORIZATION, format!("OAuth {token}"))
        .header("Client-Id", crate::channel_points_claim_auth::TV_CLIENT_ID)
        .header(
            "Client-Session-Id",
            crate::channel_points_claim_auth::client_session_id(),
        )
        .header("Client-Version", client_version)
        .header("X-Device-Id", crate::channel_points_claim_auth::device_id())
        .header("Origin", TWITCH_URL)
        .header(REFERER, format!("{TWITCH_URL}/{channel_login}"))
        .json(payload)
        .send()
        .await
        .map_err(|error| {
            if error.is_timeout() {
                ChannelPointsError::Message("Twitch bonus claim timed out".into())
            } else if error.is_connect() {
                ChannelPointsError::Message("Twitch bonus claim connection failed".into())
            } else {
                ChannelPointsError::Message("Twitch bonus claim request failed".into())
            }
        })?;

    decode_gql_response(response).await
}

fn context_payload(channel_login: &str, hash: &str) -> Value {
    json!({
        "operationName": "ChannelPointsContext",
        "variables": { "channelLogin": channel_login },
        "extensions": {
            "persistedQuery": {
                "version": 1,
                "sha256Hash": hash
            }
        }
    })
}

fn vote_poll_input(poll_id: &str, choice_id: &str, cost: u64) -> Value {
    json!({
        "pollID": poll_id,
        "choiceID": choice_id,
        "cost": cost
    })
}

fn vote_poll_query_payload(poll_id: &str, choice_id: &str, cost: u64) -> Value {
    json!({
        "operationName": "VotePoll",
        "query": VOTE_POLL_QUERY,
        "variables": {
            "input": vote_poll_input(poll_id, choice_id, cost)
        }
    })
}

fn vote_in_poll_query_payload(poll_id: &str, choice_id: &str, cost: u64) -> Value {
    json!({
        "operationName": "VoteInPoll",
        "query": VOTE_IN_POLL_QUERY,
        "variables": {
            "input": vote_poll_input(poll_id, choice_id, cost)
        }
    })
}

fn claim_payload(channel_id: &str, claim_id: &str) -> Value {
    json!({
        "operationName": "ClaimCommunityPoints",
        "variables": {
            "input": {
                "channelID": channel_id,
                "claimID": claim_id
            }
        },
        "extensions": {
            "persistedQuery": {
                "version": 1,
                "sha256Hash": CLAIM_COMMUNITY_POINTS_HASH
            }
        }
    })
}

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

pub async fn refresh(
    raw_channel_login: &str,
    include_poll: bool,
) -> Result<ChannelPointsSnapshot, ChannelPointsError> {
    let channel_login = raw_channel_login.trim().to_ascii_lowercase();
    if !valid_login(&channel_login) {
        return Err(ChannelPointsError::Message(
            "invalid Twitch channel login".into(),
        ));
    }

    let points_auth = crate::twitch_web_auth::load_session()
        .map_err(|error| ChannelPointsError::Message(error.to_string()))?
        .ok_or_else(|| {
            ChannelPointsError::Message("Twitch Website Authentication is not configured".into())
        })?;
    let session = crate::auth::get_session()
        .await
        .map_err(|error| ChannelPointsError::Message(error.to_string()))?;
    if !session.logged_in || session.user_id.as_deref() != Some(points_auth.user_id.as_str()) {
        return Err(ChannelPointsError::Message(
            "Twitch Website Authentication does not match the current Twitch account".into(),
        ));
    }

    let client_version = current_client_version().await;
    let mut context = fetch_context(&channel_login, &points_auth.token, &client_version).await?;
    if context.rewards.is_empty() {
        let extra = fetch_custom_rewards(&channel_login, &points_auth.token, &client_version).await;
        if !extra.is_empty() {
            context.rewards = extra;
        }
    }
    let mut bonus_claimed = false;
    let mut claim_http_status = None;
    let mut claim_error = None;

    if let Some(claim_id) = context.claim_id.clone() {
        match crate::channel_points_claim_auth::load_session()
            .map_err(|error| ChannelPointsError::Message(error.to_string()))?
        {
            Some(claim_auth) if session.user_id.as_deref() == Some(claim_auth.user_id.as_str()) => {
                match claim_bonus(
                    &channel_login,
                    &context.channel_id,
                    &claim_id,
                    &claim_auth.token,
                    &client_version,
                )
                .await
                {
                    Ok(status) => {
                        bonus_claimed = true;
                        claim_http_status = Some(status);
                        context.claim_id = None;
                        if let Ok(updated) =
                            fetch_context(&channel_login, &points_auth.token, &client_version).await
                        {
                            context = updated;
                        }
                    }
                    Err(error) => claim_error = Some(error.to_string()),
                }
            }
            Some(_) => {
                claim_error =
                    Some("Bonus-claim authentication belongs to a different Twitch account".into());
            }
            None => {
                claim_error = Some(
                    "Bonus-claim authentication is not configured; connect bonus claims once"
                        .into(),
                );
            }
        }
    }

    let mut prediction = None;
    if include_poll {
        if context.poll.is_none() {
            context.poll = cached_poll(&context.channel_id);
        }
        prediction = cached_prediction(&context.channel_id);
        if context.poll.is_none() && !should_skip_poll_gql(&channel_login) {
            if let Ok(body) = crate::helix::fetch(
                "polls",
                &[("broadcaster_id".into(), context.channel_id.clone())],
            )
            .await
            {
                context.poll = parse_helix_polls(&body);
            }
            if context.poll.is_none() {
                context.poll =
                    fetch_viewable_poll(&channel_login, &points_auth.token, &client_version).await;
            }
            if context.poll.is_none() {
                mark_poll_gql_miss(&channel_login);
            }
        }
        if prediction.is_none() && !should_skip_prediction_gql(&channel_login) {
            prediction =
                fetch_prediction(&channel_login, &points_auth.token, &client_version).await;
            if let Some(ref next) = prediction {
                store_prediction(&context.channel_id, next.clone());
            } else {
                mark_prediction_gql_miss(&channel_login);
            }
        }
    } else {
        context.poll = None;
    }

    remember_context(&channel_login, &context);
    Ok(ChannelPointsSnapshot {
        channel_login,
        balance: context.balance,
        bonus_available: context.claim_id.is_some(),
        bonus_claimed,
        claim_http_status,
        claim_error,
        poll: context.poll,
        prediction,
        rewards: context.rewards.clone(),
    })
}

pub async fn vote_poll(
    raw_channel_login: &str,
    poll_id: &str,
    choice_id: &str,
    cost: u64,
) -> Result<ChannelPointsSnapshot, ChannelPointsError> {
    let channel_login = raw_channel_login.trim().to_ascii_lowercase();
    if !valid_login(&channel_login) {
        return Err(ChannelPointsError::Message(
            "invalid Twitch channel login".into(),
        ));
    }
    if poll_id.trim().is_empty() || choice_id.trim().is_empty() {
        return Err(ChannelPointsError::Message("invalid poll vote".into()));
    }
    let points_auth = crate::twitch_web_auth::load_session()
        .map_err(|error| ChannelPointsError::Message(error.to_string()))?
        .ok_or_else(|| {
            ChannelPointsError::Message("Twitch Website Authentication is not configured".into())
        })?;
    let session = crate::auth::get_session()
        .await
        .map_err(|error| ChannelPointsError::Message(error.to_string()))?;
    if !session.logged_in || session.user_id.as_deref() != Some(points_auth.user_id.as_str()) {
        return Err(ChannelPointsError::Message(
            "Twitch Website Authentication does not match the current Twitch account".into(),
        ));
    }
    let client_version = current_client_version().await;
    let cost = cost.min(1_000_000);
    let payloads = [
        vote_poll_query_payload(poll_id.trim(), choice_id.trim(), cost),
        vote_in_poll_query_payload(poll_id.trim(), choice_id.trim(), cost),
    ];
    let mut last_error = None;
    for payload in payloads {
        let (status, body) = post_web_gql(
            &payload,
            &channel_login,
            &points_auth.token,
            &client_version,
        )
        .await?;
        if !status.is_success() {
            last_error = Some(ChannelPointsError::Message(format!(
                "Twitch rejected the poll vote (HTTP {status})"
            )));
            continue;
        }
        if let Some(message) = gql_error_message(&body) {
            last_error = Some(ChannelPointsError::Message(message));
            continue;
        }
        return refresh(&channel_login, true).await;
    }
    Err(last_error
        .unwrap_or_else(|| ChannelPointsError::Message("Twitch rejected the poll vote".into())))
}

pub async fn vote_prediction(
    raw_channel_login: &str,
    event_id: &str,
    outcome_id: &str,
    points: u64,
) -> Result<ChannelPointsSnapshot, ChannelPointsError> {
    let channel_login = raw_channel_login.trim().to_ascii_lowercase();
    if !valid_login(&channel_login) {
        return Err(ChannelPointsError::Message(
            "invalid Twitch channel login".into(),
        ));
    }
    if event_id.trim().is_empty() || outcome_id.trim().is_empty() {
        return Err(ChannelPointsError::Message(
            "invalid prediction vote".into(),
        ));
    }
    let points_auth = crate::twitch_web_auth::load_session()
        .map_err(|error| ChannelPointsError::Message(error.to_string()))?
        .ok_or_else(|| {
            ChannelPointsError::Message("Twitch Website Authentication is not configured".into())
        })?;
    let session = crate::auth::get_session()
        .await
        .map_err(|error| ChannelPointsError::Message(error.to_string()))?;
    if !session.logged_in || session.user_id.as_deref() != Some(points_auth.user_id.as_str()) {
        return Err(ChannelPointsError::Message(
            "Twitch Website Authentication does not match the current Twitch account".into(),
        ));
    }
    let points = points.clamp(10, 250_000);
    let client_version = current_client_version().await;
    let transaction_id = uuid::Uuid::new_v4().to_string();
    let payloads = [
        make_prediction_payload(event_id.trim(), outcome_id.trim(), points, &transaction_id),
        make_prediction_persisted_payload(
            event_id.trim(),
            outcome_id.trim(),
            points,
            &transaction_id,
        ),
    ];
    let mut last_error = None;
    for payload in payloads {
        let (status, body) = post_web_gql(
            &payload,
            &channel_login,
            &points_auth.token,
            &client_version,
        )
        .await?;
        if !status.is_success() {
            last_error = Some(ChannelPointsError::Message(format!(
                "Twitch rejected the prediction (HTTP {status})"
            )));
            continue;
        }
        if let Some(message) = gql_error_message(&body) {
            last_error = Some(ChannelPointsError::Message(message));
            continue;
        }
        if make_prediction_closed(&body) {
            drop_cached_prediction(&channel_login);
            return refresh(&channel_login, true).await;
        }
        if let Some(message) = make_prediction_error(&body) {
            return Err(ChannelPointsError::Message(message));
        }
        return refresh(&channel_login, true).await;
    }
    Err(last_error
        .unwrap_or_else(|| ChannelPointsError::Message("Twitch rejected the prediction".into())))
}

pub async fn redeem_reward(
    raw_channel_login: &str,
    reward_id: &str,
    text: Option<String>,
) -> Result<ChannelPointsSnapshot, ChannelPointsError> {
    let channel_login = raw_channel_login.trim().to_ascii_lowercase();
    if !valid_login(&channel_login) {
        return Err(ChannelPointsError::Message(
            "invalid Twitch channel login".into(),
        ));
    }
    let reward_id = reward_id.trim();
    if reward_id.is_empty() {
        return Err(ChannelPointsError::Message("invalid reward".into()));
    }
    let text = text
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty());
    let points_auth = crate::twitch_web_auth::load_session()
        .map_err(|error| ChannelPointsError::Message(error.to_string()))?
        .ok_or_else(|| {
            ChannelPointsError::Message("Twitch Website Authentication is not configured".into())
        })?;
    let session = crate::auth::get_session()
        .await
        .map_err(|error| ChannelPointsError::Message(error.to_string()))?;
    if !session.logged_in || session.user_id.as_deref() != Some(points_auth.user_id.as_str()) {
        return Err(ChannelPointsError::Message(
            "Twitch Website Authentication does not match the current Twitch account".into(),
        ));
    }
    let _ = refresh(&channel_login, false).await?;
    let context = last_contexts()
        .lock()
        .ok()
        .and_then(|cache| cache.get(&channel_login).cloned())
        .ok_or_else(|| ChannelPointsError::Message("Channel Points are unavailable".into()))?;
    let requires_input = context
        .rewards
        .iter()
        .find(|reward| reward.id == reward_id)
        .map(|reward| reward.is_user_input_required)
        .unwrap_or(false);
    if requires_input && text.is_none() {
        return Err(ChannelPointsError::Message(
            "this reward needs a message".into(),
        ));
    }
    let payload = redeem_reward_payload(&context.channel_id, reward_id, text.as_deref());
    let client_version = current_client_version().await;
    let (status, body) = post_web_gql(
        &payload,
        &channel_login,
        &points_auth.token,
        &client_version,
    )
    .await?;
    if !status.is_success() {
        return Err(ChannelPointsError::Message(format!(
            "Twitch rejected the reward (HTTP {status})"
        )));
    }
    if let Some(message) = gql_error_message(&body) {
        return Err(ChannelPointsError::Message(message));
    }
    if let Some(code) = body
        .pointer("/data/redeemCommunityPointsCustomReward/error/code")
        .and_then(Value::as_str)
        .filter(|code| !code.is_empty())
    {
        return Err(ChannelPointsError::Message(format!(
            "Twitch rejected the reward ({code})"
        )));
    }
    refresh(&channel_login, true).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds_channel_points_context_payload() {
        let payload = context_payload("example", CHANNEL_POINTS_CONTEXT_HASHES[0]);
        assert_eq!(payload["operationName"], "ChannelPointsContext");
        assert_eq!(payload["variables"]["channelLogin"], "example");
        assert_eq!(
            payload["extensions"]["persistedQuery"]["sha256Hash"],
            CHANNEL_POINTS_CONTEXT_HASHES[0]
        );
    }

    #[test]
    fn builds_prediction_context_payload() {
        let payload = prediction_context_payload("example");
        assert_eq!(payload["operationName"], "ChannelPointsPredictionContext");
        assert_eq!(payload["variables"]["channelLogin"], "example");
        assert_eq!(payload["variables"]["count"], 1);
        assert_eq!(
            payload["extensions"]["persistedQuery"]["sha256Hash"],
            CHANNEL_POINTS_PREDICTION_CONTEXT_HASH
        );
    }

    #[test]
    fn builds_vote_poll_query_payload_without_persisted_hash() {
        let payload = vote_poll_query_payload("poll-1", "choice-2", 10);
        assert_eq!(payload["operationName"], "VotePoll");
        assert_eq!(payload["variables"]["input"]["pollID"], "poll-1");
        assert_eq!(payload["variables"]["input"]["choiceID"], "choice-2");
        assert_eq!(payload["variables"]["input"]["cost"], 10);
        assert!(payload.get("extensions").is_none());
    }

    #[test]
    fn builds_claim_payload() {
        let payload = claim_payload("123", "claim-456");
        assert_eq!(payload["operationName"], "ClaimCommunityPoints");
        assert_eq!(payload["variables"]["input"]["channelID"], "123");
        assert_eq!(payload["variables"]["input"]["claimID"], "claim-456");
        assert_eq!(
            payload["extensions"]["persistedQuery"]["sha256Hash"],
            CLAIM_COMMUNITY_POINTS_HASH
        );
    }

    #[test]
    fn parses_balance_and_available_claim() {
        let body = json!({
            "data": {
                "community": {
                    "channel": {
                        "id": "123",
                        "self": {
                            "communityPoints": {
                                "balance": 18450,
                                "availableClaim": { "id": "claim-456" }
                            }
                        }
                    }
                }
            }
        });
        let context = parse_context(&body).unwrap();
        assert_eq!(context.channel_id, "123");
        assert_eq!(context.balance, 18450);
        assert_eq!(context.claim_id.as_deref(), Some("claim-456"));
        assert!(context.poll.is_none());
        assert!(context.rewards.is_empty());
    }

    #[test]
    fn parses_custom_channel_points_rewards() {
        let body = json!({
            "data": {
                "community": {
                    "channel": {
                        "id": "123",
                        "communityPointsSettings": {
                            "customRewards": [
                                {
                                    "id": "r1",
                                    "title": "Highlight",
                                    "cost": 50,
                                    "isPaused": false,
                                    "isEnabled": true,
                                    "isInStock": true,
                                    "isUserInputRequired": false,
                                    "prompt": "Clip that play",
                                    "image": { "url": "https://example/a.png" }
                                },
                                {
                                    "id": "r2",
                                    "title": "Shoutout",
                                    "cost": 500,
                                    "isPaused": true,
                                    "isEnabled": true,
                                    "isInStock": true,
                                    "isUserInputRequired": true,
                                    "prompt": "Who should I shout out?",
                                    "cooldownSeconds": 30
                                },
                                { "id": "", "title": "bad" }
                            ]
                        },
                        "self": {
                            "communityPoints": {
                                "balance": 100,
                                "availableClaim": null
                            }
                        }
                    }
                }
            }
        });
        let context = parse_context(&body).unwrap();
        assert_eq!(context.rewards.len(), 2);
        assert_eq!(context.rewards[0].id, "r1");
        assert_eq!(context.rewards[0].title, "Highlight");
        assert_eq!(context.rewards[0].cost, 50);
        assert_eq!(
            context.rewards[0].image_url.as_deref(),
            Some("https://example/a.png")
        );
        assert!(!context.rewards[0].is_user_input_required);
        assert_eq!(context.rewards[0].prompt.as_deref(), Some("Clip that play"));
        assert!(context.rewards[1].is_paused);
        assert!(context.rewards[1].is_user_input_required);
        assert_eq!(
            context.rewards[1].prompt.as_deref(),
            Some("Who should I shout out?")
        );
        assert_eq!(context.rewards[1].cooldown_seconds, 30);
    }

    #[test]
    fn parses_custom_rewards_from_graphql_edges() {
        let body = json!({
            "data": {
                "community": {
                    "channel": {
                        "id": "123",
                        "communityPointsSettings": {
                            "customRewards": {
                                "edges": [
                                    {
                                        "node": {
                                            "id": "edge-1",
                                            "title": "Hype",
                                            "cost": 25,
                                            "isPaused": false,
                                            "isEnabled": true,
                                            "isInStock": true
                                        }
                                    }
                                ]
                            }
                        },
                        "self": {
                            "communityPoints": { "balance": 10, "availableClaim": null }
                        }
                    }
                }
            }
        });
        let context = parse_context(&body).unwrap();
        assert_eq!(context.rewards.len(), 1);
        assert_eq!(context.rewards[0].id, "edge-1");
        assert_eq!(context.rewards[0].cost, 25);
    }

    #[test]
    fn builds_redeem_reward_payload_without_empty_text() {
        let with_text = redeem_reward_payload("123", "r1", Some(" hello "));
        assert_eq!(
            with_text["operationName"],
            "RedeemCommunityPointsCustomReward"
        );
        assert_eq!(with_text["variables"]["input"]["channelID"], "123");
        assert_eq!(with_text["variables"]["input"]["rewardID"], "r1");
        assert_eq!(with_text["variables"]["input"]["text"], "hello");
        let without = redeem_reward_payload("123", "r1", Some("  "));
        assert!(without["variables"]["input"].get("text").is_none());
    }

    #[test]
    fn parses_active_channel_points_poll() {
        let body = json!({
            "data": {
                "community": {
                    "channel": {
                        "id": "123",
                        "activePoll": {
                            "id": "poll-1",
                            "title": "Next game?",
                            "status": "ACTIVE",
                            "remainingSeconds": 42,
                            "cost": 10,
                            "self": { "choiceID": "a" },
                            "choices": [
                                { "id": "a", "title": "Minecraft", "votes": 12, "points": 120 },
                                { "id": "b", "title": "GTA", "votes": 3, "points": 30 }
                            ]
                        },
                        "self": {
                            "communityPoints": {
                                "balance": 100,
                                "availableClaim": null
                            }
                        }
                    }
                }
            }
        });
        let context = parse_context(&body).unwrap();
        let poll = context.poll.expect("poll");
        assert_eq!(poll.id, "poll-1");
        assert_eq!(poll.title, "Next game?");
        assert_eq!(poll.cost, 10);
        assert_eq!(poll.remaining_seconds, Some(42));
        assert_eq!(poll.voted_choice_id.as_deref(), Some("a"));
        assert_eq!(poll.choices.len(), 2);
        assert_eq!(poll.choices[0].title, "Minecraft");
    }

    #[test]
    fn parses_viewable_poll_with_channel_points_cost() {
        let body = json!({
            "data": {
                "channel": {
                    "viewerPoll": {
                        "id": "poll-2",
                        "title": "Map?",
                        "status": "ACTIVE",
                        "remainingDurationSeconds": 30,
                        "channelPointsVotingEnabled": true,
                        "channelPointsPerVote": 50,
                        "choices": [
                            { "id": "a", "title": "Dust2", "channelPointsVotes": 8 },
                            { "id": "b", "title": "Mirage", "channelPointsVotes": 2 }
                        ]
                    }
                }
            }
        });
        let poll = parse_viewable_poll_body(&body).expect("poll");
        assert_eq!(poll.id, "poll-2");
        assert_eq!(poll.cost, 50);
        assert_eq!(poll.remaining_seconds, Some(30));
        assert_eq!(poll.choices[0].votes, 8);
    }

    #[test]
    fn ignores_polls_without_channel_points_voting() {
        let body = json!({
            "data": {
                "channel": {
                    "viewerPoll": {
                        "id": "poll-3",
                        "title": "Bits only",
                        "status": "ACTIVE",
                        "channelPointsVotingEnabled": false,
                        "choices": [{ "id": "a", "title": "Yes", "votes": 1 }]
                    }
                }
            }
        });
        assert!(parse_viewable_poll_body(&body).is_none());
    }

    #[test]
    fn parses_pubsub_channel_points_poll() {
        let event = json!({
            "type": "POLL_CREATE",
            "data": {
                "poll": {
                    "poll_id": "poll-4",
                    "title": "Next map?",
                    "status": "ACTIVE",
                    "remaining_duration_milliseconds": 45_000,
                    "settings": {
                        "channel_points_votes": { "is_enabled": true, "cost": 20 }
                    },
                    "choices": [
                        {
                            "choice_id": "a",
                            "title": "Dust2",
                            "votes": { "total": 4, "channel_points": 4 }
                        },
                        {
                            "choice_id": "b",
                            "title": "Mirage",
                            "votes": { "total": 1, "channel_points": 1 }
                        }
                    ]
                }
            }
        });
        ingest_pubsub("polls.999", &event);
        let poll = cached_poll("999").expect("cached poll");
        assert_eq!(poll.id, "poll-4");
        assert_eq!(poll.cost, 20);
        assert_eq!(poll.remaining_seconds, Some(45));
        assert_eq!(poll.choices[0].title, "Dust2");
        assert_eq!(poll.choices[0].votes, 4);
        ingest_pubsub("polls.999", &json!({ "type": "POLL_COMPLETE" }));
        assert!(cached_poll("999").is_none());
    }

    #[test]
    fn parses_user_channel_current_poll() {
        let body = json!({
            "data": {
                "user": {
                    "channel": {
                        "currentPoll": {
                            "id": "poll-5",
                            "title": "Game?",
                            "status": "ACTIVE",
                            "remainingDurationMilliseconds": 12_000,
                            "settings": {
                                "channelPointsVotes": { "isEnabled": true, "cost": 15 }
                            },
                            "choices": [
                                { "id": "a", "title": "Yes", "votes": { "total": 2, "channelPoints": 2 } }
                            ]
                        }
                    }
                }
            }
        });
        let poll = parse_viewable_poll_body(&body).expect("poll");
        assert_eq!(poll.id, "poll-5");
        assert_eq!(poll.cost, 15);
        assert_eq!(poll.remaining_seconds, Some(12));
    }

    #[test]
    fn parses_helix_active_channel_points_poll() {
        let body = json!({
            "data": [{
                "id": "poll-6",
                "title": "Draw?",
                "status": "ACTIVE",
                "channel_points_voting_enabled": true,
                "channel_points_per_vote": 25,
                "choices": [
                    { "id": "a", "title": "Yes", "votes": 4 },
                    { "id": "b", "title": "No", "votes": 1 }
                ]
            }]
        });
        let poll = parse_helix_polls(&body).expect("poll");
        assert_eq!(poll.id, "poll-6");
        assert_eq!(poll.cost, 25);
        assert_eq!(poll.choices[0].title, "Yes");
    }

    #[test]
    fn ignores_completed_or_expired_polls_on_stream_load() {
        let completed = json!({
            "data": [{
                "id": "poll-done",
                "title": "Closed",
                "status": "COMPLETED",
                "channel_points_voting_enabled": true,
                "choices": [{ "id": "a", "title": "Yes", "votes": 1 }]
            }]
        });
        assert!(parse_helix_polls(&completed).is_none());

        let expired = json!({
            "data": {
                "channel": {
                    "viewerPoll": {
                        "id": "poll-expired",
                        "title": "Ended",
                        "status": "ACTIVE",
                        "remainingSeconds": 0,
                        "channelPointsVotingEnabled": true,
                        "choices": [{ "id": "a", "title": "Yes", "votes": 1 }]
                    }
                }
            }
        });
        assert!(parse_viewable_poll_body(&expired).is_none());

        let untitled_status = json!({
            "data": {
                "channel": {
                    "viewerPoll": {
                        "id": "poll-nostatus",
                        "title": "Stale",
                        "channelPointsVotingEnabled": true,
                        "choices": [{ "id": "a", "title": "Yes", "votes": 1 }]
                    }
                }
            }
        });
        assert!(parse_viewable_poll_body(&untitled_status).is_none());
    }

    #[test]
    fn parses_active_prediction() {
        let body = json!({
            "data": {
                "channel": {
                    "activePredictionEvents": [
                        {
                            "id": "pred-1",
                            "title": "Will it be a draw?",
                            "status": "ACTIVE",
                            "createdAt": "2026-08-20T20:00:00Z",
                            "predictionWindowSeconds": 120,
                            "outcomes": [
                                { "id": "yes", "title": "Yes", "totalPoints": 400, "totalUsers": 8 },
                                { "id": "no", "title": "No", "totalPoints": 150, "totalUsers": 3 }
                            ],
                            "self": { "prediction": { "points": 50, "outcome": { "id": "yes" } } }
                        }
                    ]
                }
            }
        });
        let prediction = parse_prediction(&body).expect("prediction");
        assert_eq!(prediction.id, "pred-1");
        assert_eq!(prediction.title, "Will it be a draw?");
        assert_eq!(prediction.predicted_outcome_id.as_deref(), Some("yes"));
        assert_eq!(prediction.predicted_points, Some(50));
        assert_eq!(prediction.outcomes[0].points, 400);
        assert_eq!(prediction.outcomes.len(), 2);
    }

    #[test]
    fn parses_community_prediction_context_active_and_locked() {
        let active = json!({
            "data": {
                "community": {
                    "channel": {
                        "activePredictionEvents": [{
                            "id": "pred-active",
                            "title": "Active?",
                            "status": "ACTIVE",
                            "createdAt": "2026-08-26T20:00:00Z",
                            "predictionWindowSeconds": 120,
                            "outcomes": [
                                { "id": "a", "title": "Yes", "totalPoints": 12, "totalUsers": 2 },
                                { "id": "b", "title": "No", "totalPoints": 8, "totalUsers": 1 }
                            ]
                        }],
                        "lockedPredictionEvents": []
                    }
                }
            }
        });
        assert_eq!(parse_prediction(&active).expect("active").id, "pred-active");

        let locked = json!({
            "data": {
                "community": {
                    "channel": {
                        "activePredictionEvents": [],
                        "lockedPredictionEvents": [{
                            "id": "pred-locked",
                            "title": "Locked?",
                            "status": "LOCKED",
                            "createdAt": "2026-08-26T20:00:00Z",
                            "predictionWindowSeconds": 60,
                            "outcomes": [
                                { "id": "a", "title": "Yes", "totalPoints": 30, "totalUsers": 3 },
                                { "id": "b", "title": "No", "totalPoints": 20, "totalUsers": 2 }
                            ]
                        }]
                    }
                }
            }
        });
        let prediction = parse_prediction(&locked).expect("locked");
        assert_eq!(prediction.id, "pred-locked");
        assert_eq!(prediction.status, "LOCKED");
    }

    #[test]
    fn builds_make_prediction_payload() {
        let payload = make_prediction_payload("event-1", "yes", 25, "tx-1");
        assert_eq!(payload["operationName"], "MakePrediction");
        assert_eq!(payload["variables"]["input"]["eventID"], "event-1");
        assert_eq!(payload["variables"]["input"]["outcomeID"], "yes");
        assert_eq!(payload["variables"]["input"]["points"], 25);
        assert_eq!(payload["variables"]["input"]["transactionID"], "tx-1");
        let query = payload["query"]
            .as_str()
            .expect("inline MakePrediction query");
        assert!(
            query.contains("prediction {"),
            "MakePredictionPayload.prediction is the user bet, not predictionEvent"
        );
        assert!(
            !query.contains("predictionEvent"),
            "Twitch rejects predictionEvent on MakePredictionPayload"
        );
        let persisted = make_prediction_persisted_payload("event-1", "yes", 25, "tx-1");
        assert_eq!(
            persisted["extensions"]["persistedQuery"]["sha256Hash"],
            MAKE_PREDICTION_HASH
        );
    }

    #[test]
    fn maps_make_prediction_errors() {
        let body = json!({
            "data": { "makePrediction": { "error": { "code": "NOT_ENOUGH_POINTS" } } }
        });
        assert_eq!(
            make_prediction_error(&body).as_deref(),
            Some("Not enough Channel Points to make that prediction")
        );
        let closed = json!({
            "data": { "makePrediction": { "error": { "code": "EVENT_NOT_ACTIVE" } } }
        });
        assert_eq!(
            make_prediction_error(&closed).as_deref(),
            Some("This prediction is no longer accepting votes")
        );
        assert!(make_prediction_closed(&closed));
    }

    #[test]
    fn parses_pubsub_prediction_event() {
        let event = json!({
            "id": "pred-2",
            "title": "Next map?",
            "status": "ACTIVE",
            "created_at": "2026-08-20T20:00:00Z",
            "prediction_window_seconds": 60,
            "outcomes": [
                { "id": "a", "title": "Dust2", "total_points": 10, "total_users": 1 },
                { "id": "b", "title": "Mirage", "total_points": 20, "total_users": 2 }
            ]
        });
        let prediction = parse_prediction_event(&event).expect("prediction");
        assert_eq!(prediction.id, "pred-2");
        assert_eq!(prediction.window_seconds, Some(60));
        assert_eq!(prediction.outcomes[1].users, 2);
    }

    #[test]
    fn caches_pubsub_prediction_events() {
        ingest_pubsub(
            "predictions-channel-v1.42",
            &json!({
                "type": "event-updated",
                "data": {
                    "event": {
                        "id": "pred-3",
                        "title": "Ace?",
                        "status": "ACTIVE",
                        "outcomes": [
                            { "id": "a", "title": "Yes", "total_points": 1, "total_users": 1 },
                            { "id": "b", "title": "No", "total_points": 2, "total_users": 2 }
                        ]
                    }
                }
            }),
        );
        let prediction = cached_prediction("42").expect("cached");
        assert_eq!(prediction.id, "pred-3");
        ingest_pubsub(
            "predictions-channel-v1.42",
            &json!({ "type": "event-complete" }),
        );
        assert!(cached_prediction("42").is_none());
    }

    #[test]
    fn clears_prediction_when_event_updated_to_resolved() {
        ingest_pubsub(
            "predictions-channel-v1.43",
            &json!({
                "type": "event-updated",
                "data": {
                    "event": {
                        "id": "pred-43",
                        "title": "Done?",
                        "status": "ACTIVE",
                        "outcomes": [
                            { "id": "a", "title": "Yes", "total_points": 1, "total_users": 1 },
                            { "id": "b", "title": "No", "total_points": 2, "total_users": 2 }
                        ]
                    }
                }
            }),
        );
        assert!(cached_prediction("43").is_some());
        assert!(ingest_pubsub(
            "predictions-channel-v1.43",
            &json!({
                "type": "event-updated",
                "data": {
                    "event": {
                        "id": "pred-43",
                        "title": "Done?",
                        "status": "RESOLVED",
                        "outcomes": [
                            { "id": "a", "title": "Yes", "total_points": 1, "total_users": 1 },
                            { "id": "b", "title": "No", "total_points": 2, "total_users": 2 }
                        ]
                    }
                }
            }),
        ));
        assert!(cached_prediction("43").is_none());
    }

    #[test]
    fn caches_locked_prediction_for_overlay_recap() {
        ingest_pubsub(
            "predictions-channel-v1.44",
            &json!({
                "type": "event-updated",
                "data": {
                    "event": {
                        "id": "pred-44",
                        "title": "Over?",
                        "status": "ACTIVE",
                        "outcomes": [
                            { "id": "a", "title": "Yes", "total_points": 1, "total_users": 1 },
                            { "id": "b", "title": "No", "total_points": 2, "total_users": 2 }
                        ]
                    }
                }
            }),
        );
        assert!(cached_prediction("44").is_some());
        assert!(ingest_pubsub(
            "predictions-channel-v1.44",
            &json!({
                "type": "event-updated",
                "data": {
                    "event": {
                        "id": "pred-44",
                        "title": "Over?",
                        "status": "LOCKED",
                        "outcomes": [
                            { "id": "a", "title": "Yes", "total_points": 1, "total_users": 1 },
                            { "id": "b", "title": "No", "total_points": 2, "total_users": 2 }
                        ]
                    }
                }
            }),
        ));
        assert_eq!(cached_prediction("44").expect("cached").status, "LOCKED");
    }

    #[test]
    fn clears_prediction_on_event_completed() {
        ingest_pubsub(
            "predictions-channel-v1.45",
            &json!({
                "type": "event-updated",
                "data": {
                    "event": {
                        "id": "pred-45",
                        "title": "Over?",
                        "status": "ACTIVE",
                        "outcomes": [
                            { "id": "a", "title": "Yes", "total_points": 1, "total_users": 1 },
                            { "id": "b", "title": "No", "total_points": 2, "total_users": 2 }
                        ]
                    }
                }
            }),
        );
        assert!(cached_prediction("45").is_some());
        assert!(ingest_pubsub(
            "predictions-channel-v1.45",
            &json!({ "type": "event-completed" }),
        ));
        assert!(cached_prediction("45").is_none());
    }

    #[test]
    fn ingest_pubsub_reports_whether_the_cache_changed() {
        assert!(ingest_pubsub(
            "predictions-channel-v1.77",
            &json!({
                "type": "event-updated",
                "data": {
                    "event": {
                        "id": "pred-77",
                        "title": "Live?",
                        "status": "ACTIVE",
                        "outcomes": [
                            { "id": "a", "title": "Yes", "total_points": 1, "total_users": 1 },
                            { "id": "b", "title": "No", "total_points": 2, "total_users": 2 }
                        ]
                    }
                }
            }),
        ));
        assert!(!ingest_pubsub(
            "video-playback-by-id.77",
            &json!({ "type": "viewcount" }),
        ));
    }

    #[test]
    fn cached_snapshot_merges_pubsub_into_last_context() {
        remember_context(
            "cachesnap",
            &ContextState {
                channel_id: "88".into(),
                balance: 42,
                claim_id: None,
                poll: None,
                rewards: vec![],
            },
        );
        assert!(ingest_pubsub(
            "predictions-channel-v1.88",
            &json!({
                "type": "event-updated",
                "data": {
                    "event": {
                        "id": "pred-88",
                        "title": "Cached?",
                        "status": "ACTIVE",
                        "outcomes": [
                            { "id": "a", "title": "Yes", "total_points": 1, "total_users": 1 },
                            { "id": "b", "title": "No", "total_points": 2, "total_users": 2 }
                        ]
                    }
                }
            }),
        ));
        let snapshot = cached_snapshot("cachesnap").expect("snapshot");
        assert_eq!(snapshot.balance, 42);
        assert_eq!(snapshot.prediction.expect("prediction").id, "pred-88");
    }

    #[test]
    fn parses_community_points_vote_settings() {
        let body = json!({
            "data": {
                "channel": {
                    "currentPoll": {
                        "id": "poll-7",
                        "title": "Pick",
                        "status": "ACTIVE",
                        "settings": {
                            "communityPointsVotes": { "isEnabled": true, "cost": 40 }
                        },
                        "choices": [{ "id": "a", "title": "A", "votes": { "total": 3, "communityPoints": 3 } }]
                    }
                }
            }
        });
        let poll = parse_viewable_poll_body(&body).expect("poll");
        assert_eq!(poll.cost, 40);
        assert_eq!(poll.choices[0].votes, 3);
    }
}
