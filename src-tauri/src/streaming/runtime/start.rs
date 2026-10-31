// Stream startup: Streamlink process, mpv spawn, and session registration.

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
    let auto_catch_up = req.auto_catch_up.unwrap_or(false);

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
                let effective_volume = match req.volume_mode.as_deref() {
                    Some(mode) => selected_player_volume(
                        mode,
                        req.volume_custom.unwrap_or(100.0),
                        req.volume_max.unwrap_or(100.0),
                        live_player_volume(state).or_else(remembered_volume),
                        mpv_initial_volume(&preset_player_args),
                    ),
                    None => inherited_or_initial(
                        live_player_volume(state).or_else(remembered_volume),
                        mpv_initial_volume(&preset_player_args),
                    ),
                };
                use_fast = true;
                fast_ctx = Some(Arc::new(FastPlayerCtx {
                    pipe: pipe.clone(),
                    port,
                    player_path: player_path.clone(),
                    fallback_argv: dock_argv,
                    volume: effective_volume,
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
                catch_up: auto_catch_up && player_id == "mpv",
                catch_up_active: false,
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
