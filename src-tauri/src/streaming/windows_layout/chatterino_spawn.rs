/// Monotonic counter serializing layout_watching retile threads (latest wins).
static LAYOUT_GENERATION: AtomicU64 = AtomicU64::new(0);

enum DockChatterinoSpawn {
    Child(Child),
    Adopted(u32),
}

fn launch_chatterino_with_path(
    path: &Path,
    channels_arg: &str,
    place_beside: bool,
    track_pid: bool,
) -> Result<(), StreamError> {
    let dock_appdata = chatterino_dock_appdata();
    let _ = fs::create_dir_all(&dock_appdata);
    seed_chatterino_dock_home(&dock_appdata);
    strip_dock_chatterino_window_layout(&dock_appdata);
    // Patch the *dock* copy, never the user's %APPDATA%\Chatterino2.
    #[cfg(windows)]
    suppress_chatterino_changelog_prompt(path, &dock_appdata);

    crate::diagnostics::log_line(&format!(
        "[chatterino] spawn exe={} channels={channels_arg} appdata={}",
        path.display(),
        dock_appdata.display()
    ));
    let spawn_epoch = current_chatterino_close_epoch();
    let spawned = spawn_dock_chatterino_process(path, channels_arg, &dock_appdata)?;
    if chatterino_spawn_is_stale(spawn_epoch, current_chatterino_close_epoch()) {
        crate::diagnostics::log_line("[chatterino] spawn discarded; stream already closed");
        match spawned {
            DockChatterinoSpawn::Child(mut child) => {
                let pid = child.id();
                let _ = child.kill();
                let _ = child.wait();
                #[cfg(windows)]
                terminate_pid(pid);
            }
            DockChatterinoSpawn::Adopted(pid) => {
                #[cfg(windows)]
                terminate_pid(pid);
                let _ = pid;
            }
        }
        return Ok(());
    }
    let pid = match &spawned {
        DockChatterinoSpawn::Child(child) => child.id(),
        DockChatterinoSpawn::Adopted(pid) => *pid,
    };
    if track_pid {
        if let Ok(mut guard) = owned_chatterino_pid().lock() {
            *guard = Some(pid);
        }
    }
    if place_beside {
        thread::spawn(move || {
            for i in 0..40 {
                let pid = owned_chatterino_pid()
                    .lock()
                    .ok()
                    .and_then(|g| *g)
                    .or_else(find_rillmux_dock_chatterino_pid)
                    .unwrap_or(pid);
                place_chatterino_window_right(pid);
                #[cfg(windows)]
                if find_main_window_for_pid(pid).is_some() {
                    thread::sleep(Duration::from_millis(80));
                    place_chatterino_window_right(pid);
                    // Isolated profiles open a blank notebook first. Keep
                    // waiting until the --channels split exists, or we time out.
                    if chatterino_pid_has_split_window(pid) || i >= 24 {
                        break;
                    }
                }
                thread::sleep(Duration::from_millis(50));
            }
        });
    }
    if let DockChatterinoSpawn::Child(child) = spawned {
        thread::spawn(move || {
            let mut child = child;
            let status = child.wait();
            crate::diagnostics::log_line(&format!(
                "[chatterino] child pid={pid} exited status={status:?}"
            ));
            if !track_pid {
                return;
            }
            let dock_pids = list_rillmux_dock_chatterino_pids();
            if let Ok(mut guard) = owned_chatterino_pid().lock() {
                let keep = chatterino_pid_after_child_exit(*guard, pid, &dock_pids);
                if let Some(keep) = keep {
                    if Some(keep) != Some(pid) {
                        crate::diagnostics::log_line(&format!(
                            "[chatterino] stub exited; keeping dock pid={keep}"
                        ));
                    }
                }
                *guard = keep;
            }
        });
    }
    Ok(())
}

fn independent_chatterino_appdata(channel: &str) -> PathBuf {
    let safe = channel
        .trim()
        .trim_start_matches("t:")
        .trim_start_matches('#')
        .to_ascii_lowercase();
    chatterino_dock_appdata().join("independent").join(safe)
}

