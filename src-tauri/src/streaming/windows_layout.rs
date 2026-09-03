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

/// Dock arg parts for mpv, shared by the classic --player-args string and the
/// fast-start path, which spawns mpv directly with an argv vector.
/// Branded loading image used for the offline goodbye screen only.
/// Written to the temp dir once. Do not use as the initial fast-start media —
/// starting on a silent image breaks audio when the live stream attaches.
fn loading_image_path() -> Option<PathBuf> {
    static BYTES: &[u8] = include_bytes!("../../assets/loading.png");
    let path = std::env::temp_dir().join("rillmux-loading.png");
    match std::fs::metadata(&path) {
        Ok(m) if m.len() as usize == BYTES.len() => Some(path),
        _ => std::fs::write(&path, BYTES).ok().map(|_| path),
    }
}

fn mpv_dock_arg_parts(
    channel: &str,
    stream_title: &str,
    reserve_chat: bool,
    preset_args: &str,
    index: usize,
    count: usize,
    layout: Option<&str>,
) -> Vec<String> {
    let geo = mpv_geometry_for_dock(reserve_chat, index, count, layout)
        .unwrap_or_else(|| "--geometry=82%x100%+0+0".into());
    let mut parts: Vec<String> = vec![
        // Geometry first; watch-later-options-clr stops mpv restoring an old window size.
        geo,
        "--force-window=yes".into(),
        // Stay open on EOF so we can show the offline screen; we quit via IPC.
        "--keep-open=yes".into(),
        "--no-border".into(),
        // Live: don't fill a demuxer cache before showing the first frame.
        "--cache=no".into(),
        "--demuxer-readahead-secs=0.5".into(),
        "--watch-later-options-clr".into(),
        // Never inherit a muted watch-later / conf default.
        "--mute=no".into(),
    ];
    // Options the dock owns; matching preset flags are dropped. Everything
    // else the user configured (loop-*, demuxer cache, custom extras, …) is
    // kept — silently discarding it made dock mode diverge from the settings.
    // mpv is last-one-wins for repeated options, so a preset --cache=yes
    // still overrides our --cache=no default above.
    const DOCK_OWNED: &[&str] = &[
        "--geometry",
        "--window-maximized",
        "--title",
        "--force-media-title",
        "--force-window",
        "--keep-open",
        "--no-border",
        "--watch-later-options-clr",
        "--mute",
    ];
    for p in rebuild_player_args_preserving_quotes(preset_args) {
        let key = p.split('=').next().unwrap_or(p.as_str());
        if DOCK_OWNED.contains(&key) {
            continue;
        }
        if !parts.iter().any(|x| x == &p) {
            parts.push(p);
        }
    }
    // Unique title so Win32 can find this mpv window (not a browser tab named after the channel).
    let title = mpv_window_title(channel, stream_title);
    parts.push(format!("--title={title}"));
    parts.push(format!("--force-media-title={title}"));
    // Last-one-wins: keep audible even if a custom extra tried to mute.
    parts.push("--mute=no".into());
    parts
}

fn build_mpv_dock_args(
    channel: &str,
    stream_title: &str,
    reserve_chat: bool,
    preset_args: &str,
    index: usize,
    count: usize,
    layout: Option<&str>,
) -> String {
    mpv_dock_arg_parts(
        channel,
        stream_title,
        reserve_chat,
        preset_args,
        index,
        count,
        layout,
    )
    .join(" ")
}

fn sanitize_player_channel(channel: &str) -> String {
    let ch = channel
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || *c == '_' || *c == '-')
        .collect::<String>();
    if ch.is_empty() {
        "stream".into()
    } else {
        ch.to_ascii_lowercase()
    }
}

fn sanitize_player_fragment(value: &str) -> String {
    let mut out = String::new();
    let mut prev_sep = false;
    for c in value.chars() {
        if c.is_ascii_alphanumeric() || c == '_' || c == '-' {
            out.push(c.to_ascii_lowercase());
            prev_sep = false;
        } else if !out.is_empty() && !prev_sep {
            out.push('_');
            prev_sep = true;
        }
    }
    while out.ends_with('_') || out.ends_with('-') {
        out.pop();
    }
    if out.len() > 80 {
        out.truncate(80);
        while out.ends_with('_') || out.ends_with('-') {
            out.pop();
        }
    }
    if out.is_empty() {
        "stream".into()
    } else {
        out
    }
}

fn mpv_window_title(channel: &str, stream_title: &str) -> String {
    format!(
        "{}-{}",
        sanitize_player_channel(channel),
        sanitize_player_fragment(stream_title)
    )
}

fn legacy_mpv_window_title(channel: &str) -> String {
    format!(
        "{PLAYER_WINDOW_PREFIX_LEGACY}-{}",
        sanitize_player_channel(channel)
    )
}

