#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OverlayRect {
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
}

const RAID_OVERLAY_WIDTH: i32 = 420;
const RAID_OVERLAY_HEIGHT: i32 = 92;
const RAID_OVERLAY_INSET: i32 = 16;

fn overlay_rect_from_host(host: OverlayRect) -> OverlayRect {
    let width = (host.width - RAID_OVERLAY_INSET * 2).clamp(240, RAID_OVERLAY_WIDTH);
    OverlayRect {
        x: host.x + RAID_OVERLAY_INSET,
        y: host.y + RAID_OVERLAY_INSET,
        width,
        height: RAID_OVERLAY_HEIGHT,
    }
}

#[cfg(windows)]
fn overlay_rect_from_hwnd(hwnd: *mut core::ffi::c_void) -> Option<OverlayRect> {
    if hwnd.is_null() {
        return None;
    }
    #[link(name = "user32")]
    unsafe extern "system" {
        fn GetWindowRect(hwnd: *mut core::ffi::c_void, rect: *mut WinRect) -> i32;
    }
    let mut rect = WinRect {
        left: 0,
        top: 0,
        right: 0,
        bottom: 0,
    };
    if unsafe { GetWindowRect(hwnd, &mut rect) } == 0 {
        return None;
    }
    let width = rect.right - rect.left;
    let height = rect.bottom - rect.top;
    if width < 80 || height < 40 {
        return None;
    }
    Some(OverlayRect {
        x: rect.left,
        y: rect.top,
        width,
        height,
    })
}

/// Visible DWM frame. GetWindowRect includes the hidden Win11 thickframe.
#[cfg(windows)]
fn dwm_visible_overlay_rect(hwnd: *mut core::ffi::c_void) -> Option<OverlayRect> {
    if hwnd.is_null() {
        return None;
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
    let mut rect = WinRect {
        left: 0,
        top: 0,
        right: 0,
        bottom: 0,
    };
    let hr = unsafe {
        DwmGetWindowAttribute(
            hwnd,
            DWMWA_EXTENDED_FRAME_BOUNDS,
            &mut rect as *mut _ as *mut _,
            std::mem::size_of::<WinRect>() as u32,
        )
    };
    if hr != 0 {
        return overlay_rect_from_hwnd(hwnd);
    }
    let width = rect.right - rect.left;
    let height = rect.bottom - rect.top;
    if width < 80 || height < 40 {
        return None;
    }
    Some(OverlayRect {
        x: rect.left,
        y: rect.top,
        width,
        height,
    })
}

const CHANNEL_POINTS_HUD_MIN_WIDTH: i32 = 200;
const CHANNEL_POINTS_HUD_MIN_HEIGHT: i32 = 120;
/// Matches `--title-bar-height` / `#tbo-controls` (CSS px, scaled to physical).
const POINTS_HUD_TITLE_BAR_HEIGHT_CSS: f64 = 38.0;
/// Matches `--title-bar-controls-width` (3 × 46px caption buttons).
const POINTS_HUD_CAPTION_WIDTH_CSS: f64 = 138.0;

#[derive(Debug, Clone, PartialEq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChannelPointsHudPlace {
    pub player: OverlayRect,
    pub caption_avoid: Option<OverlayRect>,
    pub hidden: bool,
}

/// Top-right min/max/close strip. `host` is the webview inner rect — `#tbo-controls`
/// is `position:fixed; right:0` on the webview, not the outer frame (8px borders).
pub fn caption_avoid_from_main(host: OverlayRect, scale: f64) -> OverlayRect {
    let scale = if scale.is_finite() && scale > 0.0 {
        scale
    } else {
        1.0
    };
    let width = (POINTS_HUD_CAPTION_WIDTH_CSS * scale).round() as i32;
    let height = (POINTS_HUD_TITLE_BAR_HEIGHT_CSS * scale).round() as i32;
    OverlayRect {
        x: host.x + host.width - width.max(1),
        y: host.y,
        width: width.max(1),
        height: height.max(1),
    }
}

