// mpv command line construction, sanitizing, and window-title matching.

/// Dock arg parts for mpv, shared by the classic --player-args string and the
/// fast-start path, which spawns mpv directly with an argv vector.
/// Branded loading image used for the offline goodbye screen only.
/// Written to the temp dir once. Do not use as the initial fast-start media —
/// starting on a silent image breaks audio when the live stream attaches.
fn loading_image_path() -> Option<PathBuf> {
    static BYTES: &[u8] = include_bytes!("../../../../assets/loading.png");
    let path = std::env::temp_dir().join("rillmux-loading.png");
    match std::fs::metadata(&path) {
        Ok(m) if m.len() as usize == BYTES.len() => Some(path),
        _ => std::fs::write(&path, BYTES).ok().map(|_| path),
    }
}

fn mpv_dock_arg_parts(
    channel: &str,
    stream_title: &str,
    reserve_chat: bool,
    preset_args: &str,
    index: usize,
    count: usize,
    layout: Option<&str>,
) -> Vec<String> {
    let geo = mpv_geometry_for_dock(reserve_chat, index, count, layout)
        .unwrap_or_else(|| "--geometry=82%x100%+0+0".into());
    let mut parts: Vec<String> = vec![
        // Geometry first; watch-later-options-clr stops mpv restoring an old window size.
        geo,
        "--force-window=yes".into(),
        // Stay open on EOF so we can show the offline screen; we quit via IPC.
        "--keep-open=yes".into(),
        "--no-border".into(),
        // Live: don't fill a demuxer cache before showing the first frame.
        "--cache=no".into(),
        "--demuxer-readahead-secs=0.5".into(),
        "--watch-later-options-clr".into(),
        // Never inherit a muted watch-later / conf default.
        "--mute=no".into(),
    ];
    // Options the dock owns; matching preset flags are dropped. Everything
    // else the user configured (loop-*, demuxer cache, custom extras, …) is
    // kept — silently discarding it made dock mode diverge from the settings.
    // mpv is last-one-wins for repeated options, so a preset --cache=yes
    // still overrides our --cache=no default above.
    const DOCK_OWNED: &[&str] = &[
        "--geometry",
        "--window-maximized",
        "--title",
        "--force-media-title",
        "--force-window",
        "--keep-open",
        "--no-border",
        "--watch-later-options-clr",
        "--mute",
    ];
    for p in rebuild_player_args_preserving_quotes(preset_args) {
        let key = p.split('=').next().unwrap_or(p.as_str());
        if DOCK_OWNED.contains(&key) {
            continue;
        }
        if !parts.iter().any(|x| x == &p) {
            parts.push(p);
        }
    }
    // Unique title so Win32 can find this mpv window (not a browser tab named after the channel).
    let title = mpv_window_title(channel, stream_title);
    parts.push(format!("--title={title}"));
    parts.push(format!("--force-media-title={title}"));
    // Last-one-wins: keep audible even if a custom extra tried to mute.
    parts.push("--mute=no".into());
    parts
}

fn build_mpv_dock_args(
    channel: &str,
    stream_title: &str,
    reserve_chat: bool,
    preset_args: &str,
    index: usize,
    count: usize,
    layout: Option<&str>,
) -> String {
    mpv_dock_arg_parts(
        channel,
        stream_title,
        reserve_chat,
        preset_args,
        index,
        count,
        layout,
    )
    .join(" ")
}

fn sanitize_player_channel(channel: &str) -> String {
    let ch = channel
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || *c == '_' || *c == '-')
        .collect::<String>();
    if ch.is_empty() {
        "stream".into()
    } else {
        ch.to_ascii_lowercase()
    }
}

fn sanitize_player_fragment(value: &str) -> String {
    let mut out = String::new();
    let mut prev_sep = false;
    for c in value.chars() {
        if c.is_ascii_alphanumeric() || c == '_' || c == '-' {
            out.push(c.to_ascii_lowercase());
            prev_sep = false;
        } else if !out.is_empty() && !prev_sep {
            out.push('_');
            prev_sep = true;
        }
    }
    while out.ends_with('_') || out.ends_with('-') {
        out.pop();
    }
    if out.len() > 80 {
        out.truncate(80);
        while out.ends_with('_') || out.ends_with('-') {
            out.pop();
        }
    }
    if out.is_empty() {
        "stream".into()
    } else {
        out
    }
}

fn mpv_window_title(channel: &str, stream_title: &str) -> String {
    format!(
        "{}-{}",
        sanitize_player_channel(channel),
        sanitize_player_fragment(stream_title)
    )
}

fn legacy_mpv_window_title(channel: &str) -> String {
    format!(
        "{PLAYER_WINDOW_PREFIX_LEGACY}-{}",
        sanitize_player_channel(channel)
    )
}

fn prefixed_mpv_window_title(channel: &str) -> String {
    format!(
        "{PLAYER_WINDOW_PREFIX}-{}",
        sanitize_player_channel(channel)
    )
}

fn player_window_title_matches(title: &str, channel: &str) -> bool {
    let lower = title.to_ascii_lowercase();
    let ch = sanitize_player_channel(channel);
    lower.starts_with(&format!("{ch}-"))
        || lower.starts_with(&prefixed_mpv_window_title(channel))
        || lower.starts_with(&legacy_mpv_window_title(channel))
}

pub(crate) fn player_channel_for_title(title: &str, channels: &[String]) -> Option<String> {
    channels
        .iter()
        .find(|channel| player_window_title_matches(title, channel))
        .cloned()
}

pub(crate) fn player_layout_emit_due(
    last: Option<Instant>,
    now: Instant,
    min_gap: Duration,
    force: bool,
) -> bool {
    force || last.is_none_or(|prev| now.saturating_duration_since(prev) >= min_gap)
}

pub(crate) const EVENT_SYSTEM_MOVESIZEEND: u32 = 0x000B;
pub(crate) const EVENT_OBJECT_LOCATIONCHANGE: u32 = 0x800B;
pub(crate) const OBJID_WINDOW: i32 = 0;
pub(crate) const CHILDID_SELF: i32 = 0;
