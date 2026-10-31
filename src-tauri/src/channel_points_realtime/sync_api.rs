// Public sync entry point and the supervisor that keeps one session alive.

pub async fn sync(
    enabled: bool,
    targets: &[crate::viewer_presence::ViewerPresenceTarget],
) -> Result<(), String> {
    if !enabled || targets.is_empty() {
        debug_credit(
            "hermes.sync.skip",
            &format!("enabled={enabled} target_count={}", targets.len()),
        );
        clear();
        return Ok(());
    }

    let web_auth = crate::twitch_web_auth::load_session()
        .map_err(|error| error.to_string())?
        .ok_or_else(|| "Twitch Website Authentication is not configured".to_string())?;
    let auth_session = crate::auth::get_session()
        .await
        .map_err(|error| error.to_string())?;
    let viewer_id = auth_session
        .user_id
        .ok_or_else(|| "log in with Twitch before enabling channel points".to_string())?;
    if !auth_session.logged_in || web_auth.user_id != viewer_id {
        debug_credit("hermes.sync.reject", "reason=account_mismatch");
        clear();
        return Err(
            "Twitch Website Authentication does not match the current Twitch account".into(),
        );
    }

    let mut logins_by_id = HashMap::new();
    let mut channel_ids = targets
        .iter()
        .filter_map(|target| {
            let channel_id = target.channel_id.trim();
            if channel_id.is_empty()
                || !channel_id
                    .chars()
                    .all(|character| character.is_ascii_digit())
            {
                return None;
            }
            let login = target.channel_login.trim().to_ascii_lowercase();
            if !login.is_empty() {
                logins_by_id.insert(channel_id.to_string(), login);
            }
            Some(channel_id.to_string())
        })
        .collect::<Vec<_>>();
    channel_ids.sort();
    channel_ids.dedup();
    if channel_ids.is_empty() {
        debug_credit("hermes.sync.skip", "reason=no_valid_channels");
        clear();
        return Ok(());
    }

    let desired = DesiredPresence {
        token: web_auth.token,
        viewer_id,
        channel_ids,
        logins_by_id,
    };
    let realtime = state();
    let (generation, changed) = {
        let mut current = realtime
            .desired
            .lock()
            .map_err(|_| "Channel Points realtime state is poisoned".to_string())?;
        if current.as_ref() == Some(&desired) {
            (realtime.generation.load(Ordering::Acquire), false)
        } else {
            *current = Some(desired.clone());
            realtime.ready.store(false, Ordering::Release);
            if let Ok(mut error) = realtime.last_error.lock() {
                *error = None;
            }
            (realtime.generation.fetch_add(1, Ordering::AcqRel) + 1, true)
        }
    };
    debug_credit(
        "hermes.sync.desired",
        &format!(
            "generation={generation} changed={changed} channel_count={} viewer={}",
            desired.channel_ids.len(),
            crate::diagnostics::redact_id(&desired.viewer_id)
        ),
    );

    ensure_supervisor();
    realtime.wake.notify_waiters();
    let result = wait_until_ready(generation).await;
    debug_credit(
        "hermes.sync.result",
        &format!("generation={generation} ready={}", result.is_ok()),
    );
    result
}

async fn wait_until_ready(generation: u64) -> Result<(), String> {
    let realtime = state();
    let deadline = tokio::time::Instant::now() + CONNECT_TIMEOUT;
    loop {
        if realtime.generation.load(Ordering::Acquire) != generation {
            debug_credit(
                "hermes.wait.cancelled",
                &format!("generation={generation} reason=reconfigured"),
            );
            return Err("Channel Points realtime presence was reconfigured".into());
        }
        if realtime.ready.load(Ordering::Acquire) {
            debug_credit("hermes.wait.ready", &format!("generation={generation}"));
            return Ok(());
        }
        if tokio::time::Instant::now() >= deadline {
            let detail = realtime
                .last_error
                .lock()
                .ok()
                .and_then(|error| error.clone())
                .unwrap_or_else(|| "waiting for Twitch realtime presence".to_string());
            debug_credit(
                "hermes.wait.timeout",
                &format!(
                    "generation={generation} reason={}",
                    hermes_error_class(&detail)
                ),
            );
            return Err(format!("Channel Points realtime is not ready: {detail}"));
        }
        tokio::select! {
            _ = realtime.changed.notified() => {}
            _ = tokio::time::sleep(Duration::from_millis(100)) => {}
        }
    }
}

async fn run_supervisor() {
    let realtime = state();
    let mut backoff = Duration::from_secs(1);

    loop {
        let (desired, generation) = snapshot();
        let Some(desired) = desired else {
            realtime.wake.notified().await;
            continue;
        };

        realtime.ready.store(false, Ordering::Release);
        realtime.changed.notify_waiters();
        debug_credit(
            "hermes.session.start",
            &format!(
                "generation={generation} channel_count={} backoff_ms={}",
                desired.channel_ids.len(),
                backoff.as_millis()
            ),
        );
        match run_session(&desired, generation).await {
            Ok(()) => {
                debug_credit(
                    "hermes.session.end",
                    &format!("generation={generation} reason=reconfigured"),
                );
                backoff = Duration::from_secs(1);
            }
            Err(error) => {
                let reason = hermes_error_class(&error);
                debug_credit(
                    "hermes.reconnect",
                    &format!(
                        "generation={generation} reason={reason} backoff_ms={}",
                        backoff.as_millis()
                    ),
                );
                mark_not_ready(generation, Some(error));
                tokio::select! {
                    _ = realtime.wake.notified() => {
                        backoff = Duration::from_secs(1);
                    }
                    _ = tokio::time::sleep(backoff) => {
                        backoff = (backoff * 2).min(MAX_BACKOFF);
                    }
                }
            }
        }
    }
}
