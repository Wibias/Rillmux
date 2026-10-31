// Website-auth types, storage model, and error surface.

use keyring::Entry;
use serde::{Deserialize, Serialize};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard, OnceLock};
use std::time::{Duration, Instant};
use thiserror::Error;
use uuid::Uuid;

use crate::branding::{KEYRING_SERVICE, KEYRING_SERVICE_LEGACY};
use crate::http::{reset_on_transport, shared_client};

const USER: &str = "twitch-website-oauth";
const VALIDATE_URL: &str = "https://id.twitch.tv/oauth2/validate";
const STATUS_FAILURE_BACKOFF_START: Duration = Duration::from_secs(2);
const STATUS_FAILURE_BACKOFF_MAX: Duration = Duration::from_secs(30);

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

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TwitchWebAuthStatus {
    pub configured: bool,
    pub login: Option<String>,
    pub user_id: Option<String>,
    pub playback_ready: bool,
}

pub const STATUS_CHANGED_EVENT: &str = "twitch-web-auth-changed";

struct WebsiteAuthCache {
    status: Option<TwitchWebAuthStatus>,
    retry_at: Option<Instant>,
    backoff: Duration,
    last_error: Option<String>,
}
