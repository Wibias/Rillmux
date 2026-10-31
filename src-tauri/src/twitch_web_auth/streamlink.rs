// Ephemeral Streamlink auth config and legacy managed-block cleanup.

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
