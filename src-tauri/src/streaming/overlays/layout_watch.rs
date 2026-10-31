// Player layout change events emitted to the frontend.

pub const PLAYER_LAYOUT_CHANGED_EVENT: &str = "player-layout-changed";

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct PlayerLayoutChanged {
    channel: String,
}

pub fn init_player_layout_watch(app: AppHandle) {
    #[cfg(windows)]
    start_player_layout_watch(app);
    #[cfg(not(windows))]
    let _ = app;
}

#[cfg(windows)]
fn start_player_layout_watch(app: AppHandle) {
    static STARTED: AtomicBool = AtomicBool::new(false);
    if STARTED.swap(true, Ordering::SeqCst) {
        return;
    }
    let _ = PLAYER_LAYOUT_APP.set(app);
    thread::spawn(|| {
        const WINEVENT_OUTOFCONTEXT: u32 = 0x0000;
        const WINEVENT_SKIPOWNPROCESS: u32 = 0x0002;
        #[link(name = "user32")]
        unsafe extern "system" {
            fn SetWinEventHook(
                event_min: u32,
                event_max: u32,
                module: *mut core::ffi::c_void,
                proc: unsafe extern "system" fn(
                    *mut core::ffi::c_void,
                    u32,
                    *mut core::ffi::c_void,
                    i32,
                    i32,
                    u32,
                    u32,
                ),
                pid: u32,
                tid: u32,
                flags: u32,
            ) -> *mut core::ffi::c_void;
            fn GetMessageW(
                msg: *mut Win32Msg,
                hwnd: *mut core::ffi::c_void,
                min: u32,
                max: u32,
            ) -> i32;
        }
        let flags = WINEVENT_OUTOFCONTEXT | WINEVENT_SKIPOWNPROCESS;
        unsafe {
            let location = SetWinEventHook(
                EVENT_OBJECT_LOCATIONCHANGE,
                EVENT_OBJECT_LOCATIONCHANGE,
                core::ptr::null_mut(),
                on_player_layout_win_event,
                0,
                0,
                flags,
            );
            let movesize = SetWinEventHook(
                EVENT_SYSTEM_MOVESIZEEND,
                EVENT_SYSTEM_MOVESIZEEND,
                core::ptr::null_mut(),
                on_player_layout_win_event,
                0,
                0,
                flags,
            );
            let location_ok = !location.is_null();
            let movesize_ok = !movesize.is_null();
            if !location_ok || !movesize_ok {
                crate::diagnostics::log_event(
                    crate::diagnostics::DebugCategory::Windows,
                    "player-layout.hook",
                    &format!("location={location_ok} movesize={movesize_ok}"),
                );
            }
            if !player_layout_watch_should_pump(location_ok, movesize_ok) {
                return;
            }
            let mut msg = core::mem::MaybeUninit::<Win32Msg>::zeroed();
            while GetMessageW(msg.as_mut_ptr(), core::ptr::null_mut(), 0, 0) > 0 {}
        }
    });
}

static PLAYER_LAYOUT_APP: OnceLock<AppHandle> = OnceLock::new();
static PLAYER_LAYOUT_LAST_EMIT: OnceLock<Mutex<HashMap<String, Instant>>> = OnceLock::new();

#[cfg(windows)]
unsafe extern "system" fn on_player_layout_win_event(
    _hook: *mut core::ffi::c_void,
    event: u32,
    hwnd: *mut core::ffi::c_void,
    id_object: i32,
    id_child: i32,
    _thread: u32,
    _time: u32,
) {
    if hwnd.is_null() {
        return;
    }
    if !player_layout_event_relevant(event, id_object, id_child) {
        return;
    }
    let Some(app) = PLAYER_LAYOUT_APP.get() else {
        return;
    };
    let Some(state) = app.try_state::<SharedStreaming>() else {
        return;
    };
    let Ok(map) = state.inner.lock() else {
        return;
    };
    let channels: Vec<String> = map
        .values()
        .map(|session| session.info.channel.clone())
        .collect();
    drop(map);
    if channels.is_empty() {
        return;
    }
    let Some(channel) = player_channel_for_title(&hwnd_window_title(hwnd), &channels) else {
        return;
    };
    let now = Instant::now();
    let force = event == EVENT_SYSTEM_MOVESIZEEND;
    let mut last = PLAYER_LAYOUT_LAST_EMIT
        .get_or_init(|| Mutex::new(HashMap::new()))
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let due = player_layout_emit_due(
        last.get(&channel).copied(),
        now,
        Duration::from_millis(16),
        force,
    );
    if !due {
        return;
    }
    last.insert(channel.clone(), now);
    drop(last);
    let _ = app.emit(PLAYER_LAYOUT_CHANGED_EVENT, PlayerLayoutChanged { channel });
}

#[cfg(windows)]
fn hwnd_window_title(hwnd: *mut core::ffi::c_void) -> String {
    #[link(name = "user32")]
    unsafe extern "system" {
        fn GetWindowTextW(hwnd: *mut core::ffi::c_void, buf: *mut u16, max: i32) -> i32;
    }
    let mut buf = [0u16; 512];
    let n = unsafe { GetWindowTextW(hwnd, buf.as_mut_ptr(), buf.len() as i32) };
    if n > 0 {
        String::from_utf16_lossy(&buf[..n as usize])
    } else {
        String::new()
    }
}
