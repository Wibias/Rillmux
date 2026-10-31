// Player handoff between stream starts and the streamlink output readers.

fn schedule_handoff(
    app: AppHandle,
    state: SharedStreaming,
    session_id: String,
    replace_ids: Vec<String>,
    handoff_done: Arc<AtomicBool>,
) {
    if replace_ids.is_empty() {
        return;
    }
    if handoff_done.swap(true, Ordering::SeqCst) {
        return;
    }
    thread::spawn(move || {
        // Match StreamLinkerino: brief overlap for a smoother player swap.
        thread::sleep(Duration::from_millis(600));
        for old_id in replace_ids {
            if old_id == session_id {
                continue;
            }
            let _ = stop_stream(&state, &old_id);
        }
        let _ = app.emit("stream-sessions-changed", ());
    });
}

#[allow(clippy::too_many_arguments)]
fn spawn_output_readers(
    app: AppHandle,
    state: SharedStreaming,
    id: String,
    channel: String,
    stdout: impl std::io::Read + Send + 'static,
    stderr: impl std::io::Read + Send + 'static,
    replace_ids: Vec<String>,
    handoff_done: Arc<AtomicBool>,
    fast: Option<Arc<FastPlayerCtx>>,
) {
    let drain = |pipe: Box<dyn std::io::Read + Send>,
                 app: AppHandle,
                 state: SharedStreaming,
                 id: String,
                 channel: String,
                 replace_ids: Vec<String>,
                 handoff_done: Arc<AtomicBool>,
                 fast: Option<Arc<FastPlayerCtx>>,
                 emit_lines: bool| {
        thread::spawn(move || {
            let reader = BufReader::new(pipe);
            for line in reader.lines().map_while(Result::ok) {
                let trimmed = line.trim();
                if trimmed.is_empty() {
                    continue;
                }
                if !emit_lines {
                    continue;
                }
                // Fast start: Streamlink's local HTTP server is up — attach
                // the pre-launched mpv via IPC and mark the session ready.
                if let Some(fx) = fast.as_ref() {
                    let url = format!("http://127.0.0.1:{}/", fx.port);
                    if trimmed.contains(&url) && !fx.fired.swap(true, Ordering::SeqCst) {
                        let fx = fx.clone();
                        let app2 = app.clone();
                        let state2 = state.clone();
                        let id2 = id.clone();
                        let channel2 = channel.clone();
                        let replace2 = replace_ids.clone();
                        let handoff2 = handoff_done.clone();
                        thread::spawn(move || {
                            // Clear any prior idle/loading playlist state, then
                            // attach the live HTTP stream as a fresh demuxer.
                            let _ =
                                mpv_ipc_command(&fx.pipe, &["stop"], Duration::from_millis(800));
                            let attached = mpv_ipc_command(
                                &fx.pipe,
                                &["loadfile", &url, "replace"],
                                Duration::from_secs(5),
                            )
                            .is_ok();
                            if attached {
                                // Image/idle sessions leave mute/aid in a bad
                                // state (speaker "!"); force audible playback.
                                mpv_ensure_audible(&fx.pipe, fx.volume);
                                // Clear the loading-phase show-text now that
                                // video frames are on screen.
                                let _ = mpv_ipc_command(
                                    &fx.pipe,
                                    &["show-text", "", "1"],
                                    Duration::from_secs(2),
                                );
                            }
                            if !attached {
                                // Fallback: spawn mpv with the URL directly
                                // (no IPC). Title-based closing still finds it.
                                let _ = Command::new(&fx.player_path)
                                    .args(&fx.fallback_argv)
                                    .arg(&url)
                                    .stdin(Stdio::null())
                                    .spawn();
                            }
                            let status = "Playing".to_string();
                            update_session_status(&state2, &id2, &status, "ready", true);
                            emit_status(
                                &app2,
                                StreamStatusPayload {
                                    id: id2.clone(),
                                    channel: channel2,
                                    line: "Starting player: mpv (fast start)".into(),
                                    status,
                                    phase: "ready".into(),
                                    ready: true,
                                },
                            );
                            schedule_handoff(app2, state2, id2, replace2, handoff2);
                        });
                        continue;
                    }
                }
                let status = display_status(trimmed);
                let (phase, ready) = classify_line(trimmed);
                let already_ready = state
                    .inner
                    .lock()
                    .ok()
                    .and_then(|map| map.get(&id).map(|session| session.info.ready))
                    .unwrap_or(false);
                if !should_forward_status(already_ready, phase, ready) {
                    continue;
                }
                // Mirror loading phases ("Waiting for pre-roll ads…",
                // resolving, errors) onto the idle player's OSD — show-text
                // repaints immediately and replaces the previous message.
                if let Some(fx) = fast.as_ref() {
                    if !fx.fired.load(Ordering::SeqCst) && phase != "info" {
                        if let Ok(mut last) = fx.osd.lock() {
                            if *last != status {
                                *last = status.clone();
                                let pipe = fx.pipe.clone();
                                let msg = status.clone();
                                thread::spawn(move || {
                                    let _ = mpv_ipc_command(
                                        &pipe,
                                        &["show-text", msg.as_str(), "600000"],
                                        Duration::from_secs(2),
                                    );
                                });
                            }
                        }
                    }
                }
                update_session_status(&state, &id, &status, phase, ready);
                emit_status(
                    &app,
                    StreamStatusPayload {
                        id: id.clone(),
                        channel: channel.clone(),
                        line: trimmed.to_string(),
                        status: status.clone(),
                        phase: phase.to_string(),
                        ready,
                    },
                );
                if ready {
                    schedule_handoff(
                        app.clone(),
                        state.clone(),
                        id.clone(),
                        replace_ids.clone(),
                        handoff_done.clone(),
                    );
                }
            }
            // Streamlink closed its pipes: the stream ended or it died.
            if let Some(fx) = fast.as_ref() {
                if fx.no_close {
                    // Leave the pre-launched player; prune the Streamlink session.
                } else if !fx.fired.load(Ordering::SeqCst) {
                    // Never attached playback — nothing to show; close now.
                    close_session_player(&state, &id, true);
                } else if !fx.goodbye.swap(true, Ordering::SeqCst) {
                    // Show branded offline screen for a few seconds, then quit.
                    begin_offline_goodbye(
                        app.clone(),
                        state.clone(),
                        id.clone(),
                        channel.clone(),
                        fx.pipe.clone(),
                    );
                    return;
                } else {
                    // Sibling drain thread already started goodbye.
                    return;
                }
            }
            // Prune right away (closes the owned Chatterino) instead of
            // waiting for the watchdog tick. Give the process handle a
            // moment to signal exit after its pipes closed.
            thread::sleep(Duration::from_millis(200));
            if let Ok(true) = prune_dead_sessions(&state) {
                let _ = app.emit("stream-sessions-changed", ());
            }
        });
    };

    // Streamlink 8.x on Windows writes CLI logs to stdout (stderr is empty).
    // Parse both so "Starting player" marks the session ready.
    drain(
        Box::new(stdout),
        app.clone(),
        state.clone(),
        id.clone(),
        channel.clone(),
        replace_ids.clone(),
        handoff_done.clone(),
        fast.clone(),
        true,
    );
    drain(
        Box::new(stderr),
        app,
        state,
        id,
        channel,
        replace_ids,
        handoff_done,
        fast,
        true,
    );
}

/// Grab a free loopback port for Streamlink's external HTTP server.
fn free_loopback_port() -> Result<u16, StreamError> {
    let listener = std::net::TcpListener::bind("127.0.0.1:0")?;
    Ok(listener.local_addr()?.port())
}
