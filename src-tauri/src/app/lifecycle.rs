// App lifecycle: tray, overlay shutdown, Sentry, title bar, and data migration.

const MAIN_TRAY_ID: &str = "main-tray";

fn close_overlay_windows(app: &AppHandle) {
    for (label, window) in app.webview_windows() {
        if label != "main" {
            let _ = window.close();
        }
    }
}

/// Overlay webviews are destroyed on a later event-loop turn. Exiting in the
/// same turn makes Chromium unregister its window class while overlay HWNDs are
/// still alive, which prints "Failed to unregister class Chrome_WidgetWin_0"
/// (error 1412) on the debug console. Close the overlays first, give them a
/// bounded moment to go away, then leave.
fn exit_after_overlays_close(app: &AppHandle) {
    let app = app.clone();
    std::thread::spawn(move || {
        let deadline = std::time::Instant::now() + std::time::Duration::from_millis(750);
        while std::time::Instant::now() < deadline {
            let overlays_left = app
                .webview_windows()
                .keys()
                .any(|label| label.as_str() != "main");
            if !overlays_left {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(25));
        }
        let host = app.clone();
        let _ = app.run_on_main_thread(move || {
            cleanup_on_exit(&host);
            host.exit(0);
        });
    });
}

fn cleanup_on_exit(app: &AppHandle) {
    channel_points_realtime::clear();
    if let Some(state) = app.try_state::<SharedStreaming>() {
        let _ = streaming::stop_all(state.inner());
    }
    streaming::close_owned_chatterino();
    dock::clear_session();
    let _ = app.remove_tray_by_id(MAIN_TRAY_ID);
    close_overlay_windows(app);
}

fn native_sentry_slot() -> &'static Mutex<Option<sentry::ClientInitGuard>> {
    static SLOT: OnceLock<Mutex<Option<sentry::ClientInitGuard>>> = OnceLock::new();
    SLOT.get_or_init(|| Mutex::new(None))
}

fn init_native_sentry() -> Option<sentry::ClientInitGuard> {
    let dsn = option_env!("SENTRY_DSN")
        .map(str::trim)
        .filter(|value| !value.is_empty())?;
    let mut opts = sentry::apply_defaults(sentry::ClientOptions::default());
    opts.dsn = Some(dsn.parse().ok()?);
    opts.release = Some(std::borrow::Cow::Borrowed(env!("CARGO_PKG_VERSION")));
    opts.send_default_pii = false;
    Some(sentry::init(opts))
}

fn set_native_sentry_enabled(enabled: bool) {
    let mut slot = native_sentry_slot()
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    if enabled {
        if slot.is_none() {
            *slot = init_native_sentry();
        }
    } else {
        let guard = slot.take();
        drop(slot);
        drop(guard);
    }
}

#[cfg(not(debug_assertions))]
fn show_main_window(app: &AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.show();
        let _ = window.unminimize();
        let _ = window.set_focus();
    }
}

fn enable_main_title_bar_overlay(window: &tauri::WebviewWindow) -> Result<(), String> {
    // decorum owns the frameless titlebar: it injects the DOM chrome and its
    // show_snap_overlay command keeps Win11 Snap Layouts reachable from the DOM
    // maximize button. Decorating twice would stack page-load listeners, so the
    // first successful call wins and the webview's later invoke is a no-op.
    static APPLIED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
    if APPLIED.load(std::sync::atomic::Ordering::SeqCst) {
        return Ok(());
    }
    window
        .create_overlay_titlebar()
        .map_err(|e| e.to_string())?;
    APPLIED.store(true, std::sync::atomic::Ordering::SeqCst);
    Ok(())
}

/// Kept for the webview's post-mount call: the overlay is already applied from
/// `setup`, so this only guards against a stale no-op invoke.
#[tauri::command]
fn enable_title_bar_overlay(window: tauri::WebviewWindow) -> Result<(), String> {
    enable_main_title_bar_overlay(&window)
}

fn migrate_legacy_app_data(app: &AppHandle) {
    migrate_settings_file(app.path().app_data_dir().ok());
    migrate_settings_file(app.path().app_config_dir().ok());
}

fn migrate_settings_file(new_dir: Option<PathBuf>) {
    let Some(new_dir) = new_dir else {
        return;
    };
    let dest = new_dir.join("settings.json");
    if dest.exists() {
        return;
    }
    let Some(parent) = new_dir.parent() else {
        return;
    };
    let src = parent
        .join(branding::APP_IDENTIFIER_LEGACY)
        .join("settings.json");
    if !src.is_file() {
        return;
    }
    if std::fs::create_dir_all(&new_dir).is_err() {
        return;
    }
    let _ = std::fs::copy(src, dest);
}

fn install_rustls_crypto_provider() {
    // tokio-tungstenite's rustls build does not enable a crate-feature provider.
    // EventSub panics on connect unless one is installed for the process.
    let _ = rustls::crypto::ring::default_provider().install_default();
}
