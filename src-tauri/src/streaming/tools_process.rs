fn bundled_streamlink() -> Option<PathBuf> {
    let exe = std::env::current_exe().ok()?;
    let dir = exe.parent()?;
    let candidates = [
        dir.join("resources")
            .join("streamlink")
            .join("streamlinkw.exe"),
        dir.join("streamlink").join("streamlinkw.exe"),
        // Dev: relative to src-tauri/resources
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("resources")
            .join("streamlink")
            .join("streamlinkw.exe"),
    ];
    candidates.into_iter().find(|p| p.is_file())
}

fn resolve_streamlink(
    source: &str,
    custom: Option<&str>,
) -> Result<(PathBuf, String), StreamError> {
    let custom_key = custom.map(str::to_string);
    if let Ok(guard) = streamlink_cache().lock() {
        if let Some((cached_source, cached_custom, path)) = guard.as_ref() {
            if cached_source == source && cached_custom.as_deref() == custom && path.is_file() {
                return Ok((path.clone(), cached_source.clone()));
            }
        }
    }

    let resolved = match source {
        "custom" => {
            let path = custom
                .filter(|s| !s.is_empty())
                .map(PathBuf::from)
                .ok_or_else(|| StreamError::Message("custom Streamlink path is empty".into()))?;
            if !path.is_file() {
                return Err(StreamError::Message(format!(
                    "Streamlink not found at {}",
                    path.display()
                )));
            }
            (path, "custom".into())
        }
        "bundled" => {
            if let Some(path) = bundled_streamlink() {
                (path, "bundled".into())
            } else {
                find_streamlink_path()
                    .map(|p| (p, "system".into()))
                    .ok_or_else(|| StreamError::Message("Streamlink executable not found".into()))?
            }
        }
        _ => find_streamlink_path()
            .map(|p| (p, "system".into()))
            .ok_or_else(|| StreamError::Message("Streamlink executable not found".into()))?,
    };

    if let Ok(mut guard) = streamlink_cache().lock() {
        *guard = Some((source.to_string(), custom_key, resolved.0.clone()));
    }
    Ok(resolved)
}

fn resolve_player(player_id: &str, custom: Option<&str>) -> Result<Option<PathBuf>, StreamError> {
    match player_id {
        "custom" => {
            let path = custom
                .filter(|s| !s.is_empty())
                .map(PathBuf::from)
                .ok_or_else(|| StreamError::Message("custom player path is empty".into()))?;
            if !path.is_file() {
                return Err(StreamError::Message(format!(
                    "player not found at {}",
                    path.display()
                )));
            }
            Ok(Some(path))
        }
        "default" => Ok(None),
        id => {
            let names: &[&str] = match id {
                "mpv" => &["mpv.exe", "mpv"],
                "vlc" => &["vlc.exe", "vlc"],
                "mpc" => &["mpc-hc64.exe", "mpc-hc.exe", "mpc-be64.exe"],
                "potplayer" => &["PotPlayerMini64.exe", "PotPlayerMini.exe"],
                _ => &["mpv.exe", "mpv"],
            };
            if let Some(path) = which_on_path(names) {
                return Ok(Some(path));
            }
            // Prefer fast fallbacks (no --version probes) for stream start latency.
            if id == "mpv" {
                if let Ok(guard) = mpv_cache().lock() {
                    if let Some(path) = guard.as_ref() {
                        if path.is_file() {
                            return Ok(Some(path.clone()));
                        }
                    }
                }
                if let Some(path) = find_mpv_path() {
                    if let Ok(mut guard) = mpv_cache().lock() {
                        *guard = Some(path.clone());
                    }
                    return Ok(Some(path));
                }
            }
            Err(StreamError::Message(format!(
                "player '{id}' not found on PATH"
            )))
        }
    }
}