pub fn union_overlay_rect(a: OverlayRect, b: OverlayRect) -> OverlayRect {
    let x = a.x.min(b.x);
    let y = a.y.min(b.y);
    let right = (a.x + a.width).max(b.x + b.width);
    let bottom = (a.y + a.height).max(b.y + b.height);
    OverlayRect {
        x,
        y,
        width: (right - x).max(1),
        height: (bottom - y).max(1),
    }
}

/// DWM caption-button bounds are window-relative (including the 8px frame).
#[cfg(windows)]
fn dwm_caption_buttons_screen(
    hwnd: *mut core::ffi::c_void,
    window_x: i32,
    window_y: i32,
) -> Option<OverlayRect> {
    #[repr(C)]
    struct Rect {
        left: i32,
        top: i32,
        right: i32,
        bottom: i32,
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
    const DWMWA_CAPTION_BUTTON_BOUNDS: u32 = 5;
    let mut bounds = Rect {
        left: 0,
        top: 0,
        right: 0,
        bottom: 0,
    };
    let hr = unsafe {
        DwmGetWindowAttribute(
            hwnd,
            DWMWA_CAPTION_BUTTON_BOUNDS,
            &mut bounds as *mut Rect as *mut _,
            std::mem::size_of::<Rect>() as u32,
        )
    };
    if hr != 0 || bounds.right <= bounds.left || bounds.bottom <= bounds.top {
        return None;
    }
    Some(OverlayRect {
        x: window_x + bounds.left,
        y: window_y + bounds.top,
        width: bounds.right - bounds.left,
        height: bounds.bottom - bounds.top,
    })
}

pub fn main_window_caption_avoid(app: &AppHandle) -> Option<OverlayRect> {
    let win = app.get_webview_window("main")?;
    let scale = win
        .scale_factor()
        .ok()
        .filter(|value| value.is_finite() && *value > 0.0)
        .unwrap_or(1.0);
    let inner_pos = win.inner_position().ok()?;
    let inner_size = win.inner_size().ok()?;
    let plugin = caption_avoid_from_main(
        OverlayRect {
            x: inner_pos.x,
            y: inner_pos.y,
            width: inner_size.width as i32,
            height: inner_size.height as i32,
        },
        scale,
    );
    #[cfg(windows)]
    {
        if let (Ok(hwnd), Ok(outer)) = (win.hwnd(), win.outer_position()) {
            if let Some(dwm) = dwm_caption_buttons_screen(hwnd.0, outer.x, outer.y) {
                return Some(union_overlay_rect(plugin, dwm));
            }
        }
    }
    Some(plugin)
}

/// Min/max/close of the *player* window, not the Rillmux chrome. Synthetic
/// top-right strip when DWM has no caption buttons (borderless mpv OSC).
pub fn player_caption_avoid(channel_login: &str, player: OverlayRect) -> Option<OverlayRect> {
    #[cfg(windows)]
    {
        let hwnd = find_player_window(channel_login);
        let scale = hwnd.map(hwnd_dpi_scale).unwrap_or(1.0);
        if let Some(hwnd) = hwnd {
            if let Some(dwm) = dwm_caption_buttons_screen(hwnd, player.x, player.y) {
                if overlay_rects_overlap(dwm, player) {
                    return Some(dwm);
                }
            }
        }
        Some(caption_avoid_from_main(player, scale))
    }
    #[cfg(not(windows))]
    {
        let _ = channel_login;
        Some(caption_avoid_from_main(player, 1.0))
    }
}

#[cfg(windows)]
fn hwnd_dpi_scale(hwnd: *mut core::ffi::c_void) -> f64 {
    #[link(name = "user32")]
    unsafe extern "system" {
        fn GetDpiForWindow(hwnd: *mut core::ffi::c_void) -> u32;
    }
    if hwnd.is_null() {
        return 1.0;
    }
    let dpi = unsafe { GetDpiForWindow(hwnd) };
    if dpi == 0 {
        1.0
    } else {
        dpi as f64 / 96.0
    }
}

pub fn overlay_rects_overlap(a: OverlayRect, b: OverlayRect) -> bool {
    a.x < b.x + b.width && a.x + a.width > b.x && a.y < b.y + b.height && a.y + a.height > b.y
}

fn overlay_rect_differs(a: OverlayRect, b: OverlayRect, slop: i32) -> bool {
    (a.x - b.x).abs() > slop
        || (a.y - b.y).abs() > slop
        || (a.width - b.width).abs() > slop
        || (a.height - b.height).abs() > slop
}

/// Move the HUD when forced, when the chip jumped (monitor switch), or when
/// it still covers the caption keepout. Do not skip a monitor jump just
/// because the chip is not overlapping the (new) caption strip.
pub fn hud_overlay_should_apply(
    force: bool,
    current: Option<OverlayRect>,
    desired: OverlayRect,
    keepout: Option<OverlayRect>,
    slop: i32,
) -> bool {
    if force {
        return true;
    }
    let Some(current) = current else {
        return true;
    };
    if overlay_rect_differs(current, desired, slop) {
        return true;
    }
    keepout.is_some_and(|k| overlay_rects_overlap(current, k))
}

/// Channel login from a `points-hud-<login>` webview label.
pub fn points_hud_channel_from_label(label: &str) -> Option<&str> {
    let channel = label.strip_prefix("points-hud-")?;
    if channel.is_empty() {
        None
    } else {
        Some(channel)
    }
}

/// `SetWindowPos` insert-after so the HUD sits immediately above the player.
/// `player_prev` is `GW_HWNDPREV` of the player (0 if none). `None` means the
/// HUD is already there — do not raise it over other apps.
pub fn hud_z_insert_after(hud: isize, player_prev: isize) -> Option<isize> {
    if player_prev == 0 {
        Some(0)
    } else if player_prev == hud {
        None
    } else {
        Some(player_prev)
    }
}

/// Tauri owns overlay webviews by the creator (main). Clearing
/// GWLP_HWNDPARENT orphans the chip. Never re-own it to mpv either — that
/// made the Rillmux main window impossible to activate while a stream ran.
#[cfg(test)]
pub fn hud_needs_detach_owner(_owner: isize) -> bool {
    false
}

#[cfg(test)]
pub fn hud_needs_reown(_current_owner: isize, _player: isize) -> bool {
    false
}

/// Qt restores window-layout.json after a monitor change, often after 800ms.
pub fn chatterino_place_retry_ms() -> &'static [u64] {
    &[0, 80, 200, 400, 800, 1600, 3000]
}

