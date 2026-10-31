// Application bootstrap and Tauri builder wiring.

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    install_rustls_crypto_provider();
    diagnostics::install_hooks();
    let streaming = Arc::new(StreamingState::new());
    let viewer_presence = Arc::new(viewer_presence::ViewerPresenceState::new());

    let builder = tauri::Builder::default();
    #[cfg(not(debug_assertions))]
    let builder = builder.plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
        show_main_window(app);
    }));
    builder
        .plugin(tauri_plugin_deep_link::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_process::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_store::Builder::new().build())
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_decorum::init())
        .manage(streaming)
        .manage(viewer_presence)
        .invoke_handler(tauri::generate_handler![
            get_doctor_report,
            get_twitch_client_id,
            auth_get_session,
            auth_start_device_login,
            auth_poll_device_login,
            auth_logout,
            twitch_web_auth_status,
            twitch_web_auth_save,
            twitch_web_auth_clear,
            channel_points_claim_auth_status,
            channel_points_claim_auth_start_device_login,
            channel_points_claim_auth_poll_device_login,
            channel_points_claim_auth_clear,
            viewer_presence_sync,
            viewer_presence_status,
            channel_points_refresh,
            channel_points_cached,
            channel_points_vote_poll,
            channel_points_vote_prediction,
            channel_points_redeem_reward,
            helix_fetch,
            stream_start,
            stream_list,
            stream_stop,
            stream_stop_all,
            stream_toggle_mute,
            open_chatterino_chat,
            open_independent_chatterino_chat,
            close_independent_chatterino_chat,
            close_owned_chatterino,
            layout_watching,
            dock_set_linked,
            dock_set_chat_fraction,
            dock_cycle_monitor,
            diagnostics_set_debug,
            diagnostics_set_debug_categories,
            diagnostics_log_event,
            diagnostics_set_sentry_enabled,
            diagnostics_open_logs,
            diagnostics_open_crashes,
            eventsub_sync,
            eventsub_status,
            raid_overlay_place,
            channel_points_hud_place,
            points_hud_place_window,
            overlay_fit_webview,
            overlay_place_hud,
            poll_overlay_place,
            poll_overlay_raise,
            app_quit,
            enable_title_bar_overlay
        ])
        .setup(|app| {
            migrate_legacy_app_data(app.handle());
            if let Ok(resource_dir) = app.path().resource_dir() {
                std::thread::spawn(move || {
                    let freed = streaming::cleanup_legacy_streamlink_resources(&resource_dir);
                    if freed > 0 {
                        diagnostics::log_line(&format!(
                            "[streamlink] removed {:.1} MB of legacy bundled files",
                            freed as f64 / (1024.0 * 1024.0)
                        ));
                    }
                });
            }
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.set_decorations(false);
                let _ = window.set_shadow(true);
                let _ = enable_main_title_bar_overlay(&window);
            }
            streaming::init_dock(app.handle().clone());
            streaming::init_player_layout_watch(app.handle().clone());
            eventsub::init(app.handle().clone());
            channel_points_realtime::init(app.handle().clone());
            viewer_presence::init(app.handle().clone());
            let state = app.state::<SharedStreaming>().inner().clone();
            streaming::start_session_watchdog(app.handle().clone(), state);
            // Warm Streamlink so the first watch doesn't pay Python/plugin cold-start.
            std::thread::spawn(|| {
                if let Some(path) = doctor::find_streamlink_path() {
                    let mut cmd = std::process::Command::new(path);
                    cmd.arg("--version")
                        .stdin(std::process::Stdio::null())
                        .stdout(std::process::Stdio::null())
                        .stderr(std::process::Stdio::null());
                    #[cfg(windows)]
                    {
                        use std::os::windows::process::CommandExt;
                        cmd.creation_flags(0x0800_0000);
                    }
                    let _ = cmd.status();
                }
            });
            Ok(())
        })
        .on_window_event(|window, event| {
            if window.label() == "main" {
                if let tauri::WindowEvent::CloseRequested { .. } = event {
                    // Overlay HWNDs can sit on the caption and keep the process
                    // alive after main hides/closes. Don't drop the tray here:
                    // close-to-tray still needs it.
                    close_overlay_windows(window.app_handle());
                    #[cfg(debug_assertions)]
                    exit_after_overlays_close(window.app_handle());
                }
            }
        })
        .build({
            let ctx = tauri::generate_context!();
            #[cfg(all(windows, debug_assertions))]
            let mut ctx = ctx;
            // Tauri always sets a WebView2 data dir (and ignores
            // WEBVIEW2_USER_DATA_FOLDER). Give debug a folder of its own so
            // `tauri:dev` can start while the installed app is already open.
            #[cfg(all(windows, debug_assertions))]
            if let Some(window) = ctx.config_mut().app.windows.first_mut() {
                window.data_directory = Some(PathBuf::from("webview-dev"));
            }
            ctx
        })
        .expect("error while building tauri application")
        .run(|app, event| {
            if matches!(
                event,
                tauri::RunEvent::ExitRequested { .. } | tauri::RunEvent::Exit
            ) {
                cleanup_on_exit(app);
            }
        });
}
