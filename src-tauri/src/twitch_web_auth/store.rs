// Keyring access and token persistence.

#[derive(Debug, Deserialize)]
struct ValidateResponse {
    login: String,
    user_id: String,
}

fn ensure_keyring() -> Result<(), TwitchWebAuthError> {
    static INIT: OnceLock<Result<(), String>> = OnceLock::new();
    let result = INIT.get_or_init(|| {
        #[cfg(windows)]
        {
            let store =
                windows_native_keyring_store::Store::new().map_err(|error| error.to_string())?;
            keyring_core::set_default_store(store);
        }
        Ok(())
    });
    result.clone().map_err(TwitchWebAuthError::Keyring)
}

fn entry_for(service: &str) -> Result<Entry, TwitchWebAuthError> {
    ensure_keyring()?;
    Entry::new(service, USER).map_err(|error| TwitchWebAuthError::Keyring(error.to_string()))
}

fn read_auth(service: &str) -> Result<Option<StoredWebsiteAuth>, TwitchWebAuthError> {
    match entry_for(service)?.get_password() {
        Ok(secret) => Ok(Some(serde_json::from_str(&secret)?)),
        Err(keyring::Error::NoEntry) => Ok(None),
        Err(error) => Err(TwitchWebAuthError::Keyring(error.to_string())),
    }
}

fn delete_service(service: &str) -> Result<(), TwitchWebAuthError> {
    match entry_for(service)?.delete_credential() {
        Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
        Err(error) => Err(TwitchWebAuthError::Keyring(error.to_string())),
    }
}

fn load_auth() -> Result<Option<StoredWebsiteAuth>, TwitchWebAuthError> {
    if let Some(auth) = read_auth(KEYRING_SERVICE)? {
        return Ok(Some(auth));
    }
    let Some(auth) = read_auth(KEYRING_SERVICE_LEGACY)? else {
        return Ok(None);
    };
    save_auth(&auth)?;
    let _ = delete_service(KEYRING_SERVICE_LEGACY);
    Ok(Some(auth))
}

fn save_auth(auth: &StoredWebsiteAuth) -> Result<(), TwitchWebAuthError> {
    let payload = serde_json::to_string(auth)?;
    entry_for(KEYRING_SERVICE)?
        .set_password(&payload)
        .map_err(|error| TwitchWebAuthError::Keyring(error.to_string()))
}

fn clear_auth() -> Result<(), TwitchWebAuthError> {
    delete_service(KEYRING_SERVICE)?;
    delete_service(KEYRING_SERVICE_LEGACY)
}

#[allow(dead_code)]
pub(crate) fn load_token() -> Result<Option<String>, TwitchWebAuthError> {
    Ok(load_auth()?.map(|auth| auth.token))
}
