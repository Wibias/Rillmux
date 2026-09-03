fn strip_ansi(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let mut chars = input.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\u{1b}' {
            if chars.peek() == Some(&'[') {
                chars.next();
                for n in chars.by_ref() {
                    if n.is_ascii_alphabetic() {
                        break;
                    }
                }
            }
            continue;
        }
        out.push(c);
    }
    out
}

fn display_status(raw: &str) -> String {
    let cleaned = strip_ansi(raw).trim().to_string();
    // Drop Streamlink log prefixes like [cli][info]
    let mut s = cleaned.as_str();
    while s.starts_with('[') {
        if let Some(end) = s.find(']') {
            s = s[end + 1..].trim_start();
        } else {
            break;
        }
    }
    if s.is_empty() {
        cleaned
    } else {
        s.to_string()
    }
}

/// After the player is up, HLS playlist noise must not wake the UI.
fn should_forward_status(already_ready: bool, phase: &str, _ready: bool) -> bool {
    match phase {
        "ended" | "error" | "ads" => true,
        "ready" => !already_ready,
        _ => !already_ready,
    }
}

fn classify_line(line: &str) -> (&'static str, bool) {
    let lower = line.to_lowercase();
    if lower.contains("pre-roll ads") {
        ("ads", false)
    } else if is_fatal_streamlink_error(line) {
        ("error", false)
    } else if lower.contains("player:")
        || lower.contains("starting player")
        || lower.contains("writing to player")
    {
        // Ready = the player process actually started. "Opening stream" only
        // means Streamlink began fetching — it must NOT mark the session
        // ready (layout, handoff and the missing-window grace all key off it).
        ("ready", true)
    } else if lower.contains("opening stream")
        || lower.contains("available streams")
        || lower.contains("found matching plugin")
    {
        ("starting", false)
    } else {
        ("info", false)
    }
}

fn is_fatal_streamlink_error(line: &str) -> bool {
    let lower = line.to_lowercase();
    if lower.contains("[cli][error]") {
        return true;
    }
    let display = display_status(line).to_lowercase();
    display.starts_with("error:") || display.starts_with("error ")
}

fn update_session_status(state: &StreamingState, id: &str, status: &str, phase: &str, ready: bool) {
    if let Ok(mut map) = state.inner.lock() {
        if let Some(session) = map.get_mut(id) {
            // HLS "Low latency streaming…" / playlist reload errors must not
            // replace Playing or clear ready after the player has started.
            if session.info.ready && !ready && phase != "ended" {
                return;
            }
            session.info.status = status.to_string();
            session.info.phase = phase.to_string();
            if ready && !session.info.ready {
                session.ready_at = Some(Instant::now());
            }
            session.info.ready = ready;
        }
    }
}

fn emit_status(app: &AppHandle, payload: StreamStatusPayload) {
    let _ = app.emit("stream-status", payload);
}

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
                                mpv_ensure_audible(&fx.pipe);
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

