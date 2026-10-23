use serde_json::Value;
use thiserror::Error;

use crate::auth::{self, AuthError};
use crate::http::{reset_on_transport, shared_client};

#[derive(Debug, Error)]
pub enum HelixError {
    #[error("{0}")]
    Message(String),
    #[error(transparent)]
    Auth(#[from] AuthError),
    #[error(transparent)]
    Http(#[from] reqwest::Error),
}

fn helix_url(path: &str, query: &[(String, String)]) -> Result<url::Url, HelixError> {
    let mut url = url::Url::parse(&format!(
        "https://api.twitch.tv/helix/{}",
        path.trim_start_matches('/')
    ))
    .map_err(|e| HelixError::Message(format!("invalid helix path '{path}': {e}")))?;
    for (key, value) in query {
        url.query_pairs_mut().append_pair(key, value);
    }
    Ok(url)
}

/// Proxy a Helix GET so the OAuth access token never leaves the Rust side.
/// `query` is a list of key/value pairs; repeated keys are preserved
/// (Helix uses e.g. `?login=a&login=b`).
pub async fn fetch(path: &str, query: &[(String, String)]) -> Result<Value, HelixError> {
    let creds = auth::credentials_for_api().await?;
    let url = helix_url(path, query)?;
    let res = shared_client()
        .get(url)
        .header("Client-Id", creds.client_id)
        .bearer_auth(creds.access_token)
        .send()
        .await
        .map_err(reset_on_transport)?;
    let status = res.status();
    if !status.is_success() {
        let body = res.text().await.unwrap_or_default();
        return Err(HelixError::Message(format!("helix {status}: {body}")));
    }
    Ok(res.json().await?)
}

/// Proxy a token-bearing Helix mutation (POST/DELETE) without exposing the
/// access token to the webview. Follow endpoints accept an empty JSON body.
pub async fn mutate(
    method: &str,
    path: &str,
    query: &[(String, String)],
) -> Result<(), HelixError> {
    let verb = match method {
        "POST" => reqwest::Method::POST,
        "DELETE" => reqwest::Method::DELETE,
        other => {
            return Err(HelixError::Message(format!(
                "unsupported helix method '{other}'"
            )))
        }
    };
    let creds = auth::credentials_for_api().await?;
    let url = helix_url(path, query)?;
    let mut request = shared_client()
        .request(verb, url)
        .header("Client-Id", creds.client_id)
        .bearer_auth(creds.access_token);
    if method == "POST" {
        request = request.json(&serde_json::json!({}));
    }
    let res = request.send().await.map_err(reset_on_transport)?;
    let status = res.status();
    if !status.is_success() {
        let body = res.text().await.unwrap_or_default();
        return Err(HelixError::Message(format!("helix {status}: {body}")));
    }
    Ok(())
}
