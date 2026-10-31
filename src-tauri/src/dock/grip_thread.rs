// Grip thread entry point and the commands the UI/dock layer posts to it.

fn ensure_grip_thread() {
    #[cfg(windows)]
    {
        if GRIP_THREAD_STARTED.swap(true, Ordering::SeqCst) {
            return;
        }
        thread::spawn(grip_thread_main);
    }
    #[cfg(not(windows))]
    {
        let _ = &GRIP_THREAD_STARTED;
    }
}

#[cfg(windows)]
#[derive(Clone)]
enum DockCmd {
    Sync,
    /// Hide all grip HWNDs (dock minimized).
    HideGrips,
    /// Hide only the video|chat seam grips so Chatterino popups aren't clipped.
    SuppressSeam,
    /// Re-show seam grips after a Chatterino popup closes (no full rebuild).
    RestoreSeam,
    /// Restack regular grips immediately above the topmost dock member without
    /// entering Windows' global TOPMOST band.
    RestackGrips(isize),
    /// Show Windows-style monitor numbers; click to pick.
    PickMonitor,
}

#[cfg(windows)]
fn cmd_queue() -> &'static Mutex<Vec<DockCmd>> {
    static Q: OnceLock<Mutex<Vec<DockCmd>>> = OnceLock::new();
    Q.get_or_init(|| Mutex::new(Vec::new()))
}

#[cfg(windows)]
fn post_cmd(cmd: DockCmd) {
    ensure_grip_thread();
    if let Ok(mut q) = cmd_queue().lock() {
        // Coalesce z-order spam from the watchdog: only the latest dock anchor matters.
        if matches!(&cmd, DockCmd::RestackGrips(_)) {
            q.retain(|c| !matches!(c, DockCmd::RestackGrips(_)));
        }
        q.push(cmd);
    }
}

#[cfg(windows)]
static GRIPS_GROUP_MINIMIZED: AtomicBool = AtomicBool::new(false);

/// Hide dock grips while the player/chat group is minimized.
pub fn hide_grips() {
    #[cfg(windows)]
    {
        GRIPS_GROUP_MINIMIZED.store(true, Ordering::SeqCst);
        post_cmd(DockCmd::HideGrips);
    }
}

/// Re-show / rebuild grips after restore.
pub fn show_grips() {
    #[cfg(windows)]
    {
        GRIPS_GROUP_MINIMIZED.store(false, Ordering::SeqCst);
        post_cmd(DockCmd::Sync);
    }
}

/// Keep regular dock grips directly above the topmost mpv/owned Chatterino
/// window in the normal Z-order. They must never enter the global TOPMOST band.
pub fn restack_grips_above(anchor: isize) {
    #[cfg(windows)]
    {
        if anchor == 0 || GRIPS_GROUP_MINIMIZED.load(Ordering::SeqCst) {
            return;
        }
        post_cmd(DockCmd::RestackGrips(anchor));
    }
    #[cfg(not(windows))]
    let _ = anchor;
}

/// True when `hwnd` is one of our dock grip windows (class `StguiDockGrip`).
#[cfg(windows)]
pub fn is_grip_hwnd(hwnd: *mut core::ffi::c_void) -> bool {
    if hwnd.is_null() {
        return false;
    }
    #[link(name = "user32")]
    unsafe extern "system" {
        fn GetClassNameW(hwnd: *mut core::ffi::c_void, buf: *mut u16, n: i32) -> i32;
    }
    let mut buf = [0u16; 64];
    let n = unsafe { GetClassNameW(hwnd, buf.as_mut_ptr(), buf.len() as i32) };
    if n <= 0 {
        return false;
    }
    let class = String::from_utf16_lossy(&buf[..n as usize]);
    class == "StguiDockGrip"
}

#[cfg(not(windows))]
pub fn is_grip_hwnd(_hwnd: *mut core::ffi::c_void) -> bool {
    false
}