fn prefixed_mpv_window_title(channel: &str) -> String {
    format!(
        "{PLAYER_WINDOW_PREFIX}-{}",
        sanitize_player_channel(channel)
    )
}

fn player_window_title_matches(title: &str, channel: &str) -> bool {
    let lower = title.to_ascii_lowercase();
    let ch = sanitize_player_channel(channel);
    lower.starts_with(&format!("{ch}-"))
        || lower.starts_with(&prefixed_mpv_window_title(channel))
        || lower.starts_with(&legacy_mpv_window_title(channel))
}

pub(crate) fn player_channel_for_title(title: &str, channels: &[String]) -> Option<String> {
    channels
        .iter()
        .find(|channel| player_window_title_matches(title, channel))
        .cloned()
}

pub(crate) fn player_layout_emit_due(
    last: Option<Instant>,
    now: Instant,
    min_gap: Duration,
    force: bool,
) -> bool {
    force || last.is_none_or(|prev| now.saturating_duration_since(prev) >= min_gap)
}

pub(crate) const EVENT_SYSTEM_MOVESIZEEND: u32 = 0x000B;
pub(crate) const EVENT_OBJECT_LOCATIONCHANGE: u32 = 0x800B;
pub(crate) const OBJID_WINDOW: i32 = 0;
pub(crate) const CHILDID_SELF: i32 = 0;

/// Win32 `POINT`. Field sizes follow `LONG`.
#[repr(C)]
#[allow(dead_code)]
struct Win32Point {
    x: i32,
    y: i32,
}

/// Win32 `MSG` (`tagMSG`), including trailing `DWORD lPrivate`.
/// Do not omit `l_private` and rely on tail padding: 32-bit `MSG` is 32 bytes.
#[repr(C)]
#[allow(dead_code)]
struct Win32Msg {
    hwnd: *mut core::ffi::c_void,
    message: u32,
    wparam: usize,
    lparam: isize,
    time: u32,
    pt: Win32Point,
    l_private: u32,
}

#[cfg(target_pointer_width = "64")]
const _: () = {
    assert!(core::mem::size_of::<Win32Point>() == 8);
    assert!(core::mem::align_of::<Win32Point>() == 4);
    assert!(core::mem::size_of::<Win32Msg>() == 48);
    assert!(core::mem::align_of::<Win32Msg>() == 8);
    assert!(core::mem::offset_of!(Win32Msg, hwnd) == 0);
    assert!(core::mem::offset_of!(Win32Msg, message) == 8);
    assert!(core::mem::offset_of!(Win32Msg, wparam) == 16);
    assert!(core::mem::offset_of!(Win32Msg, lparam) == 24);
    assert!(core::mem::offset_of!(Win32Msg, time) == 32);
    assert!(core::mem::offset_of!(Win32Msg, pt) == 36);
    assert!(core::mem::offset_of!(Win32Msg, l_private) == 44);
};

#[cfg(target_pointer_width = "32")]
const _: () = {
    assert!(core::mem::size_of::<Win32Point>() == 8);
    assert!(core::mem::align_of::<Win32Point>() == 4);
    assert!(core::mem::size_of::<Win32Msg>() == 32);
    assert!(core::mem::align_of::<Win32Msg>() == 4);
    assert!(core::mem::offset_of!(Win32Msg, hwnd) == 0);
    assert!(core::mem::offset_of!(Win32Msg, message) == 4);
    assert!(core::mem::offset_of!(Win32Msg, wparam) == 8);
    assert!(core::mem::offset_of!(Win32Msg, lparam) == 12);
    assert!(core::mem::offset_of!(Win32Msg, time) == 16);
    assert!(core::mem::offset_of!(Win32Msg, pt) == 20);
    assert!(core::mem::offset_of!(Win32Msg, l_private) == 28);
};

/// Top-level window geometry only. `EVENT_OBJECT_LOCATIONCHANGE` also fires for
/// caret/cursor/client objects; those must not touch session state.
pub(crate) fn player_layout_event_relevant(event: u32, id_object: i32, id_child: i32) -> bool {
    if id_child != CHILDID_SELF || id_object != OBJID_WINDOW {
        return false;
    }
    event == EVENT_SYSTEM_MOVESIZEEND || event == EVENT_OBJECT_LOCATIONCHANGE
}

pub(crate) fn player_layout_watch_should_pump(location_hook: bool, movesize_hook: bool) -> bool {
    location_hook || movesize_hook
}

#[cfg(windows)]
fn find_player_window(channel: &str) -> Option<*mut core::ffi::c_void> {
    find_window_by_title(&format!("{}-", sanitize_player_channel(channel)), false)
        .or_else(|| find_window_by_title(&prefixed_mpv_window_title(channel), true))
        .or_else(|| find_window_by_title(&legacy_mpv_window_title(channel), true))
}

