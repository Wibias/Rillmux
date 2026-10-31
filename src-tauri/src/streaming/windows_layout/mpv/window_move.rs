// Moving and raising overlay/player HWNDs.

/// Place window on `rect`. `expand_dwm` fills invisible Win11 borders (use for Chatterino).
/// Borderless mpv should pass `expand_dwm = false`.
#[cfg(windows)]
fn move_hwnd_to(hwnd: *mut core::ffi::c_void, rect: WinRect, expand_dwm: bool) {
    #[link(name = "user32")]
    unsafe extern "system" {
        fn GetWindowRect(hwnd: *mut core::ffi::c_void, rect: *mut WinRect) -> i32;
        fn MoveWindow(
            hwnd: *mut core::ffi::c_void,
            x: i32,
            y: i32,
            w: i32,
            h: i32,
            repaint: i32,
        ) -> i32;
        fn ShowWindow(hwnd: *mut core::ffi::c_void, cmd: i32) -> i32;
        fn GetWindowLongPtrW(hwnd: *mut core::ffi::c_void, index: i32) -> isize;
        fn SetWindowLongPtrW(hwnd: *mut core::ffi::c_void, index: i32, value: isize) -> isize;
        fn SetWindowPos(
            hwnd: *mut core::ffi::c_void,
            after: *mut core::ffi::c_void,
            x: i32,
            y: i32,
            cx: i32,
            cy: i32,
            flags: u32,
        ) -> i32;
        fn SetThreadDpiAwarenessContext(context: isize) -> isize;
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
    const SW_RESTORE: i32 = 9;
    const GWL_STYLE: i32 = -16;
    const WS_MAXIMIZE: isize = 0x0100_0000;
    const WS_THICKFRAME: isize = 0x0004_0000;
    const WS_MAXIMIZEBOX: isize = 0x0001_0000;
    const SWP_NOZORDER: u32 = 0x0004;
    const SWP_NOMOVE: u32 = 0x0002;
    const SWP_NOSIZE: u32 = 0x0001;
    const SWP_FRAMECHANGED: u32 = 0x0020;
    const DPI_CTX: isize = -4;

    let target_w = (rect.right - rect.left).max(1);
    let target_h = (rect.bottom - rect.top).max(1);

    unsafe {
        let _prev = SetThreadDpiAwarenessContext(DPI_CTX);
        let mut style = GetWindowLongPtrW(hwnd, GWL_STYLE);
        if style & WS_MAXIMIZE != 0 {
            style &= !WS_MAXIMIZE;
            SetWindowLongPtrW(hwnd, GWL_STYLE, style);
        }
        // Live dock: strip Chatterino's resize border so the visible frame
        // fills the chat slot (otherwise a gap sits to the right of chat).
        if expand_dwm && style & (WS_THICKFRAME | WS_MAXIMIZEBOX) != 0 {
            style &= !(WS_THICKFRAME | WS_MAXIMIZEBOX);
            SetWindowLongPtrW(hwnd, GWL_STYLE, style);
            SetWindowPos(
                hwnd,
                std::ptr::null_mut(),
                0,
                0,
                0,
                0,
                SWP_NOMOVE | SWP_NOSIZE | SWP_NOZORDER | SWP_FRAMECHANGED,
            );
        }
        ShowWindow(hwnd, SW_RESTORE);

        if !expand_dwm {
            MoveWindow(hwnd, rect.left, rect.top, target_w, target_h, 1);
            return;
        }

        let mut outer = WinRect {
            left: 0,
            top: 0,
            right: 0,
            bottom: 0,
        };
        let mut frame = outer;
        let has_outer = GetWindowRect(hwnd, &mut outer) != 0;
        let has_frame = has_outer
            && DwmGetWindowAttribute(
                hwnd,
                DWMWA_EXTENDED_FRAME_BOUNDS,
                &mut frame as *mut _ as *mut _,
                std::mem::size_of::<WinRect>() as u32,
            ) == 0;

        let (x, y, w, h) = if has_outer && has_frame {
            let bl = frame.left - outer.left;
            let bt = frame.top - outer.top;
            let br = outer.right - frame.right;
            let bb = outer.bottom - frame.bottom;
            (
                rect.left - bl,
                rect.top - bt,
                target_w + bl + br,
                target_h + bt + bb,
            )
        } else {
            (rect.left, rect.top, target_w, target_h)
        };
        MoveWindow(hwnd, x, y, w.max(1), h.max(1), 1);

        if DwmGetWindowAttribute(
            hwnd,
            DWMWA_EXTENDED_FRAME_BOUNDS,
            &mut frame as *mut _ as *mut _,
            std::mem::size_of::<WinRect>() as u32,
        ) == 0
            && GetWindowRect(hwnd, &mut outer) != 0
        {
            let dx = rect.left - frame.left;
            let dy = rect.top - frame.top;
            let dw = rect.right - frame.right;
            let dh = rect.bottom - frame.bottom;
            if dx != 0 || dy != 0 || dw != 0 || dh != 0 {
                MoveWindow(
                    hwnd,
                    outer.left + dx,
                    outer.top + dy,
                    ((outer.right - outer.left) + dw).max(1),
                    ((outer.bottom - outer.top) + dh).max(1),
                    1,
                );
            }
        }
    }
}

/// Keep the poll/prediction overlay above Chatterino. Dock raises chat with
/// HWND_TOP on a timer, which otherwise buries a separate overlay window.
#[cfg(windows)]
fn raise_poll_overlay() {
    let Some(app) = DOCK_APP.get() else {
        return;
    };
    let Some(win) = app.get_webview_window("poll-overlay") else {
        return;
    };
    let Ok(hwnd) = win.hwnd() else {
        return;
    };
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
        fn ShowWindow(hwnd: *mut core::ffi::c_void, cmd: i32) -> i32;
    }
    const HWND_TOPMOST: isize = -1;
    const SWP_NOMOVE: u32 = 0x0002;
    const SWP_NOSIZE: u32 = 0x0001;
    const SWP_SHOWWINDOW: u32 = 0x0040;
    const SWP_NOACTIVATE: u32 = 0x0010;
    const SW_SHOWNA: i32 = 8;
    let ptr = hwnd.0;
    if ptr.is_null() {
        return;
    }
    unsafe {
        ShowWindow(ptr, SW_SHOWNA);
        SetWindowPos(
            ptr,
            HWND_TOPMOST as *mut core::ffi::c_void,
            0,
            0,
            0,
            0,
            SWP_NOMOVE | SWP_NOSIZE | SWP_SHOWWINDOW | SWP_NOACTIVATE,
        );
    }
}

#[cfg(not(windows))]
fn raise_poll_overlay() {}

pub fn raise_poll_overlay_window() {
    raise_poll_overlay();
}
