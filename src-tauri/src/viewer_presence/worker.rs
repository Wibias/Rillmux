// Per-channel watch worker loop, cadence, and cancellation.

async fn run_worker(
    state: SharedViewerPresence,
    target: ViewerPresenceTarget,
    viewer_id: String,
    token: String,
    generation: u64,
    cancel: Arc<AtomicBool>,
) {
    let mut runtime: Option<RuntimeConfig> = None;
    let mut failures = 0u32;

    loop {
        if cancel.load(Ordering::Acquire)
            || state.generation.load(Ordering::Acquire) != generation
            || !website_auth_still_matches(&viewer_id, &token)
        {
            break;
        }

        let cycle = run_watch_cycle(&state, &target, &viewer_id, &token, &mut runtime).await;

        let delay = match cycle {
            Ok(status) => {
                failures = 0;
                set_diagnostic(
                    &state,
                    &target,
                    "telemetry-accepted",
                    Some(status.as_u16()),
                    None,
                    Some(unix_time_ms()),
                );
                success_interval()
            }
            Err(error) => {
                failures = failures.saturating_add(1);
                if matches!(error.stage, "runtime-config" | "playback-token") {
                    runtime = None;
                }
                set_diagnostic(
                    &state,
                    &target,
                    error.stage,
                    error.status,
                    Some(error.message),
                    None,
                );
                failure_delay(failures)
            }
        };

        if wait_or_cancel(&cancel, delay).await {
            break;
        }
    }

    if let Ok(mut workers) = state.workers.lock() {
        let should_remove = workers
            .get(&target.session_id)
            .map(|worker| Arc::ptr_eq(&worker.cancel, &cancel))
            .unwrap_or(false);
        if should_remove {
            workers.remove(&target.session_id);
        }
    }
}

async fn run_watch_cycle(
    state: &ViewerPresenceState,
    target: &ViewerPresenceTarget,
    viewer_id: &str,
    token: &str,
    runtime: &mut Option<RuntimeConfig>,
) -> Result<StatusCode, ProtocolError> {
    if runtime.is_none() {
        set_diagnostic(state, target, "runtime-config", None, None, None);
        *runtime = Some(resolve_runtime(&target.channel_login).await?);
    }
    let runtime = runtime
        .as_ref()
        .ok_or_else(|| ProtocolError::new("runtime-config", "runtime configuration unavailable"))?;

    set_diagnostic(state, target, "playback-token", None, None, None);
    let playback = fetch_playback_access_token(
        &target.channel_login,
        token,
        &state.device_id,
        &state.client_session,
        &runtime.client_version,
    )
    .await?;

    set_diagnostic(state, target, "master-playlist", None, None, None);
    let master_url = build_usher_url(&target.channel_login, &playback.signature, &playback.value)?;
    let (master, master_status) = fetch_text(&master_url, "master-playlist").await?;
    set_diagnostic(
        state,
        target,
        "master-playlist",
        Some(master_status.as_u16()),
        None,
        None,
    );

    let media_url = select_media_playlist_url(&master, &master_url)?;

    set_diagnostic(state, target, "media-playlist", None, None, None);
    let (media, media_status) = fetch_text(&media_url, "media-playlist").await?;
    set_diagnostic(
        state,
        target,
        "media-playlist",
        Some(media_status.as_u16()),
        None,
        None,
    );
    let segment_url = select_media_segment_url(&media, &media_url)?;

    set_diagnostic(state, target, "media-segment", None, None, None);
    let segment_status = touch_media_segment(&segment_url).await?;
    set_diagnostic(
        state,
        target,
        "media-segment",
        Some(segment_status.as_u16()),
        None,
        None,
    );

    set_diagnostic(state, target, "telemetry", None, None, None);
    send_minute_watched(&runtime.spade_url, target, viewer_id).await
}

fn website_auth_still_matches(viewer_id: &str, expected_token: &str) -> bool {
    crate::twitch_web_auth::load_session()
        .ok()
        .flatten()
        .map(|session| session.token == expected_token && session.user_id == viewer_id)
        .unwrap_or(false)
}

fn success_interval_from_seed(seed: u64) -> Duration {
    let span = MAX_SUCCESS_INTERVAL_SECS - MIN_SUCCESS_INTERVAL_SECS + 1;
    Duration::from_secs(MIN_SUCCESS_INTERVAL_SECS + seed % span)
}

fn success_interval() -> Duration {
    let seed = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos()
        .try_into()
        .unwrap_or(u64::MAX);
    success_interval_from_seed(seed)
}

fn failure_delay(failures: u32) -> Duration {
    let exponent = failures.saturating_sub(1).min(4);
    Duration::from_secs((30u64.saturating_mul(1u64 << exponent)).min(300))
}

async fn wait_or_cancel(cancel: &AtomicBool, duration: Duration) -> bool {
    let mut remaining = duration.as_secs();
    while remaining > 0 {
        if cancel.load(Ordering::Acquire) {
            return true;
        }
        tokio::time::sleep(Duration::from_secs(1)).await;
        remaining -= 1;
    }
    cancel.load(Ordering::Acquire)
}

fn unix_time_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .try_into()
        .unwrap_or(u64::MAX)
}