pub fn start_stream(
    app: &AppHandle,
    state: &SharedStreaming,
    req: LaunchRequest,
) -> Result<StreamSession, StreamError> {
    let channel = req.channel.trim().trim_start_matches('#').to_lowercase();
    if channel.is_empty() {
        return Err(StreamError::Message("channel is empty".into()));
    }
    // Twitch logins: 1–25 chars of [a-z0-9_]. Reject everything else so the
    // value is always safe to embed in URLs, window titles and player args.
    if channel.len() > 25
        || !channel
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
    {
        return Err(StreamError::Message(format!(
            "invalid channel name: {channel}"
        )));
    }

    let quality = req
        .quality
        .filter(|q| !q.is_empty())
        .unwrap_or_else(|| "best".into());
    // Quality is passed as a bare CLI argument to Streamlink; restrict it to
    // selector characters (e.g. "best", "720p60", "1080p,720p,best") so a
    // malformed settings value can never be interpreted as a flag.
    if quality.len() > 64
        || !quality
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, ',' | '_' | '+' | '-'))
    {
        return Err(StreamError::Message(format!(
            "invalid quality selector: {quality}"
        )));
    }
    let source = req.streamlink_source.as_deref().unwrap_or("bundled");
    let player_id = req.player_id.as_deref().unwrap_or("mpv");

    let (streamlink, _source_label) =
        resolve_streamlink(source, req.streamlink_custom_path.as_deref())?;
    let player = resolve_player(player_id, req.player_custom_path.as_deref())?;

    let title = req.title.clone().unwrap_or_else(|| channel.clone());
    let game = req.game.clone().unwrap_or_default();

    // Fast start (Windows + mpv + stdin pipe): pre-launch mpv idle so the
    // window appears immediately, then serve the stream through Streamlink's
    // loopback HTTP server and attach playback via mpv's IPC pipe (measured:
    // window at ~0.4 s instead of ~2.3 s after clicking watch).
    let reserve_chat = req.reserve_chat.unwrap_or(false);
    let slot_index = req.slot_index.unwrap_or(0) as usize;
    let slot_count = req.slot_count.unwrap_or(1) as usize;
    let preset_player_args = req
        .player_custom_args
        .clone()
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| default_player_args(player_id, &channel, &title, &game));
    let mut fast_player: Option<FastPlayer> = None;
    let mut fast_ctx: Option<Arc<FastPlayerCtx>> = None;
    let mut use_fast = false;
    let mut fast_pid: Option<u32> = None;
    #[cfg(windows)]
    if player_id == "mpv" && req.player_input.as_deref().unwrap_or("default") == "default" {
        if let Some(player_path) = &player {
            let no_close = req.player_no_close.unwrap_or(false);
            let port = free_loopback_port()?;
            let pipe = format!(r"\\.\pipe\rillmux-mpv-{}", Uuid::new_v4().simple());
            let dock_argv = mpv_dock_arg_parts(
                &channel,
                &title,
                reserve_chat,
                &preset_player_args,
                slot_index,
                slot_count,
                req.layout.as_deref(),
            );
            let mut idle_argv = dock_argv.clone();
            idle_argv.push("--idle=yes".into());
            idle_argv.push(format!("--input-ipc-server={pipe}"));
            // Do NOT play loading.png as the first media file: an image has no
            // audio track, and mpv can then fail to attach sound when loadfile
            // replaces it with the live stream (speaker shows "!"). Branding
            // is the OSD show-text below on a dark idle window instead.
            idle_argv.push("--force-window=immediate".into());
            if let Ok(mpv_child) = Command::new(player_path)
                .args(&idle_argv)
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()
            {
                let job = assign_job(&mpv_child);
                fast_pid = Some(mpv_child.id());
                // Idle window is otherwise empty until the stream attaches.
                // show-text repaints immediately; osd-msg1 does not.
                let osd_pipe = pipe.clone();
                let osd_msg = format!("Starting {channel}…");
                thread::spawn(move || {
                    let _ = mpv_ipc_command(
                        &osd_pipe,
                        &["show-text", osd_msg.as_str(), "600000"],
                        Duration::from_secs(3),
                    );
                });
                use_fast = true;
                fast_ctx = Some(Arc::new(FastPlayerCtx {
                    pipe: pipe.clone(),
                    port,
                    player_path: player_path.clone(),
                    fallback_argv: dock_argv,
                    fired: Arc::new(AtomicBool::new(false)),
                    goodbye: Arc::new(AtomicBool::new(false)),
                    osd: Mutex::new(String::new()),
                    no_close,
                }));
                fast_player = Some(FastPlayer {
                    child: mpv_child,
                    job,
                    pipe,
                    no_close,
                });
            }
        }
    }

    let streamlink_auth_config = match crate::twitch_web_auth::streamlink_auth_config() {
        Ok(config) => config,
        Err(error) => {
            close_fast_player(&mut fast_player, false);
            return Err(StreamError::Message(error.to_string()));
        }
    };
    let mut args: Vec<String> = Vec::new();
    if let Some(config) = streamlink_auth_config.as_ref() {
        args.push("--config".into());
        args.push(config.path().to_string_lossy().into_owned());
    }
    if req.low_latency.unwrap_or(false) {
        args.push("--twitch-low-latency".into());
    }
    if req.disable_ads.unwrap_or(false) {
        args.push("--twitch-disable-ads".into());
    }
    if !use_fast {
        match req.player_input.as_deref().unwrap_or("default") {
            "fifo" => args.push("--player-fifo".into()),
            "http" => args.push("--player-continuous-http".into()),
            // "default" = stdin pipe (recommended). Passthrough is intentionally unsupported.
            _ => {}
        }
    }
    if req.webbrowser.unwrap_or(false) {
        args.push("--webbrowser".into());
        args.push("yes".into());
        if req.webbrowser_headless.unwrap_or(true) {
            args.push("--webbrowser-headless".into());
            args.push("yes".into());
        }
        if let Some(exec) = req
            .webbrowser_executable
            .as_deref()
            .filter(|s| !s.is_empty())
        {
            args.push("--webbrowser-executable".into());
            args.push(exec.to_string());
        }
    } else {
        args.push("--webbrowser".into());
        args.push("no".into());
    }
    if let Some(delay) = req.retry_streams {
        args.push("--retry-streams".into());
        args.push(delay.to_string());
    }
    if let Some(max) = req.retry_max {
        args.push("--retry-max".into());
        args.push(max.to_string());
    }
    if req.player_no_close.unwrap_or(false) {
        args.push("--player-no-close".into());
    }
    // Parallel segment fetch + stream data as it arrives (faster first frame).
    args.push("--stream-segment-threads".into());
    args.push("3".into());
    args.push("--hls-segment-stream-data".into());
    if use_fast {
        // Serve the stream on loopback HTTP; the pre-launched mpv attaches
        // via IPC once the watcher sees the printed URL.
        let port = fast_ctx.as_ref().map(|fx| fx.port).unwrap_or(0);
        args.push("--player-external-http".into());
        // Loopback only — never expose the stream on the network.
        args.push("--player-external-http-interface".into());
        args.push("127.0.0.1".into());
        args.push("--player-external-http-port".into());
        args.push(port.to_string());
        // Exit when the stream ends so the player can be cleaned up.
        args.push("--player-external-http-continuous".into());
        args.push("no".into());
    } else {
        // Keep Streamlink's own title short so it doesn't override our mpv --title=.
        args.push("--title".into());
        args.push(mpv_window_title(&channel, &title));
        if let Some(player_path) = &player {
            args.push("--player".into());
            args.push(player_path.to_string_lossy().to_string());
            let player_args = if player_id == "mpv" {
                build_mpv_dock_args(
                    &channel,
                    &title,
                    reserve_chat,
                    &preset_player_args,
                    slot_index,
                    slot_count,
                    req.layout.as_deref(),
                )
            } else {
                preset_player_args.clone()
            };
            if !player_args.is_empty() {
                args.push("--player-args".into());
                args.push(player_args);
            }
        }
    }
    args.push(format!("twitch.tv/{channel}"));
    args.push(quality.clone());

    let replace_existing = req.replace_existing.unwrap_or(false);
    let replace_ids = if replace_existing {
        let map = state
            .inner
            .lock()
            .map_err(|_| StreamError::Message("streaming state poisoned".into()))?;
        map.keys().cloned().collect::<Vec<_>>()
    } else {
        Vec::new()
    };

    let mut child_cmd = Command::new(&streamlink);
    child_cmd
        .args(&args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        child_cmd.creation_flags(CREATE_NO_WINDOW);
    }
    let mut child = match child_cmd.spawn() {
        Ok(c) => c,
        Err(e) => {
            // Don't leave the pre-launched idle player behind.
            close_fast_player(&mut fast_player, false);
            return Err(e.into());
        }
    };

    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| StreamError::Message("failed to capture Streamlink stdout".into()))?;
    let stderr = child
        .stderr
        .take()
        .ok_or_else(|| StreamError::Message("failed to capture Streamlink stderr".into()))?;

    // Chatterino is opened from the frontend (`open_chatterino_chat`) so failures
    // surface in the UI. This worker only starts Streamlink/mpv.
    let id = Uuid::new_v4().to_string();
    let initial_status = if replace_ids.is_empty() {
        "Starting Streamlink…".to_string()
    } else {
        format!("Switching to {channel}…")
    };

    let info = StreamSession {
        id: id.clone(),
        channel: channel.clone(),
        quality,
        title: req.title,
        game: req.game,
        running: true,
        status: initial_status.clone(),
        phase: "starting".into(),
        ready: false,
        muted: false,
    };

    let handoff_done = Arc::new(AtomicBool::new(false));
    {
        // Put the child in a kill-job BEFORE inserting: the player it spawns
        // joins the job, so stop/prune can kill the whole tree.
        let job = assign_job(&child);
        let mut map = state
            .inner
            .lock()
            .map_err(|_| StreamError::Message("streaming state poisoned".into()))?;
        map.insert(
            id.clone(),
            LiveSession {
                info: info.clone(),
                child,
                job,
                _streamlink_auth_config: streamlink_auth_config,
                player: fast_player,
                ready_at: None,
                mpv_missing_since: None,
                offline_until: None,
            },
        );
    }

    // Close the session (and with it the owned Chatterino) the instant the
    // pre-launched player process exits, e.g. the user closed its window.
    #[cfg(windows)]
    if let Some(pid) = fast_pid {
        watch_player_exit(pid, state.clone(), app.clone());
    }

    spawn_output_readers(
        app.clone(),
        state.clone(),
        id.clone(),
        channel.clone(),
        stdout,
        stderr,
        replace_ids,
        handoff_done,
        fast_ctx,
    );

    emit_status(
        app,
        StreamStatusPayload {
            id: info.id.clone(),
            channel: info.channel.clone(),
            line: initial_status.clone(),
            status: initial_status,
            phase: "starting".into(),
            ready: false,
        },
    );

    Ok(info)
}

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
    });
}

