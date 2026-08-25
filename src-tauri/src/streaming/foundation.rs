use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs::{self, OpenOptions};
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::thread;
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter, Manager};
use thiserror::Error;
use uuid::Uuid;

#[cfg(windows)]
use std::os::windows::ffi::OsStrExt;
#[cfg(windows)]
use std::os::windows::io::AsRawHandle;

use crate::branding::{PLAYER_WINDOW_PREFIX, PLAYER_WINDOW_PREFIX_LEGACY};
use crate::doctor::{find_chatterino_path, find_mpv_path, find_streamlink_path, which_on_path};

/// Cached tool paths so every `stream_start` does not re-walk PATH/fallbacks.
type StreamlinkCacheEntry = (String, Option<String>, PathBuf);
static STREAMLINK_PATH_CACHE: OnceLock<Mutex<Option<StreamlinkCacheEntry>>> = OnceLock::new();
static MPV_PATH_CACHE: OnceLock<Mutex<Option<PathBuf>>> = OnceLock::new();
static CHATTERINO_PATH_CACHE: OnceLock<Mutex<Option<PathBuf>>> = OnceLock::new();

/// How long the player window must be continuously missing (window-title
/// heuristic) before a session is treated as closed and Streamlink is killed.
/// Title scans run only for sessions without an owned mpv process; the
/// watchdog otherwise waits on duplicated process handles.
const MPV_MISSING_GRACE: Duration = Duration::from_secs(40);

pub(crate) fn dock_watchdog_interval_ms(dock_active: bool, needs_fast_tick: bool) -> u64 {
    if !dock_active {
        500
    } else if needs_fast_tick {
        100
    } else {
        400
    }
}

pub(crate) fn dock_watchdog_needs_fast_tick(
    group_minimized: bool,
    any_iconic: bool,
    any_zoomed: bool,
    focus_changed: bool,
    popup_changed: bool,
    hwnds_changed: bool,
) -> bool {
    group_minimized || any_iconic || any_zoomed || focus_changed || popup_changed || hwnds_changed
}

pub(crate) fn session_title_scan_needed(owns_player_process: bool) -> bool {
    !owns_player_process
}

pub(crate) fn session_watchdog_timeout_ms(session_count: usize) -> u64 {
    if session_count == 0 {
        2500
    } else {
        1500
    }
}

fn streamlink_cache() -> &'static Mutex<Option<StreamlinkCacheEntry>> {
    STREAMLINK_PATH_CACHE.get_or_init(|| Mutex::new(None))
}
fn mpv_cache() -> &'static Mutex<Option<PathBuf>> {
    MPV_PATH_CACHE.get_or_init(|| Mutex::new(None))
}
fn chatterino_cache() -> &'static Mutex<Option<PathBuf>> {
    CHATTERINO_PATH_CACHE.get_or_init(|| Mutex::new(None))
}

/// PID of the Chatterino process we spawned (never resize unrelated user windows).
fn owned_chatterino_pid() -> &'static Mutex<Option<u32>> {
    static PID: OnceLock<Mutex<Option<u32>>> = OnceLock::new();
    PID.get_or_init(|| Mutex::new(None))
}

fn last_chatterino_channels() -> &'static Mutex<String> {
    static LAST: OnceLock<Mutex<String>> = OnceLock::new();
    LAST.get_or_init(|| Mutex::new(String::new()))
}

fn last_chatterino_watchdog_relaunch() -> &'static Mutex<Option<Instant>> {
    static LAST: OnceLock<Mutex<Option<Instant>>> = OnceLock::new();
    LAST.get_or_init(|| Mutex::new(None))
}

fn chatterino_launch_lock() -> &'static Mutex<()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
}

fn chatterino_close_epoch() -> &'static AtomicU64 {
    static EPOCH: AtomicU64 = AtomicU64::new(0);
    &EPOCH
}

fn bump_chatterino_close_epoch() -> u64 {
    chatterino_close_epoch().fetch_add(1, Ordering::SeqCst) + 1
}

fn current_chatterino_close_epoch() -> u64 {
    chatterino_close_epoch().load(Ordering::SeqCst)
}

fn chatterino_spawn_is_stale(spawn_epoch: u64, close_epoch: u64) -> bool {
    spawn_epoch != close_epoch
}

fn chatterino_pids_to_close(owned: Option<u32>, dock_pids: &[u32]) -> Vec<u32> {
    let mut pids = Vec::new();
    if let Some(pid) = owned {
        pids.push(pid);
    }
    for pid in dock_pids {
        if !pids.contains(pid) {
            pids.push(*pid);
        }
    }
    pids
}

/// Chatterino's QCommandLineParser exits on unknown flags. Bind every
/// dock process to the Rillmux process that spawned it via an exact owner token.
const CHATTERINO_DOCK_OWNER_ENV: &str = "RILLMUX_DOCK_OWNER";

fn chatterino_dock_owner() -> &'static str {
    static OWNER: OnceLock<String> = OnceLock::new();
    OWNER
        .get_or_init(|| format!("{}-{}", std::process::id(), Uuid::new_v4().simple()))
        .as_str()
}

fn chatterino_dock_appdata() -> PathBuf {
    let folder = if cfg!(debug_assertions) {
        "chatterino-dock-dev"
    } else {
        "chatterino-dock"
    };
    let base = crate::diagnostics::app_data_dir().join(folder);
    if cfg!(debug_assertions) {
        base.join(chatterino_dock_owner())
    } else {
        base
    }
}

fn user_chatterino_settings_dir() -> Option<PathBuf> {
    let appdata = std::env::var_os("APPDATA")?;
    Some(PathBuf::from(appdata).join("Chatterino2").join("Settings"))
}

