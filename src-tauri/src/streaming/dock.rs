fn sync_layout_chat_fraction(chat_fraction: Option<f64>) {
    let Some(requested) = chat_fraction else {
        return;
    };
    let requested = crate::dock::clamp_chat_fraction(requested);
    let current = crate::dock::chat_fraction();
    // Ordinary session/layout refreshes always carry the persisted fraction.
    // Re-entering the interactive setter when it is unchanged synchronously
    // runs a full Win32 dock apply against mpv/Chatterino and can block for
    // seconds while Chatterino is restarting for a new multistream channel.
    if (current - requested).abs() >= 0.001 {
        crate::dock::set_chat_fraction(requested);
    }
}

/// Re-tile mpv windows for active channels; optionally leave the right strip for chat.
pub fn layout_watching(
    channels: &[String],
    reserve_chat: bool,
    layout: Option<&str>,
    linked_dock: Option<bool>,
    chat_fraction: Option<f64>,
    main_side: Option<&str>,
) -> Result<(), StreamError> {
    let cleaned: Vec<String> = channels
        .iter()
        .map(|c| c.trim().trim_start_matches('#').to_lowercase())
        .filter(|c| !c.is_empty())
        .collect();
    if cleaned.is_empty() {
        crate::dock::clear_session();
        return Ok(());
    }
    let layout = normalize_layout(layout);
    sync_layout_chat_fraction(chat_fraction);
    if let Some(side) = main_side {
        crate::dock::set_main_side(side);
    }
    let linked = linked_dock.unwrap_or_else(|| crate::dock::snapshot().linked);
    crate::dock::sync_session(&cleaned, &layout, reserve_chat, linked);
    #[cfg(windows)]
    {
        let cleaned = cleaned.clone();
        // Latest request wins: when several streams become ready at once, the
        // frontend fires layout_watching repeatedly. Older threads exit as
        // soon as they notice a newer generation instead of fighting over
        // window placement.
        let generation = LAYOUT_GENERATION.fetch_add(1, Ordering::SeqCst) + 1;
        let expected = cleaned.len().clamp(1, 8);
        thread::spawn(move || {
            thread::sleep(Duration::from_millis(400));
            let mut streak = 0;
            for _ in 0..28 {
                if LAYOUT_GENERATION.load(Ordering::SeqCst) != generation {
                    return;
                }
                let found = retile_player_windows(&cleaned, reserve_chat, &layout);
                let mut chat_ok = true;
                if reserve_chat {
                    let chat_pid = owned_chatterino_pid()
                        .lock()
                        .ok()
                        .and_then(|g| *g)
                        .unwrap_or(0);
                    chat_ok = chat_pid != 0 && find_main_window_for_pid(chat_pid).is_some();
                    if chat_ok {
                        place_chatterino_window_right(chat_pid);
                    }
                }
                if found >= expected && chat_ok {
                    streak += 1;
                    if streak >= 2 {
                        return;
                    }
                } else {
                    streak = 0;
                }
                thread::sleep(Duration::from_millis(250));
            }
        });
    }
    #[cfg(not(windows))]
    {
        let _ = (cleaned, reserve_chat, layout);
    }
    Ok(())
}

/// When true, skip retile/place so we don't un-minimize the dock group.
static DOCK_GROUP_MINIMIZED: AtomicBool = AtomicBool::new(false);

#[cfg(windows)]
fn apply_dock_layout_inner(raise_after_apply: bool) {
    if DOCK_GROUP_MINIMIZED.load(Ordering::SeqCst) {
        return;
    }
    let cfg = crate::dock::snapshot();
    if cfg.channels.is_empty() {
        return;
    }
    let _ = retile_player_windows(&cfg.channels, cfg.reserve_chat, &cfg.layout);
    if cfg.reserve_chat {
        place_chatterino_window_right(0);
        schedule_chatterino_place();
    }
    if raise_after_apply {
        raise_dock_windows(&cfg.channels, cfg.reserve_chat);
    }
    // Raising mpv buries an owned HUD under the player. Restack after.
    if let Some(app) = DOCK_APP.get() {
        restack_all_points_huds(app);
    }
}

/// Immediate retile from dock grip drags (no delayed retry loop).
pub fn apply_dock_layout() {
    #[cfg(windows)]
    {
        apply_dock_layout_inner(crate::dock::take_raise_after_apply());
    }
}