#[cfg(windows)]
fn wait_session_processes(state: &StreamingState, timeout_ms: u32) {
    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn CloseHandle(handle: *mut core::ffi::c_void) -> i32;
        fn DuplicateHandle(
            source_process: *mut core::ffi::c_void,
            source: *mut core::ffi::c_void,
            target_process: *mut core::ffi::c_void,
            target: *mut *mut core::ffi::c_void,
            desired: u32,
            inherit: i32,
            options: u32,
        ) -> i32;
        fn GetCurrentProcess() -> *mut core::ffi::c_void;
        fn WaitForMultipleObjects(
            count: u32,
            handles: *const *mut core::ffi::c_void,
            wait_all: i32,
            millis: u32,
        ) -> u32;
    }
    const DUPLICATE_SAME_ACCESS: u32 = 0x0000_0002;
    let duplicates: Vec<*mut core::ffi::c_void> = {
        let Ok(map) = state.inner.lock() else {
            thread::sleep(Duration::from_millis(timeout_ms as u64));
            return;
        };
        let process = unsafe { GetCurrentProcess() };
        let mut duplicates = Vec::new();
        let mut push_dup = |source: *mut core::ffi::c_void| {
            if source.is_null() {
                return;
            }
            let mut dup = std::ptr::null_mut();
            let ok = unsafe {
                DuplicateHandle(
                    process,
                    source,
                    process,
                    &mut dup,
                    0,
                    0,
                    DUPLICATE_SAME_ACCESS,
                )
            };
            if ok != 0 && !dup.is_null() {
                duplicates.push(dup);
            }
        };
        for session in map.values() {
            push_dup(session.child.as_raw_handle());
            if let Some(player) = &session.player {
                push_dup(player.child.as_raw_handle());
            }
        }
        duplicates
    };
    if duplicates.is_empty() {
        thread::sleep(Duration::from_millis(timeout_ms as u64));
        return;
    }
    let count = duplicates.len().min(64) as u32;
    unsafe {
        WaitForMultipleObjects(count, duplicates.as_ptr(), 0, timeout_ms);
        for handle in duplicates {
            CloseHandle(handle);
        }
    }
}

