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

