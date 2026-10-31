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

#[cfg(windows)]
fn independent_chat_key(channel: &str) -> String {
    channel
        .trim()
        .trim_start_matches("t:")
        .trim_start_matches('#')
        .to_ascii_lowercase()
}

#[cfg(windows)]
fn independent_chat_hwnd(channel: &str) -> Option<*mut core::ffi::c_void> {
    let key = independent_chat_key(channel);
    let pid = independent_chatterino_pids()
        .lock()
        .ok()
        .and_then(|pids| pids.get(&key).copied())?;
    find_main_window_for_pid(pid)
}

#[cfg(windows)]
fn place_independent_pairs() -> usize {
    let mut ready = 0usize;
    for (channel, video, chat) in crate::dock::independent_window_splits() {
        let player = find_player_window(&channel);
        let chat_hwnd = independent_chat_hwnd(&channel);
        if let Some(hwnd) = player {
            move_hwnd_to(hwnd, rect_from_dock(video), false);
        }
        if let Some(hwnd) = chat_hwnd {
            move_hwnd_to(hwnd, rect_from_dock(chat), true);
            if let Some(pid) = independent_chatterino_pids()
                .lock()
                .ok()
                .and_then(|pids| pids.get(&independent_chat_key(&channel)).copied())
            {
                close_extra_chatterino_windows(pid, hwnd);
            }
        }
        if player.is_some() && chat_hwnd.is_some() {
            ready += 1;
        }
    }
    ready
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