/// Split player-args like a shell (keeps "quoted titles" as one token).
fn rebuild_player_args_preserving_quotes(args: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut in_quotes = false;
    for c in args.chars() {
        match c {
            '"' => {
                in_quotes = !in_quotes;
                cur.push(c);
            }
            ch if ch.is_whitespace() && !in_quotes => {
                if !cur.is_empty() {
                    out.push(std::mem::take(&mut cur));
                }
            }
            ch => cur.push(ch),
        }
    }
    if !cur.is_empty() {
        out.push(cur);
    }
    out
}

/// Prefer an exact title match (mpv `--title=channel`), else prefix / contains.
/// Includes minimized windows so dock group min/restore can still find players.
#[cfg(windows)]
fn find_window_by_title(needle: &str, exact_preferred: bool) -> Option<*mut core::ffi::c_void> {
    #[link(name = "user32")]
    unsafe extern "system" {
        fn EnumWindows(
            cb: unsafe extern "system" fn(*mut core::ffi::c_void, isize) -> i32,
            lparam: isize,
        ) -> i32;
        fn IsWindowVisible(hwnd: *mut core::ffi::c_void) -> i32;
        fn IsIconic(hwnd: *mut core::ffi::c_void) -> i32;
        fn GetWindow(hwnd: *mut core::ffi::c_void, cmd: u32) -> *mut core::ffi::c_void;
        fn GetWindowTextW(hwnd: *mut core::ffi::c_void, lp: *mut u16, n: i32) -> i32;
        fn GetWindowRect(hwnd: *mut core::ffi::c_void, rect: *mut WinRect) -> i32;
        fn GetWindowPlacement(hwnd: *mut core::ffi::c_void, place: *mut WindowPlacement) -> i32;
    }
    #[repr(C)]
    struct WindowPlacement {
        length: u32,
        flags: u32,
        show_cmd: u32,
        min_x: i32,
        min_y: i32,
        max_x: i32,
        max_y: i32,
        normal: WinRect,
    }
    const GW_OWNER: u32 = 4;
    struct Data {
        needle: String,
        exact: *mut core::ffi::c_void,
        best: *mut core::ffi::c_void,
        best_area: i64,
    }
    unsafe extern "system" fn enum_cb(hwnd: *mut core::ffi::c_void, lparam: isize) -> i32 {
        let data = &mut *(lparam as *mut Data);
        if !GetWindow(hwnd, GW_OWNER).is_null() {
            return 1;
        }
        let visible = IsWindowVisible(hwnd) != 0;
        let iconic = IsIconic(hwnd) != 0;
        // Minimized windows still report visible; accept either path.
        if !visible && !iconic {
            return 1;
        }
        let mut buf = [0u16; 512];
        let n = GetWindowTextW(hwnd, buf.as_mut_ptr(), buf.len() as i32);
        if n <= 0 {
            return 1;
        }
        let title = String::from_utf16_lossy(&buf[..n as usize]).to_ascii_lowercase();
        // Never steal browser / our own app windows when matching a channel name.
        if title.contains("chrome")
            || title.contains("firefox")
            || title.contains("edge")
            || title.contains("streamlink twitch")
        {
            return 1;
        }
        let is_exact = title == data.needle;
        let is_soft = title.contains(&data.needle);
        if !is_exact && !is_soft {
            return 1;
        }
        // GetWindowRect for iconic windows is often tiny (~taskbar button size).
        let mut area = 0i64;
        if iconic {
            let mut place = WindowPlacement {
                length: std::mem::size_of::<WindowPlacement>() as u32,
                flags: 0,
                show_cmd: 0,
                min_x: 0,
                min_y: 0,
                max_x: 0,
                max_y: 0,
                normal: WinRect {
                    left: 0,
                    top: 0,
                    right: 0,
                    bottom: 0,
                },
            };
            if GetWindowPlacement(hwnd, &mut place) != 0 {
                let r = place.normal;
                area = (r.right - r.left).max(0) as i64 * (r.bottom - r.top).max(0) as i64;
            }
        } else {
            let mut rect = WinRect {
                left: 0,
                top: 0,
                right: 0,
                bottom: 0,
            };
            if GetWindowRect(hwnd, &mut rect) == 0 {
                return 1;
            }
            area = (rect.right - rect.left).max(0) as i64 * (rect.bottom - rect.top).max(0) as i64;
        }
        if area < 20_000 {
            // Exact title + iconic: trust it even when the shell shrinks the
            // visible rect to a taskbar thumbnail size.
            if !(is_exact && iconic) {
                return 1;
            }
        }
        if is_exact {
            data.exact = hwnd;
        }
        if area > data.best_area {
            data.best_area = area;
            data.best = hwnd;
        }
        1
    }
    let mut data = Data {
        needle: needle.to_ascii_lowercase(),
        exact: std::ptr::null_mut(),
        best: std::ptr::null_mut(),
        best_area: 0,
    };
    unsafe {
        EnumWindows(enum_cb, &mut data as *mut _ as isize);
    }
    if exact_preferred && !data.exact.is_null() {
        Some(data.exact)
    } else if !data.best.is_null() {
        Some(data.best)
    } else {
        None
    }
}

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

