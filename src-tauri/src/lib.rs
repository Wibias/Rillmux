// MSVC prints “.lib/.exp werden erstellt” to stdout while linking cdylibs; ignore that noise.
#![allow(linker_messages)]

mod auth;
mod branding;
mod channel_points;
mod channel_points_claim_auth;
mod channel_points_realtime;
mod diagnostics;
mod dock;
mod doctor;
mod eventsub;
mod helix;
mod http;
mod overlay;
mod streaming;
mod twitch_web_auth;
mod viewer_presence;

use auth::{AuthSession, DeviceCodeResponse};
use doctor::DoctorReport;
use std::path::PathBuf;
use std::sync::Arc;
use streaming::{
    ChannelPointsHudPlace, LaunchRequest, OverlayRect, SharedStreaming, StreamSession,
    StreamingState,
};
use tauri::{AppHandle, Manager};
use tauri_plugin_window_controls::{TitleBarColors, WindowControlsExt};

#[tauri::command]
async fn get_doctor_report() -> Result<DoctorReport, String> {
    // Probing `streamlink --version`, `mpv --version` and the registry can
    // take seconds (AV scans, cold Python start) — never run it on the
    // main thread (sync commands) or a runtime worker without offloading.
    tauri::async_runtime::spawn_blocking(doctor::run_doctor)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
fn get_twitch_client_id() -> Result<String, String> {
    auth::public_client_id().map_err(|e| e.to_string())
}

#[tauri::command]
async fn auth_get_session() -> Result<AuthSession, String> {
    auth::get_session().await.map_err(|e| e.to_string())
}

#[tauri::command]
async fn auth_start_device_login() -> Result<DeviceCodeResponse, String> {
    auth::start_device_flow().await.map_err(|e| e.to_string())
}

#[tauri::command]
async fn auth_poll_device_login(device_code: String) -> Result<auth::DevicePoll, String> {
    auth::poll_device_token(&device_code)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
async fn auth_logout(
    presence: tauri::State<'_, viewer_presence::SharedViewerPresence>,
) -> Result<(), String> {
    channel_points_realtime::clear();
    viewer_presence::cancel_all(presence.inner());
    auth::logout().await.map_err(|e| e.to_string())
}

#[tauri::command]
fn twitch_web_auth_status() -> Result<twitch_web_auth::TwitchWebAuthStatus, String> {
    twitch_web_auth::get_status().map_err(|e| e.to_string())
}

#[tauri::command]
async fn twitch_web_auth_save(
    token: String,
) -> Result<twitch_web_auth::TwitchWebAuthStatus, String> {
    twitch_web_auth::save(&token)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
fn twitch_web_auth_clear(
    presence: tauri::State<'_, viewer_presence::SharedViewerPresence>,
) -> Result<twitch_web_auth::TwitchWebAuthStatus, String> {
    channel_points_realtime::clear();
    viewer_presence::cancel_all(presence.inner());
    twitch_web_auth::clear().map_err(|e| e.to_string())
}

#[tauri::command]
fn channel_points_claim_auth_status(
) -> Result<channel_points_claim_auth::ChannelPointsClaimAuthStatus, String> {
    channel_points_claim_auth::get_status().map_err(|e| e.to_string())
}

#[tauri::command]
async fn channel_points_claim_auth_start_device_login(
) -> Result<channel_points_claim_auth::TvDeviceCodeResponse, String> {
    channel_points_claim_auth::start_device_flow()
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
async fn channel_points_claim_auth_poll_device_login(
    device_code: String,
) -> Result<channel_points_claim_auth::TvDevicePoll, String> {
    channel_points_claim_auth::poll_device_token(&device_code)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
fn channel_points_claim_auth_clear(
) -> Result<channel_points_claim_auth::ChannelPointsClaimAuthStatus, String> {
    channel_points_claim_auth::clear().map_err(|e| e.to_string())
}

#[tauri::command]
async fn viewer_presence_sync(
    state: tauri::State<'_, viewer_presence::SharedViewerPresence>,
    enabled: bool,
    targets: Vec<viewer_presence::ViewerPresenceTarget>,
) -> Result<viewer_presence::ViewerPresenceStatus, String> {
    if let Err(error) = channel_points_realtime::sync(enabled, &targets).await {
        viewer_presence::cancel_all(state.inner());
        return Err(error);
    }
    if enabled && !targets.is_empty() && !channel_points_realtime::is_ready() {
        viewer_presence::cancel_all(state.inner());
        return Err("waiting for Twitch realtime presence".into());
    }
    viewer_presence::sync(state.inner().clone(), enabled, targets)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
fn viewer_presence_status(
    state: tauri::State<'_, viewer_presence::SharedViewerPresence>,
) -> Result<viewer_presence::ViewerPresenceStatus, String> {
    viewer_presence::get_status(state.inner()).map_err(|e| e.to_string())
}

#[tauri::command]
async fn channel_points_refresh(
    channel_login: String,
    include_poll: Option<bool>,
) -> Result<channel_points::ChannelPointsSnapshot, String> {
    channel_points::refresh(&channel_login, include_poll.unwrap_or(false))
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
fn channel_points_cached(channel_login: String) -> Option<channel_points::ChannelPointsSnapshot> {
    channel_points::cached_snapshot(&channel_login)
}

#[tauri::command]
async fn channel_points_vote_poll(
    channel_login: String,
    poll_id: String,
    choice_id: String,
    cost: u64,
) -> Result<channel_points::ChannelPointsSnapshot, String> {
    channel_points::vote_poll(&channel_login, &poll_id, &choice_id, cost)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
async fn channel_points_vote_prediction(
    channel_login: String,
    event_id: String,
    outcome_id: String,
    points: u64,
) -> Result<channel_points::ChannelPointsSnapshot, String> {
    channel_points::vote_prediction(&channel_login, &event_id, &outcome_id, points)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
async fn channel_points_redeem_reward(
    channel_login: String,
    reward_id: String,
    text: Option<String>,
) -> Result<channel_points::ChannelPointsSnapshot, String> {
    channel_points::redeem_reward(&channel_login, &reward_id, text)
        .await
        .map_err(|e| e.to_string())
}

/// Helix GET proxy: keeps the OAuth token inside Rust (never in the webview).
#[tauri::command]
async fn helix_fetch(
    path: String,
    query: Option<Vec<(String, String)>>,
) -> Result<serde_json::Value, String> {
    helix::fetch(&path, &query.unwrap_or_default())
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
async fn stream_start(
    app: AppHandle,
    state: tauri::State<'_, SharedStreaming>,
    request: LaunchRequest,
) -> Result<StreamSession, String> {
    // Path resolution + process spawn off the main thread.
    let state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        streaming::start_stream(&app, &state, request).map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
async fn stream_list(
    state: tauri::State<'_, SharedStreaming>,
) -> Result<Vec<StreamSession>, String> {
    let state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        streaming::list_sessions(&state).map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
async fn stream_stop(state: tauri::State<'_, SharedStreaming>, id: String) -> Result<(), String> {
    // child.wait() blocks until Streamlink exits — offload it.
    let state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        streaming::stop_stream(&state, &id).map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
async fn stream_stop_all(state: tauri::State<'_, SharedStreaming>) -> Result<(), String> {
    let state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        streaming::stop_all(&state).map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
fn stream_toggle_mute(
    state: tauri::State<'_, SharedStreaming>,
    id: String,
) -> Result<bool, String> {
    streaming::toggle_stream_mute(&state, &id).map_err(|e| e.to_string())
}

#[tauri::command]
async fn open_chatterino_chat(channels: Vec<String>) -> Result<String, String> {
    tauri::async_runtime::spawn_blocking(move || {
        streaming::launch_chatterino_for_channels(&channels).map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
async fn close_owned_chatterino() -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(streaming::close_owned_chatterino)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
async fn layout_watching(
    channels: Vec<String>,
    reserve_chat: bool,
    layout: Option<String>,
    linked_dock: Option<bool>,
    chat_fraction: Option<f64>,
    main_side: Option<String>,
) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || {
        streaming::layout_watching(
            &channels,
            reserve_chat,
            layout.as_deref(),
            linked_dock,
            chat_fraction,
            main_side.as_deref(),
        )
        .map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
async fn dock_set_linked(enabled: bool) {
    let _ = tauri::async_runtime::spawn_blocking(move || streaming::dock_set_linked(enabled)).await;
}

#[tauri::command]
async fn dock_set_chat_fraction(fraction: f64) {
    let _ =
        tauri::async_runtime::spawn_blocking(move || streaming::dock_set_chat_fraction(fraction))
            .await;
}

#[tauri::command]
async fn dock_cycle_monitor() {
    let _ = tauri::async_runtime::spawn_blocking(streaming::dock_cycle_monitor).await;
}

#[tauri::command]
fn diagnostics_set_debug(enabled: bool) {
    diagnostics::set_debug_enabled(enabled);
}

#[tauri::command]
fn diagnostics_open_logs() -> Result<(), String> {
    diagnostics::ensure_dirs();
    open_folder(&diagnostics::logs_dir())
}

#[tauri::command]
fn diagnostics_open_crashes() -> Result<(), String> {
    diagnostics::ensure_dirs();
    open_folder(&diagnostics::crashes_dir())
}

fn open_folder(path: &std::path::Path) -> Result<(), String> {
    #[cfg(windows)]
    {
        std::process::Command::new("explorer")
            .arg(path)
            .spawn()
            .map_err(|e| e.to_string())?;
        Ok(())
    }
    #[cfg(not(windows))]
    {
        let _ = path;
        Err("opening the logs folder is only supported on Windows".into())
    }
}

#[tauri::command]
fn eventsub_sync(enabled: bool, channels: Vec<String>) -> Result<(), String> {
    eventsub::sync(enabled, channels);
    Ok(())
}

#[tauri::command]
fn raid_overlay_place(from_channel: String) -> Option<OverlayRect> {
    streaming::raid_overlay_host(&from_channel)
}

#[tauri::command]
fn channel_points_hud_place(
    app: AppHandle,
    channel_login: String,
) -> Option<ChannelPointsHudPlace> {
    streaming::restack_hud_above_player(
        &app,
        &format!("points-hud-{}", channel_login.trim().to_ascii_lowercase()),
    );
    let player = streaming::channel_points_hud_host(&channel_login)?;
    Some(ChannelPointsHudPlace {
        player,
        caption_avoid: streaming::player_caption_avoid(&channel_login, player),
    })
}

/// Force the overlay HWND and its WebView2 child to the physical size.
/// Transparent windows often keep the old child size after `setSize`.
#[tauri::command]
fn overlay_fit_webview(window: tauri::WebviewWindow, width: i32, height: i32) {
    streaming::fit_overlay_webview(&window, width, height);
}

#[tauri::command]
fn overlay_place_hud(
    app: AppHandle,
    label: String,
    x: i32,
    y: i32,
    width: i32,
    height: i32,
    force: bool,
) {
    streaming::place_hud_overlay(&app, &label, x, y, width, height, force);
}

#[tauri::command]
async fn poll_overlay_place() -> Option<OverlayRect> {
    tauri::async_runtime::spawn_blocking(streaming::poll_overlay_chat_host)
        .await
        .ok()
        .flatten()
}

#[tauri::command]
fn poll_overlay_raise() {
    streaming::raise_poll_overlay_window();
}

#[tauri::command]
fn app_quit(app: AppHandle) {
    cleanup_on_exit(&app);
    app.exit(0);
}

const MAIN_TRAY_ID: &str = "main-tray";

fn close_overlay_windows(app: &AppHandle) {
    for (label, window) in app.webview_windows() {
        if label != "main" {
            let _ = window.close();
        }
    }
}

fn cleanup_on_exit(app: &AppHandle) {
    channel_points_realtime::clear();
    let _ = app.remove_tray_by_id(MAIN_TRAY_ID);
    close_overlay_windows(app);
}

fn init_sentry() -> Option<sentry::ClientInitGuard> {
    let dsn = std::env::var("SENTRY_DSN").ok().filter(|s| !s.is_empty())?;
    let mut opts = sentry::apply_defaults(sentry::ClientOptions::default());
    opts.dsn = Some(dsn.parse().ok()?);
    opts.release = Some(std::borrow::Cow::Borrowed(env!("CARGO_PKG_VERSION")));
    opts.send_default_pii = false;
    Some(sentry::init(opts))
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
    window.set_title_bar_height(38).map_err(|e| e.to_string())?;
    let light = TitleBarColors {
        symbol: Some("#0e0e10".into()),
        hover: Some("#00000014".into()),
        pressed: Some("#0000000a".into()),
        ..Default::default()
    };
    let dark = TitleBarColors {
        symbol: Some("#efeff1".into()),
        hover: Some("#ffffff14".into()),
        pressed: Some("#ffffff0a".into()),
        ..Default::default()
    };
    window
        .set_title_bar_colors(light, dark)
        .map_err(|e| e.to_string())?;
    // Native DWM caption buttons steal clicks from HTML/plugin chrome
    // without delivering WM_CLOSE, so X appears to do nothing.
    Ok(())
}

/// Inject native Win11 caption buttons into the frameless main window.
/// Called from the webview after `__TAURI_INTERNALS__` exists so the overlay
/// script does not bail out on a too-early eval.
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

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    install_rustls_crypto_provider();
    let _sentry_guard = init_sentry();
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
        .plugin(tauri_plugin_window_controls::init())
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
            close_owned_chatterino,
            layout_watching,
            dock_set_linked,
            dock_set_chat_fraction,
            dock_cycle_monitor,
            diagnostics_set_debug,
            diagnostics_open_logs,
            diagnostics_open_crashes,
            eventsub_sync,
            raid_overlay_place,
            channel_points_hud_place,
            overlay_fit_webview,
            overlay_place_hud,
            poll_overlay_place,
            poll_overlay_raise,
            app_quit,
            enable_title_bar_overlay
        ])
        .setup(|app| {
            migrate_legacy_app_data(app.handle());
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.set_decorations(false);
                let _ = window.set_shadow(true);
                let _ = enable_main_title_bar_overlay(&window);
            }
            streaming::init_dock(app.handle().clone());
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
                    {
                        cleanup_on_exit(window.app_handle());
                        window.app_handle().exit(0);
                    }
                }
            }
        })
        .build({
            let mut ctx = tauri::generate_context!();
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