fn apply_dock_layout_cb() {
    #[cfg(windows)]
    {
        // A monitor-picker click runs this callback from the grip window-proc
        // thread. Cross-process mpv/Chatterino placement can block, so dispatch
        // monitor changes to a worker and let the grip thread process its queued
        // Sync immediately. Normal divider drags remain synchronous.
        if crate::dock::take_raise_after_apply() {
            thread::spawn(|| apply_dock_layout_inner(true));
            return;
        }
        apply_dock_layout_inner(false);
    }
    #[cfg(not(windows))]
    apply_dock_layout();
}

static CHATTERINO_PLACE_GEN: AtomicU64 = AtomicU64::new(0);

#[cfg(windows)]
fn schedule_chatterino_place() {
    let gen = CHATTERINO_PLACE_GEN.fetch_add(1, Ordering::SeqCst) + 1;
    thread::spawn(move || {
        let mut elapsed = 0u64;
        for &at in chatterino_place_retry_ms() {
            if CHATTERINO_PLACE_GEN.load(Ordering::SeqCst) != gen {
                return;
            }
            if at > elapsed {
                thread::sleep(Duration::from_millis(at - elapsed));
                elapsed = at;
            }
            if CHATTERINO_PLACE_GEN.load(Ordering::SeqCst) != gen {
                return;
            }
            let cfg = crate::dock::snapshot();
            if !cfg.reserve_chat || cfg.channels.is_empty() {
                return;
            }
            place_chatterino_window_right(0);
        }
    });
}

static DOCK_APP: OnceLock<AppHandle> = OnceLock::new();

fn emit_dock_fraction(f: f64) {
    if let Some(app) = DOCK_APP.get() {
        let _ = app.emit("dock-chat-fraction", f);
    }
}

/// Register dock callbacks once the Tauri app handle exists.
pub fn init_dock(app: AppHandle) {
    let _ = DOCK_APP.set(app);
    crate::dock::register_apply_layout(apply_dock_layout_cb);
    crate::dock::register_fraction_emit(emit_dock_fraction);
    // Starts Win32 grip thread + global Ctrl+Shift+M (works while mpv focused).
    crate::dock::start_background();
    start_dock_visibility_watchdog();
}

