// Dock member HWND discovery and window state helpers.

#[cfg(windows)]
fn pid_image_path(pid: u32) -> Option<String> {
    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn OpenProcess(access: u32, inherit: i32, pid: u32) -> *mut core::ffi::c_void;
        fn CloseHandle(handle: *mut core::ffi::c_void) -> i32;
        fn QueryFullProcessImageNameW(
            process: *mut core::ffi::c_void,
            flags: u32,
            name: *mut u16,
            size: *mut u32,
        ) -> i32;
    }
    const PROCESS_QUERY_LIMITED_INFORMATION: u32 = 0x1000;
    let handle = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid) };
    if handle.is_null() {
        return None;
    }
    let mut buf = [0u16; 512];
    let mut size = buf.len() as u32;
    let ok = unsafe { QueryFullProcessImageNameW(handle, 0, buf.as_mut_ptr(), &mut size) };
    unsafe {
        let _ = CloseHandle(handle);
    }
    if ok == 0 || size == 0 {
        return None;
    }
    Some(String::from_utf16_lossy(&buf[..size as usize]))
}

#[cfg(windows)]
fn pid_is_alive(pid: u32) -> bool {
    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn OpenProcess(access: u32, inherit: i32, pid: u32) -> *mut core::ffi::c_void;
        fn GetExitCodeProcess(handle: *mut core::ffi::c_void, code: *mut u32) -> i32;
        fn CloseHandle(handle: *mut core::ffi::c_void) -> i32;
    }
    const PROCESS_QUERY_LIMITED_INFORMATION: u32 = 0x1000;
    const STILL_ACTIVE: u32 = 259;
    let handle = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid) };
    if handle.is_null() {
        return false;
    }
    let mut code = 0u32;
    let ok = unsafe { GetExitCodeProcess(handle, &mut code) };
    unsafe {
        let _ = CloseHandle(handle);
    }
    ok != 0 && code == STILL_ACTIVE
}

#[cfg(not(windows))]
fn pid_is_alive(_pid: u32) -> bool {
    false
}

#[cfg(windows)]
fn dock_member_hwnds(channels: &[String], reserve_chat: bool) -> Vec<*mut core::ffi::c_void> {
    let mut out = Vec::new();
    for channel in channels.iter().take(8) {
        if let Some(hwnd) = find_player_window(channel) {
            out.push(hwnd);
        }
    }
    if crate::dock::has_independent_placements() {
        for channel in channels.iter().take(8) {
            if let Some(hwnd) = independent_chat_hwnd(channel) {
                out.push(hwnd);
            }
        }
    } else if reserve_chat {
        let pid = owned_chatterino_pid()
            .lock()
            .ok()
            .and_then(|g| *g)
            .unwrap_or(0);
        if let Some(hwnd) = find_main_window_for_pid(pid) {
            out.push(hwnd);
        }
    }
    out
}

#[cfg(windows)]
fn topmost_dock_member(members: &[*mut core::ffi::c_void]) -> Option<*mut core::ffi::c_void> {
    #[link(name = "user32")]
    unsafe extern "system" {
        fn GetTopWindow(hwnd: *mut core::ffi::c_void) -> *mut core::ffi::c_void;
        fn GetWindow(hwnd: *mut core::ffi::c_void, cmd: u32) -> *mut core::ffi::c_void;
    }
    const GW_HWNDNEXT: u32 = 2;
    let mut hwnd = unsafe { GetTopWindow(std::ptr::null_mut()) };
    for _ in 0..4096 {
        if hwnd.is_null() {
            break;
        }
        if members.contains(&hwnd) && is_hwnd_alive(hwnd) {
            return Some(hwnd);
        }
        hwnd = unsafe { GetWindow(hwnd, GW_HWNDNEXT) };
    }
    members.iter().copied().find(|&hwnd| is_hwnd_alive(hwnd))
}

#[cfg(windows)]
fn is_hwnd_alive(hwnd: *mut core::ffi::c_void) -> bool {
    #[link(name = "user32")]
    unsafe extern "system" {
        fn IsWindow(hwnd: *mut core::ffi::c_void) -> i32;
    }
    !hwnd.is_null() && unsafe { IsWindow(hwnd) != 0 }
}