fn default_player_args(player_id: &str, channel: &str, title: &str, game: &str) -> String {
    match player_id {
        // Fallback when the UI sends no args. Prefer frontend composeMpvPlayerArgs
        // (wiki Recommendations, verified against mpv master manual).
        "mpv" => {
            let label = format!("{channel} - {game} - {title}").replace('"', "");
            format!(
                "--force-window=yes --keep-open=yes --no-border --no-keepaspect-window --loop-playlist=inf --loop-file=inf --title=\"{label}\" --force-media-title=\"{label}\""
            )
        }
        "vlc" => {
            // Same rillmux-<channel> marker mpv uses, so stop/prune can find
            // and close the window (close_player_windows_for_channel matches
            // the prefix). VLC shows it as "<title> - VLC media player".
            let label = mpv_window_title(channel);
            format!("--play-and-exit --input-title-format \"{label}\"")
        }
        _ => String::new(),
    }
}

pub fn launch_chatterino_for_channels(channels: &[String]) -> Result<String, StreamError> {
    let _launch = chatterino_launch_lock()
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    let cleaned: Vec<String> = channels
        .iter()
        .map(|c| c.trim().trim_start_matches('#').to_lowercase())
        .filter(|c| !c.is_empty())
        .collect::<std::collections::BTreeSet<_>>()
        .into_iter()
        .collect();
    if cleaned.is_empty() {
        return Err(StreamError::Message("no channels for Chatterino".into()));
    }
    let path = cached_chatterino_path()
        .ok_or_else(|| StreamError::Message("Chatterino not found".into()))?;
    let list = cleaned
        .iter()
        .map(|c| format!("t:{c}"))
        .collect::<Vec<_>>()
        .join(";");
    let tracked_pid = owned_chatterino_pid().lock().ok().and_then(|g| *g);
    let tracked_alive = tracked_pid.is_some_and(pid_is_alive);
    let discovered = find_rillmux_dock_chatterino_pid().filter(|p| pid_is_alive(*p));
    let owned_pid = chatterino_pick_owned_pid(tracked_pid, tracked_alive, discovered);
    if owned_pid.is_some() && owned_pid != tracked_pid {
        if let Ok(mut guard) = owned_chatterino_pid().lock() {
            *guard = owned_pid;
        }
    }
    let owned_alive = owned_pid.is_some();
    let last = last_chatterino_channels()
        .lock()
        .ok()
        .map(|g| g.clone())
        .unwrap_or_default();
    crate::diagnostics::log_line(&format!(
        "[chatterino] launch plan={:?} alive={owned_alive} last={last} next={list} exe={}",
        chatterino_launch_plan(owned_alive, &last, &list),
        path.display()
    ));
    match chatterino_launch_plan(owned_alive, &last, &list) {
        ChatterinoLaunchPlan::Reuse => {
            if let Some(pid) = owned_pid {
                place_chatterino_window_right(pid);
            }
            schedule_chatterino_place();
            return Ok(path.to_string_lossy().into_owned());
        }
        ChatterinoLaunchPlan::RestartOwned => {
            // Isolated APPDATA keeps the user's own window. Wait until our
            // previous Qt instance is gone — spawning into a live lockfile
            // creates a stub plus a leftover blank window on top of chat.
            close_owned_chatterino_wait(Duration::from_millis(1500));
        }
        ChatterinoLaunchPlan::SpawnFresh => {}
    }
    let spawn_epoch = current_chatterino_close_epoch();
    launch_chatterino_with_path(&path, &list, true, true)?;
    if chatterino_spawn_is_stale(spawn_epoch, current_chatterino_close_epoch()) {
        close_owned_chatterino();
        return Ok(path.to_string_lossy().into_owned());
    }
    if let Ok(mut guard) = last_chatterino_channels().lock() {
        *guard = list.clone();
    }
    Ok(path.to_string_lossy().into_owned())
}

