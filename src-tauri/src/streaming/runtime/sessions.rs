// Session listing, mute toggling, pruning, and the watchdog.

pub fn list_sessions(state: &StreamingState) -> Result<Vec<StreamSession>, StreamError> {
    let _ = prune_dead_sessions(state)?;
    let map = state
        .inner
        .lock()
        .map_err(|_| StreamError::Message("streaming state poisoned".into()))?;
    Ok(map.values().map(|s| s.info.clone()).collect())
}

/// Toggle mpv mute for a session (by id). Returns the new muted state.
pub fn toggle_stream_mute(state: &StreamingState, id: &str) -> Result<bool, StreamError> {
    let mut map = state
        .inner
        .lock()
        .map_err(|_| StreamError::Message("streaming state poisoned".into()))?;
    let session = map
        .get_mut(id)
        .ok_or_else(|| StreamError::Message(format!("unknown session {id}")))?;
    let next = !session.info.muted;
    let pipe = session
        .player
        .as_ref()
        .map(|p| p.pipe.clone())
        .ok_or_else(|| {
            StreamError::Message(
                "mute needs a fast-start mpv session (IPC). Restart the stream.".into(),
            )
        })?;
    // JSON boolean — string "yes"/"no" is wrong for flag properties over IPC.
    mpv_ipc_json(
        &pipe,
        vec![
            serde_json::Value::String("set_property".into()),
            serde_json::Value::String("mute".into()),
            serde_json::Value::Bool(next),
        ],
        Duration::from_millis(800),
    )?;
    session.info.muted = next;
    Ok(next)
}

/// Drop sessions whose Streamlink exited or (when ready) whose mpv window is gone.
/// Returns true if any session was removed.
pub fn prune_dead_sessions(state: &StreamingState) -> Result<bool, StreamError> {
    let mut map = state
        .inner
        .lock()
        .map_err(|_| StreamError::Message("streaming state poisoned".into()))?;
    let mut remove: Vec<(String, String)> = Vec::new();
    for (id, session) in map.iter_mut() {
        let child_dead = match session.child.try_wait() {
            Ok(Some(_)) => true,
            Ok(None) => false,
            Err(_) => true,
        };
        // Fast-start sessions own the pre-launched mpv, so its process state
        // is authoritative: closing the player window exits mpv even in
        // --idle mode. Detect that within one watchdog tick instead of
        // waiting out the MPV_MISSING_GRACE window-title timeout.
        let player_dead = session
            .player
            .as_mut()
            .map(|p| !matches!(p.child.try_wait(), Ok(None)))
            .unwrap_or(false);
        // Natural offline goodbye: keep mpv up until offline_until (unless the
        // user closed the player window themselves).
        let in_offline_grace = session
            .offline_until
            .map(|t| Instant::now() < t)
            .unwrap_or(false);
        if in_offline_grace && !player_dead {
            if child_dead {
                session.info.running = false;
                session.info.phase = "ended".into();
            }
            continue;
        }
        // The window-title lookup is a heuristic and can produce false
        // negatives (renamed window, Unicode title, DWM timing). Never kill a
        // stream on a single miss: require the player window to be missing
        // continuously for MPV_MISSING_GRACE before treating it as closed.
        let window_missing = session_title_scan_needed(session.player.is_some())
            && session.info.ready
            && session
                .ready_at
                .map(|t| t.elapsed() > Duration::from_secs(8))
                .unwrap_or(false)
            && !mpv_window_alive(&session.info.channel);
        let mpv_gone = if window_missing {
            let since = session.mpv_missing_since.get_or_insert_with(Instant::now);
            since.elapsed() > MPV_MISSING_GRACE
        } else {
            session.mpv_missing_since = None;
            false
        };
        if child_dead || player_dead || mpv_gone {
            session.info.running = false;
            session.info.phase = "ended".into();
            if session.info.status.is_empty() {
                session.info.status = "Stopped".into();
            }
            remove.push((id.clone(), session.info.channel.clone()));
        }
    }
    if remove.is_empty() {
        return Ok(false);
    }
    for (id, channel) in &remove {
        let mut keep_player = false;
        if let Some(mut session) = map.remove(id) {
            // Natural end: honor --player-no-close and leave a pre-launched
            // player running (it becomes an unowned window of the user).
            keep_player = session.player.as_ref().is_some_and(|p| p.no_close);
            let _ = session.child.kill();
            let _ = session.child.wait();
            // Kill the whole tree (orphaned player included) via the job.
            terminate_job(&mut session.job);
            if keep_player {
                session.player = None;
            } else {
                close_fast_player(&mut session.player, true);
            }
        }
        if !keep_player {
            close_player_windows_for_channel(channel);
        }
    }
    if map.is_empty() {
        drop(map);
        close_owned_chatterino();
        crate::dock::clear_session();
    } else {
        let remaining: Vec<String> = map
            .values()
            .map(|session| session.info.channel.to_ascii_lowercase())
            .collect();
        drop(map);
        crate::dock::drop_closed_channels(&remaining);
        relaunch_dock_chatterino(&remaining);
    }
    Ok(true)
}

/// Background poll so closing mpv updates sessions without waiting for the UI refresh.
pub fn start_session_watchdog(app: AppHandle, state: SharedStreaming) {
    thread::spawn(move || loop {
        let count = state.inner.lock().map(|map| map.len()).unwrap_or(0);
        let timeout_ms = session_watchdog_timeout_ms(count);
        #[cfg(windows)]
        wait_session_processes(&state, timeout_ms as u32);
        #[cfg(not(windows))]
        thread::sleep(Duration::from_millis(timeout_ms));
        match prune_dead_sessions(&state) {
            Ok(true) => {
                let _ = app.emit("stream-sessions-changed", ());
            }
            Ok(false) => {}
            Err(_) => {}
        }
        // Opt-in: nudge mpv speed when a session has drifted behind live.
        apply_catch_up(&state);
    });
}