fn mpv_window_alive(channel: &str) -> bool {
    #[cfg(windows)]
    {
        find_player_window(channel).is_some()
    }
    #[cfg(not(windows))]
    {
        let _ = channel;
        true
    }
}

fn wait_child_timeout(child: &mut Child, timeout: Duration) {
    let deadline = Instant::now() + timeout;
    loop {
        match child.try_wait() {
            Ok(Some(_)) => return,
            Ok(None) if Instant::now() < deadline => {
                thread::sleep(Duration::from_millis(50));
            }
            _ => {
                let _ = child.kill();
                let _ = child.try_wait();
                return;
            }
        }
    }
}

pub fn stop_stream(state: &StreamingState, id: &str) -> Result<(), StreamError> {
    let session = {
        let mut map = state
            .inner
            .lock()
            .map_err(|_| StreamError::Message("streaming state poisoned".into()))?;
        map.remove(id)
    };
    if let Some(mut session) = session {
        let channel = session.info.channel.clone();
        let _ = session.child.kill();
        wait_child_timeout(&mut session.child, Duration::from_secs(2));
        // Streamlink exit leaves the player orphaned; with --loop-file=inf it
        // keeps replaying the buffer instead of closing. The job kills the
        // whole tree; title-based closing is the fallback.
        terminate_job(&mut session.job);
        // Explicit stop always closes a pre-launched player (no_close only
        // applies to natural stream ends).
        close_fast_player(&mut session.player, true);
        close_player_windows_for_channel(&channel);
    }
    let remaining: Vec<String> = state
        .inner
        .lock()
        .ok()
        .map(|map| {
            map.values()
                .map(|session| session.info.channel.to_ascii_lowercase())
                .collect()
        })
        .unwrap_or_default();
    if remaining.is_empty() {
        close_owned_chatterino();
        crate::dock::clear_session();
    } else {
        crate::dock::drop_closed_channels(&remaining);
        relaunch_dock_chatterino(&remaining);
    }
    Ok(())
}

