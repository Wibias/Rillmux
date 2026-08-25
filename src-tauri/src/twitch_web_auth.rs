use keyring::Entry;
use serde::{Deserialize, Serialize};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use thiserror::Error;
use uuid::Uuid;

use crate::branding::{KEYRING_SERVICE, KEYRING_SERVICE_LEGACY};
use crate::http::{reset_on_transport, shared_client};

const USER: &str = "twitch-website-oauth";
const VALIDATE_URL: &str = "https://id.twitch.tv/oauth2/validate";

#[allow(dead_code)]
pub(crate) const WEB_CLIENT_ID: &str = "kimne78kx3ncx6brgo4mv6wki5h1ko";
pub(crate) const MANAGED_BEGIN: &str = "# BEGIN rillmux managed Twitch auth";
pub(crate) const MANAGED_END: &str = "# END rillmux managed Twitch auth";
const MANAGED_BEGIN_LEGACY: &str = "# BEGIN streamlink-twitch-gui managed Twitch auth";
const MANAGED_END_LEGACY: &str = "# END streamlink-twitch-gui managed Twitch auth";

#[derive(Debug, Error)]
pub enum TwitchWebAuthError {
    #[error("{0}")]
    Message(String),
    #[error(transparent)]
    Http(#[from] reqwest::Error),
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Serde(#[from] serde_json::Error),
    #[error("keyring error: {0}")]
    Keyring(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct StoredWebsiteAuth {
    token: String,
    user_id: String,
    login: String,
}

#[allow(dead_code)]
#[derive(Debug, Clone)]
pub(crate) struct TwitchWebAuthSession {
    pub token: String,
    pub user_id: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TwitchWebAuthStatus {
    pub configured: bool,
    pub login: Option<String>,
    pub user_id: Option<String>,
    pub playback_ready: bool,
}

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

pub(crate) struct StreamlinkAuthConfig {
    path: PathBuf,
}

impl StreamlinkAuthConfig {
    pub(crate) fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for StreamlinkAuthConfig {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
    }
}

fn streamlink_auth_config_for(token: &str) -> Result<StreamlinkAuthConfig, TwitchWebAuthError> {
    let path = std::env::temp_dir().join(format!(
        "rillmux-streamlink-auth-{}-{}.conf",
        std::process::id(),
        Uuid::new_v4().simple()
    ));
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options.open(&path)?;
    writeln!(file, "twitch-api-header=Authorization=OAuth {token}")?;
    file.flush()?;
    Ok(StreamlinkAuthConfig { path })
}

pub(crate) fn streamlink_auth_config() -> Result<Option<StreamlinkAuthConfig>, TwitchWebAuthError> {
    // Streamlink loads plugin-specific config.twitch after a custom --config,
    // so remove only auth blocks managed by Rillmux before every launch.
    remove_streamlink_auth()?;
    let Some(token) = load_token()? else {
        return Ok(None);
    };
    streamlink_auth_config_for(&token).map(Some)
}

#[allow(dead_code)]
pub(crate) fn load_session() -> Result<Option<TwitchWebAuthSession>, TwitchWebAuthError> {
    Ok(load_auth()?.map(|auth| TwitchWebAuthSession {
        token: auth.token,
        user_id: auth.user_id,
    }))
}

#[allow(dead_code)]
pub(crate) fn device_id() -> &'static str {
    static VALUE: OnceLock<String> = OnceLock::new();
    VALUE
        .get_or_init(|| Uuid::new_v4().simple().to_string())
        .as_str()
}

#[allow(dead_code)]
pub(crate) fn client_session_id() -> &'static str {
    static VALUE: OnceLock<String> = OnceLock::new();
    VALUE
        .get_or_init(|| Uuid::new_v4().simple().to_string())
        .as_str()
}

pub(crate) fn normalize_token(raw: &str) -> Result<String, TwitchWebAuthError> {
    let trimmed = raw.trim();
    let lower = trimmed.to_ascii_lowercase();
    let token = if lower.starts_with("oauth:") || lower.starts_with("oauth ") {
        &trimmed[6..]
    } else {
        trimmed
    };

    if !(20..=200).contains(&token.len())
        || !token
            .chars()
            .all(|character| character.is_ascii_alphanumeric())
    {
        return Err(TwitchWebAuthError::Message(
            "invalid Twitch website token format".into(),
        ));
    }
    Ok(token.to_string())
}

fn streamlink_config_path() -> Result<PathBuf, TwitchWebAuthError> {
    streamlink_config_path_for(std::env::consts::OS, |key| std::env::var(key).ok())
}

pub(crate) fn streamlink_config_path_for(
    os: &str,
    env: impl Fn(&str) -> Option<String>,
) -> Result<PathBuf, TwitchWebAuthError> {
    let base = match os {
        "windows" => env("APPDATA")
            .map(PathBuf::from)
            .ok_or_else(|| TwitchWebAuthError::Message("APPDATA is not set".into()))?,
        "macos" => env("HOME")
            .map(PathBuf::from)
            .map(|home| home.join("Library").join("Application Support"))
            .ok_or_else(|| TwitchWebAuthError::Message("HOME is not set".into()))?,
        _ => env("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .or_else(|| {
                env("HOME")
                    .map(PathBuf::from)
                    .map(|home| home.join(".config"))
            })
            .ok_or_else(|| {
                TwitchWebAuthError::Message("neither XDG_CONFIG_HOME nor HOME is set".into())
            })?,
    };
    Ok(base.join("streamlink").join("config.twitch"))
}

pub(crate) fn remove_managed_block(existing: &str) -> String {
    let mut result = existing.to_string();
    for (begin, end) in [
        (MANAGED_BEGIN, MANAGED_END),
        (MANAGED_BEGIN_LEGACY, MANAGED_END_LEGACY),
    ] {
        result = remove_marker_block(&result, begin, end);
    }
    result
}

fn remove_marker_block(existing: &str, begin_marker: &str, end_marker: &str) -> String {
    let mut result = existing.to_string();
    while let Some(begin) = result.find(begin_marker) {
        let after_begin = begin + begin_marker.len();
        let Some(relative_end) = result[after_begin..].find(end_marker) else {
            let prefix = result[..begin].trim_end_matches(['\r', '\n']);
            return if prefix.is_empty() {
                String::new()
            } else {
                format!("{prefix}\n")
            };
        };
        let end = after_begin + relative_end + end_marker.len();
        let mut after = end;
        if result[after..].starts_with("\r\n") {
            after += 2;
        } else if result[after..].starts_with('\n') {
            after += 1;
        }

        let prefix = result[..begin].trim_end_matches(['\r', '\n']);
        let suffix = result[after..].trim_start_matches(['\r', '\n']);
        result = match (prefix.is_empty(), suffix.is_empty()) {
            (true, true) => String::new(),
            (false, true) => format!("{prefix}\n"),
            (true, false) => suffix.to_string(),
            (false, false) => format!("{prefix}\n\n{suffix}"),
        };
    }
    result
}

fn read_config(path: &Path) -> Result<String, TwitchWebAuthError> {
    match std::fs::read_to_string(path) {
        Ok(content) => Ok(content),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(String::new()),
        Err(error) => Err(error.into()),
    }
}

fn write_secure(path: &Path, content: &str) -> Result<(), TwitchWebAuthError> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let temporary = path.with_extension(format!("twitch.tmp-{}", std::process::id()));
    std::fs::write(&temporary, content)?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&temporary, std::fs::Permissions::from_mode(0o600))?;
    }

    if path.exists() {
        std::fs::remove_file(path)?;
    }
    std::fs::rename(temporary, path)?;
    Ok(())
}

