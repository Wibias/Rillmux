// Player HWND discovery by window title plus Win32 ABI types.

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
