// Unit tests for website authentication.

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

    #[test]
    fn status_cache_skips_repeated_migration_and_keyring_loads() {
        let runtime = WebsiteAuthRuntime::new();
        let loads = std::sync::atomic::AtomicUsize::new(0);
        let load = || {
            loads.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            Ok(status_from(None))
        };
        assert_eq!(runtime.get_status(load).unwrap(), status_from(None));
        assert_eq!(runtime.get_status(load).unwrap(), status_from(None));
        assert_eq!(loads.load(std::sync::atomic::Ordering::SeqCst), 1);
    }

    #[test]
    fn save_and_clear_update_cached_status_without_reloading() {
        let runtime = WebsiteAuthRuntime::new();
        let loads = std::sync::atomic::AtomicUsize::new(0);
        let configured = TwitchWebAuthStatus {
            configured: true,
            login: Some("forsen".into()),
            user_id: Some("123".into()),
            playback_ready: true,
        };
        runtime.remember(configured.clone());
        let loaded = runtime
            .get_status(|| {
                loads.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                Ok(status_from(None))
            })
            .unwrap();
        assert_eq!(loaded, configured);
        assert_eq!(loads.load(std::sync::atomic::Ordering::SeqCst), 0);

        runtime.remember(status_from(None));
        let cleared = runtime
            .get_status(|| {
                loads.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                Ok(configured.clone())
            })
            .unwrap();
        assert_eq!(cleared, status_from(None));
        assert_eq!(loads.load(std::sync::atomic::Ordering::SeqCst), 0);
    }

    #[test]
    fn status_payload_does_not_include_the_token() {
        let stored = StoredWebsiteAuth {
            token: "super-secret-token".into(),
            user_id: "123".into(),
            login: "forsen".into(),
        };
        let json = serde_json::to_string(&status_from(Some(stored))).unwrap();
        assert!(!json.contains("super-secret-token"));
        assert!(!json.to_ascii_lowercase().contains("token"));
    }

    #[test]
    fn failed_status_load_uses_backoff_until_retry_or_save() {
        use std::sync::atomic::{AtomicUsize, Ordering};
        let runtime = WebsiteAuthRuntime::new();
        let loads = AtomicUsize::new(0);
        let t0 = Instant::now();
        let fail = || {
            loads.fetch_add(1, Ordering::SeqCst);
            Err(TwitchWebAuthError::Message("keyring busy".into()))
        };
        assert!(runtime.get_status_at(t0, fail).is_err());
        assert_eq!(loads.load(Ordering::SeqCst), 1);

        assert!(runtime
            .get_status_at(t0 + Duration::from_millis(500), fail)
            .is_err());
        assert_eq!(loads.load(Ordering::SeqCst), 1);

        let recovered = runtime
            .get_status_at(t0 + STATUS_FAILURE_BACKOFF_START, || {
                loads.fetch_add(1, Ordering::SeqCst);
                Ok(status_from(None))
            })
            .unwrap();
        assert_eq!(recovered, status_from(None));
        assert_eq!(loads.load(Ordering::SeqCst), 2);
    }

    #[test]
    fn save_and_clear_supersede_status_failure_backoff() {
        use std::sync::atomic::{AtomicUsize, Ordering};
        let t0 = Instant::now();

        let cleared = WebsiteAuthRuntime::new();
        let clear_loads = AtomicUsize::new(0);
        assert!(cleared
            .get_status_at(t0, || {
                clear_loads.fetch_add(1, Ordering::SeqCst);
                Err(TwitchWebAuthError::Message("keyring busy".into()))
            })
            .is_err());
        cleared.remember(status_from(None));
        let loaded = cleared
            .get_status_at(t0 + Duration::from_millis(100), || {
                clear_loads.fetch_add(1, Ordering::SeqCst);
                Ok(status_from(None))
            })
            .unwrap();
        assert_eq!(loaded, status_from(None));
        assert_eq!(clear_loads.load(Ordering::SeqCst), 1);

        let saved = WebsiteAuthRuntime::new();
        let save_loads = AtomicUsize::new(0);
        assert!(saved
            .get_status_at(t0, || {
                save_loads.fetch_add(1, Ordering::SeqCst);
                Err(TwitchWebAuthError::Message("keyring busy".into()))
            })
            .is_err());
        let configured = TwitchWebAuthStatus {
            configured: true,
            login: Some("forsen".into()),
            user_id: Some("123".into()),
            playback_ready: true,
        };
        saved.remember(configured.clone());
        let after_save = saved
            .get_status_at(t0 + Duration::from_millis(100), || {
                save_loads.fetch_add(1, Ordering::SeqCst);
                Ok(status_from(None))
            })
            .unwrap();
        assert_eq!(after_save, configured);
        assert_eq!(save_loads.load(Ordering::SeqCst), 1);
    }
}