fn remove_streamlink_auth_at(path: &Path) -> Result<(), TwitchWebAuthError> {
    let existing = read_config(path)?;
    if existing.is_empty() {
        return Ok(());
    }
    let updated = remove_managed_block(&existing);
    if updated.trim().is_empty() {
        match std::fs::remove_file(path) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error.into()),
        }
    } else if updated != existing {
        write_secure(path, &updated)?;
    }
    Ok(())
}

fn remove_streamlink_auth() -> Result<PathBuf, TwitchWebAuthError> {
    let path = streamlink_config_path()?;
    remove_streamlink_auth_at(&path)?;
    Ok(path)
}

fn status_from(auth: Option<StoredWebsiteAuth>) -> TwitchWebAuthStatus {
    TwitchWebAuthStatus {
        configured: auth.is_some(),
        login: auth.as_ref().map(|value| value.login.clone()),
        user_id: auth.as_ref().map(|value| value.user_id.clone()),
        playback_ready: auth.is_some(),
    }
}

pub fn get_status() -> Result<TwitchWebAuthStatus, TwitchWebAuthError> {
    // Migration cleanup for releases that previously mirrored the secret to
    // Streamlink's plugin-specific config file.
    remove_streamlink_auth()?;
    Ok(status_from(load_auth()?))
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
    Ok(status_from(Some(stored)))
}

