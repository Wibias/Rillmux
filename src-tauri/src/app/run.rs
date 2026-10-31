// Application bootstrap and Tauri builder wiring.

static BOOT_START: std::sync::OnceLock<std::time::Instant> = std::sync::OnceLock::new();

/// Logs a startup stage with milliseconds since process start so a slow first
/// paint can be attributed to a specific phase from the diagnostics log.
pub(crate) fn boot_log(stage: &str) {
    let start = *BOOT_START.get_or_init(std::time::Instant::now);
    diagnostics::log_line(&format!(
        "[boot] stage={stage} ms={}",
        start.elapsed().as_millis()
    ));
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    BOOT_START.get_or_init(std::time::Instant::now);
    boot_log("run");
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
            boot_log("setup");
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
            // The main window stays hidden behind the native splash until the
            // frontend reports its first frame. Never leave users staring at a
            // splash if that signal is lost.
            {
                let handle = app.handle().clone();
                std::thread::spawn(move || {
                    std::thread::sleep(std::time::Duration::from_secs(12));
                    let hidden = handle
                        .get_webview_window("main")
                        .and_then(|window| window.is_visible().ok())
                        .map(|visible| !visible)
                        .unwrap_or(false);
                    if hidden {
                        boot_log("frontend_ready_timeout");
                        reveal_main_window(&handle);
                    }
                });
            }
            boot_log("setup_done");
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
        .on_page_load(|webview, payload| {
            // The main window starts hidden behind the native splash. Reveal it
            // as soon as its page has loaded, independent of any frontend hook.
            if webview.label() == "main"
                && matches!(payload.event(), tauri::webview::PageLoadEvent::Finished)
            {
                boot_log("page_load_finished");
                let handle = webview.app_handle().clone();
                reveal_main_window(&handle);
                warm_streamlink_once();
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
            if matches!(event, tauri::RunEvent::Ready) {
                boot_log("run_ready");
            }
            if matches!(
                event,
                tauri::RunEvent::ExitRequested { .. } | tauri::RunEvent::Exit
            ) {
                cleanup_on_exit(app);
            }
        });
}