#[cfg(windows)]
fn raise_hwnd(hwnd: *mut core::ffi::c_void, foreground: bool) {
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
        fn BringWindowToTop(hwnd: *mut core::ffi::c_void) -> i32;
        fn SetForegroundWindow(hwnd: *mut core::ffi::c_void) -> i32;
        fn GetForegroundWindow() -> *mut core::ffi::c_void;
        fn GetWindowThreadProcessId(hwnd: *mut core::ffi::c_void, pid: *mut u32) -> u32;
        fn AttachThreadInput(attach: u32, attach_to: u32, attach_flag: i32) -> i32;
        fn ShowWindow(hwnd: *mut core::ffi::c_void, cmd: i32) -> i32;
    }
    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn GetCurrentThreadId() -> u32;
    }
    const HWND_TOP: isize = 0;
    const SWP_NOMOVE: u32 = 0x0002;
    const SWP_NOSIZE: u32 = 0x0001;
    const SWP_SHOWWINDOW: u32 = 0x0040;
    const SW_SHOW: i32 = 5;
    const SWP_NOACTIVATE: u32 = 0x0010;
    const SW_SHOWNA: i32 = 8;
    if hwnd.is_null() {
        return;
    }
    unsafe {
        ShowWindow(hwnd, if foreground { SW_SHOW } else { SW_SHOWNA });
        SetWindowPos(
            hwnd,
            HWND_TOP as *mut core::ffi::c_void,
            0,
            0,
            0,
            0,
            SWP_NOMOVE | SWP_NOSIZE | SWP_SHOWWINDOW | if foreground { 0 } else { SWP_NOACTIVATE },
        );
        if foreground {
            BringWindowToTop(hwnd);
            let fg = GetForegroundWindow();
            let mut fg_pid = 0u32;
            let fg_tid = GetWindowThreadProcessId(fg, &mut fg_pid);
            let our_tid = GetCurrentThreadId();
            if fg_tid != 0 && fg_tid != our_tid {
                AttachThreadInput(our_tid, fg_tid, 1);
                let _ = SetForegroundWindow(hwnd);
                AttachThreadInput(our_tid, fg_tid, 0);
            } else {
                let _ = SetForegroundWindow(hwnd);
            }
        }
    }
}

#[cfg(windows)]
fn raise_dock_windows(channels: &[String], reserve_chat: bool) {
    raise_dock_windows_inner(channels, reserve_chat, true);
}

#[cfg(windows)]
fn restack_dock_windows(channels: &[String], reserve_chat: bool) {
    raise_dock_windows_inner(channels, reserve_chat, false);
}

#[cfg(windows)]
fn raise_dock_windows_inner(channels: &[String], reserve_chat: bool, foreground: bool) {
    let mut first = true;
    for channel in channels.iter().take(8) {
        if let Some(hwnd) = find_player_window(channel) {
            raise_hwnd(hwnd, foreground && first);
            first = false;
        }
    }
    if reserve_chat {
        let pid = owned_chatterino_pid()
            .lock()
            .ok()
            .and_then(|g| *g)
            .unwrap_or(0);
        if let Some(hwnd) = find_main_window_for_pid(pid) {
            raise_hwnd(hwnd, false);
        }
    }
    let members = dock_member_hwnds(channels, reserve_chat);
    if let Some(anchor) = topmost_dock_member(&members) {
        crate::dock::restack_grips_above(anchor as isize);
    }
    raise_poll_overlay();
    // HWND_TOP on mpv buries the Channel Points HUD/catalog. Restack after
    // players and grips so a multistream raise still leaves the catalog clickable.
    if let Some(app) = DOCK_APP.get() {
        restack_all_points_huds(app);
    }
}

#[cfg(not(windows))]
fn restack_dock_windows(_channels: &[String], _reserve_chat: bool) {}

