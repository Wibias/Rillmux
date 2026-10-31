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

