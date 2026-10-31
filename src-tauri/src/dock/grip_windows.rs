// Grip HWND registry, creation, placement, and relative Z-order.

#[cfg(windows)]
struct GripWindows {
    chat: *mut core::ffi::c_void,
    /// Center ◀ ▶ handle — click to pick a monitor (identify overlays).
    mover: *mut core::ffi::c_void,
    extra: Vec<(*mut core::ffi::c_void, *mut core::ffi::c_void)>,
    tiles: Vec<*mut core::ffi::c_void>,
    /// Temporary Windows-style monitor number overlays.
    identifies: Vec<*mut core::ffi::c_void>,
}

#[cfg(windows)]
unsafe impl Send for GripWindows {}

/// Place the dock grips directly above `anchor` in the normal window band.
/// Kept separate from the grip registry so the Win32 Z-order behavior can be
/// exercised with real windows instead of source-text assertions.
#[cfg(windows)]
pub(crate) unsafe fn restack_grip_windows(
    anchor: *mut core::ffi::c_void,
    hwnds: &[*mut core::ffi::c_void],
) {
    #[link(name = "user32")]
    unsafe extern "system" {
        fn IsWindow(hwnd: *mut core::ffi::c_void) -> i32;
        fn GetWindow(hwnd: *mut core::ffi::c_void, cmd: u32) -> *mut core::ffi::c_void;
        fn GetWindowLongPtrW(hwnd: *mut core::ffi::c_void, index: i32) -> isize;
        fn SetWindowPos(
            hwnd: *mut core::ffi::c_void,
            after: *mut core::ffi::c_void,
            x: i32,
            y: i32,
            cx: i32,
            cy: i32,
            flags: u32,
        ) -> i32;
    }
    const WS_EX_TOPMOST: u32 = 0x0000_0008;
    const HWND_NOTOPMOST: isize = -2;
    const GWL_EXSTYLE: i32 = -20;
    const GW_HWNDPREV: u32 = 3;
    const SWP_NOACTIVATE: u32 = 0x0010;
    const SWP_SHOWWINDOW: u32 = 0x0040;
    const SWP_NOMOVE: u32 = 0x0002;
    const SWP_NOSIZE: u32 = 0x0001;

    if anchor.is_null() || IsWindow(anchor) == 0 {
        return;
    }

    // Defensive migration: an older build may have left a regular grip in
    // the TOPMOST band. Strip that state before doing relative restacking.
    for &hwnd in hwnds {
        if hwnd.is_null() || IsWindow(hwnd) == 0 {
            continue;
        }
        let style = GetWindowLongPtrW(hwnd, GWL_EXSTYLE) as u32;
        if style & WS_EX_TOPMOST != 0 {
            SetWindowPos(
                hwnd,
                HWND_NOTOPMOST as *mut core::ffi::c_void,
                0,
                0,
                0,
                0,
                SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE,
            );
        }
    }

    // Put every grip in the normal band immediately above the topmost dock
    // member. Any unrelated window already above the dock stays above it.
    let mut after = GetWindow(anchor, GW_HWNDPREV);
    while !after.is_null() && hwnds.contains(&after) {
        after = GetWindow(after, GW_HWNDPREV);
    }
    if !after.is_null() {
        let style = GetWindowLongPtrW(after, GWL_EXSTYLE) as u32;
        if style & WS_EX_TOPMOST != 0 {
            // A non-topmost grip cannot be inserted into the TOPMOST band.
            // Null means HWND_TOP: top of the normal band.
            after = std::ptr::null_mut();
        }
    }
    for &hwnd in hwnds {
        if hwnd.is_null() || IsWindow(hwnd) == 0 {
            continue;
        }
        SetWindowPos(
            hwnd,
            after,
            0,
            0,
            0,
            0,
            SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE | SWP_SHOWWINDOW,
        );
    }
}
fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

// Grip HWNDs live in this thread only.
static GRIPS: OnceLock<Mutex<GripWindows>> = OnceLock::new();
fn grips() -> &'static Mutex<GripWindows> {
    GRIPS.get_or_init(|| {
        Mutex::new(GripWindows {
            chat: std::ptr::null_mut(),
            mover: std::ptr::null_mut(),
            extra: Vec::new(),
            tiles: Vec::new(),
            identifies: Vec::new(),
        })
    })
}

const SWP_HIDEWINDOW: u32 = 0x0080;

unsafe fn destroy_grip(hwnd: *mut core::ffi::c_void) {
    if !hwnd.is_null() {
        DestroyWindow(hwnd);
    }
}

unsafe fn hide_grip(hwnd: *mut core::ffi::c_void) {
    if hwnd.is_null() {
        return;
    }
    // Park off-screen and hide without asserting TOPMOST.
    SetWindowPos(
        hwnd,
        std::ptr::null_mut(),
        -32000,
        -32000,
        1,
        1,
        SWP_NOACTIVATE | SWP_NOZORDER | SWP_HIDEWINDOW,
    );
    ShowWindow(hwnd, 0);
}