fn normalize_layout(layout: Option<&str>) -> String {
    match layout.unwrap_or("2x2") {
        s @ ("1" | "2" | "1x2" | "1x3" | "1x4" | "2plus1" | "2x2" | "3plus1" | "3x2" | "4x2"
        | "8x1") => s.to_string(),
        _ => "2x2".into(),
    }
}

/// Kill the Chatterino process we spawned (never unrelated user windows).
pub fn close_owned_chatterino() {
    bump_chatterino_close_epoch();
    close_owned_chatterino_wait(Duration::from_millis(1500));
}

fn close_owned_chatterino_wait(timeout: Duration) {
    if let Ok(mut last) = last_chatterino_channels().lock() {
        last.clear();
    }
    let owned = owned_chatterino_pid()
        .lock()
        .ok()
        .and_then(|mut g| g.take());
    let pids = chatterino_pids_to_close(owned, &list_rillmux_dock_chatterino_pids());
    if pids.is_empty() {
        return;
    }
    #[cfg(windows)]
    {
        // Prefer WM_CLOSE so Chatterino can flush settings (e.g. currentVersion
        // for the changelog prompt). Fall back to TerminateProcess.
        for pid in &pids {
            for hwnd in top_level_windows_for_pid(*pid) {
                post_close_hwnd(hwnd);
            }
        }
        let deadline = Instant::now() + timeout;
        for pid in &pids {
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                break;
            }
            wait_pid_exit(*pid, remaining);
        }
        for pid in pids {
            if pid_is_alive(pid) {
                terminate_pid(pid);
            }
        }
    }
    #[cfg(not(windows))]
    {
        let _ = (pids, timeout);
    }
}

#[cfg(windows)]
fn post_close_hwnd(hwnd: *mut core::ffi::c_void) {
    #[link(name = "user32")]
    unsafe extern "system" {
        fn PostMessageW(hwnd: *mut core::ffi::c_void, msg: u32, w: usize, l: isize) -> i32;
    }
    const WM_CLOSE: u32 = 0x0010;
    unsafe {
        let _ = PostMessageW(hwnd, WM_CLOSE, 0, 0);
    }
}

#[cfg(windows)]
fn wait_pid_exit(pid: u32, timeout: Duration) {
    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn OpenProcess(access: u32, inherit: i32, pid: u32) -> *mut core::ffi::c_void;
        fn WaitForSingleObject(handle: *mut core::ffi::c_void, ms: u32) -> u32;
        fn CloseHandle(handle: *mut core::ffi::c_void) -> i32;
    }
    const SYNCHRONIZE: u32 = 0x0010_0000;
    let handle = unsafe { OpenProcess(SYNCHRONIZE, 0, pid) };
    if handle.is_null() {
        return;
    }
    unsafe {
        let _ = WaitForSingleObject(handle, timeout.as_millis() as u32);
        let _ = CloseHandle(handle);
    }
}

#[cfg(windows)]
fn terminate_pid(pid: u32) {
    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn OpenProcess(access: u32, inherit: i32, pid: u32) -> *mut core::ffi::c_void;
        fn TerminateProcess(handle: *mut core::ffi::c_void, exit_code: u32) -> i32;
        fn WaitForSingleObject(handle: *mut core::ffi::c_void, ms: u32) -> u32;
        fn CloseHandle(handle: *mut core::ffi::c_void) -> i32;
    }
    const PROCESS_TERMINATE: u32 = 0x0001;
    const SYNCHRONIZE: u32 = 0x0010_0000;
    unsafe {
        let handle = OpenProcess(PROCESS_TERMINATE | SYNCHRONIZE, 0, pid);
        if !handle.is_null() {
            let _ = TerminateProcess(handle, 1);
            let _ = WaitForSingleObject(handle, 2000);
            let _ = CloseHandle(handle);
        }
    }
}