/// Independent streams own one Chatterino profile and process per channel.
/// The shared dock profile stays reserved for the coordinated Multistream split.
pub fn open_independent_chatterino(channel: &str) -> Result<(), StreamError> {
    let key = channel
        .trim()
        .trim_start_matches("t:")
        .trim_start_matches('#')
        .to_ascii_lowercase();
    if key.is_empty() {
        return Err(StreamError::Message("channel is empty".into()));
    }
    if let Some(pid) = independent_chatterino_pids()
        .lock()
        .ok()
        .and_then(|p| p.get(&key).copied())
    {
        if pid_is_alive(pid) {
            return Ok(());
        }
    }
    let path = cached_chatterino_path()
        .ok_or_else(|| StreamError::Message("Chatterino executable was not found".into()))?;
    let appdata = independent_chatterino_appdata(&key);
    let _ = fs::create_dir_all(&appdata);
    seed_chatterino_dock_home(&appdata);
    strip_dock_chatterino_window_layout(&appdata);
    #[cfg(windows)]
    suppress_chatterino_changelog_prompt(&path, &appdata);
    let channels_arg = format!("t:{key}");
    let spawned = spawn_dock_chatterino_process(&path, &channels_arg, &appdata)?;
    let pid = match &spawned {
        DockChatterinoSpawn::Child(child) => child.id(),
        DockChatterinoSpawn::Adopted(pid) => *pid,
    };
    if let Ok(mut pids) = independent_chatterino_pids().lock() {
        pids.insert(key.clone(), pid);
    }
    if let DockChatterinoSpawn::Child(mut child) = spawned {
        thread::spawn(move || {
            let _ = child.wait();
            if let Ok(mut pids) = independent_chatterino_pids().lock() {
                if pids.get(&key).copied() == Some(pid) {
                    pids.remove(&key);
                }
            }
        });
    }
    Ok(())
}

pub fn close_independent_chatterino(channel: &str) {
    let key = channel
        .trim()
        .trim_start_matches("t:")
        .trim_start_matches('#')
        .to_ascii_lowercase();
    let pid = independent_chatterino_pids()
        .lock()
        .ok()
        .and_then(|mut pids| pids.remove(&key));
    if let Some(pid) = pid {
        #[cfg(windows)]
        {
            for hwnd in top_level_windows_for_pid(pid) {
                post_close_hwnd(hwnd);
            }
            wait_pid_exit(pid, Duration::from_millis(600));
            if pid_is_alive(pid) {
                terminate_pid(pid);
            }
        }
    }
}

pub fn close_all_independent_chatterino() {
    let channels: Vec<String> = independent_chatterino_pids()
        .lock()
        .ok()
        .map(|pids| pids.keys().cloned().collect())
        .unwrap_or_default();
    for channel in channels {
        close_independent_chatterino(&channel);
    }
}

fn spawnable_exe_path(path: &Path) -> PathBuf {
    #[cfg(windows)]
    {
        const PREFIX: &str = r"\\?\";
        if let Some(s) = path.to_str() {
            if let Some(stripped) = s.strip_prefix(PREFIX) {
                return PathBuf::from(stripped);
            }
        }
    }
    path.to_path_buf()
}

fn dock_chatterino_command(path: &Path, channels_arg: &str, dock_appdata: &Path) -> Command {
    let mut cmd = Command::new(spawnable_exe_path(path));
    cmd.env("APPDATA", dock_appdata);
    cmd.env(CHATTERINO_DOCK_OWNER_ENV, chatterino_dock_owner());
    // WebView2/Tauri can leak Qt plugin paths. Chatterino then loads the
    // wrong platform plugin and exits before a window exists.
    for key in [
        "QT_PLUGIN_PATH",
        "QT_QPA_PLATFORM_PLUGIN_PATH",
        "QT_QPA_PLATFORM",
        "QTDIR",
        "QML2_IMPORT_PATH",
        "QT_DEBUG_PLUGINS",
    ] {
        cmd.env_remove(key);
    }
    if let Some(dir) = path.parent() {
        cmd.current_dir(dir);
    }
    // Never pass -geometry / unknown flags: QCommandLineParser can exit 1.
    cmd.arg(format!("--channels={channels_arg}"))
        .stdin(Stdio::null())
        .stdout(chatterino_dock_stdio())
        .stderr(chatterino_dock_stdio());
    cmd
}