/// Temporarily hide the chat/move seam grips (Chatterino usercard, etc.).
pub fn suppress_seam_grips() {
    #[cfg(windows)]
    post_cmd(DockCmd::SuppressSeam);
}

/// Put seam grips back without rebuilding tile grips.
pub fn restore_seam_grips() {
    #[cfg(windows)]
    post_cmd(DockCmd::RestoreSeam);
}

#[cfg(windows)]
fn grip_thread_main() {
    let class_name = wide("StguiDockGrip");
    let instance = unsafe { GetModuleHandleW(std::ptr::null()) };
    let wc = WndClassEx {
        cb_size: std::mem::size_of::<WndClassEx>() as u32,
        style: 0,
        wnd_proc: Some(wnd_proc),
        cls_extra: 0,
        wnd_extra: 0,
        instance,
        icon: std::ptr::null_mut(),
        cursor: unsafe { LoadCursorW(std::ptr::null_mut(), IDC_SIZEWE) },
        background: std::ptr::null_mut(),
        menu_name: std::ptr::null(),
        class_name: class_name.as_ptr(),
        icon_sm: std::ptr::null_mut(),
    };
    unsafe {
        RegisterClassExW(&wc);
    }

    // Create initial chat + mover grip HWNDs (hidden until sync).
    {
        let chat = unsafe { create_grip(instance, class_name.as_ptr(), 1) };
        let mover = unsafe { create_grip(instance, class_name.as_ptr(), 10) };
        unsafe {
            if !chat.is_null() {
                ShowWindow(chat, 0);
            }
            if !mover.is_null() {
                ShowWindow(mover, 0);
            }
        }
        if let Ok(mut g) = grips().lock() {
            g.chat = chat;
            g.mover = mover;
        }
    }

    // Global hotkey so Ctrl+Shift+M works while mpv has focus.
    unsafe {
        let _ = RegisterHotKey(
            std::ptr::null_mut(),
            HOTKEY_CYCLE,
            MOD_CONTROL | MOD_SHIFT | MOD_NOREPEAT,
            VK_M,
        );
    }

    loop {
        // Drain commands
        let cmds = {
            let mut q = cmd_queue().lock().unwrap_or_else(|e| e.into_inner());
            std::mem::take(&mut *q)
        };
        for cmd in cmds {
            match cmd {
                DockCmd::Sync => sync_grips_full(instance, class_name.as_ptr()),
                DockCmd::HideGrips => hide_all_grips(),
                DockCmd::SuppressSeam => suppress_seam_grips_inner(),
                DockCmd::RestoreSeam => restore_seam_grips_inner(),
                DockCmd::RestackGrips(anchor) => unsafe { restack_grips_inner(anchor) },
                DockCmd::PickMonitor => {
                    if PICKER_OPEN.load(Ordering::SeqCst) {
                        dismiss_monitor_picker();
                    } else {
                        show_monitor_picker(instance, class_name.as_ptr());
                    }
                }
            }
        }

        let mut msg = unsafe { std::mem::zeroed::<Msg>() };
        // Process pending messages without blocking forever so we can poll cmds.
        let mut had = false;
        unsafe {
            while PeekMessageW(&mut msg, std::ptr::null_mut(), 0, 0, PM_REMOVE) != 0 {
                had = true;
                if msg.message == WM_HOTKEY && msg.wparam as i32 == HOTKEY_CYCLE {
                    show_monitor_picker(instance, class_name.as_ptr());
                    continue;
                }
                if msg.message == WM_DESTROY {
                    // ignore
                }
                TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }
        }
        if PICKER_OPEN.load(Ordering::SeqCst)
            && unsafe { (GetAsyncKeyState(VK_ESCAPE) as u16) & 0x8000 != 0 }
        {
            dismiss_monitor_picker();
        }
        if !had {
            thread::sleep(Duration::from_millis(16));
        }
    }
}