unsafe fn create_grip(
    instance: *mut core::ffi::c_void,
    class: *const u16,
    kind_code: isize,
) -> *mut core::ffi::c_void {
    let title = wide("");
    let is_mover = kind_code == 10 || (500..600).contains(&kind_code);
    let is_identify = (1000..1100).contains(&kind_code);
    let layered = is_mover || is_identify;
    // Identify overlays briefly cover the desktop; regular grips must not.
    let ex = WS_EX_TOOLWINDOW
        | WS_EX_NOACTIVATE
        | if layered { WS_EX_LAYERED } else { 0 }
        | if is_identify { WS_EX_TOPMOST } else { 0 };
    let (w, h) = if is_identify {
        (220, 220)
    } else if is_mover {
        (88, 44)
    } else {
        (8, 8)
    };
    let hwnd = CreateWindowExW(
        ex,
        class,
        title.as_ptr(),
        WS_POPUP | WS_VISIBLE,
        0,
        0,
        w,
        h,
        std::ptr::null_mut(),
        std::ptr::null_mut(),
        instance,
        std::ptr::null_mut(),
    );
    if !hwnd.is_null() {
        SetWindowLongPtrW(hwnd, GWLP_USERDATA, kind_code);
        if is_mover {
            SetLayeredWindowAttributes(hwnd, 0, ALPHA_REST, LWA_ALPHA);
        } else if is_identify {
            SetLayeredWindowAttributes(hwnd, 0, ALPHA_IDENTIFY, LWA_ALPHA);
        }
        // Clear any inherited topmost bit on non-identify grips.
        if !is_identify {
            SetWindowPos(
                hwnd,
                HWND_NOTOPMOST as *mut core::ffi::c_void,
                0,
                0,
                0,
                0,
                SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE,
            );
        }
        ShowWindow(hwnd, SW_SHOWNOACTIVATE);
    }
    hwnd
}

fn title_bar_height_px() -> i32 {
    #[link(name = "user32")]
    unsafe extern "system" {
        fn GetDpiForSystem() -> u32;
    }
    let dpi = unsafe { GetDpiForSystem() }.max(96) as f64;
    ((TITLE_BAR_HEIGHT_CSS as f64 * dpi) / 96.0).round() as i32
}

fn mover_rect(video: Rect, chat_opt: Option<Rect>) -> Rect {
    // Full-width chrome strip, not only the 138px min/max/close cluster.
    let right = chat_opt.map(|c| c.right).unwrap_or(video.right);
    let caption = Rect {
        left: video.left,
        top: video.top,
        right,
        bottom: video.top + title_bar_height_px(),
    };
    monitor_switch_rect(video, chat_opt, Some(caption))
}

unsafe fn place_grip(hwnd: *mut core::ffi::c_void, r: Rect) {
    if hwnd.is_null() {
        return;
    }
    // Geometry updates never change the grip's Z-order. The streaming
    // watchdog restacks the whole grip set relative to the dock members.
    SetWindowPos(
        hwnd,
        std::ptr::null_mut(),
        r.left,
        r.top,
        r.width().max(DIVIDER_REST_THICK),
        r.height().max(DIVIDER_REST_THICK),
        SWP_NOACTIVATE | SWP_NOZORDER | SWP_SHOWWINDOW,
    );
}

fn grip_hwnds_for_zorder(g: &GripWindows) -> Vec<*mut core::ffi::c_void> {
    let mut list = Vec::with_capacity(2 + g.tiles.len() + g.extra.len() * 2);
    if !g.chat.is_null() {
        list.push(g.chat);
    }
    for (chat, _) in &g.extra {
        if !chat.is_null() {
            list.push(*chat);
        }
    }
    for &h in &g.tiles {
        if !h.is_null() {
            list.push(h);
        }
    }
    // Mover last so it sits above seam/tile grips.
    if !g.mover.is_null() {
        list.push(g.mover);
    }
    for (_, mover) in &g.extra {
        if !mover.is_null() {
            list.push(*mover);
        }
    }
    list
}

unsafe fn restack_grips_inner(anchor: isize) {
    let anchor = anchor as *mut core::ffi::c_void;
    if anchor.is_null() || IsWindow(anchor) == 0 {
        return;
    }
    let Ok(g) = grips().lock() else {
        return;
    };
    let hwnds = grip_hwnds_for_zorder(&g);
    if hwnds.is_empty() {
        return;
    }
    restack_grip_windows(anchor, &hwnds);
}

fn place_existing_tiles(g: &GripWindows, video: Rect, cfg: &DockConfig) {
    let n = cfg.channels.len().clamp(1, 8);
    let layout = effective_layout(n, &cfg.layout);
    let plan = tile_grip_plan(video, cfg, layout);
    unsafe {
        for &hwnd in &g.tiles {
            if hwnd.is_null() {
                continue;
            }
            let code = GetWindowLongPtrW(hwnd, GWLP_USERDATA);
            if let Some((_, r)) = plan.iter().find(|(c, _)| *c == code) {
                place_divider_grip(hwnd, *r);
            }
        }
    }
}

fn dismiss_monitor_picker() {
    PICKER_OPEN.store(false, Ordering::SeqCst);
    let Ok(mut g) = grips().lock() else {
        return;
    };
    unsafe {
        for h in g.identifies.drain(..) {
            destroy_grip(h);
        }
    }
}

fn show_monitor_picker(instance: *mut core::ffi::c_void, class: *const u16) {
    dismiss_monitor_picker();
    let monitors = list_monitors();
    if monitors.is_empty() {
        return;
    }
    let Ok(mut g) = grips().lock() else {
        return;
    };
    PICKER_OPEN.store(true, Ordering::SeqCst);
    const BOX: i32 = 220;
    unsafe {
        for (i, m) in monitors.iter().enumerate() {
            let hwnd = create_grip(instance, class, 1000 + i as isize);
            if hwnd.is_null() {
                continue;
            }
            let cx = m.full.left + m.full.width() / 2;
            let cy = m.full.top + m.full.height() / 2;
            place_grip(
                hwnd,
                Rect {
                    left: cx - BOX / 2,
                    top: cy - BOX / 2,
                    right: cx + BOX / 2,
                    bottom: cy + BOX / 2,
                },
            );
            g.identifies.push(hwnd);
        }
    }
}

fn reposition_all_grips_static() {
    // Called from wnd_proc — grips mutex may already be contested; best-effort.
    let _ = reposition_grips_inner();
}