#[cfg(windows)]
fn windows_dock_spawn_flags() -> &'static [u32] {
    const CREATE_UNICODE_ENVIRONMENT: u32 = 0x0000_0400;
    // Tauri/WebView2 jobs deny CREATE_BREAKAWAY_FROM_JOB (os error 5).
    // Spawn without job flags; that is what actually stays alive.
    &[0, CREATE_UNICODE_ENVIRONMENT]
}

fn spawn_dock_chatterino_process(
    path: &Path,
    channels_arg: &str,
    dock_appdata: &Path,
) -> Result<DockChatterinoSpawn, StreamError> {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        let mut last_spawn_err: Option<std::io::Error> = None;
        let mut last_exit = String::from("exited");
        for &flags in windows_dock_spawn_flags() {
            let mut cmd = dock_chatterino_command(path, channels_arg, dock_appdata);
            if flags != 0 {
                cmd.creation_flags(flags);
            }
            crate::diagnostics::log_line(&format!(
                "[chatterino] spawn attempt flags=0x{flags:08x} exe={}",
                path.display()
            ));
            match cmd.spawn() {
                Err(err) => {
                    crate::diagnostics::log_line(&format!(
                        "[chatterino] spawn failed flags=0x{flags:08x}: {err}"
                    ));
                    last_spawn_err = Some(err);
                }
                Ok(mut child) => {
                    let pid = child.id();
                    crate::diagnostics::log_line(&format!(
                        "[chatterino] spawn pid={pid} flags=0x{flags:08x}"
                    ));
                    thread::sleep(Duration::from_millis(800));
                    match child.try_wait() {
                        Ok(Some(status)) => {
                            last_exit = format!("{status:?}");
                            crate::diagnostics::log_line(&format!(
                                "[chatterino] child pid={pid} exited immediately status={status:?}"
                            ));
                            let dock_pids = list_rillmux_dock_chatterino_pids();
                            if let Some(keep) =
                                chatterino_pid_after_child_exit(Some(pid), pid, &dock_pids)
                            {
                                crate::diagnostics::log_line(&format!(
                                    "[chatterino] adopted surviving dock pid={keep}"
                                ));
                                return Ok(DockChatterinoSpawn::Adopted(keep));
                            }
                        }
                        Ok(None) | Err(_) => return Ok(DockChatterinoSpawn::Child(child)),
                    }
                }
            }
        }
        if let Some(err) = last_spawn_err {
            return Err(StreamError::Message(format!(
                "failed to start Chatterino ({}): {err}",
                path.display()
            )));
        }
        Err(StreamError::Message(format!(
            "Chatterino exited immediately ({last_exit})"
        )))
    }
    #[cfg(not(windows))]
    {
        let mut cmd = dock_chatterino_command(path, channels_arg, dock_appdata);
        let child = cmd.spawn().map_err(|err| {
            StreamError::Message(format!(
                "failed to start Chatterino ({}): {err}",
                path.display()
            ))
        })?;
        crate::diagnostics::log_line(&format!("[chatterino] spawn pid={}", child.id()));
        Ok(DockChatterinoSpawn::Child(child))
    }
}

fn chatterino_dock_stdio() -> Stdio {
    let path = crate::diagnostics::logs_dir().join("chatterino-dock.log");
    OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .map(Stdio::from)
        .unwrap_or_else(|_| Stdio::null())
}

