// Win32 monitor enumeration for the dock work area and picker.

#[cfg(windows)]
#[derive(Clone, Copy, Debug)]
struct MonitorGeom {
    work: Rect,
    /// Full monitor bounds (incl. taskbar) — use for drag hit-testing.
    full: Rect,
    display_num: u32,
    primary: bool,
}

#[cfg(windows)]
fn parse_display_num(sz_device: &[u16; 32]) -> u32 {
    let name = String::from_utf16_lossy(sz_device);
    let name = name.trim_end_matches('\0');
    // `\\.\DISPLAY1` / `\\.\DISPLAY12`
    name.rsplit(|c: char| !c.is_ascii_digit())
        .find(|s| !s.is_empty())
        .and_then(|s| s.parse().ok())
        .unwrap_or(999)
}

#[cfg(windows)]
fn list_monitors() -> Vec<MonitorGeom> {
    use std::sync::Mutex as StdMutex;
    struct Acc {
        list: StdMutex<Vec<MonitorGeom>>,
    }
    #[link(name = "user32")]
    unsafe extern "system" {
        fn EnumDisplayMonitors(
            hdc: *mut core::ffi::c_void,
            clip: *const Rect,
            cb: unsafe extern "system" fn(
                *mut core::ffi::c_void,
                *mut core::ffi::c_void,
                *mut Rect,
                isize,
            ) -> i32,
            data: isize,
        ) -> i32;
        fn GetMonitorInfoW(monitor: *mut core::ffi::c_void, info: *mut MonitorInfoEx) -> i32;
        fn SetThreadDpiAwarenessContext(context: isize) -> isize;
    }
    #[repr(C)]
    struct MonitorInfoEx {
        cb_size: u32,
        rc_monitor: Rect,
        rc_work: Rect,
        dw_flags: u32,
        sz_device: [u16; 32],
    }
    const MONITORINFOF_PRIMARY: u32 = 1;
    unsafe extern "system" fn enum_cb(
        monitor: *mut core::ffi::c_void,
        _hdc: *mut core::ffi::c_void,
        _rect: *mut Rect,
        data: isize,
    ) -> i32 {
        let acc = &*(data as *const Acc);
        let mut info = MonitorInfoEx {
            cb_size: std::mem::size_of::<MonitorInfoEx>() as u32,
            rc_monitor: Rect::default(),
            rc_work: Rect::default(),
            dw_flags: 0,
            sz_device: [0; 32],
        };
        if GetMonitorInfoW(monitor, &mut info) != 0 {
            if let Ok(mut list) = acc.list.lock() {
                list.push(MonitorGeom {
                    work: info.rc_work,
                    full: info.rc_monitor,
                    display_num: parse_display_num(&info.sz_device),
                    primary: info.dw_flags & MONITORINFOF_PRIMARY != 0,
                });
            }
        }
        1
    }
    unsafe {
        let _ = SetThreadDpiAwarenessContext(-4);
    }
    let acc = Acc {
        list: StdMutex::new(Vec::new()),
    };
    unsafe {
        EnumDisplayMonitors(
            std::ptr::null_mut(),
            std::ptr::null(),
            enum_cb,
            &acc as *const Acc as isize,
        );
    }
    let mut list = acc.list.into_inner().unwrap_or_default();
    // Match Windows Display Settings numbering (DISPLAY1, DISPLAY2, …).
    list.sort_by(|a, b| {
        a.display_num
            .cmp(&b.display_num)
            .then_with(|| a.full.left.cmp(&b.full.left))
            .then_with(|| a.full.top.cmp(&b.full.top))
    });
    list
}

#[cfg(windows)]
fn list_monitor_work() -> Vec<Rect> {
    list_monitors().into_iter().map(|m| m.work).collect()
}

#[cfg(windows)]
fn primary_work() -> Option<Rect> {
    let list = list_monitors();
    list.iter()
        .find(|m| m.primary)
        .or_else(|| list.first())
        .map(|m| m.work)
}

#[cfg(windows)]
#[allow(dead_code)]
fn monitor_index_at(x: i32, y: i32) -> Option<usize> {
    let list = list_monitors();
    // Prefer full bounds so dragging across taskbar / bezel gaps still hits.
    list.iter()
        .position(|m| x >= m.full.left && x < m.full.right && y >= m.full.top && y < m.full.bottom)
        .or_else(|| {
            list.iter().position(|m| {
                x >= m.work.left && x < m.work.right && y >= m.work.top && y < m.work.bottom
            })
        })
}
