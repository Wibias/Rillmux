// Public status, save, and clear entry points.

fn status_from(auth: Option<StoredWebsiteAuth>) -> TwitchWebAuthStatus {
    TwitchWebAuthStatus {
        configured: auth.is_some(),
        login: auth.as_ref().map(|value| value.login.clone()),
        user_id: auth.as_ref().map(|value| value.user_id.clone()),
        playback_ready: auth.is_some(),
    }
}

pub fn get_status() -> Result<TwitchWebAuthStatus, TwitchWebAuthError> {
    website_auth_runtime().get_status(|| {
        // Migration cleanup for releases that previously mirrored the secret to
        // Streamlink's plugin-specific config file.
        remove_streamlink_auth()?;
        Ok(status_from(load_auth()?))
    })
}

async fn validate_token(token: &str) -> Result<ValidateResponse, TwitchWebAuthError> {
    let response = shared_client()
        .get(VALIDATE_URL)
        .header("Authorization", format!("OAuth {token}"))
        .send()
        .await
        .map_err(reset_on_transport)?;
    if !response.status().is_success() {
        return Err(TwitchWebAuthError::Message(format!(
            "Twitch rejected the website token ({})",
            response.status()
        )));
    }
    Ok(response.json().await?)
}

pub async fn save(raw_token: &str) -> Result<TwitchWebAuthStatus, TwitchWebAuthError> {
    let token = normalize_token(raw_token)?;
    let website = validate_token(&token).await?;
    let session = crate::auth::get_session()
        .await
        .map_err(|error| TwitchWebAuthError::Message(error.to_string()))?;
    let expected_user = session.user_id.ok_or_else(|| {
        TwitchWebAuthError::Message(
            "log in with Twitch in the app before adding playback authentication".into(),
        )
    })?;
    if !session.logged_in || expected_user != website.user_id {
        return Err(TwitchWebAuthError::Message(
            "the website token belongs to a different Twitch account".into(),
        ));
    }

    // Remove any secret written by older Rillmux versions before storing
    // the newly validated credential. User-owned Streamlink settings survive.
    remove_streamlink_auth()?;
    let stored = StoredWebsiteAuth {
        token,
        user_id: website.user_id,
        login: website.login,
    };
    save_auth(&stored)?;
    let status = status_from(Some(stored));
    website_auth_runtime().remember(status.clone());
    Ok(status)
}

pub fn clear() -> Result<TwitchWebAuthStatus, TwitchWebAuthError> {
    remove_streamlink_auth()?;
    clear_auth()?;
    let status = status_from(None);
    website_auth_runtime().remember(status.clone());
    Ok(status)
}