/// Largest top-level window owned by `pid` (our spawned Chatterino only).
/// Visible --channels splits beat a cloaked/empty notebook, which otherwise
/// covers chat as a white/black sheet. Minimized windows still count so dock
/// group min/restore can find chat.
#[cfg(windows)]
fn find_main_window_for_pid(pid: u32) -> Option<*mut core::ffi::c_void> {
    if pid == 0 {
        return None;
    }
    #[link(name = "user32")]
    unsafe extern "system" {
        fn EnumWindows(
            cb: unsafe extern "system" fn(*mut core::ffi::c_void, isize) -> i32,
            lparam: isize,
        ) -> i32;
        fn IsWindowVisible(hwnd: *mut core::ffi::c_void) -> i32;
        fn IsIconic(hwnd: *mut core::ffi::c_void) -> i32;
        fn GetWindow(hwnd: *mut core::ffi::c_void, cmd: u32) -> *mut core::ffi::c_void;
        fn GetWindowThreadProcessId(hwnd: *mut core::ffi::c_void, pid: *mut u32) -> u32;
        fn GetWindowRect(hwnd: *mut core::ffi::c_void, rect: *mut WinRect) -> i32;
        fn GetWindowPlacement(hwnd: *mut core::ffi::c_void, place: *mut WindowPlacement) -> i32;
        fn GetWindowTextW(hwnd: *mut core::ffi::c_void, buf: *mut u16, max: i32) -> i32;
    }
    #[repr(C)]
    struct WindowPlacement {
        length: u32,
        flags: u32,
        show_cmd: u32,
        min_x: i32,
        min_y: i32,
        max_x: i32,
        max_y: i32,
        normal: WinRect,
    }
    const GW_OWNER: u32 = 4;
    struct Data {
        pid: u32,
        channels: String,
        best: *mut core::ffi::c_void,
        best_key: (u8, u8, i64),
    }
    fn hwnd_title(hwnd: *mut core::ffi::c_void) -> String {
        let mut buf = [0u16; 512];
        let n = unsafe { GetWindowTextW(hwnd, buf.as_mut_ptr(), buf.len() as i32) };
        if n <= 0 {
            return String::new();
        }
        String::from_utf16_lossy(&buf[..n as usize])
    }
    unsafe extern "system" fn enum_cb(hwnd: *mut core::ffi::c_void, lparam: isize) -> i32 {
        let data = &mut *(lparam as *mut Data);
        if !GetWindow(hwnd, GW_OWNER).is_null() {
            return 1;
        }
        let visible = IsWindowVisible(hwnd) != 0;
        let iconic = IsIconic(hwnd) != 0;
        let mut wpid = 0u32;
        GetWindowThreadProcessId(hwnd, &mut wpid);
        if wpid != data.pid {
            return 1;
        }
        let mut area = 0i64;
        if iconic {
            let mut place = WindowPlacement {
                length: std::mem::size_of::<WindowPlacement>() as u32,
                flags: 0,
                show_cmd: 0,
                min_x: 0,
                min_y: 0,
                max_x: 0,
                max_y: 0,
                normal: WinRect {
                    left: 0,
                    top: 0,
                    right: 0,
                    bottom: 0,
                },
            };
            if GetWindowPlacement(hwnd, &mut place) != 0 {
                let r = place.normal;
                area = (r.right - r.left).max(0) as i64 * (r.bottom - r.top).max(0) as i64;
            }
        } else {
            let mut rect = WinRect {
                left: 0,
                top: 0,
                right: 0,
                bottom: 0,
            };
            if GetWindowRect(hwnd, &mut rect) != 0 {
                area =
                    (rect.right - rect.left).max(0) as i64 * (rect.bottom - rect.top).max(0) as i64;
            }
        }
        if area < 10_000 {
            return 1;
        }
        let title_matches = chatterino_title_matches_channels(&hwnd_title(hwnd), &data.channels);
        let key = chatterino_window_pick_key(title_matches, visible, iconic, area);
        if key > data.best_key {
            data.best_key = key;
            data.best = hwnd;
        }
        1
    }
    let channels = last_chatterino_channels()
        .lock()
        .ok()
        .map(|g| g.clone())
        .unwrap_or_default();
    let mut data = Data {
        pid,
        channels,
        best: std::ptr::null_mut(),
        best_key: (0, 0, 0),
    };
    unsafe {
        EnumWindows(enum_cb, &mut data as *mut _ as isize);
    }
    if data.best.is_null() {
        None
    } else {
        Some(data.best)
    }
}

