// Helix EventSub subscription create/delete and user-id resolution.

#[derive(Debug)]
enum CreateSubscriptionError {
    Auth(String),
    Other(String),
}

fn subscription_auth_rejected(status: u16) -> bool {
    matches!(status, 401 | 403)
}

async fn sync_subscriptions(
    token: &str,
    client_id: &str,
    session_id: &str,
    desired_logins: &HashSet<String>,
    subs: &mut HashMap<String, String>,
    id_cache: &Mutex<HashMap<String, String>>,
) -> Result<(), String> {
    let stale: Vec<String> = subs
        .keys()
        .filter(|l| !desired_logins.contains(*l))
        .cloned()
        .collect();
    debug_raid(
        "eventsub.subscription.sync",
        &format!(
            "session={} desired_count={} active_count={} stale_count={}",
            crate::diagnostics::redact_id(session_id),
            desired_logins.len(),
            subs.len(),
            stale.len()
        ),
    );
    for login in stale {
        if let Some(sub_id) = subs.remove(&login) {
            let result = delete_subscription(token, client_id, &sub_id).await;
            debug_raid(
                "eventsub.subscription.delete",
                &format!(
                    "login={login} subscription={} ok={}",
                    crate::diagnostics::redact_id(&sub_id),
                    result.is_ok()
                ),
            );
        }
    }
    for login in desired_logins {
        if subs.contains_key(login) {
            continue;
        }
        let user_id = resolve_user_id(token, client_id, login, id_cache).await?;
        debug_raid(
            "eventsub.subscription.create",
            &format!(
                "login={login} broadcaster={}",
                crate::diagnostics::redact_id(&user_id)
            ),
        );
        match create_raid_subscription(token, client_id, session_id, &user_id).await {
            Ok(sub_id) => {
                debug_raid(
                    "eventsub.subscription.created",
                    &format!(
                        "login={login} subscription={}",
                        crate::diagnostics::redact_id(&sub_id)
                    ),
                );
                subs.insert(login.clone(), sub_id);
            }
            Err(CreateSubscriptionError::Auth(error)) => {
                debug_raid(
                    "eventsub.subscription.failed",
                    &format!("login={login} reason={}", eventsub_error_class(&error)),
                );
                return Err(error);
            }
            Err(CreateSubscriptionError::Other(error)) => {
                debug_raid(
                    "eventsub.subscription.failed",
                    &format!("login={login} reason={}", eventsub_error_class(&error)),
                );
                eprintln!("[eventsub] subscribe {login}: {error}");
            }
        }
    }
    Ok(())
}

async fn resolve_user_id(
    token: &str,
    client_id: &str,
    login: &str,
    cache: &Mutex<HashMap<String, String>>,
) -> Result<String, String> {
    if let Ok(g) = cache.lock() {
        if let Some(id) = g.get(login) {
            debug_raid("eventsub.user.cache", &format!("login={login} hit=true"));
            return Ok(id.clone());
        }
    }
    debug_raid("eventsub.user.lookup", &format!("login={login}"));
    let url = format!("https://api.twitch.tv/helix/users?login={login}");
    let res = shared_client()
        .get(&url)
        .header("Client-Id", client_id)
        .bearer_auth(token)
        .send()
        .await
        .map_err(|e| e.to_string())?;
    let status = res.status();
    debug_raid(
        "eventsub.user.lookup.result",
        &format!("login={login} http_status={}", status.as_u16()),
    );
    if !status.is_success() {
        let body = res.text().await.unwrap_or_default();
        return Err(format!("users lookup failed: {body}"));
    }
    let v: Value = res.json().await.map_err(|e| e.to_string())?;
    let id = v
        .pointer("/data/0/id")
        .and_then(|x| x.as_str())
        .ok_or_else(|| format!("no user id for {login}"))?
        .to_string();
    if let Ok(mut g) = cache.lock() {
        g.insert(login.to_string(), id.clone());
    }
    Ok(id)
}

async fn create_raid_subscription(
    token: &str,
    client_id: &str,
    session_id: &str,
    from_broadcaster_user_id: &str,
) -> Result<String, CreateSubscriptionError> {
    let body = json!({
        "type": "channel.raid",
        "version": "1",
        "condition": {
            "from_broadcaster_user_id": from_broadcaster_user_id
        },
        "transport": {
            "method": "websocket",
            "session_id": session_id
        }
    });
    let res = shared_client()
        .post(HELIX_EVENTSUB)
        .header("Client-Id", client_id)
        .bearer_auth(token)
        .json(&body)
        .send()
        .await
        .map_err(|error| CreateSubscriptionError::Other(error.to_string()))?;
    let status = res.status();
    debug_raid(
        "eventsub.subscription.http",
        &format!(
            "operation=create broadcaster={} http_status={}",
            crate::diagnostics::redact_id(from_broadcaster_user_id),
            status.as_u16()
        ),
    );
    let text = res.text().await.unwrap_or_default();
    if !status.is_success() {
        let message = format!("create sub {status}: {text}");
        if subscription_auth_rejected(status.as_u16()) {
            return Err(CreateSubscriptionError::Auth(message));
        }
        return Err(CreateSubscriptionError::Other(message));
    }
    let v: Value = serde_json::from_str(&text)
        .map_err(|error| CreateSubscriptionError::Other(error.to_string()))?;
    v.pointer("/data/0/id")
        .and_then(|x| x.as_str())
        .map(|s| s.to_string())
        .ok_or_else(|| CreateSubscriptionError::Other(format!("create sub missing id: {v}")))
}

async fn delete_subscription(token: &str, client_id: &str, id: &str) -> Result<(), String> {
    let url = format!("{HELIX_EVENTSUB}?id={id}");
    let res = shared_client()
        .delete(&url)
        .header("Client-Id", client_id)
        .bearer_auth(token)
        .send()
        .await
        .map_err(|e| e.to_string())?;
    let status = res.status();
    debug_raid(
        "eventsub.subscription.http",
        &format!(
            "operation=delete subscription={} http_status={}",
            crate::diagnostics::redact_id(id),
            status.as_u16()
        ),
    );
    if status.is_success() || status.as_u16() == 404 {
        Ok(())
    } else {
        Err(format!(
            "delete sub {}: {}",
            status,
            res.text().await.unwrap_or_default()
        ))
    }
}
