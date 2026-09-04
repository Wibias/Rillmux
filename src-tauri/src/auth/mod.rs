mod store;

use std::sync::OnceLock;

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::http::shared_client;
use store::{clear_tokens, load_tokens, now_unix, save_tokens, StoredTokens};

const AUTH_URL: &str = "https://id.twitch.tv/oauth2/device";
const TOKEN_URL: &str = "https://id.twitch.tv/oauth2/token";
const VALIDATE_URL: &str = "https://id.twitch.tv/oauth2/validate";
const REVOKE_URL: &str = "https://id.twitch.tv/oauth2/revoke";
const DEV_CLIENT_ID: &str = "phiay4sq36lfv9zu7cbqwz2ndnesfd8";

/// Least privilege: only what the UI actually calls (followed streams).
/// Blocked-user scopes were dropped — the app has no block/unblock feature.
pub const DEFAULT_SCOPES: &[&str] = &["user:read:follows", "user:read:subscriptions"];

#[derive(Debug, Error)]
pub enum AuthError {
    #[error("{0}")]
    Message(String),
    #[error(transparent)]
    Http(#[from] reqwest::Error),
    #[error(transparent)]
    Store(#[from] store::TokenStoreError),
}

/// Twitch returns snake_case; we re-serialize to camelCase for the frontend.
#[derive(Debug, Deserialize)]
struct TwitchDeviceCodeBody {
    device_code: String,
    expires_in: u64,
    interval: u64,
    user_code: String,
    verification_uri: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DeviceCodeResponse {
    pub device_code: String,
    pub expires_in: u64,
    pub interval: u64,
    pub user_code: String,
    pub verification_uri: String,
}

impl From<TwitchDeviceCodeBody> for DeviceCodeResponse {
    fn from(value: TwitchDeviceCodeBody) -> Self {
        Self {
            device_code: value.device_code,
            expires_in: value.expires_in,
            interval: value.interval,
            user_code: value.user_code,
            verification_uri: value.verification_uri,
        }
    }
}

#[derive(Debug, Deserialize)]
struct TokenResponse {
    access_token: String,
    refresh_token: Option<String>,
    expires_in: Option<u64>,
    scope: Option<Vec<String>>,
}

#[derive(Debug, Deserialize)]
struct TokenErrorBody {
    message: Option<String>,
    #[allow(dead_code)]
    status: Option<u16>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AuthSession {
    pub logged_in: bool,
    // The access token is intentionally NOT exposed to the frontend.
    // Helix calls are proxied through the `helix_fetch` command in Rust.
    pub user_id: Option<String>,
    pub login: Option<String>,
    pub display_name: Option<String>,
    pub profile_image_url: Option<String>,
    pub scopes: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct ValidateResponse {
    client_id: String,
    login: String,
    user_id: String,
    scopes: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct HelixUsersResponse {
    data: Vec<HelixUser>,
}

#[derive(Debug, Deserialize)]
struct HelixUser {
    id: String,
    login: String,
    display_name: String,
    profile_image_url: String,
}

fn token_state_gate() -> &'static tokio::sync::Mutex<()> {
    static GATE: OnceLock<tokio::sync::Mutex<()>> = OnceLock::new();
    GATE.get_or_init(|| tokio::sync::Mutex::new(()))
}

fn select_client_id(
    compiled: Option<&str>,
    runtime: Option<&str>,
    allow_dev_fallback: bool,
) -> Result<String, AuthError> {
    if let Some(id) = compiled.map(str::trim).filter(|id| !id.is_empty()) {
        return Ok(id.to_string());
    }
    if allow_dev_fallback {
        if let Some(id) = runtime.map(str::trim).filter(|id| !id.is_empty()) {
            return Ok(id.to_string());
        }
        return Ok(DEV_CLIENT_ID.to_string());
    }
    Err(AuthError::Message(
        "release build is missing its Twitch client ID".into(),
    ))
}

fn client_id() -> Result<String, AuthError> {
    // Official releases compile the registered Rillmux client ID into the
    // binary. Runtime environment variables are intentionally a debug-only
    // convenience so an installed release cannot silently fall back to the
    // upstream Streamlink Twitch GUI application identity.
    let runtime = std::env::var("TWITCH_CLIENT_ID").ok();
    select_client_id(
        option_env!("TWITCH_CLIENT_ID"),
        runtime.as_deref(),
        cfg!(debug_assertions),
    )
}

fn stored_token_client_id(token_client_id: Option<&str>) -> Option<&str> {
    token_client_id.map(str::trim).filter(|id| !id.is_empty())
}

fn token_bound_client_id(token_client_id: Option<&str>, app_client_id: &str) -> String {
    stored_token_client_id(token_client_id)
        .unwrap_or(app_client_id)
        .to_string()
}

fn should_clear_tokens_after_refresh_rejection(client_id_known: bool) -> bool {
    client_id_known
}

fn http() -> reqwest::Client {
    shared_client()
}

fn map_http(err: reqwest::Error) -> AuthError {
    if crate::http::is_transient(&err) {
        if err.status().is_none() {
            crate::http::reset_shared_client();
        }
        AuthError::Message(crate::http::NETWORK_UNAVAILABLE.into())
    } else {
        AuthError::Http(err)
    }
}

fn map_session_error(err: AuthError) -> AuthError {
    match err {
        AuthError::Http(http_err) => map_http(http_err),
        AuthError::Message(msg) if msg.contains("will retry later") => {
            AuthError::Message(crate::http::NETWORK_UNAVAILABLE.into())
        }
        other => other,
    }
}

pub async fn start_device_flow() -> Result<DeviceCodeResponse, AuthError> {
    let client_id = client_id()?;
    let scope = DEFAULT_SCOPES.join(" ");
    let http = http();
    let res = http
        .post(AUTH_URL)
        .form(&[
            ("client_id", client_id.as_str()),
            ("scopes", scope.as_str()),
        ])
        .send()
        .await
        .map_err(map_http)?;
    let status = res.status();
    let body = res.text().await.unwrap_or_default();
    if !status.is_success() {
        return Err(AuthError::Message(format!(
            "device code request failed ({status}): {body}"
        )));
    }
    serde_json::from_str::<TwitchDeviceCodeBody>(&body)
        .map(DeviceCodeResponse::from)
        .map_err(|e| {
            AuthError::Message(format!(
                "device code response decode failed: {e}; body={body}"
            ))
        })
}

/// Result of one device-flow poll. `SlowDown` is distinct from `Pending` so
/// the frontend can increase its interval as RFC 8628 requires (Twitch
/// enforces this and rate-limits clients that ignore it).
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase", tag = "state")]
pub enum DevicePoll {
    #[serde(rename = "pending")]
    Pending,
    #[serde(rename = "slowDown")]
    SlowDown,
    #[serde(rename = "done")]
    Done { session: AuthSession },
}

pub async fn poll_device_token(device_code: &str) -> Result<DevicePoll, AuthError> {
    let client_id = client_id()?;
    let http = http();
    let res = http
        .post(TOKEN_URL)
        .form(&[
            ("client_id", client_id.as_str()),
            ("grant_type", "urn:ietf:params:oauth:grant-type:device_code"),
            ("device_code", device_code),
        ])
        .send()
        .await
        .map_err(map_http)?;

    if res.status().is_success() {
        let token: TokenResponse = res.json().await?;
        let _guard = token_state_gate().lock().await;
        let stored = StoredTokens {
            access_token: token.access_token,
            refresh_token: token.refresh_token,
            expires_at: token.expires_in.map(|s| now_unix().saturating_add(s)),
            scopes: token.scope.unwrap_or_default(),
            client_id: Some(client_id.clone()),
        };
        save_tokens(&stored)?;
        return Ok(DevicePoll::Done {
            session: session_from_tokens(stored).await?,
        });
    }

    let status = res.status();
    let err: TokenErrorBody = res.json().await.unwrap_or(TokenErrorBody {
        message: None,
        status: None,
    });
    let message = err.message.unwrap_or_default();
    if message == "authorization_pending" {
        return Ok(DevicePoll::Pending);
    }
    if message == "slow_down" {
        return Ok(DevicePoll::SlowDown);
    }
    if message == "expired_token" || message == "access_denied" {
        return Err(AuthError::Message(message));
    }
    Err(AuthError::Message(format!(
        "token poll failed ({status}): {message}"
    )))
}

async fn refresh_if_needed(mut tokens: StoredTokens) -> Result<StoredTokens, AuthError> {
    let needs_refresh = tokens
        .expires_at
        .map(|exp| now_unix() + 60 >= exp)
        .unwrap_or(false);
    if !needs_refresh {
        return Ok(tokens);
    }
    let Some(refresh) = tokens.refresh_token.clone() else {
        return Ok(tokens);
    };
    let client_id_known = stored_token_client_id(tokens.client_id.as_deref()).is_some();
    let app_client_id = client_id()?;
    let client_id = token_bound_client_id(tokens.client_id.as_deref(), &app_client_id);
    let http = http();
    let res = http
        .post(TOKEN_URL)
        .form(&[
            ("client_id", client_id.as_str()),
            ("grant_type", "refresh_token"),
            ("refresh_token", refresh.as_str()),
        ])
        .send()
        .await
        .map_err(map_http)?;
    if !res.status().is_success() {
        let status = res.status();
        // Only wipe the shared stored session when Twitch rejects a refresh
        // attempted with the persisted issuing client id. For pre-#70 rows,
        // the app client is only a fallback; rejecting it does not prove that
        // the refresh token itself is invalid.
        if status.as_u16() == 400 || status.as_u16() == 401 {
            if should_clear_tokens_after_refresh_rejection(client_id_known) {
                clear_tokens()?;
                return Err(AuthError::Message(
                    "session expired; please log in again".into(),
                ));
            }
            return Err(AuthError::Message(
                "session refresh was rejected before the issuing Twitch client ID could be recovered"
                    .into(),
            ));
        }
        return Err(AuthError::Message(crate::http::NETWORK_UNAVAILABLE.into()));
    }
    let token: TokenResponse = res.json().await?;
    tokens = StoredTokens {
        access_token: token.access_token,
        refresh_token: token.refresh_token.or(tokens.refresh_token),
        expires_at: token.expires_in.map(|s| now_unix().saturating_add(s)),
        scopes: token.scope.unwrap_or(tokens.scopes),
        client_id: tokens.client_id.or(Some(client_id)),
    };
    save_tokens(&tokens)?;
    Ok(tokens)
}

async fn validate_access_token(access_token: &str) -> Result<ValidateResponse, AuthError> {
    http()
        .get(VALIDATE_URL)
        .bearer_auth(access_token)
        .send()
        .await
        .map_err(map_http)?
        .error_for_status()
        .map_err(map_http)?
        .json()
        .await
        .map_err(map_http)
}

async fn recover_legacy_client_id_if_possible(tokens: &mut StoredTokens) -> Result<(), AuthError> {
    if stored_token_client_id(tokens.client_id.as_deref()).is_some() {
        return Ok(());
    }
    if tokens
        .expires_at
        .is_some_and(|expires_at| now_unix() >= expires_at)
    {
        return Ok(());
    }

    match validate_access_token(&tokens.access_token).await {
        Ok(validate) => {
            tokens.client_id = Some(validate.client_id);
            save_tokens(tokens)?;
            Ok(())
        }
        Err(AuthError::Http(error))
            if error.status() == Some(reqwest::StatusCode::UNAUTHORIZED) =>
        {
            // The access token may have expired before its recorded expiry.
            // Leave the legacy row intact and let refresh attempt the app
            // client as a non-destructive fallback.
            Ok(())
        }
        Err(error) => Err(error),
    }
}

async fn session_from_tokens(mut tokens: StoredTokens) -> Result<AuthSession, AuthError> {
    recover_legacy_client_id_if_possible(&mut tokens).await?;
    tokens = refresh_if_needed(tokens).await?;
    let validate = validate_access_token(&tokens.access_token).await?;

    let users: HelixUsersResponse = http()
        .get("https://api.twitch.tv/helix/users")
        .header("Client-Id", &validate.client_id)
        .bearer_auth(&tokens.access_token)
        .send()
        .await
        .map_err(map_http)?
        .error_for_status()
        .map_err(map_http)?
        .json()
        .await?;

    if tokens.client_id.as_deref() != Some(validate.client_id.as_str()) {
        tokens.client_id = Some(validate.client_id.clone());
        save_tokens(&tokens)?;
    }

    let user = users.data.into_iter().next();
    Ok(AuthSession {
        logged_in: true,
        user_id: user
            .as_ref()
            .map(|u| u.id.clone())
            .or(Some(validate.user_id)),
        login: user
            .as_ref()
            .map(|u| u.login.clone())
            .or(Some(validate.login)),
        display_name: user.as_ref().map(|u| u.display_name.clone()),
        profile_image_url: user.as_ref().map(|u| u.profile_image_url.clone()),
        scopes: validate.scopes,
    })
}

pub async fn get_session() -> Result<AuthSession, AuthError> {
    let _guard = token_state_gate().lock().await;
    match load_tokens()? {
        Some(tokens) => session_from_tokens(tokens).await.map_err(map_session_error),
        None => Ok(AuthSession {
            logged_in: false,
            user_id: None,
            login: None,
            display_name: None,
            profile_image_url: None,
            scopes: vec![],
        }),
    }
}

pub async fn logout() -> Result<(), AuthError> {
    let _guard = token_state_gate().lock().await;
    if let Some(tokens) = load_tokens()? {
        let app_client_id = client_id()?;
        let client_id = token_bound_client_id(tokens.client_id.as_deref(), &app_client_id);
        let http = http();
        let _ = http
            .post(REVOKE_URL)
            .form(&[
                ("client_id", client_id.as_str()),
                ("token", tokens.access_token.as_str()),
            ])
            .send()
            .await;
    }
    clear_tokens()?;
    Ok(())
}

/// Token + the Twitch application that issued it. Helix rejects a Bearer
/// token when `Client-Id` does not match, even if `/oauth2/validate` succeeded.
pub struct ApiCredentials {
    pub access_token: String,
    pub client_id: String,
}

pub async fn credentials_for_api() -> Result<ApiCredentials, AuthError> {
    let _guard = token_state_gate().lock().await;
    let mut tokens = load_tokens()?.ok_or_else(|| AuthError::Message("not logged in".into()))?;
    recover_legacy_client_id_if_possible(&mut tokens).await?;
    tokens = refresh_if_needed(tokens).await?;
    let client_id = match stored_token_client_id(tokens.client_id.as_deref()) {
        Some(id) => id.to_string(),
        None => {
            let validate = validate_access_token(&tokens.access_token).await?;
            tokens.client_id = Some(validate.client_id.clone());
            save_tokens(&tokens)?;
            validate.client_id
        }
    };
    Ok(ApiCredentials {
        access_token: tokens.access_token,
        client_id,
    })
}

pub fn public_client_id() -> Result<String, AuthError> {
    client_id()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decodes_twitch_device_code_snake_case() {
        let body = r#"{
            "device_code":"abc",
            "expires_in":1800,
            "interval":5,
            "user_code":"ABCD1234",
            "verification_uri":"https://www.twitch.tv/activate?device-code=ABCD1234"
        }"#;
        let parsed: TwitchDeviceCodeBody = serde_json::from_str(body).unwrap();
        let dto = DeviceCodeResponse::from(parsed);
        assert_eq!(dto.user_code, "ABCD1234");
        assert_eq!(dto.interval, 5);
        let json = serde_json::to_string(&dto).unwrap();
        assert!(json.contains("userCode"));
        assert!(json.contains("verificationUri"));
    }

    #[test]
    fn release_client_id_prefers_compiled_identity() {
        let id = select_client_id(Some("release-client"), Some("runtime-client"), false).unwrap();
        assert_eq!(id, "release-client");
    }

    #[test]
    fn release_client_id_does_not_fall_back_at_runtime() {
        let err = select_client_id(None, Some("runtime-client"), false).unwrap_err();
        assert_eq!(
            err.to_string(),
            "release build is missing its Twitch client ID"
        );
    }

    #[test]
    fn debug_client_id_supports_runtime_override_and_dev_fallback() {
        assert_eq!(
            select_client_id(None, Some("runtime-client"), true).unwrap(),
            "runtime-client"
        );
        assert_eq!(select_client_id(None, None, true).unwrap(), DEV_CLIENT_ID);
    }

    #[test]
    fn helix_and_refresh_use_the_token_client_when_it_differs_from_the_app() {
        assert_eq!(
            token_bound_client_id(Some("token-app"), "compiled-app"),
            "token-app"
        );
    }

    #[test]
    fn helix_and_refresh_fall_back_to_the_app_client_for_legacy_tokens() {
        assert_eq!(token_bound_client_id(None, "compiled-app"), "compiled-app");
        assert_eq!(
            token_bound_client_id(Some("  "), "compiled-app"),
            "compiled-app"
        );
    }

    #[test]
    fn refresh_rejection_only_clears_when_issuing_client_is_known() {
        assert!(should_clear_tokens_after_refresh_rejection(true));
        assert!(!should_clear_tokens_after_refresh_rejection(false));
        assert_eq!(
            stored_token_client_id(Some(" token-app ")),
            Some("token-app")
        );
        assert_eq!(stored_token_client_id(Some("   ")), None);
        assert_eq!(stored_token_client_id(None), None);
    }

    #[tokio::test]
    async fn token_state_gate_serializes_callers() {
        let first = token_state_gate().lock().await;
        assert!(token_state_gate().try_lock().is_err());
        drop(first);
        let second = token_state_gate()
            .try_lock()
            .expect("token state gate should be available after owner drops");
        drop(second);
    }

    #[test]
    fn maps_later_refresh_to_network_unavailable() {
        let err = map_session_error(AuthError::Message(
            "token refresh failed (503); will retry later".into(),
        ));
        assert_eq!(err.to_string(), crate::http::NETWORK_UNAVAILABLE);
    }

    #[test]
    fn keeps_expired_session_errors() {
        let err = map_session_error(AuthError::Message(
            "session expired; please log in again".into(),
        ));
        assert_eq!(err.to_string(), "session expired; please log in again");
    }
}