/// Visible-frame slop. 64px hid the ~27px left-gap after DWM expand.
pub fn chatterino_place_slop_px() -> i32 {
    12
}

/// Keep retrying place when the dock still has a Chatterino process but no HWND
/// (Qt recreates the window on monitor switch and drops the SetProp tag).
pub fn chatterino_hwnd_lost_needs_retry(found_hwnd: bool, have_dock_pid: bool) -> bool {
    !found_hwnd && have_dock_pid
}

/// Watchdog / place retry: missing HWND, unreadable frame, or visible drift.
pub fn chatterino_watchdog_should_place(
    found_hwnd: bool,
    expect_chat: bool,
    visible: Option<OverlayRect>,
    target: OverlayRect,
    slop: i32,
) -> bool {
    if chatterino_hwnd_lost_needs_retry(found_hwnd, expect_chat) {
        return true;
    }
    match visible {
        Some(got) => overlay_rect_drifted(got, target, slop),
        None => found_hwnd,
    }
}

pub fn overlay_rect_drifted(current: OverlayRect, target: OverlayRect, slop: i32) -> bool {
    (current.x - target.x).abs() > slop
        || (current.y - target.y).abs() > slop
        || (current.width - target.width).abs() > slop
        || (current.height - target.height).abs() > slop
}

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