pub fn clear() -> Result<TwitchWebAuthStatus, TwitchWebAuthError> {
    remove_streamlink_auth()?;
    clear_auth()?;
    Ok(status_from(None))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use std::path::PathBuf;

    const TOKEN: &str = "abcdefghijklmnopqrstuvwxyz0123";

    #[test]
    fn normalizes_supported_token_forms() {
        assert_eq!(normalize_token(TOKEN).unwrap(), TOKEN);

        let oauth_prefix = format!("oauth:{TOKEN}");
        assert_eq!(normalize_token(&oauth_prefix).unwrap(), TOKEN);

        let header_prefix = format!("OAuth {TOKEN}");
        assert_eq!(normalize_token(&header_prefix).unwrap(), TOKEN);
    }

    #[test]
    fn rejects_invalid_tokens() {
        assert!(normalize_token("abc defghijklmnopqrstuvwxyz0123").is_err());
        assert!(normalize_token("abc\ndefghijklmnopqrstuvwxyz0123").is_err());
        assert!(normalize_token("short").is_err());
    }

    #[test]
    fn ephemeral_streamlink_auth_config_is_removed_on_drop() {
        let config = streamlink_auth_config_for(TOKEN).unwrap();
        let path = config.path().to_path_buf();
        assert_eq!(
            std::fs::read_to_string(&path).unwrap(),
            format!("twitch-api-header=Authorization=OAuth {TOKEN}\n")
        );
        drop(config);
        assert!(!path.exists());
    }

    #[test]
    fn removes_managed_block_without_touching_user_config() {
        let existing = concat!(
            "player=mpv\n\n",
            "# BEGIN streamlink-twitch-gui managed Twitch auth\n",
            "twitch-api-header=Authorization=OAuth token\n",
            "# END streamlink-twitch-gui managed Twitch auth\n\n",
            "retry-streams=1\n",
        );
        let cleaned = remove_managed_block(existing);
        assert_eq!(cleaned, "player=mpv\n\nretry-streams=1\n");
    }

    #[test]
    fn removes_legacy_managed_auth_from_existing_plugin_config() {
        let dir = std::env::temp_dir().join(format!(
            "rillmux-streamlink-config-{}-{}",
            std::process::id(),
            Uuid::new_v4().simple()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("config.twitch");
        let existing = concat!(
            "player=mpv\n\n",
            "# BEGIN streamlink-twitch-gui managed Twitch auth\n",
            "twitch-api-header=Authorization=OAuth stale-token\n",
            "# END streamlink-twitch-gui managed Twitch auth\n\n",
            "retry-streams=1\n",
        );
        std::fs::write(&path, existing).unwrap();

        remove_streamlink_auth_at(&path).unwrap();
        let cleaned = std::fs::read_to_string(&path).unwrap();
        assert_eq!(cleaned, "player=mpv\n\nretry-streams=1\n");
        assert!(!cleaned.contains("twitch-api-header"));

        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn resolves_platform_config_paths() {
        let windows = HashMap::from([("APPDATA", r"C:\Users\Janik\AppData\Roaming")]);
        let windows_path = streamlink_config_path_for("windows", |key| {
            windows.get(key).map(|value| value.to_string())
        })
        .unwrap();
        let expected_windows = PathBuf::from(r"C:\Users\Janik\AppData\Roaming")
            .join("streamlink")
            .join("config.twitch");
        assert_eq!(windows_path, expected_windows);

        let linux = HashMap::from([("HOME", "/home/janik")]);
        let linux_path = streamlink_config_path_for("linux", |key| {
            linux.get(key).map(|value| value.to_string())
        })
        .unwrap();
        let expected_linux = PathBuf::from("/home/janik/.config/streamlink/config.twitch");
        assert_eq!(linux_path, expected_linux);
    }

    #[test]
    fn web_identity_helpers_are_stable() {
        assert_eq!(WEB_CLIENT_ID, "kimne78kx3ncx6brgo4mv6wki5h1ko");
        assert!(!device_id().is_empty());
        assert_eq!(device_id(), device_id());
        assert!(!client_session_id().is_empty());
        assert_eq!(client_session_id(), client_session_id());
    }
}
