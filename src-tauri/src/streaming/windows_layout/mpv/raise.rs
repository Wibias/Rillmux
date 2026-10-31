// Raising and restacking dock members.

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