/// Align `%APPDATA%\Chatterino2\Settings\settings.json` → `misc.currentVersion`
/// with the executable's ProductVersion so the changelog QMessageBox is skipped.
#[cfg(windows)]
fn suppress_chatterino_changelog_prompt(exe: &Path, appdata: &Path) {
    let Some(ver) = file_product_version(exe) else {
        return;
    };
    let path = appdata
        .join("Chatterino2")
        .join("Settings")
        .join("settings.json");
    if !path.is_file() {
        return;
    }
    let Ok(raw) = std::fs::read_to_string(&path) else {
        return;
    };
    let Ok(mut value) = serde_json::from_str::<serde_json::Value>(&raw) else {
        return;
    };
    let Some(misc) = value.get_mut("misc") else {
        // Create misc object if missing.
        value
            .as_object_mut()
            .map(|o| o.insert("misc".into(), serde_json::json!({ "currentVersion": ver })));
        let _ = std::fs::write(&path, serde_json::to_string_pretty(&value).unwrap_or(raw));
        return;
    };
    let current = misc
        .get("currentVersion")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    if current == ver {
        return;
    }
    if let Some(obj) = misc.as_object_mut() {
        obj.insert("currentVersion".into(), serde_json::Value::String(ver));
    }
    if let Ok(out) = serde_json::to_string_pretty(&value) {
        let _ = std::fs::write(&path, out);
    }
}

const VS_FIXED_FILE_INFO_SIGNATURE: u32 = 0xFEEF04BD;
const DWORD_BYTES: usize = std::mem::size_of::<u32>();
const VS_FIXED_FILE_INFO_SIZE: usize = 13 * DWORD_BYTES;
const PRODUCT_VERSION_MS_OFFSET: usize = 4 * DWORD_BYTES;
const PRODUCT_VERSION_LS_OFFSET: usize = 5 * DWORD_BYTES;

fn read_u32_le(bytes: &[u8], offset: usize) -> Option<u32> {
    let end = offset.checked_add(DWORD_BYTES)?;
    let raw: [u8; 4] = bytes.get(offset..end)?.try_into().ok()?;
    Some(u32::from_le_bytes(raw))
}

fn fixed_file_product_version(buf: &[u8], offset: usize, reported_len: usize) -> Option<String> {
    if reported_len < VS_FIXED_FILE_INFO_SIZE {
        return None;
    }
    let end = offset.checked_add(VS_FIXED_FILE_INFO_SIZE)?;
    let info = buf.get(offset..end)?;
    if read_u32_le(info, 0)? != VS_FIXED_FILE_INFO_SIGNATURE {
        return None;
    }
    let product_version_ms = read_u32_le(info, PRODUCT_VERSION_MS_OFFSET)?;
    let product_version_ls = read_u32_le(info, PRODUCT_VERSION_LS_OFFSET)?;
    let major = (product_version_ms >> 16) & 0xffff;
    let minor = product_version_ms & 0xffff;
    let patch = (product_version_ls >> 16) & 0xffff;
    let build = product_version_ls & 0xffff;
    if build == 0 {
        Some(format!("{major}.{minor}.{patch}"))
    } else {
        Some(format!("{major}.{minor}.{patch}.{build}"))
    }
}

#[cfg(windows)]
fn file_product_version(exe: &Path) -> Option<String> {
    #[link(name = "version")]
    unsafe extern "system" {
        fn GetFileVersionInfoSizeW(path: *const u16, handle: *mut u32) -> u32;
        fn GetFileVersionInfoW(
            path: *const u16,
            handle: u32,
            len: u32,
            data: *mut core::ffi::c_void,
        ) -> i32;
        fn VerQueryValueW(
            block: *const core::ffi::c_void,
            sub: *const u16,
            buf: *mut *mut core::ffi::c_void,
            len: *mut u32,
        ) -> i32;
    }
    let wide: Vec<u16> = exe
        .as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect();
    unsafe {
        let mut dummy = 0u32;
        let size = GetFileVersionInfoSizeW(wide.as_ptr(), &mut dummy);
        if size == 0 {
            return None;
        }
        let mut buf = vec![0u8; size as usize];
        if GetFileVersionInfoW(wide.as_ptr(), 0, size, buf.as_mut_ptr().cast()) == 0 {
            return None;
        }
        let sub: Vec<u16> = "\\".encode_utf16().chain(std::iter::once(0)).collect();
        let mut ptr: *mut core::ffi::c_void = std::ptr::null_mut();
        let mut len = 0u32;
        if VerQueryValueW(buf.as_ptr().cast(), sub.as_ptr(), &mut ptr, &mut len) == 0
            || ptr.is_null()
        {
            return None;
        }
        // The API returns an address into `buf`. Use it only to derive
        // a checked offset, then parse the owned bytes safely.
        let buf_start = buf.as_ptr() as usize;
        let offset = (ptr as usize).checked_sub(buf_start)?;
        fixed_file_product_version(&buf, offset, len as usize)
    }
}