/// Prune the session the instant a fast session's pre-launched mpv exits —
/// evidence: mpv exits 0.2–0.3 s after its window is closed, so blocking on
/// the process handle closes the owned Chatterino in well under a second
/// instead of waiting for the 1.5 s watchdog tick.
#[cfg(windows)]
fn watch_player_exit(pid: u32, state: SharedStreaming, app: AppHandle) {
    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn OpenProcess(access: u32, inherit: i32, pid: u32) -> *mut core::ffi::c_void;
        fn WaitForSingleObject(handle: *mut core::ffi::c_void, ms: u32) -> u32;
        fn CloseHandle(handle: *mut core::ffi::c_void) -> i32;
    }
    const SYNCHRONIZE: u32 = 0x0010_0000;
    thread::spawn(move || unsafe {
        let handle = OpenProcess(SYNCHRONIZE, 0, pid);
        if handle.is_null() {
            return;
        }
        WaitForSingleObject(handle, u32::MAX);
        CloseHandle(handle);
        if let Ok(true) = prune_dead_sessions(&state) {
            let _ = app.emit("stream-sessions-changed", ());
        }
    });
}

/// Windows Job Object wrapper: terminating the job kills the whole process
/// tree rooted at the Streamlink child — including whatever player it spawned
/// (mpv/VLC/…), regardless of window titles. Children spawned by a job member
/// join the job automatically (Windows 8+), so even the orphaned player left
/// behind by a dead Streamlink is cleaned up.
#[cfg(windows)]
mod process_job {
    pub struct JobHandle(*mut core::ffi::c_void);
    unsafe impl Send for JobHandle {}

    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn CreateJobObjectW(
            attrs: *mut core::ffi::c_void,
            name: *const u16,
        ) -> *mut core::ffi::c_void;
        fn OpenProcess(access: u32, inherit: i32, pid: u32) -> *mut core::ffi::c_void;
        fn AssignProcessToJobObject(
            job: *mut core::ffi::c_void,
            process: *mut core::ffi::c_void,
        ) -> i32;
        fn TerminateJobObject(job: *mut core::ffi::c_void, exit_code: u32) -> i32;
        fn CloseHandle(handle: *mut core::ffi::c_void) -> i32;
    }
    const PROCESS_SET_QUOTA: u32 = 0x0100;
    const PROCESS_TERMINATE: u32 = 0x0001;

    /// Create a job and assign the freshly spawned child (by PID) to it.
    /// Returns None on any failure — callers fall back to title-based closing.
    pub fn assign(pid: u32) -> Option<JobHandle> {
        unsafe {
            let job = CreateJobObjectW(std::ptr::null_mut(), std::ptr::null());
            if job.is_null() {
                return None;
            }
            let process = OpenProcess(PROCESS_SET_QUOTA | PROCESS_TERMINATE, 0, pid);
            if process.is_null() {
                let _ = CloseHandle(job);
                return None;
            }
            let ok = AssignProcessToJobObject(job, process);
            let _ = CloseHandle(process);
            if ok == 0 {
                let _ = CloseHandle(job);
                return None;
            }
            Some(JobHandle(job))
        }
    }

    /// Terminate every process in the job and close the handle.
    pub fn terminate(job: JobHandle) {
        unsafe {
            let _ = TerminateJobObject(job.0, 1);
            let _ = CloseHandle(job.0);
        }
    }
}

#[cfg(windows)]
type JobSlot = Option<process_job::JobHandle>;
#[cfg(not(windows))]
type JobSlot = ();

fn assign_job(child: &Child) -> JobSlot {
    #[cfg(windows)]
    {
        process_job::assign(child.id())
    }
    #[cfg(not(windows))]
    {
        let _ = child;
    }
}

/// Kill the player's whole process tree (job). The title-based
/// `close_player_windows_for_channel` remains as a fallback for sessions
/// whose job assignment failed at spawn time.
fn terminate_job(slot: &mut JobSlot) {
    #[cfg(windows)]
    if let Some(job) = slot.take() {
        process_job::terminate(job);
    }
    #[cfg(not(windows))]
    let _ = slot;
}