#[cfg(windows)]
fn top_level_windows_for_pid(pid: u32) -> Vec<*mut core::ffi::c_void> {
    if pid == 0 {
        return Vec::new();
    }
    #[link(name = "user32")]
    unsafe extern "system" {
        fn EnumWindows(
            cb: unsafe extern "system" fn(*mut core::ffi::c_void, isize) -> i32,
            lparam: isize,
        ) -> i32;
        fn GetWindow(hwnd: *mut core::ffi::c_void, cmd: u32) -> *mut core::ffi::c_void;
        fn GetWindowThreadProcessId(hwnd: *mut core::ffi::c_void, pid: *mut u32) -> u32;
        fn IsWindow(hwnd: *mut core::ffi::c_void) -> i32;
    }
    const GW_OWNER: u32 = 4;
    struct Data {
        pid: u32,
        hwnds: Vec<*mut core::ffi::c_void>,
    }
    unsafe extern "system" fn enum_cb(hwnd: *mut core::ffi::c_void, lparam: isize) -> i32 {
        let data = &mut *(lparam as *mut Data);
        if IsWindow(hwnd) == 0 || !GetWindow(hwnd, GW_OWNER).is_null() {
            return 1;
        }
        let mut wpid = 0u32;
        GetWindowThreadProcessId(hwnd, &mut wpid);
        if wpid == data.pid {
            data.hwnds.push(hwnd);
        }
        1
    }
    let mut data = Data {
        pid,
        hwnds: Vec::new(),
    };
    unsafe {
        EnumWindows(enum_cb, &mut data as *mut _ as isize);
    }
    data.hwnds
}

#[cfg(windows)]
fn close_extra_chatterino_windows(pid: u32, keep: *mut core::ffi::c_void) {
    #[link(name = "user32")]
    unsafe extern "system" {
        fn PostMessageW(hwnd: *mut core::ffi::c_void, msg: u32, w: usize, l: isize) -> i32;
        fn IsWindowVisible(hwnd: *mut core::ffi::c_void) -> i32;
        fn GetWindowRect(hwnd: *mut core::ffi::c_void, rect: *mut WinRect) -> i32;
        fn GetWindowTextW(hwnd: *mut core::ffi::c_void, buf: *mut u16, max: i32) -> i32;
    }
    const WM_CLOSE: u32 = 0x0010;
    let have_split = chatterino_pid_has_split_window(pid);
    for hwnd in top_level_windows_for_pid(pid) {
        if !is_hwnd_alive(hwnd) {
            continue;
        }
        let visible = unsafe { IsWindowVisible(hwnd) } != 0;
        let mut rect = WinRect {
            left: 0,
            top: 0,
            right: 0,
            bottom: 0,
        };
        let area = if unsafe { GetWindowRect(hwnd, &mut rect) } != 0 {
            (rect.right - rect.left).max(0) as i64 * (rect.bottom - rect.top).max(0) as i64
        } else {
            0
        };
        let mut buf = [0u16; 512];
        let n = unsafe { GetWindowTextW(hwnd, buf.as_mut_ptr(), buf.len() as i32) };
        let title = if n > 0 {
            String::from_utf16_lossy(&buf[..n as usize])
        } else {
            String::new()
        };
        if chatterino_should_close_duplicate_main(hwnd == keep, visible, area, &title, have_split) {
            unsafe {
                let _ = PostMessageW(hwnd, WM_CLOSE, 0, 0);
            }
        }
    }
}

#[cfg(windows)]
fn chatterino_pid_has_split_window(pid: u32) -> bool {
    let Some(hwnd) = find_main_window_for_pid(pid) else {
        return false;
    };
    let channels = last_chatterino_channels()
        .lock()
        .ok()
        .map(|g| g.clone())
        .unwrap_or_default();
    if channels.is_empty() {
        return false;
    }
    #[link(name = "user32")]
    unsafe extern "system" {
        fn GetWindowTextW(hwnd: *mut core::ffi::c_void, buf: *mut u16, max: i32) -> i32;
    }
    let mut buf = [0u16; 512];
    let n = unsafe { GetWindowTextW(hwnd, buf.as_mut_ptr(), buf.len() as i32) };
    if n <= 0 {
        return false;
    }
    let title = String::from_utf16_lossy(&buf[..n as usize]);
    chatterino_title_matches_channels(&title, &channels)
}

#[cfg(not(windows))]
fn chatterino_pid_has_split_window(_pid: u32) -> bool {
    true
}

