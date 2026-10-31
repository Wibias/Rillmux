/// Dock arg parts for mpv, shared by the classic --player-args string and the
/// fast-start path, which spawns mpv directly with an argv vector.
/// Branded loading image used for the offline goodbye screen only.
/// Written to the temp dir once. Do not use as the initial fast-start media —
/// starting on a silent image breaks audio when the live stream attaches.
fn loading_image_path() -> Option<PathBuf> {
    static BYTES: &[u8] = include_bytes!("../../../assets/loading.png");
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

/// Win32 `POINT`. Field sizes follow `LONG`.
#[repr(C)]
#[allow(dead_code)]
struct Win32Point {
    x: i32,
    y: i32,
}

/// Win32 `MSG` (`tagMSG`), including trailing `DWORD lPrivate`.
/// Do not omit `l_private` and rely on tail padding: 32-bit `MSG` is 32 bytes.
#[repr(C)]
#[allow(dead_code)]
struct Win32Msg {
    hwnd: *mut core::ffi::c_void,
    message: u32,
    wparam: usize,
    lparam: isize,
    time: u32,
    pt: Win32Point,
    l_private: u32,
}

#[cfg(target_pointer_width = "64")]
const _: () = {
    assert!(core::mem::size_of::<Win32Point>() == 8);
    assert!(core::mem::align_of::<Win32Point>() == 4);
    assert!(core::mem::size_of::<Win32Msg>() == 48);
    assert!(core::mem::align_of::<Win32Msg>() == 8);
    assert!(core::mem::offset_of!(Win32Msg, hwnd) == 0);
    assert!(core::mem::offset_of!(Win32Msg, message) == 8);
    assert!(core::mem::offset_of!(Win32Msg, wparam) == 16);
    assert!(core::mem::offset_of!(Win32Msg, lparam) == 24);
    assert!(core::mem::offset_of!(Win32Msg, time) == 32);
    assert!(core::mem::offset_of!(Win32Msg, pt) == 36);
    assert!(core::mem::offset_of!(Win32Msg, l_private) == 44);
};

#[cfg(target_pointer_width = "32")]
const _: () = {
    assert!(core::mem::size_of::<Win32Point>() == 8);
    assert!(core::mem::align_of::<Win32Point>() == 4);
    assert!(core::mem::size_of::<Win32Msg>() == 32);
    assert!(core::mem::align_of::<Win32Msg>() == 4);
    assert!(core::mem::offset_of!(Win32Msg, hwnd) == 0);
    assert!(core::mem::offset_of!(Win32Msg, message) == 4);
    assert!(core::mem::offset_of!(Win32Msg, wparam) == 8);
    assert!(core::mem::offset_of!(Win32Msg, lparam) == 12);
    assert!(core::mem::offset_of!(Win32Msg, time) == 16);
    assert!(core::mem::offset_of!(Win32Msg, pt) == 20);
    assert!(core::mem::offset_of!(Win32Msg, l_private) == 28);
};

/// Top-level window geometry only. `EVENT_OBJECT_LOCATIONCHANGE` also fires for
/// caret/cursor/client objects; those must not touch session state.
pub(crate) fn player_layout_event_relevant(event: u32, id_object: i32, id_child: i32) -> bool {
    if id_child != CHILDID_SELF || id_object != OBJID_WINDOW {
        return false;
    }
    event == EVENT_SYSTEM_MOVESIZEEND || event == EVENT_OBJECT_LOCATIONCHANGE
}

pub(crate) fn player_layout_watch_should_pump(location_hook: bool, movesize_hook: bool) -> bool {
    location_hook || movesize_hook
}

#[cfg(windows)]
fn find_player_window(channel: &str) -> Option<*mut core::ffi::c_void> {
    find_window_by_title(&format!("{}-", sanitize_player_channel(channel)), false)
        .or_else(|| find_window_by_title(&prefixed_mpv_window_title(channel), true))
        .or_else(|| find_window_by_title(&legacy_mpv_window_title(channel), true))
}

/// Split player-args like a shell (keeps "quoted titles" as one token).
fn rebuild_player_args_preserving_quotes(args: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut in_quotes = false;
    for c in args.chars() {
        match c {
            '"' => {
                in_quotes = !in_quotes;
                cur.push(c);
            }
            ch if ch.is_whitespace() && !in_quotes => {
                if !cur.is_empty() {
                    out.push(std::mem::take(&mut cur));
                }
            }
            ch => cur.push(ch),
        }
    }
    if !cur.is_empty() {
        out.push(cur);
    }
    out
}

/// Prefer an exact title match (mpv `--title=channel`), else prefix / contains.
/// Initial `--volume=` from the composed player args — the frontend owns mpv
/// flags, so the opt-in booster's target volume travels inside them. Args
/// without a volume flag (and junk values) mean mpv's normal 100%.
fn mpv_initial_volume(player_args: &str) -> f64 {
    rebuild_player_args_preserving_quotes(player_args)
        .iter()
        .filter_map(|arg| arg.strip_prefix("--volume="))
        .filter_map(|value| value.trim().parse::<f64>().ok())
        .rfind(|value| value.is_finite() && *value > 0.0)
        .unwrap_or(100.0)
}

/// Prefer an exact title match (mpv `--title=channel`), else prefix / contains.
/// Includes minimized windows so dock group min/restore can still find players.
#[cfg(windows)]
fn find_window_by_title(needle: &str, exact_preferred: bool) -> Option<*mut core::ffi::c_void> {
    #[link(name = "user32")]
    unsafe extern "system" {
        fn EnumWindows(
            cb: unsafe extern "system" fn(*mut core::ffi::c_void, isize) -> i32,
            lparam: isize,
        ) -> i32;
        fn IsWindowVisible(hwnd: *mut core::ffi::c_void) -> i32;
        fn IsIconic(hwnd: *mut core::ffi::c_void) -> i32;
        fn GetWindow(hwnd: *mut core::ffi::c_void, cmd: u32) -> *mut core::ffi::c_void;
        fn GetWindowTextW(hwnd: *mut core::ffi::c_void, lp: *mut u16, n: i32) -> i32;
        fn GetWindowRect(hwnd: *mut core::ffi::c_void, rect: *mut WinRect) -> i32;
        fn GetWindowPlacement(hwnd: *mut core::ffi::c_void, place: *mut WindowPlacement) -> i32;
    }
    #[repr(C)]
    struct WindowPlacement {
        length: u32,
        flags: u32,
        show_cmd: u32,
        min_x: i32,
        min_y: i32,
        max_x: i32,
        max_y: i32,
        normal: WinRect,
    }
    const GW_OWNER: u32 = 4;
    struct Data {
        needle: String,
        exact: *mut core::ffi::c_void,
        best: *mut core::ffi::c_void,
        best_area: i64,
    }
    unsafe extern "system" fn enum_cb(hwnd: *mut core::ffi::c_void, lparam: isize) -> i32 {
        let data = &mut *(lparam as *mut Data);
        if !GetWindow(hwnd, GW_OWNER).is_null() {
            return 1;
        }
        let visible = IsWindowVisible(hwnd) != 0;
        let iconic = IsIconic(hwnd) != 0;
        // Minimized windows still report visible; accept either path.
        if !visible && !iconic {
            return 1;
        }
        let mut buf = [0u16; 512];
        let n = GetWindowTextW(hwnd, buf.as_mut_ptr(), buf.len() as i32);
        if n <= 0 {
            return 1;
        }
        let title = String::from_utf16_lossy(&buf[..n as usize]).to_ascii_lowercase();
        // Never steal browser / our own app windows when matching a channel name.
        if title.contains("chrome")
            || title.contains("firefox")
            || title.contains("edge")
            || title.contains("streamlink twitch")
        {
            return 1;
        }
        let is_exact = title == data.needle;
        let is_soft = title.contains(&data.needle);
        if !is_exact && !is_soft {
            return 1;
        }
        // GetWindowRect for iconic windows is often tiny (~taskbar button size).
        let mut area = 0i64;
        if iconic {
            let mut place = WindowPlacement {
                length: std::mem::size_of::<WindowPlacement>() as u32,
                flags: 0,
                show_cmd: 0,
                min_x: 0,
                min_y: 0,
                max_x: 0,
                max_y: 0,
                normal: WinRect {
                    left: 0,
                    top: 0,
                    right: 0,
                    bottom: 0,
                },
            };
            if GetWindowPlacement(hwnd, &mut place) != 0 {
                let r = place.normal;
                area = (r.right - r.left).max(0) as i64 * (r.bottom - r.top).max(0) as i64;
            }
        } else {
            let mut rect = WinRect {
                left: 0,
                top: 0,
                right: 0,
                bottom: 0,
            };
            if GetWindowRect(hwnd, &mut rect) == 0 {
                return 1;
            }
            area = (rect.right - rect.left).max(0) as i64 * (rect.bottom - rect.top).max(0) as i64;
        }
        if area < 20_000 {
            // Exact title + iconic: trust it even when the shell shrinks the
            // visible rect to a taskbar thumbnail size.
            if !(is_exact && iconic) {
                return 1;
            }
        }
        if is_exact {
            data.exact = hwnd;
        }
        if area > data.best_area {
            data.best_area = area;
            data.best = hwnd;
        }
        1
    }
    let mut data = Data {
        needle: needle.to_ascii_lowercase(),
        exact: std::ptr::null_mut(),
        best: std::ptr::null_mut(),
        best_area: 0,
    };
    unsafe {
        EnumWindows(enum_cb, &mut data as *mut _ as isize);
    }
    if exact_preferred && !data.exact.is_null() {
        Some(data.exact)
    } else if !data.best.is_null() {
        Some(data.best)
    } else {
        None
    }
}

/// Place window on `rect`. `expand_dwm` fills invisible Win11 borders (use for Chatterino).
/// Borderless mpv should pass `expand_dwm = false`.
#[cfg(windows)]
fn move_hwnd_to(hwnd: *mut core::ffi::c_void, rect: WinRect, expand_dwm: bool) {
    #[link(name = "user32")]
    unsafe extern "system" {
        fn GetWindowRect(hwnd: *mut core::ffi::c_void, rect: *mut WinRect) -> i32;
        fn MoveWindow(
            hwnd: *mut core::ffi::c_void,
            x: i32,
            y: i32,
            w: i32,
            h: i32,
            repaint: i32,
        ) -> i32;
        fn ShowWindow(hwnd: *mut core::ffi::c_void, cmd: i32) -> i32;
        fn GetWindowLongPtrW(hwnd: *mut core::ffi::c_void, index: i32) -> isize;
        fn SetWindowLongPtrW(hwnd: *mut core::ffi::c_void, index: i32, value: isize) -> isize;
        fn SetWindowPos(
            hwnd: *mut core::ffi::c_void,
            after: *mut core::ffi::c_void,
            x: i32,
            y: i32,
            cx: i32,
            cy: i32,
            flags: u32,
        ) -> i32;
        fn SetThreadDpiAwarenessContext(context: isize) -> isize;
    }
    #[link(name = "dwmapi")]
    unsafe extern "system" {
        fn DwmGetWindowAttribute(
            hwnd: *mut core::ffi::c_void,
            attr: u32,
            pv: *mut core::ffi::c_void,
            size: u32,
        ) -> i32;
    }
    const DWMWA_EXTENDED_FRAME_BOUNDS: u32 = 9;
    const SW_RESTORE: i32 = 9;
    const GWL_STYLE: i32 = -16;
    const WS_MAXIMIZE: isize = 0x0100_0000;
    const WS_THICKFRAME: isize = 0x0004_0000;
    const WS_MAXIMIZEBOX: isize = 0x0001_0000;
    const SWP_NOZORDER: u32 = 0x0004;
    const SWP_NOMOVE: u32 = 0x0002;
    const SWP_NOSIZE: u32 = 0x0001;
    const SWP_FRAMECHANGED: u32 = 0x0020;
    const DPI_CTX: isize = -4;

    let target_w = (rect.right - rect.left).max(1);
    let target_h = (rect.bottom - rect.top).max(1);

    unsafe {
        let _prev = SetThreadDpiAwarenessContext(DPI_CTX);
        let mut style = GetWindowLongPtrW(hwnd, GWL_STYLE);
        if style & WS_MAXIMIZE != 0 {
            style &= !WS_MAXIMIZE;
            SetWindowLongPtrW(hwnd, GWL_STYLE, style);
        }
        // Live dock: strip Chatterino's resize border so the visible frame
        // fills the chat slot (otherwise a gap sits to the right of chat).
        if expand_dwm && style & (WS_THICKFRAME | WS_MAXIMIZEBOX) != 0 {
            style &= !(WS_THICKFRAME | WS_MAXIMIZEBOX);
            SetWindowLongPtrW(hwnd, GWL_STYLE, style);
            SetWindowPos(
                hwnd,
                std::ptr::null_mut(),
                0,
                0,
                0,
                0,
                SWP_NOMOVE | SWP_NOSIZE | SWP_NOZORDER | SWP_FRAMECHANGED,
            );
        }
        ShowWindow(hwnd, SW_RESTORE);

        if !expand_dwm {
            MoveWindow(hwnd, rect.left, rect.top, target_w, target_h, 1);
            return;
        }

        let mut outer = WinRect {
            left: 0,
            top: 0,
            right: 0,
            bottom: 0,
        };
        let mut frame = outer;
        let has_outer = GetWindowRect(hwnd, &mut outer) != 0;
        let has_frame = has_outer
            && DwmGetWindowAttribute(
                hwnd,
                DWMWA_EXTENDED_FRAME_BOUNDS,
                &mut frame as *mut _ as *mut _,
                std::mem::size_of::<WinRect>() as u32,
            ) == 0;

        let (x, y, w, h) = if has_outer && has_frame {
            let bl = frame.left - outer.left;
            let bt = frame.top - outer.top;
            let br = outer.right - frame.right;
            let bb = outer.bottom - frame.bottom;
            (
                rect.left - bl,
                rect.top - bt,
                target_w + bl + br,
                target_h + bt + bb,
            )
        } else {
            (rect.left, rect.top, target_w, target_h)
        };
        MoveWindow(hwnd, x, y, w.max(1), h.max(1), 1);

        if DwmGetWindowAttribute(
            hwnd,
            DWMWA_EXTENDED_FRAME_BOUNDS,
            &mut frame as *mut _ as *mut _,
            std::mem::size_of::<WinRect>() as u32,
        ) == 0
            && GetWindowRect(hwnd, &mut outer) != 0
        {
            let dx = rect.left - frame.left;
            let dy = rect.top - frame.top;
            let dw = rect.right - frame.right;
            let dh = rect.bottom - frame.bottom;
            if dx != 0 || dy != 0 || dw != 0 || dh != 0 {
                MoveWindow(
                    hwnd,
                    outer.left + dx,
                    outer.top + dy,
                    ((outer.right - outer.left) + dw).max(1),
                    ((outer.bottom - outer.top) + dh).max(1),
                    1,
                );
            }
        }
    }
}

/// Keep the poll/prediction overlay above Chatterino. Dock raises chat with
/// HWND_TOP on a timer, which otherwise buries a separate overlay window.
#[cfg(windows)]
fn raise_poll_overlay() {
    let Some(app) = DOCK_APP.get() else {
        return;
    };
    let Some(win) = app.get_webview_window("poll-overlay") else {
        return;
    };
    let Ok(hwnd) = win.hwnd() else {
        return;
    };
    #[link(name = "user32")]
    unsafe extern "system" {
        fn SetWindowPos(
            hwnd: *mut core::ffi::c_void,
            after: *mut core::ffi::c_void,
            x: i32,
            y: i32,
            cx: i32,
            cy: i32,
            flags: u32,
        ) -> i32;
        fn ShowWindow(hwnd: *mut core::ffi::c_void, cmd: i32) -> i32;
    }
    const HWND_TOPMOST: isize = -1;
    const SWP_NOMOVE: u32 = 0x0002;
    const SWP_NOSIZE: u32 = 0x0001;
    const SWP_SHOWWINDOW: u32 = 0x0040;
    const SWP_NOACTIVATE: u32 = 0x0010;
    const SW_SHOWNA: i32 = 8;
    let ptr = hwnd.0;
    if ptr.is_null() {
        return;
    }
    unsafe {
        ShowWindow(ptr, SW_SHOWNA);
        SetWindowPos(
            ptr,
            HWND_TOPMOST as *mut core::ffi::c_void,
            0,
            0,
            0,
            0,
            SWP_NOMOVE | SWP_NOSIZE | SWP_SHOWWINDOW | SWP_NOACTIVATE,
        );
    }
}

#[cfg(not(windows))]
fn raise_poll_overlay() {}

pub fn raise_poll_overlay_window() {
    raise_poll_overlay();
}

#[cfg(windows)]
fn raise_hwnd(hwnd: *mut core::ffi::c_void, foreground: bool) {
    #[link(name = "user32")]
    unsafe extern "system" {
        fn SetWindowPos(
            hwnd: *mut core::ffi::c_void,
            after: *mut core::ffi::c_void,
            x: i32,
            y: i32,
            cx: i32,
            cy: i32,
            flags: u32,
        ) -> i32;
        fn BringWindowToTop(hwnd: *mut core::ffi::c_void) -> i32;
        fn SetForegroundWindow(hwnd: *mut core::ffi::c_void) -> i32;
        fn GetForegroundWindow() -> *mut core::ffi::c_void;
        fn GetWindowThreadProcessId(hwnd: *mut core::ffi::c_void, pid: *mut u32) -> u32;
        fn AttachThreadInput(attach: u32, attach_to: u32, attach_flag: i32) -> i32;
        fn ShowWindow(hwnd: *mut core::ffi::c_void, cmd: i32) -> i32;
    }
    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn GetCurrentThreadId() -> u32;
    }
    const HWND_TOP: isize = 0;
    const SWP_NOMOVE: u32 = 0x0002;
    const SWP_NOSIZE: u32 = 0x0001;
    const SWP_SHOWWINDOW: u32 = 0x0040;
    const SW_SHOW: i32 = 5;
    const SWP_NOACTIVATE: u32 = 0x0010;
    const SW_SHOWNA: i32 = 8;
    if hwnd.is_null() {
        return;
    }
    unsafe {
        ShowWindow(hwnd, if foreground { SW_SHOW } else { SW_SHOWNA });
        SetWindowPos(
            hwnd,
            HWND_TOP as *mut core::ffi::c_void,
            0,
            0,
            0,
            0,
            SWP_NOMOVE | SWP_NOSIZE | SWP_SHOWWINDOW | if foreground { 0 } else { SWP_NOACTIVATE },
        );
        if foreground {
            BringWindowToTop(hwnd);
            let fg = GetForegroundWindow();
            let mut fg_pid = 0u32;
            let fg_tid = GetWindowThreadProcessId(fg, &mut fg_pid);
            let our_tid = GetCurrentThreadId();
            if fg_tid != 0 && fg_tid != our_tid {
                AttachThreadInput(our_tid, fg_tid, 1);
                let _ = SetForegroundWindow(hwnd);
                AttachThreadInput(our_tid, fg_tid, 0);
            } else {
                let _ = SetForegroundWindow(hwnd);
            }
        }
    }
}