#[cfg(windows)]
#[derive(Clone, Copy, Debug)]
#[repr(C)]
struct WinRect {
    left: i32,
    top: i32,
    right: i32,
    bottom: i32,
}

#[cfg(windows)]
fn rect_from_dock(r: crate::dock::Rect) -> WinRect {
    WinRect {
        left: r.left,
        top: r.top,
        right: r.right,
        bottom: r.bottom,
    }
}

/// Video left / chat right. Uses linked-dock work monitor + chat fraction.
#[cfg(windows)]
fn chat_video_split(reserve_chat: bool) -> Option<(WinRect, Option<WinRect>)> {
    let (video, chat) = crate::dock::chat_video_split(reserve_chat)?;
    Some((rect_from_dock(video), chat.map(rect_from_dock)))
}

/// Effective grid for `count` running channels under the chosen preset.
/// A partially filled preset shrinks to the count-based grid, so a single
/// stream never lands in a quarter tile of the default "2x2" preset.
/// Asymmetric presets keep their split whenever 2+ channels run.
#[cfg(windows)]
fn effective_layout(count: usize, preset: &str) -> &str {
    if preset == "3plus1" && count >= 2 {
        return "3plus1";
    }
    if preset == "2plus1" && count >= 2 {
        return "2plus1";
    }
    if preset == "8x1" && count >= 2 {
        return "8x1";
    }
    if (preset == "1x2" || preset == "1x3" || preset == "1x4") && count >= 2 {
        return preset;
    }
    match count {
        0 | 1 => "1",
        2 => "2",
        3 | 4 => "2x2",
        5 | 6 => "3x2",
        _ => "4x2",
    }
}

#[cfg(windows)]
fn tile_rect(video: WinRect, index: usize, layout: &str) -> WinRect {
    let r = crate::dock::tile_rect(
        crate::dock::Rect {
            left: video.left,
            top: video.top,
            right: video.right,
            bottom: video.bottom,
        },
        index,
        layout,
    );
    rect_from_dock(r)
}

/// Pixel-exact launch geometry for the planned tile, computed with the same
/// math as the retile pass (measured: mpv honors pixel geometry exactly, so
/// the window opens already snapped instead of resizing visibly afterwards).
/// The retile pass still runs afterwards — the final tiling depends on how
/// many streams are running once the player is ready, which can change
/// between launch and ready.
#[cfg(windows)]
fn mpv_geometry_for_dock(
    reserve_chat: bool,
    index: usize,
    count: usize,
    layout: Option<&str>,
) -> Option<String> {
    let (video, _) = chat_video_split(reserve_chat)?;
    let preset = normalize_layout(layout);
    let n = count.clamp(1, 8);
    let eff = effective_layout(n, &preset);
    let tile = tile_rect(video, index.min(n - 1), eff);
    let w = (tile.right - tile.left).max(1);
    let h = (tile.bottom - tile.top).max(1);
    Some(format!(
        "--geometry={w}x{h}+{x}+{y}",
        x = tile.left,
        y = tile.top
    ))
}

#[cfg(not(windows))]
fn mpv_geometry_for_dock(
    _reserve_chat: bool,
    _index: usize,
    _count: usize,
    _layout: Option<&str>,
) -> Option<String> {
    None
}
