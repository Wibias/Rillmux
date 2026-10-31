// Session teardown and dock ownership handover.

#[cfg(windows)]
fn wait_session_processes(state: &StreamingState, timeout_ms: u32) {
    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn CloseHandle(handle: *mut core::ffi::c_void) -> i32;
        fn DuplicateHandle(
            source_process: *mut core::ffi::c_void,
            source: *mut core::ffi::c_void,
            target_process: *mut core::ffi::c_void,
            target: *mut *mut core::ffi::c_void,
            desired: u32,
            inherit: i32,
            options: u32,
        ) -> i32;
        fn GetCurrentProcess() -> *mut core::ffi::c_void;
        fn WaitForMultipleObjects(
            count: u32,
            handles: *const *mut core::ffi::c_void,
            wait_all: i32,
            millis: u32,
        ) -> u32;
    }
    const DUPLICATE_SAME_ACCESS: u32 = 0x0000_0002;
    let duplicates: Vec<*mut core::ffi::c_void> = {
        let Ok(map) = state.inner.lock() else {
            thread::sleep(Duration::from_millis(timeout_ms as u64));
            return;
        };
        let process = unsafe { GetCurrentProcess() };
        let mut duplicates = Vec::new();
        let mut push_dup = |source: *mut core::ffi::c_void| {
            if source.is_null() {
                return;
            }
            let mut dup = std::ptr::null_mut();
            let ok = unsafe {
                DuplicateHandle(
                    process,
                    source,
                    process,
                    &mut dup,
                    0,
                    0,
                    DUPLICATE_SAME_ACCESS,
                )
            };
            if ok != 0 && !dup.is_null() {
                duplicates.push(dup);
            }
        };
        for session in map.values() {
            push_dup(session.child.as_raw_handle());
            if let Some(player) = &session.player {
                push_dup(player.child.as_raw_handle());
            }
        }
        duplicates
    };
    if duplicates.is_empty() {
        thread::sleep(Duration::from_millis(timeout_ms as u64));
        return;
    }
    let count = duplicates.len().min(64) as u32;
    unsafe {
        WaitForMultipleObjects(count, duplicates.as_ptr(), 0, timeout_ms);
        for handle in duplicates {
            CloseHandle(handle);
        }
    }
}

fn mpv_window_alive(channel: &str) -> bool {
    #[cfg(windows)]
    {
        find_player_window(channel).is_some()
    }
    #[cfg(not(windows))]
    {
        let _ = channel;
        true
    }
}

fn wait_child_timeout(child: &mut Child, timeout: Duration) {
    let deadline = Instant::now() + timeout;
    loop {
        match child.try_wait() {
            Ok(Some(_)) => return,
            Ok(None) if Instant::now() < deadline => {
                thread::sleep(Duration::from_millis(50));
            }
            _ => {
                let _ = child.kill();
                let _ = child.try_wait();
                return;
            }
        }
    }
}

pub fn stop_stream(state: &StreamingState, id: &str) -> Result<(), StreamError> {
    let session = {
        let mut map = state
            .inner
            .lock()
            .map_err(|_| StreamError::Message("streaming state poisoned".into()))?;
        map.remove(id)
    };
    if let Some(mut session) = session {
        let channel = session.info.channel.clone();
        let _ = session.child.kill();
        wait_child_timeout(&mut session.child, Duration::from_secs(2));
        // Streamlink exit leaves the player orphaned; with --loop-file=inf it
        // keeps replaying the buffer instead of closing. The job kills the
        // whole tree; title-based closing is the fallback.
        terminate_job(&mut session.job);
        // Explicit stop always closes a pre-launched player (no_close only
        // applies to natural stream ends).
        close_fast_player(&mut session.player, true);
        close_player_windows_for_channel(&channel);
    }
    let remaining: Vec<String> = state
        .inner
        .lock()
        .ok()
        .map(|map| {
            map.values()
                .map(|session| session.info.channel.to_ascii_lowercase())
                .collect()
        })
        .unwrap_or_default();
    if remaining.is_empty() {
        close_owned_chatterino();
        crate::dock::clear_session();
    } else {
        crate::dock::drop_closed_channels(&remaining);
        relaunch_dock_chatterino(&remaining);
    }
    Ok(())
}

/// Restart owned Chatterino onto the remaining `--channels=` split. Apply only
/// retiles mpv; leftover tabs stay until this relaunch. Off the caller thread
/// because RestartOwned waits for the previous Qt process to exit.
fn relaunch_dock_chatterino(remaining: &[String]) {
    let cfg = crate::dock::snapshot();
    if remaining.is_empty()
        || !cfg.reserve_chat
        || cfg.layout == "independent"
        || crate::dock::has_independent_placements()
    {
        return;
    }
    let channels = remaining.to_vec();
    thread::spawn(move || {
        let _ = launch_chatterino_for_channels(&channels);
    });
}

pub fn stop_all(state: &StreamingState) -> Result<(), StreamError> {
    let sessions: Vec<_> = {
        let mut map = state
            .inner
            .lock()
            .map_err(|_| StreamError::Message("streaming state poisoned".into()))?;
        map.drain().map(|(_, session)| session).collect()
    };
    let channels: Vec<String> = sessions
        .iter()
        .map(|session| session.info.channel.clone())
        .collect();
    for mut session in sessions {
        let _ = session.child.kill();
        wait_child_timeout(&mut session.child, Duration::from_secs(2));
        terminate_job(&mut session.job);
        close_fast_player(&mut session.player, true);
    }
    for channel in channels {
        close_player_windows_for_channel(&channel);
    }
    close_owned_chatterino();
    crate::dock::clear_session();
    Ok(())
}