#[cfg(windows)]
fn raise_dock_windows(channels: &[String], reserve_chat: bool) {
    raise_dock_windows_inner(channels, reserve_chat, true);
}

#[cfg(windows)]
fn restack_dock_windows(channels: &[String], reserve_chat: bool) {
    raise_dock_windows_inner(channels, reserve_chat, false);
}

#[cfg(windows)]
fn raise_dock_windows_inner(channels: &[String], reserve_chat: bool, foreground: bool) {
    let mut first = true;
    for channel in channels.iter().take(8) {
        if let Some(hwnd) = find_player_window(channel) {
            raise_hwnd(hwnd, foreground && first);
            first = false;
        }
    }
    if reserve_chat {
        let pid = owned_chatterino_pid()
            .lock()
            .ok()
            .and_then(|g| *g)
            .unwrap_or(0);
        if let Some(hwnd) = find_main_window_for_pid(pid) {
            raise_hwnd(hwnd, false);
        }
    }
    if crate::dock::has_independent_placements() {
        for channel in channels.iter().take(8) {
            if let Some(hwnd) = independent_chat_hwnd(channel) {
                raise_hwnd(hwnd, false);
            }
        }
    }
    let members = dock_member_hwnds(channels, reserve_chat);
    if let Some(anchor) = topmost_dock_member(&members) {
        crate::dock::restack_grips_above(anchor as isize);
    }
    raise_poll_overlay();
    // HWND_TOP on mpv buries the Channel Points HUD/catalog. Restack after
    // players and grips so a multistream raise still leaves the catalog clickable.
    if let Some(app) = DOCK_APP.get() {
        restack_all_points_huds(app);
    }
}

#[cfg(not(windows))]
fn restack_dock_windows(_channels: &[String], _reserve_chat: bool) {}

