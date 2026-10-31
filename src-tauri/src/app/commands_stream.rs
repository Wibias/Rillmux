// Tauri commands for streams, Chatterino docking, and dock layout.

#[tauri::command]
async fn stream_start(
    app: AppHandle,
    state: tauri::State<'_, SharedStreaming>,
    request: LaunchRequest,
) -> Result<StreamSession, String> {
    diagnostics::log_event(
        diagnostics::DebugCategory::Windows,
        "stream.start.native",
        &format!(
            "channel={} slot_index={:?} slot_count={:?} reserve_chat={:?} replace_existing={:?}",
            request.channel,
            request.slot_index,
            request.slot_count,
            request.reserve_chat,
            request.replace_existing
        ),
    );
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
    diagnostics::log_event(
        diagnostics::DebugCategory::Windows,
        "stream.stop.native",
        &format!("session={}", diagnostics::redact_id(&id)),
    );
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
    diagnostics::log_event(
        diagnostics::DebugCategory::Windows,
        "stream.stop_all.native",
        "requested=true",
    );
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
    let channel_list = channels.join(",");
    diagnostics::log_event(
        diagnostics::DebugCategory::Windows,
        "chatterino.open.native",
        &format!("channels={channel_list}"),
    );
    tauri::async_runtime::spawn_blocking(move || {
        streaming::debug_chatterino_windows("open.before");
        let result =
            streaming::launch_chatterino_for_channels(&channels).map_err(|e| e.to_string());
        streaming::debug_chatterino_windows("open.after");
        diagnostics::log_event(
            diagnostics::DebugCategory::Windows,
            "chatterino.open.native.result",
            &format!("ok={}", result.is_ok()),
        );
        result
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
async fn open_independent_chatterino_chat(channel: String) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || {
        streaming::open_independent_chatterino(&channel).map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
async fn close_independent_chatterino_chat(channel: String) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || streaming::close_independent_chatterino(&channel))
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
async fn close_owned_chatterino() -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(|| {
        streaming::debug_chatterino_windows("close.before");
        streaming::close_owned_chatterino();
        streaming::debug_chatterino_windows("close.after");
        diagnostics::log_event(
            diagnostics::DebugCategory::Windows,
            "chatterino.close.native.result",
            "ok=true",
        );
    })
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
    let debug_channels = channels.join(",");
    let debug_layout = layout.clone().unwrap_or_else(|| "default".into());
    diagnostics::log_event(
        diagnostics::DebugCategory::Windows,
        "layout.native.request",
        &format!(
            "channels={debug_channels} reserve_chat={reserve_chat} layout={debug_layout} linked={linked_dock:?} fraction={chat_fraction:?} side={main_side:?}"
        ),
    );
    tauri::async_runtime::spawn_blocking(move || {
        let result = streaming::layout_watching(
            &channels,
            reserve_chat,
            layout.as_deref(),
            linked_dock,
            chat_fraction,
            main_side.as_deref(),
        )
        .map_err(|e| e.to_string());
        diagnostics::log_event(
            diagnostics::DebugCategory::Windows,
            "layout.native.result",
            &format!("ok={}", result.is_ok()),
        );
        result
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