fn seed_chatterino_dock_home(dock_appdata: &Path) {
    let dest = dock_appdata.join("Chatterino2").join("Settings");
    let dest_file = dest.join("settings.json");
    if dest_file.is_file() {
        return;
    }
    let Some(src) = user_chatterino_settings_dir() else {
        let _ = fs::create_dir_all(&dest);
        return;
    };
    let src_file = src.join("settings.json");
    if !src_file.is_file() {
        let _ = fs::create_dir_all(&dest);
        return;
    }
    let _ = fs::create_dir_all(&dest);
    let _ = fs::copy(src_file, dest_file);
}

/// Chatterino restores this file over `-geometry` after a monitor switch.
fn strip_dock_chatterino_window_layout(dock_appdata: &Path) {
    let path = dock_appdata
        .join("Chatterino2")
        .join("Settings")
        .join("window-layout.json");
    let _ = fs::remove_file(path);
}

fn chatterino_should_reuse(alive: bool, last: &str, next: &str) -> bool {
    alive && !last.is_empty() && last == next
}

/// What to do with the Chatterino process *we* spawned — never the user's own window.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ChatterinoLaunchPlan {
    /// Our instance is still up with the same channels — only retile.
    Reuse,
    /// Our instance is up but the channel list changed — close ours, spawn a new `--channels=` window.
    RestartOwned,
    /// We have no owned process — start `--channels=` beside the stream (leave other Chatterino windows alone).
    SpawnFresh,
}

fn chatterino_pick_owned_pid(
    tracked: Option<u32>,
    tracked_alive: bool,
    discovered: Option<u32>,
) -> Option<u32> {
    if tracked_alive {
        tracked
    } else {
        discovered
    }
}

fn chatterino_launch_plan(owned_alive: bool, last: &str, next: &str) -> ChatterinoLaunchPlan {
    if chatterino_should_reuse(owned_alive, last, next) {
        ChatterinoLaunchPlan::Reuse
    } else if owned_alive {
        ChatterinoLaunchPlan::RestartOwned
    } else {
        ChatterinoLaunchPlan::SpawnFresh
    }
}

/// Relaunch the dock instance when our process died but we still expect chat.
fn chatterino_watchdog_should_relaunch(
    pid_alive: bool,
    expect_chat: bool,
    last_channels_set: bool,
    millis_since_last: u64,
    cooldown_ms: u64,
) -> bool {
    expect_chat && !pid_alive && last_channels_set && millis_since_last >= cooldown_ms
}

/// After the spawned child exits: keep a surviving dock PID (stub/IPC), else
/// drop tracking so the watchdog can spawn again.
fn chatterino_pid_after_child_exit(
    tracked: Option<u32>,
    child_pid: u32,
    dock_pids: &[u32],
) -> Option<u32> {
    if tracked != Some(child_pid) {
        return tracked;
    }
    dock_pids.iter().copied().find(|&p| p != child_pid)
}

/// Close only duplicate visible Chatterino mains. Hidden Qt helper windows
/// (IME, offscreen surfaces) also show up in EnumWindows; WM_CLOSE on those
/// makes the process exit 0 — which is the watchdog relaunch loop.
fn chatterino_should_close_duplicate_main(
    is_keep: bool,
    visible: bool,
    area: i64,
    title: &str,
    have_split: bool,
) -> bool {
    if is_keep || !visible || !have_split || area < 10_000 {
        return false;
    }
    title.trim().is_empty() || title.trim().eq_ignore_ascii_case("Chatterino")
}

/// Current Chatterino Windows main windows use Version::fullVersion(), e.g.
/// "Chatterino 2.5.3" or "Chatterino Nightly 2.5.3". Require a numeric
/// version after the stable prefix so modal titles such as
/// "Chatterino - Editing Settings Forbidden" never classify as the dock main.
fn chatterino_title_is_main_window(title: &str) -> bool {
    let title = title.trim();
    let Some(version) = title.strip_prefix("Chatterino ") else {
        return false;
    };
    let version = version.strip_prefix("Nightly ").unwrap_or(version);
    version
        .chars()
        .next()
        .is_some_and(|ch| ch.is_ascii_digit())
}

/// Current Windows builds use a version title instead of the Twitch channel.
/// Keep the channel-in-title fallback for older Chatterino builds.
fn chatterino_title_matches_channels(title: &str, channels_arg: &str) -> bool {
    if chatterino_title_is_main_window(title) {
        return true;
    }
    let title = title.to_ascii_lowercase();
    channels_arg.split(';').any(|part| {
        let name = part
            .trim()
            .trim_start_matches("t:")
            .trim_start_matches('#')
            .to_ascii_lowercase();
        !name.is_empty() && title.contains(name.as_str())
    })
}

/// Prefer a --channels split over a blank notebook, then a visible frame over
/// a cloaked ghost (which otherwise lands as a white/black sheet on top of chat).
fn chatterino_window_pick_key(
    title_matches: bool,
    visible: bool,
    iconic: bool,
    area: i64,
) -> (u8, u8, i64) {
    let split = u8::from(title_matches);
    let vis = if visible && !iconic {
        2
    } else if iconic {
        1
    } else {
        0
    };
    (split, vis, area)
}

fn cached_chatterino_path() -> Option<PathBuf> {
    if let Ok(guard) = chatterino_cache().lock() {
        if let Some(path) = guard.as_ref() {
            if path.is_file() {
                return Some(path.clone());
            }
        }
    }
    let found = find_chatterino_path()?;
    if let Ok(mut guard) = chatterino_cache().lock() {
        *guard = Some(found.clone());
    }
    Some(found)
}
