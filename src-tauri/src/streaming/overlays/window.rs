// Overlay HWND sizing and host selection.

/// Resize a transparent overlay HWND and its WebView2 child to `width`×`height`
/// physical pixels. Tauri `setSize` often leaves the child at the previous size.
/// Move + resize the overlay HWND in screen physical pixels. Tauri
/// `setPosition` often no-ops when the window already exists on another monitor.
#[cfg(windows)]
fn move_overlay_hwnd(hwnd: *mut core::ffi::c_void, x: i32, y: i32, width: i32, height: i32) {
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
        fn GetWindow(hwnd: *mut core::ffi::c_void, cmd: u32) -> *mut core::ffi::c_void;
        fn IsWindow(hwnd: *mut core::ffi::c_void) -> i32;
    }
    const GW_CHILD: u32 = 5;
    const SWP_NOZORDER: u32 = 0x0004;
    const SWP_NOACTIVATE: u32 = 0x0010;
    const SWP_FRAMECHANGED: u32 = 0x0020;
    const SWP_NOMOVE: u32 = 0x0002;
    let width = width.max(1);
    let height = height.max(1);
    unsafe {
        if hwnd.is_null() || IsWindow(hwnd) == 0 {
            return;
        }
        let _ = SetWindowPos(
            hwnd,
            std::ptr::null_mut(),
            x,
            y,
            width,
            height,
            SWP_NOZORDER | SWP_NOACTIVATE | SWP_FRAMECHANGED,
        );
        let child = GetWindow(hwnd, GW_CHILD);
        if !child.is_null() && IsWindow(child) != 0 {
            let _ = SetWindowPos(
                child,
                std::ptr::null_mut(),
                0,
                0,
                width,
                height,
                SWP_NOMOVE | SWP_NOZORDER | SWP_NOACTIVATE,
            );
        }
    }
}

#[cfg(windows)]
pub fn fit_overlay_webview(win: &tauri::WebviewWindow, width: i32, height: i32) {
    let Ok(hwnd) = win.hwnd() else {
        return;
    };
    let width = width.max(1);
    let height = height.max(1);
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
        fn GetWindow(hwnd: *mut core::ffi::c_void, cmd: u32) -> *mut core::ffi::c_void;
        fn IsWindow(hwnd: *mut core::ffi::c_void) -> i32;
    }
    const GW_CHILD: u32 = 5;
    const SWP_NOMOVE: u32 = 0x0002;
    const SWP_NOZORDER: u32 = 0x0004;
    const SWP_NOACTIVATE: u32 = 0x0010;
    const SWP_FRAMECHANGED: u32 = 0x0020;
    let flags = SWP_NOMOVE | SWP_NOZORDER | SWP_NOACTIVATE | SWP_FRAMECHANGED;
    let ptr = hwnd.0;
    unsafe {
        if ptr.is_null() || IsWindow(ptr) == 0 {
            return;
        }
        let _ = SetWindowPos(ptr, std::ptr::null_mut(), 0, 0, width, height, flags);
        let child = GetWindow(ptr, GW_CHILD);
        if !child.is_null() && IsWindow(child) != 0 {
            let _ = SetWindowPos(child, std::ptr::null_mut(), 0, 0, width, height, flags);
        }
    }
    let _ = win.set_size(tauri::PhysicalSize::new(width as u32, height as u32));
}

#[cfg(not(windows))]
pub fn fit_overlay_webview(win: &tauri::WebviewWindow, width: i32, height: i32) {
    let _ = win.set_size(tauri::PhysicalSize::new(
        width.max(1) as u32,
        height.max(1) as u32,
    ));
}

/// Player first, then owned Chatterino. Main-window fallback is frontend-owned.
#[cfg(windows)]
pub fn raid_overlay_host(from_channel: &str) -> Option<OverlayRect> {
    let player = find_player_window(from_channel).and_then(overlay_rect_from_hwnd);
    if let Some(host) = player {
        return Some(overlay_rect_from_host(host));
    }
    let chat = owned_chatterino_pid()
        .lock()
        .ok()
        .and_then(|g| *g)
        .and_then(find_main_window_for_pid)
        .and_then(overlay_rect_from_hwnd);
    chat.map(overlay_rect_from_host)
}

#[cfg(not(windows))]
pub fn raid_overlay_host(_from_channel: &str) -> Option<OverlayRect> {
    None
}

/// Full owned-Chatterino window, else the reserved dock chat strip.
#[cfg(windows)]
pub fn poll_overlay_chat_host() -> Option<OverlayRect> {
    owned_chatterino_pid()
        .lock()
        .ok()
        .and_then(|g| *g)
        .and_then(find_main_window_for_pid)
        .and_then(overlay_rect_from_hwnd)
        .or_else(overlay_rect_from_reserved_chat)
}

#[cfg(not(windows))]
pub fn poll_overlay_chat_host() -> Option<OverlayRect> {
    None
}

#[cfg(windows)]
fn overlay_rect_from_reserved_chat() -> Option<OverlayRect> {
    if !crate::dock::snapshot().reserve_chat {
        return None;
    }
    let chat = crate::dock::chat_video_split(true)?.1?;
    let width = chat.width();
    let height = chat.height();
    if width < 80 || height < 40 {
        return None;
    }
    Some(OverlayRect {
        x: chat.left,
        y: chat.top,
        width,
        height,
    })
}
