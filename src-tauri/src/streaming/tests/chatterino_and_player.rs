// Chatterino reuse, dock ownership, and mpv player window behavior.


    #[test]
    fn chatterino_reuses_a_live_instance_for_the_same_channels() {
        assert!(chatterino_should_reuse(true, "t:forsen", "t:forsen"));
        assert!(!chatterino_should_reuse(false, "t:forsen", "t:forsen"));
        assert!(!chatterino_should_reuse(true, "", "t:forsen"));
        assert!(!chatterino_should_reuse(true, "t:forsen", "t:forsen;t:xqc"));
        assert_eq!(
            chatterino_launch_plan(true, "t:forsen", "t:forsen"),
            ChatterinoLaunchPlan::Reuse
        );
        assert_eq!(
            chatterino_launch_plan(true, "t:forsen", "t:forsen;t:xqc"),
            ChatterinoLaunchPlan::RestartOwned
        );
        assert_eq!(
            chatterino_launch_plan(true, "t:forsen;t:xqc", "t:forsen"),
            ChatterinoLaunchPlan::RestartOwned
        );
        let runtime = include_str!("../runtime/teardown.rs");
        assert!(
            runtime.contains("relaunch_dock_chatterino(&remaining)"),
            "closing one multistream must restart owned Chatterino onto the remaining split"
        );
        // A Chatterino window the user already had open is not "owned".
        assert_eq!(
            chatterino_launch_plan(false, "", "t:forsen"),
            ChatterinoLaunchPlan::SpawnFresh
        );
        assert_eq!(
            chatterino_launch_plan(true, "", "t:forsen"),
            ChatterinoLaunchPlan::RestartOwned
        );
    }

    #[test]
    fn chatterino_dock_appdata_is_not_the_user_chatterino_folder() {
        let dock = chatterino_dock_appdata();
        if cfg!(debug_assertions) {
            let profile = dock
                .parent()
                .and_then(|path| path.file_name())
                .and_then(|name| name.to_str());
            assert_eq!(profile, Some("chatterino-dock-dev"));
        } else {
            assert_eq!(
                dock.file_name().and_then(|name| name.to_str()),
                Some("chatterino-dock")
            );
        }
        assert!(!dock.ends_with("Chatterino2"));
    }

    #[test]
    fn chatterino_picks_the_channels_split_over_a_blank_notebook() {
        let split = chatterino_window_pick_key(true, true, false, 80_000);
        let blank = chatterino_window_pick_key(false, true, false, 400_000);
        let cloaked = chatterino_window_pick_key(false, false, false, 400_000);
        assert!(split > blank);
        assert!(blank > cloaked);
        assert!(chatterino_title_matches_channels(
            "forsen - Chatterino",
            "t:forsen"
        ));
        assert!(!chatterino_title_matches_channels("Chatterino", "t:forsen"));
    }

    #[test]
    fn fixed_file_product_version_reads_owned_bytes() {
        let mut info = vec![0u8; VS_FIXED_FILE_INFO_SIZE];
        info[0..4].copy_from_slice(&VS_FIXED_FILE_INFO_SIGNATURE.to_le_bytes());
        info[PRODUCT_VERSION_MS_OFFSET..PRODUCT_VERSION_MS_OFFSET + 4]
            .copy_from_slice(&((7u32 << 16) | 5).to_le_bytes());
        info[PRODUCT_VERSION_LS_OFFSET..PRODUCT_VERSION_LS_OFFSET + 4]
            .copy_from_slice(&((5u32 << 16) | 2).to_le_bytes());
        assert_eq!(
            fixed_file_product_version(&info, 0, info.len()).as_deref(),
            Some("7.5.5.2")
        );
    }

    #[test]
    fn fixed_file_product_version_rejects_foreign_or_short_ranges() {
        let info = vec![0u8; VS_FIXED_FILE_INFO_SIZE];
        assert_eq!(
            fixed_file_product_version(&info, info.len() + 1, info.len()),
            None
        );
        assert_eq!(
            fixed_file_product_version(&info, 0, VS_FIXED_FILE_INFO_SIZE - 1),
            None
        );
    }

    #[test]
    fn opening_stream_is_starting_not_ready() {
        // Regression: "Opening stream" was treated as ready, which started
        // the layout/handoff/missing-window timers before the player existed.
        let (phase, ready) = classify_line("[cli][info] Opening stream: source (hls)");
        assert_eq!(phase, "starting");
        assert!(!ready);
    }

    #[test]
    fn starting_player_marks_ready() {
        let (phase, ready) =
            classify_line("[cli][info] Starting player: C:\\Program Files\\mpv\\mpv.exe");
        assert_eq!(phase, "ready");
        assert!(ready);
    }

    #[test]
    fn low_latency_line_is_info_not_ready() {
        let (phase, ready) = classify_line("[cli][info] Low latency streaming (HLS live edge: 2)");
        assert_eq!(phase, "info");
        assert!(!ready);
    }

    #[test]
    fn hls_reload_error_is_not_fatal() {
        let (phase, ready) =
            classify_line("[stream.hls][error] Failed to reload playlist: Unable to open URL");
        assert_eq!(phase, "info");
        assert!(!ready);
    }

    #[test]
    fn cli_error_is_fatal() {
        let (phase, ready) = classify_line("[cli][error] Failed to start player: mpv");
        assert_eq!(phase, "error");
        assert!(!ready);
    }

    #[test]
    fn error_prefix_is_fatal() {
        let (phase, ready) =
            classify_line("error: No playable streams found on this URL: twitch.tv/foo");
        assert_eq!(phase, "error");
        assert!(!ready);
    }

    #[test]
    fn ready_session_drops_hls_noise() {
        assert!(!should_forward_status(true, "info", false));
        assert!(!should_forward_status(true, "ready", true));
        assert!(should_forward_status(true, "ended", false));
        assert!(should_forward_status(true, "error", false));
        assert!(should_forward_status(false, "info", false));
        assert!(should_forward_status(false, "ready", true));
    }

    #[test]
    fn channel_and_quality_validation() {
        // mpv_window_title is <channel>-<stream_title>; non [a-z0-9_-] becomes `_`.
        assert_eq!(
            mpv_window_title("Some_Channel-1", "Hello World!"),
            "some_channel-1-hello_world"
        );
        assert_eq!(mpv_window_title("äöü", ""), "stream-stream");
        assert_eq!(
            legacy_mpv_window_title("Some_Channel-1"),
            "stgui-some_channel-1"
        );
        assert!(player_window_title_matches(
            "some_channel-1-hello_world",
            "Some_Channel-1"
        ));
        assert!(player_window_title_matches(
            "rillmux-some_channel-1 - VLC media player",
            "Some_Channel-1"
        ));
        assert!(!player_window_title_matches(
            "other-hello",
            "Some_Channel-1"
        ));
    }

    #[test]
    fn dock_watchdog_idles_when_focus_and_layout_are_stable() {
        assert_eq!(dock_watchdog_interval_ms_with_idle(false, false, 500), 500);
        assert_eq!(
            dock_watchdog_interval_ms_with_idle(false, false, 1000),
            1000
        );
        assert_eq!(dock_watchdog_interval_ms(true, true), 100);
        assert_eq!(dock_watchdog_interval_ms(true, false), 400);
        assert!(dock_watchdog_needs_fast_tick(
            true, false, false, false, false, false
        ));
        assert!(dock_watchdog_needs_fast_tick(
            false, false, true, false, false, false
        ));
        assert!(!dock_watchdog_needs_fast_tick(
            false, false, false, false, false, false
        ));
    }

    fn streamlink_cleanup_root(label: &str) -> PathBuf {
        let root = std::env::temp_dir().join(format!(
            "rillmux-streamlink-cleanup-{label}-{}",
            Uuid::new_v4()
        ));
        fs::create_dir_all(&root).expect("create cleanup fixture");
        root
    }

    fn write_fixture_file(path: &Path) {
        fs::create_dir_all(path.parent().expect("fixture parent")).expect("create fixture parent");
        fs::write(path, b"fixture").expect("write fixture file");
    }

    #[test]
    fn removes_legacy_streamlink_payloads_only_after_bin_layout_is_ready() {
        let root = streamlink_cleanup_root("complete");
        let streamlink = root.join("streamlink");
        write_fixture_file(&streamlink.join("bin").join("streamlinkw.exe"));
        write_fixture_file(&streamlink.join("Python").join("python.exe"));
        write_fixture_file(
            &streamlink
                .join("pkgs")
                .join("streamlink")
                .join("__init__.py"),
        );
        write_fixture_file(&root.join("Python").join("python.exe"));
        write_fixture_file(&root.join("pkgs").join("streamlink").join("__init__.py"));
        write_fixture_file(&root.join("ffmpeg").join("ffmpeg.exe"));
        write_fixture_file(&streamlink.join("streamlinkw.exe"));
        write_fixture_file(
            &streamlink
                .join("streamlink-8.4.0-1-py314-x86_64")
                .join("ffmpeg")
                .join("ffmpeg.exe"),
        );

        let freed = cleanup_legacy_streamlink_resources(&root);

        assert!(freed > 0, "cleanup should report reclaimed bytes");
        assert!(streamlink.join("bin").join("streamlinkw.exe").is_file());
        assert!(streamlink.join("Python").join("python.exe").is_file());
        assert!(streamlink
            .join("pkgs")
            .join("streamlink")
            .join("__init__.py")
            .is_file());
        assert!(!root.join("Python").exists());
        assert!(!root.join("pkgs").exists());
        assert!(!root.join("ffmpeg").exists());
        assert!(!streamlink.join("streamlinkw.exe").exists());
        assert!(!streamlink.join("streamlink-8.4.0-1-py314-x86_64").exists());
        fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn keeps_legacy_streamlink_payloads_when_bin_layout_is_missing() {
        let root = streamlink_cleanup_root("incomplete");
        write_fixture_file(&root.join("Python").join("python.exe"));
        write_fixture_file(&root.join("ffmpeg").join("ffmpeg.exe"));

        assert_eq!(cleanup_legacy_streamlink_resources(&root), 0);
        assert!(root.join("Python").join("python.exe").is_file());
        assert!(root.join("ffmpeg").join("ffmpeg.exe").is_file());
        fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn session_watchdog_skips_title_scans_when_mpv_process_is_owned() {
        assert!(!session_title_scan_needed(true));
        assert!(session_title_scan_needed(false));
        assert_eq!(session_watchdog_timeout_ms(0), 2500);
        assert_eq!(session_watchdog_timeout_ms(3), 1500);
    }

    #[test]
    fn points_hud_channel_from_overlay_label() {
        assert_eq!(
            points_hud_channel_from_label("points-hud-forsen"),
            Some("forsen")
        );
        assert_eq!(points_hud_channel_from_label("raid-overlay"), None);
        assert_eq!(points_hud_channel_from_label("points-hud-"), None);
    }

    #[test]
    fn player_layout_title_maps_to_the_matching_session_channel() {
        let channels = vec!["forsen".into(), "xqc".into()];
        assert_eq!(
            player_channel_for_title("forsen-just_chatting", &channels),
            Some("forsen".into())
        );
        assert_eq!(
            player_channel_for_title("rillmux-xqc", &channels),
            Some("xqc".into())
        );
        assert_eq!(
            player_channel_for_title("stgui-forsen", &channels),
            Some("forsen".into())
        );
        assert_eq!(player_channel_for_title("Notepad", &channels), None);
    }

    #[test]
    fn player_layout_emit_is_forced_or_spaced_by_the_minimum_gap() {
        let start = Instant::now();
        assert!(player_layout_emit_due(
            None,
            start,
            Duration::from_millis(16),
            false
        ));
        assert!(!player_layout_emit_due(
            Some(start),
            start + Duration::from_millis(5),
            Duration::from_millis(16),
            false
        ));
        assert!(player_layout_emit_due(
            Some(start),
            start + Duration::from_millis(5),
            Duration::from_millis(16),
            true
        ));
        assert!(player_layout_emit_due(
            Some(start),
            start + Duration::from_millis(16),
            Duration::from_millis(16),
            false
        ));
        assert_eq!(PLAYER_LAYOUT_CHANGED_EVENT, "player-layout-changed");
    }

    #[test]
    fn player_layout_win_events_ignore_caret_and_child_objects() {
        const LOCATION: u32 = 0x800B;
        const MOVESIZEEND: u32 = 0x000B;
        const OBJID_WINDOW: i32 = 0;
        const OBJID_CARET: i32 = -8;
        const OBJID_CLIENT: i32 = -4;
        const CHILDID_SELF: i32 = 0;

        assert!(player_layout_event_relevant(
            LOCATION,
            OBJID_WINDOW,
            CHILDID_SELF
        ));
        assert!(!player_layout_event_relevant(
            LOCATION,
            OBJID_CARET,
            CHILDID_SELF
        ));
        assert!(!player_layout_event_relevant(
            LOCATION,
            OBJID_CLIENT,
            CHILDID_SELF
        ));
        assert!(!player_layout_event_relevant(LOCATION, OBJID_WINDOW, 1));
        assert!(player_layout_event_relevant(
            MOVESIZEEND,
            OBJID_WINDOW,
            CHILDID_SELF
        ));
        assert!(!player_layout_event_relevant(
            MOVESIZEEND,
            OBJID_CARET,
            CHILDID_SELF
        ));
        assert!(!player_layout_watch_should_pump(false, false));
        assert!(player_layout_watch_should_pump(true, false));
        assert!(player_layout_watch_should_pump(false, true));
        assert!(player_layout_watch_should_pump(true, true));
    }

    #[test]
    fn win32_msg_matches_documented_native_abi() {
        use std::mem::{align_of, offset_of, size_of};

        assert_eq!(size_of::<Win32Point>(), 8);
        assert_eq!(align_of::<Win32Point>(), 4);
        assert_eq!(offset_of!(Win32Point, x), 0);
        assert_eq!(offset_of!(Win32Point, y), 4);

        #[cfg(target_pointer_width = "64")]
        {
            assert_eq!(size_of::<Win32Msg>(), 48);
            assert_eq!(align_of::<Win32Msg>(), 8);
            assert_eq!(offset_of!(Win32Msg, hwnd), 0);
            assert_eq!(offset_of!(Win32Msg, message), 8);
            assert_eq!(offset_of!(Win32Msg, wparam), 16);
            assert_eq!(offset_of!(Win32Msg, lparam), 24);
            assert_eq!(offset_of!(Win32Msg, time), 32);
            assert_eq!(offset_of!(Win32Msg, pt), 36);
            assert_eq!(offset_of!(Win32Msg, l_private), 44);
        }

        #[cfg(target_pointer_width = "32")]
        {
            assert_eq!(size_of::<Win32Msg>(), 32);
            assert_eq!(align_of::<Win32Msg>(), 4);
            assert_eq!(offset_of!(Win32Msg, hwnd), 0);
            assert_eq!(offset_of!(Win32Msg, message), 4);
            assert_eq!(offset_of!(Win32Msg, wparam), 8);
            assert_eq!(offset_of!(Win32Msg, lparam), 12);
            assert_eq!(offset_of!(Win32Msg, time), 16);
            assert_eq!(offset_of!(Win32Msg, pt), 20);
            assert_eq!(offset_of!(Win32Msg, l_private), 28);
        }
    }

