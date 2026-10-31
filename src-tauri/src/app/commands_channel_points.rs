// Tauri commands for Channel Points context, voting, rewards, and Helix proxying.

fn claim_error_class(error: &str) -> &'static str {
    let lower = error.to_ascii_lowercase();
    if lower.contains("different twitch account") || lower.contains("does not match") {
        "account_mismatch"
    } else if lower.contains("not configured") {
        "not_configured"
    } else if lower.contains("timed out") {
        "timeout"
    } else if lower.contains("connection") {
        "connection"
    } else {
        "mutation_error"
    }
}

#[tauri::command]
async fn channel_points_refresh(
    channel_login: String,
    include_poll: Option<bool>,
) -> Result<channel_points::ChannelPointsSnapshot, String> {
    let include_poll = include_poll.unwrap_or(false);
    let previous_balance =
        channel_points::cached_snapshot(&channel_login).map(|snapshot| snapshot.balance);
    diagnostics::log_event(
        diagnostics::DebugCategory::Rewards,
        "context.query",
        &format!("channel={} include_poll={include_poll}", channel_login),
    );
    if diagnostics::debug_enabled() {
        for (index, hash) in twitch_gql_operations::CHANNEL_POINTS_CONTEXT_HASHES
            .iter()
            .enumerate()
        {
            diagnostics::log_event(
                diagnostics::DebugCategory::Rewards,
                "context.candidate.configured",
                &format!("index={index} hash={}", diagnostics::redact_hash(hash)),
            );
        }
    }
    let result = channel_points::refresh(&channel_login, include_poll).await;
    match &result {
        Ok(snapshot) => {
            let balance_delta =
                previous_balance.map(|previous| snapshot.balance as i128 - previous as i128);
            diagnostics::log_event(
                diagnostics::DebugCategory::PointsCredit,
                "balance.snapshot",
                &format!(
                    "channel={} balance={} previous={previous_balance:?} delta={balance_delta:?} bonus_available={} bonus_claimed={}",
                    snapshot.channel_login,
                    snapshot.balance,
                    snapshot.bonus_available,
                    snapshot.bonus_claimed
                ),
            );
            diagnostics::log_event(
                diagnostics::DebugCategory::PointsClaim,
                "claim.available",
                &format!(
                    "channel={} available={}",
                    snapshot.channel_login, snapshot.bonus_available
                ),
            );
            if snapshot.bonus_available
                || snapshot.bonus_claimed
                || snapshot.claim_http_status.is_some()
                || snapshot.claim_error.is_some()
            {
                diagnostics::log_event(
                    diagnostics::DebugCategory::PointsClaim,
                    "claim.attempt",
                    &format!("channel={}", snapshot.channel_login),
                );
                let claim_state = snapshot
                    .claim_error
                    .as_deref()
                    .map(claim_error_class)
                    .unwrap_or(if snapshot.bonus_claimed {
                        "claimed"
                    } else if snapshot.bonus_available {
                        "available"
                    } else {
                        "none"
                    });
                diagnostics::log_event(
                    diagnostics::DebugCategory::PointsClaim,
                    "claim.result",
                    &format!(
                        "channel={} claimed={} http_status={:?} state={claim_state}",
                        snapshot.channel_login, snapshot.bonus_claimed, snapshot.claim_http_status
                    ),
                );
            }
            diagnostics::log_event(
                diagnostics::DebugCategory::Rewards,
                "context.query.result",
                &format!(
                    "channel={} ok=true reward_count={}",
                    snapshot.channel_login,
                    snapshot.rewards.len()
                ),
            );
            diagnostics::log_event(
                diagnostics::DebugCategory::Rewards,
                "reward.catalog",
                &format!(
                    "channel={} count={}",
                    snapshot.channel_login,
                    snapshot.rewards.len()
                ),
            );
            if include_poll {
                diagnostics::log_event(
                    diagnostics::DebugCategory::Polls,
                    "poll.snapshot",
                    &format!(
                        "channel={} poll={} prediction={}",
                        snapshot.channel_login,
                        snapshot.poll.is_some(),
                        snapshot.prediction.is_some()
                    ),
                );
            }
        }
        Err(_) => diagnostics::log_event(
            diagnostics::DebugCategory::Rewards,
            "context.query.result",
            &format!("channel={} ok=false", channel_login),
        ),
    }
    result.map_err(|e| e.to_string())
}

#[tauri::command]
fn channel_points_cached(channel_login: String) -> Option<channel_points::ChannelPointsSnapshot> {
    channel_points::cached_snapshot(&channel_login)
}

#[tauri::command]
async fn channel_points_vote_poll(
    channel_login: String,
    poll_id: String,
    choice_id: String,
    cost: u64,
) -> Result<channel_points::ChannelPointsSnapshot, String> {
    diagnostics::log_event(
        diagnostics::DebugCategory::Polls,
        "poll.vote",
        &format!(
            "channel={} poll_id={} choice_id={} cost={cost}",
            channel_login,
            diagnostics::redact_id(&poll_id),
            diagnostics::redact_id(&choice_id)
        ),
    );
    let result = channel_points::vote_poll(&channel_login, &poll_id, &choice_id, cost).await;
    diagnostics::log_event(
        diagnostics::DebugCategory::Polls,
        "poll.vote.result",
        &format!("channel={} ok={}", channel_login, result.is_ok()),
    );
    result.map_err(|e| e.to_string())
}

#[tauri::command]
async fn channel_points_vote_prediction(
    channel_login: String,
    event_id: String,
    outcome_id: String,
    points: u64,
) -> Result<channel_points::ChannelPointsSnapshot, String> {
    diagnostics::log_event(
        diagnostics::DebugCategory::Polls,
        "prediction.vote",
        &format!(
            "channel={} event_id={} outcome_id={} points={points}",
            channel_login,
            diagnostics::redact_id(&event_id),
            diagnostics::redact_id(&outcome_id)
        ),
    );
    let result =
        channel_points::vote_prediction(&channel_login, &event_id, &outcome_id, points).await;
    diagnostics::log_event(
        diagnostics::DebugCategory::Polls,
        "prediction.vote.result",
        &format!("channel={} ok={}", channel_login, result.is_ok()),
    );
    result.map_err(|e| e.to_string())
}

#[tauri::command]
async fn channel_points_redeem_reward(
    channel_login: String,
    reward_id: String,
    text: Option<String>,
) -> Result<channel_points::ChannelPointsSnapshot, String> {
    diagnostics::log_event(
        diagnostics::DebugCategory::Rewards,
        "reward.redeem",
        &format!(
            "channel={} reward_id={} input_present={}",
            channel_login,
            diagnostics::redact_id(&reward_id),
            text.as_ref().is_some_and(|value| !value.is_empty())
        ),
    );
    let result = channel_points::redeem_reward(&channel_login, &reward_id, text).await;
    diagnostics::log_event(
        diagnostics::DebugCategory::Rewards,
        "reward.redeem.result",
        &format!("channel={} ok={}", channel_login, result.is_ok()),
    );
    result.map_err(|e| e.to_string())
}

/// Helix GET proxy: keeps the OAuth token inside Rust (never in the webview).
#[tauri::command]
async fn helix_fetch(
    path: String,
    query: Option<Vec<(String, String)>>,
) -> Result<serde_json::Value, String> {
    helix::fetch(&path, &query.unwrap_or_default())
        .await
        .map_err(|e| e.to_string())
}
