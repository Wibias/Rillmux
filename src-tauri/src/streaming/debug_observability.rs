#[cfg(windows)]
pub fn debug_chatterino_windows(stage: &str) {
    #[repr(C)]
    struct Rect {
        left: i32,
        top: i32,
        right: i32,
        bottom: i32,
    }

    #[link(name = "user32")]
    unsafe extern "system" {
        fn IsWindow(hwnd: *mut core::ffi::c_void) -> i32;
        fn IsWindowVisible(hwnd: *mut core::ffi::c_void) -> i32;
        fn IsIconic(hwnd: *mut core::ffi::c_void) -> i32;
        fn GetWindowRect(hwnd: *mut core::ffi::c_void, rect: *mut Rect) -> i32;
        fn GetWindowTextW(hwnd: *mut core::ffi::c_void, buf: *mut u16, max: i32) -> i32;
    }
    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn GetLastError() -> u32;
        fn SetLastError(code: u32);
    }

    let tracked = owned_chatterino_pid().lock().ok().and_then(|guard| *guard);
    let pids = chatterino_pids_to_close(tracked, &list_rillmux_dock_chatterino_pids());
    if pids.is_empty() {
        crate::diagnostics::log_event(
            crate::diagnostics::DebugCategory::Windows,
            "chatterino.hwnd",
            &format!("stage={stage} tracked_pid={:?} windows=0", tracked),
        );
        return;
    }

    for pid in pids {
        let alive = pid_is_alive(pid);
        let windows = top_level_windows_for_pid(pid);
        if windows.is_empty() {
            crate::diagnostics::log_event(
                crate::diagnostics::DebugCategory::Windows,
                "chatterino.hwnd",
                &format!("stage={stage} pid={pid} alive={alive} windows=0"),
            );
            continue;
        }
        let selected = find_main_window_for_pid(pid);
        let channels = last_chatterino_channels()
            .lock()
            .ok()
            .map(|guard| guard.clone())
            .unwrap_or_default();
        for hwnd in windows {
            let mut rect = Rect {
                left: 0,
                top: 0,
                right: 0,
                bottom: 0,
            };
            unsafe {
                SetLastError(0);
                let is_window = IsWindow(hwnd) != 0;
                let visible = IsWindowVisible(hwnd) != 0;
                let iconic = IsIconic(hwnd) != 0;
                let mut title = [0u16; 512];
                let title_len = GetWindowTextW(hwnd, title.as_mut_ptr(), title.len() as i32);
                let split_match = title_len > 0
                    && chatterino_title_matches_channels(
                        &String::from_utf16_lossy(&title[..title_len as usize]),
                        &channels,
                    );
                let is_selected = selected == Some(hwnd);
                SetLastError(0);
                let rect_ok = GetWindowRect(hwnd, &mut rect) != 0;
                let last_error = if rect_ok { 0 } else { GetLastError() };
                crate::diagnostics::log_event(
                    crate::diagnostics::DebugCategory::Windows,
                    "chatterino.hwnd",
                    &format!(
                        "stage={stage} pid={pid} alive={alive} hwnd={hwnd:p} is_window={is_window} visible={visible} iconic={iconic} split_match={split_match} selected={is_selected} rect_ok={rect_ok} rect={},{},{},{} last_error=0x{last_error:08x}",
                        rect.left, rect.top, rect.right, rect.bottom
                    ),
                );
            }
        }
    }
}

#[cfg(not(windows))]
pub fn debug_chatterino_windows(stage: &str) {
    crate::diagnostics::log_event(
        crate::diagnostics::DebugCategory::Windows,
        "chatterino.hwnd",
        &format!("stage={stage} platform=non-windows"),
    );
}