#[cfg(windows)]
fn process_env_block(pid: u32) -> Option<Vec<u16>> {
    #[link(name = "ntdll")]
    unsafe extern "system" {
        fn NtQueryInformationProcess(
            process: *mut core::ffi::c_void,
            class: u32,
            info: *mut core::ffi::c_void,
            len: u32,
            ret: *mut u32,
        ) -> i32;
    }
    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn OpenProcess(access: u32, inherit: i32, pid: u32) -> *mut core::ffi::c_void;
        fn CloseHandle(handle: *mut core::ffi::c_void) -> i32;
        fn ReadProcessMemory(
            process: *mut core::ffi::c_void,
            addr: *const core::ffi::c_void,
            buf: *mut core::ffi::c_void,
            size: usize,
            read: *mut usize,
        ) -> i32;
    }
    const PROCESS_QUERY_INFORMATION: u32 = 0x0400;
    const PROCESS_VM_READ: u32 = 0x0010;
    const PROCESS_BASIC_INFORMATION: u32 = 0;
    #[repr(C)]
    struct Pbi {
        reserved1: usize,
        peb: usize,
        reserved2: [usize; 2],
        pid: usize,
        reserved3: usize,
    }
    let handle = unsafe { OpenProcess(PROCESS_QUERY_INFORMATION | PROCESS_VM_READ, 0, pid) };
    if handle.is_null() {
        return None;
    }
    let mut pbi = Pbi {
        reserved1: 0,
        peb: 0,
        reserved2: [0, 0],
        pid: 0,
        reserved3: 0,
    };
    let mut needed = 0u32;
    let status = unsafe {
        NtQueryInformationProcess(
            handle,
            PROCESS_BASIC_INFORMATION,
            &mut pbi as *mut _ as *mut _,
            std::mem::size_of::<Pbi>() as u32,
            &mut needed,
        )
    };
    if status < 0 || pbi.peb == 0 {
        unsafe {
            let _ = CloseHandle(handle);
        }
        return None;
    }
    unsafe fn read_usize(
        handle: *mut core::ffi::c_void,
        addr: usize,
        read_mem: unsafe extern "system" fn(
            *mut core::ffi::c_void,
            *const core::ffi::c_void,
            *mut core::ffi::c_void,
            usize,
            *mut usize,
        ) -> i32,
    ) -> Option<usize> {
        let mut value = 0usize;
        let mut n = 0usize;
        if read_mem(
            handle,
            addr as *const _,
            &mut value as *mut _ as *mut _,
            std::mem::size_of::<usize>(),
            &mut n,
        ) == 0
            || n != std::mem::size_of::<usize>()
        {
            return None;
        }
        Some(value)
    }
    let params = unsafe { read_usize(handle, pbi.peb + 0x20, ReadProcessMemory) };
    let Some(params) = params.filter(|p| *p != 0) else {
        unsafe {
            let _ = CloseHandle(handle);
        }
        return None;
    };
    // RTL_USER_PROCESS_PARAMETERS.Environment at 0x80 on x64.
    let env_ptr = unsafe { read_usize(handle, params + 0x80, ReadProcessMemory) };
    let Some(env_ptr) = env_ptr.filter(|p| *p != 0) else {
        unsafe {
            let _ = CloseHandle(handle);
        }
        return None;
    };
    let mut wide = vec![0u16; 32_768];
    let mut n = 0usize;
    let ok = unsafe {
        ReadProcessMemory(
            handle,
            env_ptr as *const _,
            wide.as_mut_ptr() as *mut _,
            wide.len() * 2,
            &mut n,
        )
    };
    unsafe {
        let _ = CloseHandle(handle);
    }
    if ok == 0 || n < 4 {
        return None;
    }
    wide.truncate(n / 2);
    Some(wide)
}

#[cfg(windows)]
fn process_has_dock_owner(pid: u32) -> bool {
    let Some(wide) = process_env_block(pid) else {
        return false;
    };
    let needle: Vec<u16> = format!("{CHATTERINO_DOCK_OWNER_ENV}={}\0", chatterino_dock_owner())
        .encode_utf16()
        .collect();
    wide.windows(needle.len())
        .any(|window| window == needle.as_slice())
}

#[cfg(not(windows))]
fn process_has_dock_owner(_pid: u32) -> bool {
    false
}

#[cfg(windows)]
fn list_chatterino_pids() -> Vec<u32> {
    #[repr(C)]
    struct ProcessEntryW {
        size: u32,
        usage: u32,
        pid: u32,
        heap_id: usize,
        module_id: u32,
        threads: u32,
        parent: u32,
        pri: i32,
        flags: u32,
        exe: [u16; 260],
    }
    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn CreateToolhelp32Snapshot(flags: u32, pid: u32) -> *mut core::ffi::c_void;
        fn Process32FirstW(snap: *mut core::ffi::c_void, pe: *mut ProcessEntryW) -> i32;
        fn Process32NextW(snap: *mut core::ffi::c_void, pe: *mut ProcessEntryW) -> i32;
        fn CloseHandle(handle: *mut core::ffi::c_void) -> i32;
    }
    const TH32CS_SNAPPROCESS: u32 = 0x0000_0002;
    const INVALID: usize = usize::MAX;
    let snap = unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) };
    if snap.is_null() || snap as usize == INVALID {
        return Vec::new();
    }
    let mut pe = ProcessEntryW {
        size: std::mem::size_of::<ProcessEntryW>() as u32,
        usage: 0,
        pid: 0,
        heap_id: 0,
        module_id: 0,
        threads: 0,
        parent: 0,
        pri: 0,
        flags: 0,
        exe: [0; 260],
    };
    let mut out = Vec::new();
    unsafe {
        if Process32FirstW(snap, &mut pe) != 0 {
            loop {
                let name = String::from_utf16_lossy(&pe.exe);
                let name = name.trim_end_matches('\0');
                if crate::overlay::image_path_looks_like_chatterino(name) {
                    out.push(pe.pid);
                }
                pe.size = std::mem::size_of::<ProcessEntryW>() as u32;
                if Process32NextW(snap, &mut pe) == 0 {
                    break;
                }
            }
        }
        let _ = CloseHandle(snap);
    }
    out
}

