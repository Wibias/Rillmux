// Focus classification for dock members and Chatterino popups.

#[cfg(windows)]
#[derive(Clone, Copy, PartialEq, Eq)]
enum DockFocusKind {
    /// Our app, a grip, or a known dock member (mpv / owned Chatterino).
    DockOrApp,
    /// Some other process is foreground.
    Foreign,
    /// No FG window, or we have no member HWNDs to compare yet.
    Unknown,
}

/// Classify the foreground window using the watchdog's HWND cache (not a
/// fresh title scan — those miss borderless mpv and incorrectly demoted grips).
#[cfg(windows)]
fn dock_focus_kind(members: &[*mut core::ffi::c_void]) -> DockFocusKind {
    #[link(name = "user32")]
    unsafe extern "system" {
        fn GetForegroundWindow() -> *mut core::ffi::c_void;
        fn GetWindowThreadProcessId(hwnd: *mut core::ffi::c_void, pid: *mut u32) -> u32;
    }
    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn GetCurrentProcessId() -> u32;
    }
    let fg = unsafe { GetForegroundWindow() };
    if fg.is_null() {
        return DockFocusKind::Unknown;
    }
    if crate::dock::is_grip_hwnd(fg) {
        return DockFocusKind::DockOrApp;
    }
    let mut fg_pid = 0u32;
    unsafe {
        GetWindowThreadProcessId(fg, &mut fg_pid);
    }
    if fg_pid != 0 && fg_pid == unsafe { GetCurrentProcessId() } {
        return DockFocusKind::DockOrApp;
    }
    if members.is_empty() {
        return DockFocusKind::Unknown;
    }
    if members.contains(&fg) {
        return DockFocusKind::DockOrApp;
    }
    // mpv/Chatterino may focus a sibling top-level; match by process.
    if fg_pid == 0 {
        return DockFocusKind::Unknown;
    }
    let member_match = members.iter().any(|&h| {
        if h.is_null() {
            return false;
        }
        let mut pid = 0u32;
        unsafe {
            GetWindowThreadProcessId(h, &mut pid);
        }
        pid != 0 && pid == fg_pid
    });
    if member_match {
        DockFocusKind::DockOrApp
    } else {
        DockFocusKind::Foreign
    }
}

#[cfg(windows)]
fn chatterino_has_overlay_popup() -> bool {
    let pid = owned_chatterino_pid()
        .lock()
        .ok()
        .and_then(|g| *g)
        .unwrap_or(0);
    if pid == 0 {
        return false;
    }
    let Some(main) = find_main_window_for_pid(pid) else {
        return false;
    };
    #[link(name = "user32")]
    unsafe extern "system" {
        fn GetForegroundWindow() -> *mut core::ffi::c_void;
        fn GetWindowThreadProcessId(hwnd: *mut core::ffi::c_void, pid: *mut u32) -> u32;
        fn IsWindowVisible(hwnd: *mut core::ffi::c_void) -> i32;
        fn GetWindow(hwnd: *mut core::ffi::c_void, cmd: u32) -> *mut core::ffi::c_void;
        fn GetWindowRect(hwnd: *mut core::ffi::c_void, rect: *mut WinRect) -> i32;
    }
    const GW_OWNER: u32 = 4;
    unsafe {
        let fg = GetForegroundWindow();
        if fg.is_null() || fg == main || IsWindowVisible(fg) == 0 {
            return false;
        }
        let mut wpid = 0u32;
        GetWindowThreadProcessId(fg, &mut wpid);
        if wpid != pid {
            return false;
        }
        // Usercards are owned dialogs (or other non-main top-level) with real size.
        let owner = GetWindow(fg, GW_OWNER);
        let mut rc = WinRect {
            left: 0,
            top: 0,
            right: 0,
            bottom: 0,
        };
        if GetWindowRect(fg, &mut rc) == 0 {
            return false;
        }
        let w = (rc.right - rc.left).max(0);
        let h = (rc.bottom - rc.top).max(0);
        if w < 120 || h < 120 {
            return false;
        }
        // Owned by main chat, or any other focused Chatterino window that isn't main.
        !owner.is_null() || fg != main
    }
}