#[cfg(windows)]
fn start_dock_visibility_watchdog() {
    static STARTED: AtomicBool = AtomicBool::new(false);
    if STARTED.swap(true, Ordering::SeqCst) {
        return;
    }
    thread::spawn(|| {
        // 0 = shown/normal, 1 = minimized as a group
        let mut group_minimized = false;
        let mut seam_suppressed = false;
        // Remember last-known member HWNDs. Once minimized, title/area scans
        // often miss borderless mpv (iconic rect is ~160x28), so without a
        // cache the watchdog never observes IsIconic and never syncs.
        let mut cached: Vec<*mut core::ffi::c_void> = Vec::new();
        let mut last_hwnd_count = 0usize;
        let mut last_focus: Option<DockFocusKind> = None;
        let mut last_popup = false;
        let mut sleep_ms = 100u64;
        loop {
            thread::sleep(Duration::from_millis(sleep_ms));
            let cfg = crate::dock::snapshot();
            // Sync whenever the dock is active (linked grips and/or reserved chat).
            let dock_active = !cfg.channels.is_empty() && (cfg.linked || cfg.reserve_chat);
            if !dock_active {
                if group_minimized {
                    group_minimized = false;
                    DOCK_GROUP_MINIMIZED.store(false, Ordering::SeqCst);
                    crate::dock::show_grips();
                }
                cached.clear();
                last_hwnd_count = 0;
                last_focus = None;
                last_popup = false;
                sleep_ms = dock_watchdog_interval_ms(false, false);
                continue;
            }
            let found = dock_member_hwnds(&cfg.channels, cfg.reserve_chat);
            // Drop destroyed windows from the cache; merge with fresh finds.
            cached.retain(|&h| is_hwnd_alive(h));
            for &h in &found {
                if !h.is_null() && !cached.contains(&h) {
                    cached.push(h);
                }
            }
            // Prefer the union so an iconic mpv still participates.
            let hwnds: Vec<_> = if cached.is_empty() {
                found
            } else {
                cached.clone()
            };
            if hwnds.is_empty() {
                sleep_ms = dock_watchdog_interval_ms(true, true);
                continue;
            }
            let any_iconic = hwnds.iter().any(|&h| is_hwnd_iconic(h));
            let any_zoomed = hwnds.iter().any(|&h| is_hwnd_zoomed(h));
            let any_restored = hwnds.iter().any(|&h| is_hwnd_restored(h));
            let focus = if cfg.linked {
                Some(dock_focus_kind(&hwnds))
            } else {
                None
            };
            let has_popup = cfg.linked && cfg.reserve_chat && chatterino_has_overlay_popup();
            let needs_fast = dock_watchdog_needs_fast_tick(
                group_minimized,
                any_iconic,
                any_zoomed,
                focus != last_focus,
                has_popup != last_popup,
                hwnds.len() != last_hwnd_count,
            );
            last_focus = focus;
            last_popup = has_popup;
            last_hwnd_count = hwnds.len();
            sleep_ms = dock_watchdog_interval_ms(true, needs_fast);

            if !group_minimized && any_iconic {
                DOCK_GROUP_MINIMIZED.store(true, Ordering::SeqCst);
                crate::dock::hide_grips();
                minimize_dock_group(&hwnds);
                group_minimized = true;
                sleep_ms = dock_watchdog_interval_ms(true, true);
                continue;
            }
            if group_minimized {
                if any_restored {
                    DOCK_GROUP_MINIMIZED.store(false, Ordering::SeqCst);
                    restore_dock_group(&cfg.channels, cfg.reserve_chat, &cfg.layout);
                    // Clears grip-minimized latch; Sync no-ops grips when unlinked.
                    crate::dock::show_grips();
                    group_minimized = false;
                    seam_suppressed = false;
                    // Refresh cache from live finds after restore/retile.
                    cached = dock_member_hwnds(&cfg.channels, cfg.reserve_chat);
                    sleep_ms = dock_watchdog_interval_ms(true, true);
                    continue;
                }
                // Chatterino (or a new mpv) may appear after the group was
                // minimized — keep every member iconic.
                let stragglers: Vec<_> = hwnds
                    .iter()
                    .copied()
                    .filter(|&h| is_hwnd_alive(h) && !is_hwnd_iconic(h))
                    .collect();
                if !stragglers.is_empty() {
                    minimize_dock_group(&stragglers);
                    sleep_ms = dock_watchdog_interval_ms(true, true);
                }
                continue;
            }
            // Solo maximize breaks the dock — snap everyone back to tiles.
            if any_zoomed {
                restore_dock_group(&cfg.channels, cfg.reserve_chat, &cfg.layout);
                if cfg.linked {
                    crate::dock::show_grips();
                }
            }

            // Keep grey grips in the normal Z-order immediately above the
            // highest mpv/owned Chatterino member. Foreign apps therefore cover
            // the entire dock naturally; no divider enters the TOPMOST band.
            if cfg.linked {
                if let Some(anchor) = topmost_dock_member(&hwnds) {
                    crate::dock::restack_grips_above(anchor as isize);
                }
                // Preserve the existing poll-overlay focus behavior separately.
                if matches!(
                    focus.unwrap_or(DockFocusKind::Unknown),
                    DockFocusKind::DockOrApp | DockFocusKind::Unknown
                ) {
                    raise_poll_overlay();
                }
            }

            // Chatterino usercards/menus sit above the main chat window; seam
            // grips used to slice through them. Hide seam grips while a
            // secondary Chatterino window is visible.
            if cfg.linked && cfg.reserve_chat {
                if has_popup && !seam_suppressed {
                    crate::dock::suppress_seam_grips();
                    seam_suppressed = true;
                } else if !has_popup && seam_suppressed {
                    crate::dock::restore_seam_grips();
                    seam_suppressed = false;
                }
            } else if seam_suppressed {
                crate::dock::restore_seam_grips();
                seam_suppressed = false;
            }

            if cfg.reserve_chat {
                let pid = owned_chatterino_pid()
                    .lock()
                    .ok()
                    .and_then(|g| *g)
                    .filter(|p| pid_is_alive(*p))
                    .or_else(find_rillmux_dock_chatterino_pid)
                    .unwrap_or(0);
                if pid != 0 && pid_is_alive(pid) {
                    if let Ok(mut g) = owned_chatterino_pid().lock() {
                        *g = Some(pid);
                    }
                    if let Some(target) = overlay_rect_from_reserved_chat() {
                        let hwnd = find_main_window_for_pid(pid);
                        let visible = hwnd.and_then(dwm_visible_overlay_rect);
                        if chatterino_watchdog_should_place(
                            hwnd.is_some(),
                            true,
                            visible,
                            target,
                            chatterino_place_slop_px(),
                        ) {
                            place_chatterino_window_right(pid);
                        }
                    }
                } else {
                    let last_set = last_chatterino_channels()
                        .lock()
                        .ok()
                        .is_some_and(|g| !g.is_empty());
                    let elapsed = last_chatterino_watchdog_relaunch()
                        .lock()
                        .ok()
                        .and_then(|g| *g)
                        .map(|t| t.elapsed().as_millis() as u64)
                        .unwrap_or(u64::MAX);
                    if chatterino_watchdog_should_relaunch(false, true, last_set, elapsed, 2_000) {
                        if let Ok(_launch) = chatterino_launch_lock().try_lock() {
                            if let Ok(mut g) = last_chatterino_watchdog_relaunch().lock() {
                                *g = Some(Instant::now());
                            }
                            let list = last_chatterino_channels()
                                .lock()
                                .ok()
                                .map(|g| g.clone())
                                .unwrap_or_default();
                            if !list.is_empty() {
                                if let Some(path) = cached_chatterino_path() {
                                    crate::diagnostics::log_line("[chatterino] watchdog relaunch");
                                    if let Err(err) =
                                        launch_chatterino_with_path(&path, &list, true, true)
                                    {
                                        crate::diagnostics::log_line(&format!(
                                            "[chatterino] watchdog relaunch failed: {err}"
                                        ));
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    });
}

#[cfg(not(windows))]
fn start_dock_visibility_watchdog() {}

#[cfg(windows)]
#[derive(Clone, Copy, PartialEq, Eq)]
enum DockFocusKind {
    /// Our app, a grip, or a known dock member (mpv / owned Chatterino).
    DockOrApp,
    /// Some other process is foreground.
    Foreign,
    /// No FG window, or we have no member HWNDs to compare yet.
    Unknown,
}

/// Classify the foreground window using the watchdog's HWND cache (not a
/// fresh title scan — those miss borderless mpv and incorrectly demoted grips).
#[cfg(windows)]
fn dock_focus_kind(members: &[*mut core::ffi::c_void]) -> DockFocusKind {
    #[link(name = "user32")]
    unsafe extern "system" {
        fn GetForegroundWindow() -> *mut core::ffi::c_void;
        fn GetWindowThreadProcessId(hwnd: *mut core::ffi::c_void, pid: *mut u32) -> u32;
    }
    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn GetCurrentProcessId() -> u32;
    }
    let fg = unsafe { GetForegroundWindow() };
    if fg.is_null() {
        return DockFocusKind::Unknown;
    }
    if crate::dock::is_grip_hwnd(fg) {
        return DockFocusKind::DockOrApp;
    }
    let mut fg_pid = 0u32;
    unsafe {
        GetWindowThreadProcessId(fg, &mut fg_pid);
    }
    if fg_pid != 0 && fg_pid == unsafe { GetCurrentProcessId() } {
        return DockFocusKind::DockOrApp;
    }
    if members.is_empty() {
        return DockFocusKind::Unknown;
    }
    if members.contains(&fg) {
        return DockFocusKind::DockOrApp;
    }
    // mpv/Chatterino may focus a sibling top-level; match by process.
    if fg_pid == 0 {
        return DockFocusKind::Unknown;
    }
    let member_match = members.iter().any(|&h| {
        if h.is_null() {
            return false;
        }
        let mut pid = 0u32;
        unsafe {
            GetWindowThreadProcessId(h, &mut pid);
        }
        pid != 0 && pid == fg_pid
    });
    if member_match {
        DockFocusKind::DockOrApp
    } else {
        DockFocusKind::Foreign
    }
}

#[cfg(windows)]
fn chatterino_has_overlay_popup() -> bool {
    let pid = owned_chatterino_pid()
        .lock()
        .ok()
        .and_then(|g| *g)
        .unwrap_or(0);
    if pid == 0 {
        return false;
    }
    let Some(main) = find_main_window_for_pid(pid) else {
        return false;
    };
    #[link(name = "user32")]
    unsafe extern "system" {
        fn GetForegroundWindow() -> *mut core::ffi::c_void;
        fn GetWindowThreadProcessId(hwnd: *mut core::ffi::c_void, pid: *mut u32) -> u32;
        fn IsWindowVisible(hwnd: *mut core::ffi::c_void) -> i32;
        fn GetWindow(hwnd: *mut core::ffi::c_void, cmd: u32) -> *mut core::ffi::c_void;
        fn GetWindowRect(hwnd: *mut core::ffi::c_void, rect: *mut WinRect) -> i32;
    }
    const GW_OWNER: u32 = 4;
    unsafe {
        let fg = GetForegroundWindow();
        if fg.is_null() || fg == main || IsWindowVisible(fg) == 0 {
            return false;
        }
        let mut wpid = 0u32;
        GetWindowThreadProcessId(fg, &mut wpid);
        if wpid != pid {
            return false;
        }
        // Usercards are owned dialogs (or other non-main top-level) with real size.
        let owner = GetWindow(fg, GW_OWNER);
        let mut rc = WinRect {
            left: 0,
            top: 0,
            right: 0,
            bottom: 0,
        };
        if GetWindowRect(fg, &mut rc) == 0 {
            return false;
        }
        let w = (rc.right - rc.left).max(0);
        let h = (rc.bottom - rc.top).max(0);
        if w < 120 || h < 120 {
            return false;
        }
        // Owned by main chat, or any other focused Chatterino window that isn't main.
        !owner.is_null() || fg != main
    }
}

#[cfg(windows)]
fn pid_image_path(pid: u32) -> Option<String> {
    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn OpenProcess(access: u32, inherit: i32, pid: u32) -> *mut core::ffi::c_void;
        fn CloseHandle(handle: *mut core::ffi::c_void) -> i32;
        fn QueryFullProcessImageNameW(
            process: *mut core::ffi::c_void,
            flags: u32,
            name: *mut u16,
            size: *mut u32,
        ) -> i32;
    }
    const PROCESS_QUERY_LIMITED_INFORMATION: u32 = 0x1000;
    let handle = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid) };
    if handle.is_null() {
        return None;
    }
    let mut buf = [0u16; 512];
    let mut size = buf.len() as u32;
    let ok = unsafe { QueryFullProcessImageNameW(handle, 0, buf.as_mut_ptr(), &mut size) };
    unsafe {
        let _ = CloseHandle(handle);
    }
    if ok == 0 || size == 0 {
        return None;
    }
    Some(String::from_utf16_lossy(&buf[..size as usize]))
}

#[cfg(windows)]
fn pid_is_alive(pid: u32) -> bool {
    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn OpenProcess(access: u32, inherit: i32, pid: u32) -> *mut core::ffi::c_void;
        fn GetExitCodeProcess(handle: *mut core::ffi::c_void, code: *mut u32) -> i32;
        fn CloseHandle(handle: *mut core::ffi::c_void) -> i32;
    }
    const PROCESS_QUERY_LIMITED_INFORMATION: u32 = 0x1000;
    const STILL_ACTIVE: u32 = 259;
    let handle = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid) };
    if handle.is_null() {
        return false;
    }
    let mut code = 0u32;
    let ok = unsafe { GetExitCodeProcess(handle, &mut code) };
    unsafe {
        let _ = CloseHandle(handle);
    }
    ok != 0 && code == STILL_ACTIVE
}

#[cfg(not(windows))]
fn pid_is_alive(_pid: u32) -> bool {
    false
}

#[cfg(windows)]
fn dock_member_hwnds(channels: &[String], reserve_chat: bool) -> Vec<*mut core::ffi::c_void> {
    let mut out = Vec::new();
    for channel in channels.iter().take(8) {
        if let Some(hwnd) = find_player_window(channel) {
            out.push(hwnd);
        }
    }
    if reserve_chat {
        let pid = owned_chatterino_pid()
            .lock()
            .ok()
            .and_then(|g| *g)
            .unwrap_or(0);
        if let Some(hwnd) = find_main_window_for_pid(pid) {
            out.push(hwnd);
        }
    }
    out
}

#[cfg(windows)]
fn topmost_dock_member(members: &[*mut core::ffi::c_void]) -> Option<*mut core::ffi::c_void> {
    #[link(name = "user32")]
    unsafe extern "system" {
        fn GetTopWindow(hwnd: *mut core::ffi::c_void) -> *mut core::ffi::c_void;
        fn GetWindow(hwnd: *mut core::ffi::c_void, cmd: u32) -> *mut core::ffi::c_void;
    }
    const GW_HWNDNEXT: u32 = 2;
    let mut hwnd = unsafe { GetTopWindow(std::ptr::null_mut()) };
    for _ in 0..4096 {
        if hwnd.is_null() {
            break;
        }
        if members.contains(&hwnd) && is_hwnd_alive(hwnd) {
            return Some(hwnd);
        }
        hwnd = unsafe { GetWindow(hwnd, GW_HWNDNEXT) };
    }
    members.iter().copied().find(|&hwnd| is_hwnd_alive(hwnd))
}

#[cfg(windows)]
fn is_hwnd_alive(hwnd: *mut core::ffi::c_void) -> bool {
    #[link(name = "user32")]
    unsafe extern "system" {
        fn IsWindow(hwnd: *mut core::ffi::c_void) -> i32;
    }
    !hwnd.is_null() && unsafe { IsWindow(hwnd) != 0 }
}

#[cfg(windows)]
fn is_hwnd_iconic(hwnd: *mut core::ffi::c_void) -> bool {
    #[link(name = "user32")]
    unsafe extern "system" {
        fn IsIconic(hwnd: *mut core::ffi::c_void) -> i32;
    }
    is_hwnd_alive(hwnd) && unsafe { IsIconic(hwnd) != 0 }
}

#[cfg(windows)]
fn is_hwnd_zoomed(hwnd: *mut core::ffi::c_void) -> bool {
    #[link(name = "user32")]
    unsafe extern "system" {
        fn IsZoomed(hwnd: *mut core::ffi::c_void) -> i32;
    }
    !hwnd.is_null() && unsafe { IsZoomed(hwnd) != 0 }
}

#[cfg(windows)]
fn is_hwnd_visible(hwnd: *mut core::ffi::c_void) -> bool {
    #[link(name = "user32")]
    unsafe extern "system" {
        fn IsWindowVisible(hwnd: *mut core::ffi::c_void) -> i32;
    }
    !hwnd.is_null() && unsafe { IsWindowVisible(hwnd) != 0 }
}

#[cfg(windows)]
fn is_hwnd_restored(hwnd: *mut core::ffi::c_void) -> bool {
    // IsWindowVisible stays true for minimized windows — exclude iconic.
    is_hwnd_visible(hwnd) && !is_hwnd_iconic(hwnd)
}

#[cfg(windows)]
fn minimize_dock_group(hwnds: &[*mut core::ffi::c_void]) {
    #[link(name = "user32")]
    unsafe extern "system" {
        fn ShowWindow(hwnd: *mut core::ffi::c_void, cmd: i32) -> i32;
        fn CloseWindow(hwnd: *mut core::ffi::c_void) -> i32;
    }
    // SW_FORCEMINIMIZE works more reliably for borderless mpv than SW_MINIMIZE.
    const SW_FORCEMINIMIZE: i32 = 11;
    const SW_MINIMIZE: i32 = 6;
    for &hwnd in hwnds {
        if !is_hwnd_alive(hwnd) || is_hwnd_iconic(hwnd) {
            continue;
        }
        unsafe {
            if ShowWindow(hwnd, SW_FORCEMINIMIZE) == 0 && ShowWindow(hwnd, SW_MINIMIZE) == 0 {
                let _ = CloseWindow(hwnd);
            }
        }
    }
}

#[cfg(windows)]
fn restore_dock_group(channels: &[String], reserve_chat: bool, layout: &str) {
    #[link(name = "user32")]
    unsafe extern "system" {
        fn ShowWindow(hwnd: *mut core::ffi::c_void, cmd: i32) -> i32;
    }
    const SW_RESTORE: i32 = 9;
    let hwnds = dock_member_hwnds(channels, reserve_chat);
    for &hwnd in &hwnds {
        if is_hwnd_alive(hwnd) {
            unsafe {
                let _ = ShowWindow(hwnd, SW_RESTORE);
            }
        }
    }
    // Let Win32 finish restoring before we MoveWindow / retile.
    thread::sleep(Duration::from_millis(50));
    let _ = retile_player_windows(channels, reserve_chat, layout);
    if reserve_chat {
        place_chatterino_window_right(0);
        schedule_chatterino_place();
    }
    raise_dock_windows(channels, reserve_chat);
}

pub fn dock_set_linked(enabled: bool) {
    crate::dock::set_linked(enabled);
}

pub fn dock_set_chat_fraction(f: f64) {
    crate::dock::set_chat_fraction(f);
}

pub fn dock_cycle_monitor() {
    crate::dock::cycle_monitor();
}
