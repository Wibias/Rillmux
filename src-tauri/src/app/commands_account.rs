// Tauri commands for doctor, Twitch account auth, and website/claim sessions.

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
    app: AppHandle,
    token: String,
) -> Result<twitch_web_auth::TwitchWebAuthStatus, String> {
    let status = twitch_web_auth::save(&token)
        .await
        .map_err(|e| e.to_string())?;
    let _ = app.emit(twitch_web_auth::STATUS_CHANGED_EVENT, &status);
    Ok(status)
}

#[tauri::command]
fn twitch_web_auth_clear(
    app: AppHandle,
    presence: tauri::State<'_, viewer_presence::SharedViewerPresence>,
) -> Result<twitch_web_auth::TwitchWebAuthStatus, String> {
    channel_points_realtime::clear();
    viewer_presence::cancel_all(presence.inner());
    let status = twitch_web_auth::clear().map_err(|e| e.to_string())?;
    let _ = app.emit(twitch_web_auth::STATUS_CHANGED_EVENT, &status);
    Ok(status)
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
    let target_count = targets.len();
    let channels = targets
        .iter()
        .map(|target| target.channel_login.as_str())
        .collect::<Vec<_>>()
        .join(",");
    diagnostics::log_event(
        diagnostics::DebugCategory::PointsCredit,
        "presence.sync",
        &format!("enabled={enabled} target_count={target_count} channels={channels}"),
    );
    if enabled && target_count > 0 {
        diagnostics::log_event(
            diagnostics::DebugCategory::PointsCredit,
            "worker.start",
            &format!("target_count={target_count} channels={channels}"),
        );
        diagnostics::log_event(
            diagnostics::DebugCategory::PointsCredit,
            "hermes.connect",
            &format!("target_count={target_count} channels={channels}"),
        );
        diagnostics::log_event(
            diagnostics::DebugCategory::Polls,
            "poll.subscription",
            &format!("target_count={target_count} channels={channels}"),
        );
    }

    let realtime_targets = targets.clone();
    let realtime = channel_points_realtime::sync(enabled, &realtime_targets);
    let presence = viewer_presence::sync(state.inner().clone(), enabled, targets);
    let (realtime_result, presence_result) = tokio::join!(realtime, presence);
    match &realtime_result {
        Ok(()) => diagnostics::log_event(
            diagnostics::DebugCategory::PointsCredit,
            "hermes.ready",
            &format!("enabled={enabled} target_count={target_count}"),
        ),
        Err(_) => diagnostics::log_event(
            diagnostics::DebugCategory::PointsCredit,
            "hermes.not_ready",
            &format!("enabled={enabled} target_count={target_count} error_present=true"),
        ),
    }
    diagnostics::log_event(
        diagnostics::DebugCategory::PointsCredit,
        "presence.sync.result",
        &format!("ok={} target_count={target_count}", presence_result.is_ok()),
    );
    presence_result.map_err(|e| e.to_string())
}

#[tauri::command]
fn viewer_presence_status(
    state: tauri::State<'_, viewer_presence::SharedViewerPresence>,
) -> Result<viewer_presence::ViewerPresenceStatus, String> {
    let status = viewer_presence::get_status(state.inner()).map_err(|e| e.to_string())?;
    for worker in &status.workers {
        diagnostics::log_event(
            diagnostics::DebugCategory::PointsCredit,
            "minute_watched.result",
            &format!(
                "channel={} session={} stage={} http_status={:?} success_ms={:?} error_present={}",
                worker.channel_login,
                diagnostics::redact_id(&worker.session_id),
                worker.last_stage,
                worker.last_http_status,
                worker.last_success_unix_ms,
                worker.last_error.is_some()
            ),
        );
    }
    Ok(status)
}
