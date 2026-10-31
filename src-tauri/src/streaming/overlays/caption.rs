// Caption-button avoidance geometry for the main window and players.

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