#[cfg(not(windows))]
fn list_chatterino_pids() -> Vec<u32> {
    Vec::new()
}

fn process_is_dock_chatterino(pid: u32) -> bool {
    process_has_dock_owner(pid)
}

fn list_rillmux_dock_chatterino_pids() -> Vec<u32> {
    list_chatterino_pids()
        .into_iter()
        .filter(|&pid| process_is_dock_chatterino(pid))
        .collect()
}

/// Only returns Chatterino carrying this Rillmux process' exact owner token.
fn find_rillmux_dock_chatterino_pid() -> Option<u32> {
    find_rillmux_dock_chatterino_pid_by_window()
        .or_else(|| list_rillmux_dock_chatterino_pids().into_iter().next())
}

#[cfg(windows)]
fn find_rillmux_dock_chatterino_pid_by_window() -> Option<u32> {
    #[link(name = "user32")]
    unsafe extern "system" {
        fn EnumWindows(
            cb: unsafe extern "system" fn(*mut core::ffi::c_void, isize) -> i32,
            lparam: isize,
        ) -> i32;
        fn GetWindowThreadProcessId(hwnd: *mut core::ffi::c_void, pid: *mut u32) -> u32;
        fn GetWindow(hwnd: *mut core::ffi::c_void, cmd: u32) -> *mut core::ffi::c_void;
    }
    const GW_OWNER: u32 = 4;
    struct Data {
        pid: u32,
    }
    unsafe extern "system" fn enum_cb(hwnd: *mut core::ffi::c_void, lparam: isize) -> i32 {
        let data = &mut *(lparam as *mut Data);
        if !GetWindow(hwnd, GW_OWNER).is_null() {
            return 1;
        }
        let mut pid = 0u32;
        GetWindowThreadProcessId(hwnd, &mut pid);
        if pid == 0 || !pid_is_alive(pid) {
            return 1;
        }
        let looks = pid_image_path(pid)
            .is_some_and(|p| crate::overlay::image_path_looks_like_chatterino(&p));
        if !looks {
            return 1;
        }
        if process_is_dock_chatterino(pid) {
            data.pid = pid;
            return 0;
        }
        1
    }
    let mut data = Data { pid: 0 };
    unsafe {
        EnumWindows(enum_cb, &mut data as *mut _ as isize);
    }
    (data.pid != 0).then_some(data.pid)
}

#[cfg(not(windows))]
fn find_rillmux_dock_chatterino_pid_by_window() -> Option<u32> {
    None
}

#[cfg(not(windows))]
fn process_command_line(_pid: u32) -> Option<String> {
    None
}

#[cfg(windows)]
fn place_chatterino_window_right(pid: u32) {
    let Some((_, Some(chat))) = chat_video_split(true) else {
        return;
    };
    // Never fall back to "any Chatterino" — that steals the user's other windows.
    let target_pid = if pid != 0 {
        pid
    } else {
        owned_chatterino_pid()
            .lock()
            .ok()
            .and_then(|g| *g)
            .or_else(find_rillmux_dock_chatterino_pid)
            .unwrap_or(0)
    };
    if let Some(hwnd) = find_main_window_for_pid(target_pid) {
        move_hwnd_to(hwnd, chat, true);
        close_extra_chatterino_windows(target_pid, hwnd);
    }
}

#[cfg(not(windows))]
fn place_chatterino_window_right(_pid: u32) {}

#[cfg(windows)]
fn retile_player_windows(channels: &[String], reserve_chat: bool, layout: &str) -> usize {
    let Some((video, _)) = chat_video_split(reserve_chat) else {
        return 0;
    };
    let n = channels.len().clamp(1, 8);
    let eff = effective_layout(n, layout);
    let mut found = 0usize;
    for (i, channel) in channels.iter().take(n).enumerate() {
        let tile = tile_rect(video, i, eff);
        if let Some(hwnd) = find_player_window(channel) {
            // Borderless mpv: no DWM frame expand (that breaks no-border windows).
            move_hwnd_to(hwnd, tile, false);
            found += 1;
        }
    }
    found
}
