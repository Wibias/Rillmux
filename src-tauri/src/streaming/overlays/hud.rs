// Channel Points HUD placement and Z-order restacking.

/// Keep the HUD above its mpv window without WS_EX_TOPMOST (other apps stay in front).
pub fn restack_hud_above_player(app: &AppHandle, label: &str) {
    let Some(win) = app.get_webview_window(label) else {
        return;
    };
    #[cfg(windows)]
    {
        let Ok(hwnd) = win.hwnd() else {
            return;
        };
        if !is_hwnd_alive(hwnd.0) {
            return;
        }
        restack_hud_hwnd(app, &win, label);
    }
    #[cfg(not(windows))]
    let _ = win.set_always_on_top(false);
}

pub fn restack_all_points_huds(app: &AppHandle) {
    for (label, _) in app.webview_windows() {
        if label.starts_with("points-hud-") {
            restack_hud_above_player(app, &label);
        }
    }
}

#[cfg(windows)]
fn restack_hud_hwnd(app: &AppHandle, win: &tauri::WebviewWindow, label: &str) {
    let Some(channel) = points_hud_channel_from_label(label) else {
        return;
    };
    let Some(player) = find_player_window(channel) else {
        return;
    };
    let Ok(hud) = win.hwnd() else {
        return;
    };
    let hud_ptr = hud.0;
    let main_ptr = app
        .get_webview_window("main")
        .and_then(|w| w.hwnd().ok())
        .map(|h| h.0)
        .filter(|p| !p.is_null())
        .unwrap_or(std::ptr::null_mut());
    if hud_ptr.is_null() || player.is_null() {
        return;
    }
    let _ = main_ptr;
    #[link(name = "user32")]
    unsafe extern "system" {
        fn GetWindow(hwnd: *mut core::ffi::c_void, cmd: u32) -> *mut core::ffi::c_void;
        fn SetWindowPos(
            hwnd: *mut core::ffi::c_void,
            after: *mut core::ffi::c_void,
            x: i32,
            y: i32,
            cx: i32,
            cy: i32,
            flags: u32,
        ) -> i32;
        fn GetWindowLongPtrW(hwnd: *mut core::ffi::c_void, index: i32) -> isize;
        fn SetWindowLongPtrW(hwnd: *mut core::ffi::c_void, index: i32, value: isize) -> isize;
        fn IsWindow(hwnd: *mut core::ffi::c_void) -> i32;
        fn ShowWindow(hwnd: *mut core::ffi::c_void, cmd: i32) -> i32;
    }
    const GW_HWNDPREV: u32 = 3;
    const GWL_EXSTYLE: i32 = -20;
    const WS_EX_TOPMOST: isize = 0x0008;
    const SWP_NOSIZE: u32 = 0x0001;
    const SWP_NOMOVE: u32 = 0x0002;
    const SWP_NOACTIVATE: u32 = 0x0010;
    const SWP_FRAMECHANGED: u32 = 0x0020;
    const SWP_SHOWWINDOW: u32 = 0x0040;
    const SWP_NOZORDER: u32 = 0x0004;
    const SW_SHOWNA: i32 = 8;
    unsafe fn hwnd_is_above(
        a: *mut core::ffi::c_void,
        b: *mut core::ffi::c_void,
        get_window: unsafe extern "system" fn(
            *mut core::ffi::c_void,
            u32,
        ) -> *mut core::ffi::c_void,
    ) -> bool {
        if a.is_null() || b.is_null() || a == b {
            return false;
        }
        const GW_HWNDNEXT: u32 = 2;
        let mut cur = get_window(a, GW_HWNDNEXT);
        for _ in 0..4096 {
            if cur.is_null() {
                return false;
            }
            if cur == b {
                return true;
            }
            cur = get_window(cur, GW_HWNDNEXT);
        }
        false
    }
    unsafe {
        if IsWindow(hud_ptr) == 0 {
            let _ = win.close();
            return;
        }
        if IsWindow(player) == 0 {
            return;
        }
        if is_hwnd_iconic(player) {
            const SW_HIDE: i32 = 0;
            let _ = ShowWindow(hud_ptr, SW_HIDE);
            return;
        }
        let style = GetWindowLongPtrW(hud_ptr, GWL_EXSTYLE);
        if style & WS_EX_TOPMOST != 0 {
            SetWindowLongPtrW(hud_ptr, GWL_EXSTYLE, style & !WS_EX_TOPMOST);
        }
        let _ = ShowWindow(hud_ptr, SW_SHOWNA);
        let prev = GetWindow(player, GW_HWNDPREV);
        let Some(after) = hud_z_insert_after(hud_ptr as isize, prev as isize) else {
            let _ = SetWindowPos(
                hud_ptr,
                std::ptr::null_mut(),
                0,
                0,
                0,
                0,
                SWP_NOMOVE
                    | SWP_NOSIZE
                    | SWP_NOACTIVATE
                    | SWP_FRAMECHANGED
                    | SWP_NOZORDER
                    | SWP_SHOWWINDOW,
            );
            return;
        };
        let _ = SetWindowPos(
            hud_ptr,
            after as *mut core::ffi::c_void,
            0,
            0,
            0,
            0,
            SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE | SWP_FRAMECHANGED,
        );
        if !hwnd_is_above(hud_ptr, player, GetWindow) {
            let _ = SetWindowPos(
                hud_ptr,
                std::ptr::null_mut(),
                0,
                0,
                0,
                0,
                SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE | SWP_FRAMECHANGED,
            );
        }
    }
}