/// Restart owned Chatterino onto the remaining `--channels=` split. Apply only
/// retiles mpv; leftover tabs stay until this relaunch. Off the caller thread
/// because RestartOwned waits for the previous Qt process to exit.
fn relaunch_dock_chatterino(remaining: &[String]) {
    if remaining.is_empty() || !crate::dock::snapshot().reserve_chat {
        return;
    }
    let channels = remaining.to_vec();
    thread::spawn(move || {
        let _ = launch_chatterino_for_channels(&channels);
    });
}

pub fn stop_all(state: &StreamingState) -> Result<(), StreamError> {
    let sessions: Vec<_> = {
        let mut map = state
            .inner
            .lock()
            .map_err(|_| StreamError::Message("streaming state poisoned".into()))?;
        map.drain().map(|(_, session)| session).collect()
    };
    let channels: Vec<String> = sessions
        .iter()
        .map(|session| session.info.channel.clone())
        .collect();
    for mut session in sessions {
        let _ = session.child.kill();
        wait_child_timeout(&mut session.child, Duration::from_secs(2));
        terminate_job(&mut session.job);
        close_fast_player(&mut session.player, true);
    }
    for channel in channels {
        close_player_windows_for_channel(&channel);
    }
    close_owned_chatterino();
    crate::dock::clear_session();
    Ok(())
}

/// Close mpv/VLC windows whose title starts with the channel name.
fn close_player_windows_for_channel(channel: &str) {
    #[cfg(windows)]
    {
        close_player_windows_for_channel_windows(channel);
    }
    #[cfg(not(windows))]
    {
        let _ = channel;
    }
}

#[cfg(windows)]
fn close_player_windows_for_channel_windows(channel: &str) {
    use std::sync::Mutex;

    #[link(name = "user32")]
    unsafe extern "system" {
        fn EnumWindows(
            cb: unsafe extern "system" fn(*mut core::ffi::c_void, isize) -> i32,
            lparam: isize,
        ) -> i32;
        fn GetWindowTextW(hwnd: *mut core::ffi::c_void, lp: *mut u16, n: i32) -> i32;
        fn GetWindowThreadProcessId(hwnd: *mut core::ffi::c_void, pid: *mut u32) -> u32;
        fn IsWindowVisible(hwnd: *mut core::ffi::c_void) -> i32;
        fn PostMessageW(hwnd: *mut core::ffi::c_void, msg: u32, w: usize, l: isize) -> i32;
    }
    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn OpenProcess(access: u32, inherit: i32, pid: u32) -> *mut core::ffi::c_void;
        fn TerminateProcess(handle: *mut core::ffi::c_void, code: u32) -> i32;
        fn CloseHandle(handle: *mut core::ffi::c_void) -> i32;
    }

    const WM_CLOSE: u32 = 0x0010;
    const PROCESS_TERMINATE: u32 = 0x0001;

    struct Data {
        channel: String,
        pids: Mutex<Vec<u32>>,
    }

    unsafe extern "system" fn enum_cb(hwnd: *mut core::ffi::c_void, lparam: isize) -> i32 {
        let data = &*(lparam as *const Data);
        if IsWindowVisible(hwnd) == 0 {
            return 1;
        }
        let mut buf = [0u16; 512];
        let n = GetWindowTextW(hwnd, buf.as_mut_ptr(), buf.len() as i32);
        if n <= 0 {
            return 1;
        }
        let title = String::from_utf16_lossy(&buf[..n as usize]);
        // Player windows are titled <channel>-<stream_title>. Older builds used
        // rillmux-<channel> / stgui-<channel>; VLC appends " - VLC media player".
        if !player_window_title_matches(&title, &data.channel) {
            return 1;
        }
        let mut pid = 0u32;
        GetWindowThreadProcessId(hwnd, &mut pid);
        if pid != 0 {
            if let Ok(mut pids) = data.pids.lock() {
                if !pids.contains(&pid) {
                    pids.push(pid);
                }
            }
        }
        // Ask politely first.
        PostMessageW(hwnd, WM_CLOSE, 0, 0);
        1
    }

    let data = Data {
        channel: channel.to_string(),
        pids: Mutex::new(Vec::new()),
    };
    unsafe {
        EnumWindows(enum_cb, &data as *const Data as isize);
    }

    // Give WM_CLOSE a moment, then force-kill remaining processes.
    thread::sleep(Duration::from_millis(250));
    let pids = data.pids.lock().map(|g| g.clone()).unwrap_or_default();
    for pid in pids {
        unsafe {
            let handle = OpenProcess(PROCESS_TERMINATE, 0, pid);
            if !handle.is_null() {
                TerminateProcess(handle, 1);
                CloseHandle(handle);
            }
        }
    }
}

pub type SharedStreaming = Arc<StreamingState>;
