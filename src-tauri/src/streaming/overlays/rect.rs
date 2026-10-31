// Overlay rectangles, raid sizing, and Win32/DWM rect derivation.

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
