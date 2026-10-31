// Tauri commands that place and raise the overlay windows.

#[tauri::command]
fn raid_overlay_place(from_channel: String) -> Option<OverlayRect> {
    let result = streaming::raid_overlay_host(&from_channel);
    diagnostics::log_event(
        diagnostics::DebugCategory::Raids,
        "raid.overlay.place",
        &format!("from={} host_found={}", from_channel, result.is_some()),
    );
    result
}

fn hud_host_debug_state() -> &'static Mutex<HashMap<String, bool>> {
    static STATE: OnceLock<Mutex<HashMap<String, bool>>> = OnceLock::new();
    STATE.get_or_init(|| Mutex::new(HashMap::new()))
}

fn hud_host_debug_changed(channel: &str, host_found: bool) -> bool {
    let mut state = hud_host_debug_state()
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    let previous = state.insert(channel.to_string(), host_found);
    previous != Some(host_found)
}

#[tauri::command]
fn channel_points_hud_place(
    app: AppHandle,
    channel_login: String,
) -> Option<ChannelPointsHudPlace> {
    let channel = channel_login.trim().to_ascii_lowercase();
    streaming::restack_hud_above_player(&app, &format!("points-hud-{channel}"));
    let place = streaming::channel_points_hud_placement(&channel_login);
    let host_found = place.as_ref().is_some_and(|next| !next.hidden);
    if hud_host_debug_changed(&channel, host_found) {
        diagnostics::log_event(
            diagnostics::DebugCategory::Windows,
            "hud.place.request",
            &format!("channel={channel} phase=host"),
        );
        diagnostics::log_event(
            diagnostics::DebugCategory::Windows,
            "hud.place.applied",
            &format!("channel={channel} host_found={host_found}"),
        );
    }
    place
}

#[tauri::command]
fn points_hud_place_window(
    app: AppHandle,
    channel_login: String,
    x: i32,
    y: i32,
    width: i32,
    height: i32,
    force: bool,
) {
    let channel = channel_login.trim().to_ascii_lowercase();
    if channel.is_empty()
        || channel.len() > 25
        || !channel
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || character == '_')
    {
        diagnostics::log_event(
            diagnostics::DebugCategory::Windows,
            "hud.place.rejected",
            "reason=invalid_channel",
        );
        return;
    }
    let label = format!("points-hud-{channel}");
    diagnostics::log_event(
        diagnostics::DebugCategory::Windows,
        "hud.place.request",
        &format!(
            "channel={channel} phase=main x={x} y={y} width={width} height={height} force={force}"
        ),
    );
    streaming::place_hud_overlay(&app, &label, x, y, width, height, force);
    diagnostics::log_event(
        diagnostics::DebugCategory::Windows,
        "hud.place.applied",
        &format!(
            "channel={channel} phase=main x={x} y={y} width={width} height={height} force={force}"
        ),
    );
}

/// Force the overlay HWND and its WebView2 child to the physical size.
/// Transparent windows often keep the old child size after `setSize`.
#[tauri::command]
fn overlay_fit_webview(window: tauri::WebviewWindow, width: i32, height: i32) {
    streaming::fit_overlay_webview(&window, width, height);
}

#[tauri::command]
fn overlay_place_hud(
    window: tauri::WebviewWindow,
    x: i32,
    y: i32,
    width: i32,
    height: i32,
    force: bool,
) {
    let label = window.label().to_string();
    if streaming::points_hud_channel_from_label(&label).is_none() {
        diagnostics::log_event(
            diagnostics::DebugCategory::Windows,
            "hud.place.rejected",
            &format!("label={label} reason=invalid_overlay"),
        );
        return;
    }
    diagnostics::log_event(
        diagnostics::DebugCategory::Windows,
        "hud.place.request",
        &format!(
            "label={label} phase=overlay x={x} y={y} width={width} height={height} force={force}"
        ),
    );
    streaming::place_hud_overlay(window.app_handle(), &label, x, y, width, height, force);
    diagnostics::log_event(
        diagnostics::DebugCategory::Windows,
        "hud.place.applied",
        &format!(
            "label={label} phase=overlay x={x} y={y} width={width} height={height} force={force}"
        ),
    );
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
    close_overlay_windows(&app);
    exit_after_overlays_close(&app);
}