#[cfg(windows)]
fn is_hwnd_iconic(hwnd: *mut core::ffi::c_void) -> bool {
    #[link(name = "user32")]
    unsafe extern "system" {
        fn IsIconic(hwnd: *mut core::ffi::c_void) -> i32;
    }
    is_hwnd_alive(hwnd) && unsafe { IsIconic(hwnd) != 0 }
}

#[cfg(windows)]
fn is_hwnd_zoomed(hwnd: *mut core::ffi::c_void) -> bool {
    #[link(name = "user32")]
    unsafe extern "system" {
        fn IsZoomed(hwnd: *mut core::ffi::c_void) -> i32;
    }
    !hwnd.is_null() && unsafe { IsZoomed(hwnd) != 0 }
}

#[cfg(windows)]
fn is_hwnd_visible(hwnd: *mut core::ffi::c_void) -> bool {
    #[link(name = "user32")]
    unsafe extern "system" {
        fn IsWindowVisible(hwnd: *mut core::ffi::c_void) -> i32;
    }
    !hwnd.is_null() && unsafe { IsWindowVisible(hwnd) != 0 }
}

#[cfg(windows)]
fn is_hwnd_restored(hwnd: *mut core::ffi::c_void) -> bool {
    // IsWindowVisible stays true for minimized windows — exclude iconic.
    is_hwnd_visible(hwnd) && !is_hwnd_iconic(hwnd)
}

#[cfg(windows)]
fn minimize_dock_group(hwnds: &[*mut core::ffi::c_void]) {
    #[link(name = "user32")]
    unsafe extern "system" {
        fn ShowWindow(hwnd: *mut core::ffi::c_void, cmd: i32) -> i32;
        fn CloseWindow(hwnd: *mut core::ffi::c_void) -> i32;
    }
    // SW_FORCEMINIMIZE works more reliably for borderless mpv than SW_MINIMIZE.
    const SW_FORCEMINIMIZE: i32 = 11;
    const SW_MINIMIZE: i32 = 6;
    for &hwnd in hwnds {
        if !is_hwnd_alive(hwnd) || is_hwnd_iconic(hwnd) {
            continue;
        }
        unsafe {
            if ShowWindow(hwnd, SW_FORCEMINIMIZE) == 0 && ShowWindow(hwnd, SW_MINIMIZE) == 0 {
                let _ = CloseWindow(hwnd);
            }
        }
    }
}

#[cfg(windows)]
fn restore_dock_group(channels: &[String], reserve_chat: bool, layout: &str) {
    if layout == "independent" || crate::dock::has_independent_placements() {
        #[link(name = "user32")]
        unsafe extern "system" {
            fn ShowWindow(hwnd: *mut core::ffi::c_void, cmd: i32) -> i32;
        }
        const SW_RESTORE: i32 = 9;
        let hwnds = dock_member_hwnds(channels, true);
        for &hwnd in &hwnds {
            if is_hwnd_alive(hwnd) {
                unsafe {
                    let _ = ShowWindow(hwnd, SW_RESTORE);
                }
            }
        }
        thread::sleep(Duration::from_millis(50));
        let _ = place_independent_pairs();
        raise_dock_windows(channels, false);
        return;
    }
    #[link(name = "user32")]
    unsafe extern "system" {
        fn ShowWindow(hwnd: *mut core::ffi::c_void, cmd: i32) -> i32;
    }
    const SW_RESTORE: i32 = 9;
    let hwnds = dock_member_hwnds(channels, reserve_chat);
    for &hwnd in &hwnds {
        if is_hwnd_alive(hwnd) {
            unsafe {
                let _ = ShowWindow(hwnd, SW_RESTORE);
            }
        }
    }
    // Let Win32 finish restoring before we MoveWindow / retile.
    thread::sleep(Duration::from_millis(50));
    let _ = retile_player_windows(channels, reserve_chat, layout);
    if reserve_chat {
        place_chatterino_window_right(0);
        schedule_chatterino_place();
    }
    raise_dock_windows(channels, reserve_chat);
}
