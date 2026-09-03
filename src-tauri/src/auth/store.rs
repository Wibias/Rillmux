use keyring::Entry;
use serde::{Deserialize, Serialize};
use std::sync::OnceLock;
use std::time::{SystemTime, UNIX_EPOCH};
use thiserror::Error;

use crate::branding::{KEYRING_SERVICE, KEYRING_SERVICE_LEGACY};

const USER: &str = "twitch-oauth";

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StoredTokens {
    pub access_token: String,
    pub refresh_token: Option<String>,
    pub expires_at: Option<u64>,
    pub scopes: Vec<String>,
    #[serde(default)]
    pub client_id: Option<String>,
}

#[derive(Debug, Error)]
pub enum TokenStoreError {
    #[error("keyring error: {0}")]
    Keyring(String),
    #[error("serialize error: {0}")]
    Serde(#[from] serde_json::Error),
}

/// keyring 4 / keyring-core require an explicit default store. The v1 helper
/// can race under concurrent first access, so we install Windows Credential
/// Manager once ourselves before any Entry::new calls.
fn ensure_keyring() -> Result<(), TokenStoreError> {
    static INIT: OnceLock<Result<(), String>> = OnceLock::new();
    let result = INIT.get_or_init(|| {
        #[cfg(windows)]
        {
            let store = windows_native_keyring_store::Store::new().map_err(|e| e.to_string())?;
            keyring_core::set_default_store(store);
        }
        Ok(())
    });
    result.clone().map_err(TokenStoreError::Keyring)
}

fn entry_for(service: &str) -> Result<Entry, TokenStoreError> {
    ensure_keyring()?;
    Entry::new(service, USER).map_err(|e| TokenStoreError::Keyring(e.to_string()))
}

fn read_tokens(service: &str) -> Result<Option<StoredTokens>, TokenStoreError> {
    match entry_for(service)?.get_password() {
        Ok(secret) => Ok(Some(serde_json::from_str(&secret)?)),
        Err(keyring::Error::NoEntry) => Ok(None),
        Err(e) => Err(TokenStoreError::Keyring(e.to_string())),
    }
}

fn delete_service(service: &str) -> Result<(), TokenStoreError> {
    match entry_for(service)?.delete_credential() {
        Ok(()) => Ok(()),
        Err(keyring::Error::NoEntry) => Ok(()),
        Err(e) => Err(TokenStoreError::Keyring(e.to_string())),
    }
}

pub fn load_tokens() -> Result<Option<StoredTokens>, TokenStoreError> {
    if let Some(tokens) = read_tokens(KEYRING_SERVICE)? {
        return Ok(Some(tokens));
    }
    let Some(tokens) = read_tokens(KEYRING_SERVICE_LEGACY)? else {
        return Ok(None);
    };
    save_tokens(&tokens)?;
    let _ = delete_service(KEYRING_SERVICE_LEGACY);
    Ok(Some(tokens))
}

pub fn save_tokens(tokens: &StoredTokens) -> Result<(), TokenStoreError> {
    let payload = serde_json::to_string(tokens)?;
    entry_for(KEYRING_SERVICE)?
        .set_password(&payload)
        .map_err(|e| TokenStoreError::Keyring(e.to_string()))
}

pub fn clear_tokens() -> Result<(), TokenStoreError> {
    delete_service(KEYRING_SERVICE)?;
    delete_service(KEYRING_SERVICE_LEGACY)
}

pub fn now_unix() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn legacy_tokens_without_client_id_still_deserialize() {
        let tokens: StoredTokens = serde_json::from_str(
            r#"{"accessToken":"a","refreshToken":"r","expiresAt":1,"scopes":[]}"#,
        )
        .unwrap();
        assert_eq!(tokens.access_token, "a");
        assert_eq!(tokens.client_id, None);
    }

    #[test]
    fn persisted_tokens_round_trip_client_id() {
        let tokens = StoredTokens {
            access_token: "a".into(),
            refresh_token: Some("r".into()),
            expires_at: Some(1),
            scopes: vec!["user:read:follows".into()],
            client_id: Some("token-app".into()),
        };
        let json = serde_json::to_string(&tokens).unwrap();
        assert!(json.contains("clientId"));
        let parsed: StoredTokens = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.client_id.as_deref(), Some("token-app"));
    }
}