/// Keep HUD overlays off the min/max/close strip. `force` always applies the
/// rect (create / first place); otherwise only move if the HWND covers caption.
pub fn place_hud_overlay(
    app: &AppHandle,
    label: &str,
    x: i32,
    y: i32,
    width: i32,
    height: i32,
    force: bool,
) {
    let Some(win) = app.get_webview_window(label) else {
        return;
    };
    #[cfg(windows)]
    {
        let Ok(hwnd) = win.hwnd() else {
            return;
        };
        if !is_hwnd_alive(hwnd.0) {
            return;
        }
    }
    let _ = win.set_min_size(Some(tauri::Size::Physical(tauri::PhysicalSize::new(1, 1))));
    restack_hud_above_player(app, label);
    let desired = OverlayRect {
        x,
        y,
        width,
        height,
    };
    let current = win.outer_position().ok().and_then(|pos| {
        win.outer_size().ok().map(|size| OverlayRect {
            x: pos.x,
            y: pos.y,
            width: size.width as i32,
            height: size.height as i32,
        })
    });
    let keepout = main_window_caption_avoid(app).map(|avoid| OverlayRect {
        x: avoid.x,
        y: avoid.y,
        width: avoid.width,
        height: avoid.height + 16 + 36,
    });
    if !hud_overlay_should_apply(force, current, desired, keepout, 12) {
        return;
    }
    #[cfg(windows)]
    if let Ok(hwnd) = win.hwnd() {
        move_overlay_hwnd(hwnd.0, x, y, width, height);
    }
    let _ = win.set_position(tauri::PhysicalPosition::new(x, y));
    fit_overlay_webview(&win, width, height);
}

/// Full player outer rect for the Channel Points HUD, or none when hidden.
pub fn channel_points_hud_player_rect(
    player: Option<OverlayRect>,
    iconic: bool,
) -> Option<OverlayRect> {
    let rect = player?;
    if iconic
        || rect.width < CHANNEL_POINTS_HUD_MIN_WIDTH
        || rect.height < CHANNEL_POINTS_HUD_MIN_HEIGHT
    {
        return None;
    }
    Some(rect)
}

#[cfg(windows)]
fn hud_host_is_hwnd_visible(hwnd: *mut core::ffi::c_void) -> bool {
    if hwnd.is_null() {
        return false;
    }
    #[link(name = "user32")]
    unsafe extern "system" {
        fn IsWindowVisible(hwnd: *mut core::ffi::c_void) -> i32;
    }
    unsafe { IsWindowVisible(hwnd) != 0 }
}

#[cfg(windows)]
fn hud_host_hwnd_has_monitor(hwnd: *mut core::ffi::c_void) -> bool {
    if hwnd.is_null() {
        return false;
    }
    #[link(name = "user32")]
    unsafe extern "system" {
        fn MonitorFromWindow(hwnd: *mut core::ffi::c_void, flags: u32) -> *mut core::ffi::c_void;
    }
    const MONITOR_DEFAULTTONULL: u32 = 0;
    !unsafe { MonitorFromWindow(hwnd, MONITOR_DEFAULTTONULL) }.is_null()
}

#[cfg(windows)]
pub fn channel_points_hud_host(channel_login: &str) -> Option<OverlayRect> {
    let hwnd = find_player_window(channel_login)?;
    if is_hwnd_iconic(hwnd) {
        return None;
    }
    if !hud_host_is_hwnd_visible(hwnd) || !hud_host_hwnd_has_monitor(hwnd) {
        return None;
    }
    channel_points_hud_player_rect(overlay_rect_from_hwnd(hwnd), false)
}

#[cfg(windows)]
pub fn channel_points_hud_placement(channel_login: &str) -> Option<ChannelPointsHudPlace> {
    let hwnd = find_player_window(channel_login)?;
    if is_hwnd_iconic(hwnd) {
        return Some(ChannelPointsHudPlace {
            player: OverlayRect {
                x: 0,
                y: 0,
                width: 1,
                height: 1,
            },
            caption_avoid: None,
            hidden: true,
        });
    }
    let player = channel_points_hud_host(channel_login)?;
    Some(ChannelPointsHudPlace {
        player,
        caption_avoid: player_caption_avoid(channel_login, player),
        hidden: false,
    })
}

#[cfg(not(windows))]
pub fn channel_points_hud_host(_channel_login: &str) -> Option<OverlayRect> {
    None
}

#[cfg(not(windows))]
pub fn channel_points_hud_placement(_channel_login: &str) -> Option<ChannelPointsHudPlace> {
    None
}
