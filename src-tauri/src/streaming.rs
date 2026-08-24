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

/// Chatterino's QCommandLineParser exits on unknown flags. Tag the dock
/// instance with an env var instead of `--rillmux-dock`.
const CHATTERINO_DOCK_ENV: &str = "RILLMUX_DOCK";

fn chatterino_dock_appdata() -> PathBuf {
    crate::diagnostics::app_data_dir().join("chatterino-dock")
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
    title.to_ascii_lowercase().contains("chatterino")
}

/// `--channels=t:forsen` windows are titled like "forsen - Chatterino".
/// Isolated APPDATA also restores an empty notebook named just "Chatterino".
fn chatterino_title_matches_channels(title: &str, channels_arg: &str) -> bool {
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

#[derive(Debug, Error)]
pub enum StreamError {
    #[error("{0}")]
    Message(String),
    #[error(transparent)]
    Io(#[from] std::io::Error),
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LaunchRequest {
    pub channel: String,
    pub quality: Option<String>,
    pub title: Option<String>,
    pub game: Option<String>,
    pub streamlink_source: Option<String>,
    pub streamlink_custom_path: Option<String>,
    pub player_id: Option<String>,
    pub player_custom_path: Option<String>,
    pub player_custom_args: Option<String>,
    pub low_latency: Option<bool>,
    pub disable_ads: Option<bool>,
    pub player_input: Option<String>,
    pub webbrowser: Option<bool>,
    pub webbrowser_headless: Option<bool>,
    pub webbrowser_executable: Option<String>,
    pub retry_streams: Option<u32>,
    pub retry_max: Option<u32>,
    pub player_no_close: Option<bool>,
    /// Leave a right strip for Chatterino; Rust sets absolute mpv --geometry.
    pub reserve_chat: Option<bool>,
    /// When true, keep existing sessions until this one is ready, then stop them.
    pub replace_existing: Option<bool>,
    /// Planned tile of this stream in the dock grid (frontend slot order) —
    /// lets the launch geometry open the window already snapped to its tile.
    pub slot_index: Option<u32>,
    pub slot_count: Option<u32>,
    /// Multistream preset from settings (e.g. "2x2") for the launch geometry.
    pub layout: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StreamSession {
    pub id: String,
    pub channel: String,
    pub quality: String,
    pub title: Option<String>,
    pub game: Option<String>,
    pub running: bool,
    pub status: String,
    pub phase: String,
    pub ready: bool,
    /// Soft mute via mpv IPC (fast-start sessions).
    pub muted: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StreamStatusPayload {
    pub id: String,
    pub channel: String,
    pub line: String,
    pub status: String,
    pub phase: String,
    pub ready: bool,
}

struct LiveSession {
    info: StreamSession,
    child: Child,
    /// Windows Job containing the Streamlink child (and transitively the
    /// player). Terminating it kills the whole tree.
    job: JobSlot,
    /// Pre-launched mpv owned by this session (fast start, Windows only).
    player: Option<FastPlayer>,
    /// When the player became ready — grace before treating missing mpv as closed.
    ready_at: Option<Instant>,
    /// First moment the player window was observed missing (None = seen alive).
    /// The window-title lookup is only a heuristic, so a session is treated as
    /// closed solely on missing titles after a long, continuous absence.
    mpv_missing_since: Option<Instant>,
    /// Natural stream end: keep mpv alive until this Instant for the offline OSD.
    offline_until: Option<Instant>,
}

/// Pre-launched mpv owned by a session (fast start): the window appears
/// immediately after clicking watch, and the stream is attached via IPC
/// once Streamlink's local HTTP server is up.
struct FastPlayer {
    child: Child,
    /// Kill-job for the player tree (fallback when the IPC quit fails).
    job: JobSlot,
    /// mpv IPC named pipe (`\\.\pipe\rillmux-mpv-<uuid>`).
    pipe: String,
    /// --player-no-close: leave the player open when the stream ends.
    no_close: bool,
}

/// Send one command to mpv's IPC pipe, retrying until `timeout` (the pipe
/// appears shortly after the mpv process spawns).
/// Prefer [`mpv_ipc_json`] when arguments need native JSON types (booleans,
/// numbers) — string `"no"` for a flag is truthy and mutes audio.
#[cfg(windows)]
fn mpv_ipc_command(pipe: &str, cmd: &[&str], timeout: Duration) -> Result<(), StreamError> {
    let values: Vec<serde_json::Value> = cmd
        .iter()
        .map(|s| serde_json::Value::String((*s).into()))
        .collect();
    mpv_ipc_json(pipe, values, timeout)
}

#[cfg(windows)]
fn mpv_ipc_json(
    pipe: &str,
    command: Vec<serde_json::Value>,
    timeout: Duration,
) -> Result<(), StreamError> {
    use std::fs::OpenOptions;
    use std::io::{BufRead, BufReader, Write};
    let deadline = Instant::now() + timeout;
    let mut last_err: Option<std::io::Error> = None;
    while Instant::now() < deadline {
        match OpenOptions::new().read(true).write(true).open(pipe) {
            Ok(mut file) => {
                let msg = serde_json::json!({ "command": command }).to_string() + "\n";
                file.write_all(msg.as_bytes())?;
                // Events may precede the reply; read until the "error" field.
                let mut reader = BufReader::new(file);
                let mut line = String::new();
                for _ in 0..20 {
                    line.clear();
                    if reader.read_line(&mut line)? == 0 {
                        break;
                    }
                    if line.contains("\"error\"") {
                        if line.contains("\"success\"") {
                            return Ok(());
                        }
                        return Err(StreamError::Message(format!(
                            "mpv IPC error: {}",
                            line.trim()
                        )));
                    }
                }
                return Err(StreamError::Message("mpv IPC reply missing".into()));
            }
            Err(e) => {
                last_err = Some(e);
                thread::sleep(Duration::from_millis(50));
            }
        }
    }
    Err(StreamError::Message(format!(
        "mpv IPC connect failed: {last_err:?}"
    )))
}

#[cfg(not(windows))]
fn mpv_ipc_command(_pipe: &str, _cmd: &[&str], _timeout: Duration) -> Result<(), StreamError> {
    Err(StreamError::Message(
        "mpv IPC is only supported on Windows".into(),
    ))
}

#[cfg(not(windows))]
fn mpv_ipc_json(
    _pipe: &str,
    _command: Vec<serde_json::Value>,
    _timeout: Duration,
) -> Result<(), StreamError> {
    Err(StreamError::Message(
        "mpv IPC is only supported on Windows".into(),
    ))
}

/// Quit the pre-launched player (graceful IPC quit, then hard kill).
/// Idempotent via Option::take — stop, prune and the EOF watcher race here.
fn close_fast_player(player: &mut Option<FastPlayer>, graceful: bool) {
    let Some(mut p) = player.take() else {
        return;
    };
    if graceful {
        let _ = mpv_ipc_command(&p.pipe, &["quit"], Duration::from_millis(700));
        for _ in 0..10 {
            if matches!(p.child.try_wait(), Ok(Some(_))) {
                break;
            }
            thread::sleep(Duration::from_millis(50));
        }
    }
    let _ = p.child.kill();
    let _ = p.child.wait();
    terminate_job(&mut p.job);
}

/// Everything the output watcher needs to attach the pre-launched mpv once
/// Streamlink's local HTTP server is up (fast start).
struct FastPlayerCtx {
    pipe: String,
    port: u16,
    player_path: PathBuf,
    /// Dock argv for the fallback respawn (IPC failed): mpv <args> <url>.
    fallback_argv: Vec<String>,
    /// Guards one-time loadfile across the stdout/stderr watcher threads.
    fired: Arc<AtomicBool>,
    /// Guards one-time offline goodbye across the stdout/stderr watchers.
    goodbye: Arc<AtomicBool>,
    /// Last loading-phase text shown on the idle player's OSD (dedupe).
    osd: Mutex<String>,
    no_close: bool,
}

fn close_session_player(state: &StreamingState, id: &str, graceful: bool) {
    if let Ok(mut map) = state.inner.lock() {
        if let Some(session) = map.get_mut(id) {
            close_fast_player(&mut session.player, graceful);
        }
    }
}

/// After attaching a live stream, force mute off and a sane volume.
/// Important: mpv's JSON IPC needs a real boolean for flags — the string
/// `"no"` is truthy and would *enable* mute (speaker shows "!").
fn mpv_ensure_audible(pipe: &str) {
    let _ = mpv_ipc_json(
        pipe,
        vec![
            serde_json::Value::String("set_property".into()),
            serde_json::Value::String("mute".into()),
            serde_json::Value::Bool(false),
        ],
        Duration::from_millis(800),
    );
    let _ = mpv_ipc_json(
        pipe,
        vec![
            serde_json::Value::String("set_property".into()),
            serde_json::Value::String("volume".into()),
            serde_json::Value::from(100),
        ],
        Duration::from_millis(800),
    );
    let _ = mpv_ipc_command(
        pipe,
        &["set_property", "aid", "auto"],
        Duration::from_millis(800),
    );
}

const OFFLINE_GOODBYE_SECS: u64 = 5;

/// After a natural stream end: swap to the loading art, show an offline OSD,
/// wait a few seconds, then tear the session down.
fn begin_offline_goodbye(
    app: AppHandle,
    state: SharedStreaming,
    id: String,
    channel: String,
    pipe: String,
) {
    let status = format!("The streamer {channel} went offline");
    if let Ok(mut map) = state.inner.lock() {
        if let Some(session) = map.get_mut(&id) {
            session.offline_until =
                Some(Instant::now() + Duration::from_secs(OFFLINE_GOODBYE_SECS + 3));
            session.info.running = false;
            session.info.ready = false;
            session.info.phase = "ended".into();
            session.info.status = status.clone();
        }
    }
    emit_status(
        &app,
        StreamStatusPayload {
            id: id.clone(),
            channel: channel.clone(),
            line: status.clone(),
            status: status.clone(),
            phase: "ended".into(),
            ready: false,
        },
    );
    let _ = app.emit("stream-sessions-changed", ());

    thread::spawn(move || {
        // Replace the dead HTTP stream with the branded loading image so the
        // window looks like the startup screen again.
        if let Some(png) = loading_image_path() {
            let path = png.to_string_lossy().into_owned();
            let _ = mpv_ipc_command(&pipe, &["stop"], Duration::from_millis(800));
            let _ = mpv_ipc_command(&pipe, &["loadfile", &path], Duration::from_secs(2));
            let _ = mpv_ipc_command(
                &pipe,
                &["set_property", "image-display-duration", "inf"],
                Duration::from_millis(800),
            );
        }
        let _ = mpv_ipc_command(
            &pipe,
            &[
                "show-text",
                status.as_str(),
                &format!("{}", OFFLINE_GOODBYE_SECS * 1000),
            ],
            Duration::from_secs(2),
        );
        thread::sleep(Duration::from_secs(OFFLINE_GOODBYE_SECS));

        // Teardown: player first, then drop the session record.
        close_session_player(&state, &id, true);
        let empty = if let Ok(mut map) = state.inner.lock() {
            if let Some(mut session) = map.remove(&id) {
                let _ = session.child.kill();
                let _ = session.child.wait();
                terminate_job(&mut session.job);
                close_fast_player(&mut session.player, false);
                close_player_windows_for_channel(&channel);
            }
            map.is_empty()
        } else {
            false
        };
        if empty {
            close_owned_chatterino();
            crate::dock::clear_session();
        }
        let _ = app.emit("stream-sessions-changed", ());
    });
}

pub struct StreamingState {
    inner: Mutex<HashMap<String, LiveSession>>,
}

impl StreamingState {
    pub fn new() -> Self {
        Self {
            inner: Mutex::new(HashMap::new()),
        }
    }
}

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
    if let Some(f) = chat_fraction {
        crate::dock::set_chat_fraction(f);
    }
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

/// Immediate retile from dock grip drags (no delayed retry loop).
pub fn apply_dock_layout() {
    #[cfg(windows)]
    {
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
        if crate::dock::take_raise_after_apply() {
            raise_dock_windows(&cfg.channels, cfg.reserve_chat);
        }
        // Raising mpv buries an owned HUD under the player. Restack after.
        if let Some(app) = DOCK_APP.get() {
            restack_all_points_huds(app);
        }
    }
}

fn apply_dock_layout_cb() {
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
        let mut grips_elevated = false;
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
                if grips_elevated {
                    crate::dock::demote_grips();
                    grips_elevated = false;
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
                grips_elevated = false;
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
                    grips_elevated = true; // Sync elevates; track that here.
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

            // Keep grey grips above mpv/chat while the dock owns focus.
            // Re-assert TOPMOST every tick (mpv --ontop / BringWindowToTop can
            // reorder the TOPMOST band). Only demote when FG is clearly a
            // foreign app — never demote on a failed title scan.
            if cfg.linked {
                match focus.unwrap_or(DockFocusKind::Unknown) {
                    DockFocusKind::DockOrApp | DockFocusKind::Unknown => {
                        // Re-elevate even when already tracked as elevated.
                        crate::dock::raise_grips();
                        raise_poll_overlay();
                        grips_elevated = true;
                    }
                    DockFocusKind::Foreign => {
                        if grips_elevated {
                            crate::dock::demote_grips();
                            grips_elevated = false;
                        }
                    }
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
    /// Some other process is foreground — safe to drop TOPMOST.
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
    apply_dock_layout();
}

pub fn dock_cycle_monitor() {
    crate::dock::cycle_monitor();
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OverlayRect {
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
}

const RAID_OVERLAY_WIDTH: i32 = 420;
const RAID_OVERLAY_HEIGHT: i32 = 92;
const RAID_OVERLAY_INSET: i32 = 16;

fn overlay_rect_from_host(host: OverlayRect) -> OverlayRect {
    let width = (host.width - RAID_OVERLAY_INSET * 2).clamp(240, RAID_OVERLAY_WIDTH);
    OverlayRect {
        x: host.x + RAID_OVERLAY_INSET,
        y: host.y + RAID_OVERLAY_INSET,
        width,
        height: RAID_OVERLAY_HEIGHT,
    }
}

#[cfg(windows)]
fn overlay_rect_from_hwnd(hwnd: *mut core::ffi::c_void) -> Option<OverlayRect> {
    if hwnd.is_null() {
        return None;
    }
    #[link(name = "user32")]
    unsafe extern "system" {
        fn GetWindowRect(hwnd: *mut core::ffi::c_void, rect: *mut WinRect) -> i32;
    }
    let mut rect = WinRect {
        left: 0,
        top: 0,
        right: 0,
        bottom: 0,
    };
    if unsafe { GetWindowRect(hwnd, &mut rect) } == 0 {
        return None;
    }
    let width = rect.right - rect.left;
    let height = rect.bottom - rect.top;
    if width < 80 || height < 40 {
        return None;
    }
    Some(OverlayRect {
        x: rect.left,
        y: rect.top,
        width,
        height,
    })
}

/// Visible DWM frame. GetWindowRect includes the hidden Win11 thickframe.
#[cfg(windows)]
fn dwm_visible_overlay_rect(hwnd: *mut core::ffi::c_void) -> Option<OverlayRect> {
    if hwnd.is_null() {
        return None;
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
    let mut rect = WinRect {
        left: 0,
        top: 0,
        right: 0,
        bottom: 0,
    };
    let hr = unsafe {
        DwmGetWindowAttribute(
            hwnd,
            DWMWA_EXTENDED_FRAME_BOUNDS,
            &mut rect as *mut _ as *mut _,
            std::mem::size_of::<WinRect>() as u32,
        )
    };
    if hr != 0 {
        return overlay_rect_from_hwnd(hwnd);
    }
    let width = rect.right - rect.left;
    let height = rect.bottom - rect.top;
    if width < 80 || height < 40 {
        return None;
    }
    Some(OverlayRect {
        x: rect.left,
        y: rect.top,
        width,
        height,
    })
}

const CHANNEL_POINTS_HUD_MIN_WIDTH: i32 = 200;
const CHANNEL_POINTS_HUD_MIN_HEIGHT: i32 = 120;
/// Matches `--title-bar-height` / `#tbo-controls` (CSS px, scaled to physical).
const POINTS_HUD_TITLE_BAR_HEIGHT_CSS: f64 = 38.0;
/// Matches `--title-bar-controls-width` (3 × 46px caption buttons).
const POINTS_HUD_CAPTION_WIDTH_CSS: f64 = 138.0;

#[derive(Debug, Clone, PartialEq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChannelPointsHudPlace {
    pub player: OverlayRect,
    pub caption_avoid: Option<OverlayRect>,
}

/// Top-right min/max/close strip. `host` is the webview inner rect — `#tbo-controls`
/// is `position:fixed; right:0` on the webview, not the outer frame (8px borders).
pub fn caption_avoid_from_main(host: OverlayRect, scale: f64) -> OverlayRect {
    let scale = if scale.is_finite() && scale > 0.0 {
        scale
    } else {
        1.0
    };
    let width = (POINTS_HUD_CAPTION_WIDTH_CSS * scale).round() as i32;
    let height = (POINTS_HUD_TITLE_BAR_HEIGHT_CSS * scale).round() as i32;
    OverlayRect {
        x: host.x + host.width - width.max(1),
        y: host.y,
        width: width.max(1),
        height: height.max(1),
    }
}

pub fn union_overlay_rect(a: OverlayRect, b: OverlayRect) -> OverlayRect {
    let x = a.x.min(b.x);
    let y = a.y.min(b.y);
    let right = (a.x + a.width).max(b.x + b.width);
    let bottom = (a.y + a.height).max(b.y + b.height);
    OverlayRect {
        x,
        y,
        width: (right - x).max(1),
        height: (bottom - y).max(1),
    }
}

/// DWM caption-button bounds are window-relative (including the 8px frame).
#[cfg(windows)]
fn dwm_caption_buttons_screen(
    hwnd: *mut core::ffi::c_void,
    window_x: i32,
    window_y: i32,
) -> Option<OverlayRect> {
    #[repr(C)]
    struct Rect {
        left: i32,
        top: i32,
        right: i32,
        bottom: i32,
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
    const DWMWA_CAPTION_BUTTON_BOUNDS: u32 = 5;
    let mut bounds = Rect {
        left: 0,
        top: 0,
        right: 0,
        bottom: 0,
    };
    let hr = unsafe {
        DwmGetWindowAttribute(
            hwnd,
            DWMWA_CAPTION_BUTTON_BOUNDS,
            &mut bounds as *mut Rect as *mut _,
            std::mem::size_of::<Rect>() as u32,
        )
    };
    if hr != 0 || bounds.right <= bounds.left || bounds.bottom <= bounds.top {
        return None;
    }
    Some(OverlayRect {
        x: window_x + bounds.left,
        y: window_y + bounds.top,
        width: bounds.right - bounds.left,
        height: bounds.bottom - bounds.top,
    })
}

pub fn main_window_caption_avoid(app: &AppHandle) -> Option<OverlayRect> {
    let win = app.get_webview_window("main")?;
    let scale = win
        .scale_factor()
        .ok()
        .filter(|value| value.is_finite() && *value > 0.0)
        .unwrap_or(1.0);
    let inner_pos = win.inner_position().ok()?;
    let inner_size = win.inner_size().ok()?;
    let plugin = caption_avoid_from_main(
        OverlayRect {
            x: inner_pos.x,
            y: inner_pos.y,
            width: inner_size.width as i32,
            height: inner_size.height as i32,
        },
        scale,
    );
    #[cfg(windows)]
    {
        if let (Ok(hwnd), Ok(outer)) = (win.hwnd(), win.outer_position()) {
            if let Some(dwm) = dwm_caption_buttons_screen(hwnd.0, outer.x, outer.y) {
                return Some(union_overlay_rect(plugin, dwm));
            }
        }
    }
    Some(plugin)
}

/// Min/max/close of the *player* window, not the Rillmux chrome. Synthetic
/// top-right strip when DWM has no caption buttons (borderless mpv OSC).
pub fn player_caption_avoid(channel_login: &str, player: OverlayRect) -> Option<OverlayRect> {
    #[cfg(windows)]
    {
        let hwnd = find_player_window(channel_login);
        let scale = hwnd.map(hwnd_dpi_scale).unwrap_or(1.0);
        if let Some(hwnd) = hwnd {
            if let Some(dwm) = dwm_caption_buttons_screen(hwnd, player.x, player.y) {
                if overlay_rects_overlap(dwm, player) {
                    return Some(dwm);
                }
            }
        }
        Some(caption_avoid_from_main(player, scale))
    }
    #[cfg(not(windows))]
    {
        let _ = channel_login;
        Some(caption_avoid_from_main(player, 1.0))
    }
}

#[cfg(windows)]
fn hwnd_dpi_scale(hwnd: *mut core::ffi::c_void) -> f64 {
    #[link(name = "user32")]
    unsafe extern "system" {
        fn GetDpiForWindow(hwnd: *mut core::ffi::c_void) -> u32;
    }
    if hwnd.is_null() {
        return 1.0;
    }
    let dpi = unsafe { GetDpiForWindow(hwnd) };
    if dpi == 0 {
        1.0
    } else {
        dpi as f64 / 96.0
    }
}

pub fn overlay_rects_overlap(a: OverlayRect, b: OverlayRect) -> bool {
    a.x < b.x + b.width && a.x + a.width > b.x && a.y < b.y + b.height && a.y + a.height > b.y
}

fn overlay_rect_differs(a: OverlayRect, b: OverlayRect, slop: i32) -> bool {
    (a.x - b.x).abs() > slop
        || (a.y - b.y).abs() > slop
        || (a.width - b.width).abs() > slop
        || (a.height - b.height).abs() > slop
}

/// Move the HUD when forced, when the chip jumped (monitor switch), or when
/// it still covers the caption keepout. Do not skip a monitor jump just
/// because the chip is not overlapping the (new) caption strip.
pub fn hud_overlay_should_apply(
    force: bool,
    current: Option<OverlayRect>,
    desired: OverlayRect,
    keepout: Option<OverlayRect>,
    slop: i32,
) -> bool {
    if force {
        return true;
    }
    let Some(current) = current else {
        return true;
    };
    if overlay_rect_differs(current, desired, slop) {
        return true;
    }
    keepout.is_some_and(|k| overlay_rects_overlap(current, k))
}

/// Channel login from a `points-hud-<login>` webview label.
pub fn points_hud_channel_from_label(label: &str) -> Option<&str> {
    let channel = label.strip_prefix("points-hud-")?;
    if channel.is_empty() {
        None
    } else {
        Some(channel)
    }
}

/// `SetWindowPos` insert-after so the HUD sits immediately above the player.
/// `player_prev` is `GW_HWNDPREV` of the player (0 if none). `None` means the
/// HUD is already there — do not raise it over other apps.
pub fn hud_z_insert_after(hud: isize, player_prev: isize) -> Option<isize> {
    if player_prev == 0 {
        Some(0)
    } else if player_prev == hud {
        None
    } else {
        Some(player_prev)
    }
}

/// Tauri owns overlay webviews by the creator (main). Clearing
/// GWLP_HWNDPARENT orphans the chip. Never re-own it to mpv either — that
/// made the Rillmux main window impossible to activate while a stream ran.
#[cfg(test)]
pub fn hud_needs_detach_owner(_owner: isize) -> bool {
    false
}

#[cfg(test)]
pub fn hud_needs_reown(_current_owner: isize, _player: isize) -> bool {
    false
}

/// Qt restores window-layout.json after a monitor change, often after 800ms.
pub fn chatterino_place_retry_ms() -> &'static [u64] {
    &[0, 80, 200, 400, 800, 1600, 3000]
}

/// Visible-frame slop. 64px hid the ~27px left-gap after DWM expand.
pub fn chatterino_place_slop_px() -> i32 {
    12
}

/// Keep retrying place when the dock still has a Chatterino process but no HWND
/// (Qt recreates the window on monitor switch and drops the SetProp tag).
pub fn chatterino_hwnd_lost_needs_retry(found_hwnd: bool, have_dock_pid: bool) -> bool {
    !found_hwnd && have_dock_pid
}

/// Watchdog / place retry: missing HWND, unreadable frame, or visible drift.
pub fn chatterino_watchdog_should_place(
    found_hwnd: bool,
    expect_chat: bool,
    visible: Option<OverlayRect>,
    target: OverlayRect,
    slop: i32,
) -> bool {
    if chatterino_hwnd_lost_needs_retry(found_hwnd, expect_chat) {
        return true;
    }
    match visible {
        Some(got) => overlay_rect_drifted(got, target, slop),
        None => found_hwnd,
    }
}

pub fn overlay_rect_drifted(current: OverlayRect, target: OverlayRect, slop: i32) -> bool {
    (current.x - target.x).abs() > slop
        || (current.y - target.y).abs() > slop
        || (current.width - target.width).abs() > slop
        || (current.height - target.height).abs() > slop
}

/// Keep the HUD above its mpv window without WS_EX_TOPMOST (other apps stay in front).
pub fn restack_hud_above_player(app: &AppHandle, label: &str) {
    let Some(win) = app.get_webview_window(label) else {
        return;
    };
    #[cfg(windows)]
    {
        let Ok(hwnd) = win.hwnd() else {
            return;
        };
        if !is_hwnd_alive(hwnd.0) {
            return;
        }
        restack_hud_hwnd(app, &win, label);
    }
    #[cfg(not(windows))]
    let _ = win.set_always_on_top(false);
}

pub fn restack_all_points_huds(app: &AppHandle) {
    for (label, _) in app.webview_windows() {
        if label.starts_with("points-hud-") {
            restack_hud_above_player(app, &label);
        }
    }
}

#[cfg(windows)]
fn restack_hud_hwnd(app: &AppHandle, win: &tauri::WebviewWindow, label: &str) {
    let Some(channel) = points_hud_channel_from_label(label) else {
        return;
    };
    let Some(player) = find_player_window(channel) else {
        return;
    };
    let Ok(hud) = win.hwnd() else {
        return;
    };
    let hud_ptr = hud.0;
    let main_ptr = app
        .get_webview_window("main")
        .and_then(|w| w.hwnd().ok())
        .map(|h| h.0)
        .filter(|p| !p.is_null())
        .unwrap_or(std::ptr::null_mut());
    if hud_ptr.is_null() || player.is_null() {
        return;
    }
    let _ = main_ptr;
    #[link(name = "user32")]
    unsafe extern "system" {
        fn GetWindow(hwnd: *mut core::ffi::c_void, cmd: u32) -> *mut core::ffi::c_void;
        fn SetWindowPos(
            hwnd: *mut core::ffi::c_void,
            after: *mut core::ffi::c_void,
            x: i32,
            y: i32,
            cx: i32,
            cy: i32,
            flags: u32,
        ) -> i32;
        fn GetWindowLongPtrW(hwnd: *mut core::ffi::c_void, index: i32) -> isize;
        fn SetWindowLongPtrW(hwnd: *mut core::ffi::c_void, index: i32, value: isize) -> isize;
        fn IsWindow(hwnd: *mut core::ffi::c_void) -> i32;
        fn ShowWindow(hwnd: *mut core::ffi::c_void, cmd: i32) -> i32;
    }
    const GW_HWNDPREV: u32 = 3;
    const GWL_EXSTYLE: i32 = -20;
    const WS_EX_TOPMOST: isize = 0x0008;
    const SWP_NOSIZE: u32 = 0x0001;
    const SWP_NOMOVE: u32 = 0x0002;
    const SWP_NOACTIVATE: u32 = 0x0010;
    const SWP_FRAMECHANGED: u32 = 0x0020;
    const SWP_SHOWWINDOW: u32 = 0x0040;
    const SWP_NOZORDER: u32 = 0x0004;
    const SW_SHOWNA: i32 = 8;
    unsafe fn hwnd_is_above(
        a: *mut core::ffi::c_void,
        b: *mut core::ffi::c_void,
        get_window: unsafe extern "system" fn(
            *mut core::ffi::c_void,
            u32,
        ) -> *mut core::ffi::c_void,
    ) -> bool {
        if a.is_null() || b.is_null() || a == b {
            return false;
        }
        const GW_HWNDNEXT: u32 = 2;
        let mut cur = get_window(a, GW_HWNDNEXT);
        for _ in 0..4096 {
            if cur.is_null() {
                return false;
            }
            if cur == b {
                return true;
            }
            cur = get_window(cur, GW_HWNDNEXT);
        }
        false
    }
    unsafe {
        if IsWindow(hud_ptr) == 0 {
            let _ = win.close();
            return;
        }
        if IsWindow(player) == 0 {
            return;
        }
        let style = GetWindowLongPtrW(hud_ptr, GWL_EXSTYLE);
        if style & WS_EX_TOPMOST != 0 {
            SetWindowLongPtrW(hud_ptr, GWL_EXSTYLE, style & !WS_EX_TOPMOST);
        }
        let _ = ShowWindow(hud_ptr, SW_SHOWNA);
        let prev = GetWindow(player, GW_HWNDPREV);
        let Some(after) = hud_z_insert_after(hud_ptr as isize, prev as isize) else {
            let _ = SetWindowPos(
                hud_ptr,
                std::ptr::null_mut(),
                0,
                0,
                0,
                0,
                SWP_NOMOVE
                    | SWP_NOSIZE
                    | SWP_NOACTIVATE
                    | SWP_FRAMECHANGED
                    | SWP_NOZORDER
                    | SWP_SHOWWINDOW,
            );
            return;
        };
        let _ = SetWindowPos(
            hud_ptr,
            after as *mut core::ffi::c_void,
            0,
            0,
            0,
            0,
            SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE | SWP_FRAMECHANGED,
        );
        if !hwnd_is_above(hud_ptr, player, GetWindow) {
            let _ = SetWindowPos(
                hud_ptr,
                std::ptr::null_mut(),
                0,
                0,
                0,
                0,
                SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE | SWP_FRAMECHANGED,
            );
        }
    }
}

/// Keep HUD overlays off the min/max/close strip. `force` always applies the
/// rect (create / first place); otherwise only move if the HWND covers caption.
pub fn place_hud_overlay(
    app: &AppHandle,
    label: &str,
    x: i32,
    y: i32,
    width: i32,
    height: i32,
    force: bool,
) {
    let Some(win) = app.get_webview_window(label) else {
        return;
    };
    #[cfg(windows)]
    {
        let Ok(hwnd) = win.hwnd() else {
            return;
        };
        if !is_hwnd_alive(hwnd.0) {
            return;
        }
    }
    let _ = win.set_min_size(Some(tauri::Size::Physical(tauri::PhysicalSize::new(1, 1))));
    restack_hud_above_player(app, label);
    let desired = OverlayRect {
        x,
        y,
        width,
        height,
    };
    let current = win.outer_position().ok().and_then(|pos| {
        win.outer_size().ok().map(|size| OverlayRect {
            x: pos.x,
            y: pos.y,
            width: size.width as i32,
            height: size.height as i32,
        })
    });
    let keepout = main_window_caption_avoid(app).map(|avoid| OverlayRect {
        x: avoid.x,
        y: avoid.y,
        width: avoid.width,
        height: avoid.height + 16 + 36,
    });
    if !hud_overlay_should_apply(force, current, desired, keepout, 12) {
        return;
    }
    #[cfg(windows)]
    if let Ok(hwnd) = win.hwnd() {
        move_overlay_hwnd(hwnd.0, x, y, width, height);
    }
    let _ = win.set_position(tauri::PhysicalPosition::new(x, y));
    fit_overlay_webview(&win, width, height);
}

/// Full player outer rect for the Channel Points HUD, or none when hidden.
pub fn channel_points_hud_player_rect(
    player: Option<OverlayRect>,
    iconic: bool,
) -> Option<OverlayRect> {
    let rect = player?;
    if iconic
        || rect.width < CHANNEL_POINTS_HUD_MIN_WIDTH
        || rect.height < CHANNEL_POINTS_HUD_MIN_HEIGHT
    {
        return None;
    }
    Some(rect)
}

#[cfg(windows)]
pub fn channel_points_hud_host(channel_login: &str) -> Option<OverlayRect> {
    let hwnd = find_player_window(channel_login)?;
    let iconic = is_hwnd_iconic(hwnd);
    channel_points_hud_player_rect(overlay_rect_from_hwnd(hwnd), iconic)
}

#[cfg(not(windows))]
pub fn channel_points_hud_host(_channel_login: &str) -> Option<OverlayRect> {
    None
}

/// Resize a transparent overlay HWND and its WebView2 child to `width`×`height`
/// physical pixels. Tauri `setSize` often leaves the child at the previous size.
/// Move + resize the overlay HWND in screen physical pixels. Tauri
/// `setPosition` often no-ops when the window already exists on another monitor.
#[cfg(windows)]
fn move_overlay_hwnd(hwnd: *mut core::ffi::c_void, x: i32, y: i32, width: i32, height: i32) {
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
        fn GetWindow(hwnd: *mut core::ffi::c_void, cmd: u32) -> *mut core::ffi::c_void;
        fn IsWindow(hwnd: *mut core::ffi::c_void) -> i32;
    }
    const GW_CHILD: u32 = 5;
    const SWP_NOZORDER: u32 = 0x0004;
    const SWP_NOACTIVATE: u32 = 0x0010;
    const SWP_FRAMECHANGED: u32 = 0x0020;
    const SWP_NOMOVE: u32 = 0x0002;
    let width = width.max(1);
    let height = height.max(1);
    unsafe {
        if hwnd.is_null() || IsWindow(hwnd) == 0 {
            return;
        }
        let _ = SetWindowPos(
            hwnd,
            std::ptr::null_mut(),
            x,
            y,
            width,
            height,
            SWP_NOZORDER | SWP_NOACTIVATE | SWP_FRAMECHANGED,
        );
        let child = GetWindow(hwnd, GW_CHILD);
        if !child.is_null() && IsWindow(child) != 0 {
            let _ = SetWindowPos(
                child,
                std::ptr::null_mut(),
                0,
                0,
                width,
                height,
                SWP_NOMOVE | SWP_NOZORDER | SWP_NOACTIVATE,
            );
        }
    }
}

#[cfg(windows)]
pub fn fit_overlay_webview(win: &tauri::WebviewWindow, width: i32, height: i32) {
    let Ok(hwnd) = win.hwnd() else {
        return;
    };
    let width = width.max(1);
    let height = height.max(1);
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
        fn GetWindow(hwnd: *mut core::ffi::c_void, cmd: u32) -> *mut core::ffi::c_void;
        fn IsWindow(hwnd: *mut core::ffi::c_void) -> i32;
    }
    const GW_CHILD: u32 = 5;
    const SWP_NOMOVE: u32 = 0x0002;
    const SWP_NOZORDER: u32 = 0x0004;
    const SWP_NOACTIVATE: u32 = 0x0010;
    const SWP_FRAMECHANGED: u32 = 0x0020;
    let flags = SWP_NOMOVE | SWP_NOZORDER | SWP_NOACTIVATE | SWP_FRAMECHANGED;
    let ptr = hwnd.0;
    unsafe {
        if ptr.is_null() || IsWindow(ptr) == 0 {
            return;
        }
        let _ = SetWindowPos(ptr, std::ptr::null_mut(), 0, 0, width, height, flags);
        let child = GetWindow(ptr, GW_CHILD);
        if !child.is_null() && IsWindow(child) != 0 {
            let _ = SetWindowPos(child, std::ptr::null_mut(), 0, 0, width, height, flags);
        }
    }
    let _ = win.set_size(tauri::PhysicalSize::new(width as u32, height as u32));
}

#[cfg(not(windows))]
pub fn fit_overlay_webview(win: &tauri::WebviewWindow, width: i32, height: i32) {
    let _ = win.set_size(tauri::PhysicalSize::new(
        width.max(1) as u32,
        height.max(1) as u32,
    ));
}

/// Player first, then owned Chatterino. Main-window fallback is frontend-owned.
#[cfg(windows)]
pub fn raid_overlay_host(from_channel: &str) -> Option<OverlayRect> {
    let player = find_player_window(from_channel).and_then(overlay_rect_from_hwnd);
    if let Some(host) = player {
        return Some(overlay_rect_from_host(host));
    }
    let chat = owned_chatterino_pid()
        .lock()
        .ok()
        .and_then(|g| *g)
        .and_then(find_main_window_for_pid)
        .and_then(overlay_rect_from_hwnd);
    chat.map(overlay_rect_from_host)
}

#[cfg(not(windows))]
pub fn raid_overlay_host(_from_channel: &str) -> Option<OverlayRect> {
    None
}

/// Full owned-Chatterino window, else the reserved dock chat strip.
#[cfg(windows)]
pub fn poll_overlay_chat_host() -> Option<OverlayRect> {
    owned_chatterino_pid()
        .lock()
        .ok()
        .and_then(|g| *g)
        .and_then(find_main_window_for_pid)
        .and_then(overlay_rect_from_hwnd)
        .or_else(overlay_rect_from_reserved_chat)
}

#[cfg(not(windows))]
pub fn poll_overlay_chat_host() -> Option<OverlayRect> {
    None
}

#[cfg(windows)]
fn overlay_rect_from_reserved_chat() -> Option<OverlayRect> {
    if !crate::dock::snapshot().reserve_chat {
        return None;
    }
    let chat = crate::dock::chat_video_split(true)?.1?;
    let width = chat.width();
    let height = chat.height();
    if width < 80 || height < 40 {
        return None;
    }
    Some(OverlayRect {
        x: chat.left,
        y: chat.top,
        width,
        height,
    })
}

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
    cmd.env(CHATTERINO_DOCK_ENV, "1");
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
    #[repr(C)]
    struct VsFixedFileInfo {
        signature: u32,
        struc_version: u32,
        file_version_ms: u32,
        file_version_ls: u32,
        product_version_ms: u32,
        product_version_ls: u32,
        file_flags_mask: u32,
        file_flags: u32,
        file_os: u32,
        file_type: u32,
        file_subtype: u32,
        file_date_ms: u32,
        file_date_ls: u32,
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
            || (len as usize) < std::mem::size_of::<VsFixedFileInfo>()
        {
            return None;
        }
        let info = &*(ptr as *const VsFixedFileInfo);
        let major = (info.product_version_ms >> 16) & 0xffff;
        let minor = info.product_version_ms & 0xffff;
        let patch = (info.product_version_ls >> 16) & 0xffff;
        // Chatterino prints "7.5.5" (3-part); omit build when zero.
        let build = info.product_version_ls & 0xffff;
        if build == 0 {
            Some(format!("{major}.{minor}.{patch}"))
        } else {
            Some(format!("{major}.{minor}.{patch}.{build}"))
        }
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
    static BYTES: &[u8] = include_bytes!("../assets/loading.png");
    let path = std::env::temp_dir().join("rillmux-loading.png");
    match std::fs::metadata(&path) {
        Ok(m) if m.len() as usize == BYTES.len() => Some(path),
        _ => std::fs::write(&path, BYTES).ok().map(|_| path),
    }
}

fn mpv_dock_arg_parts(
    channel: &str,
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
    parts.push(format!("--title={}", mpv_window_title(channel)));
    parts.push(format!("--force-media-title={}", mpv_window_title(channel)));
    // Last-one-wins: keep audible even if a custom extra tried to mute.
    parts.push("--mute=no".into());
    parts
}

fn build_mpv_dock_args(
    channel: &str,
    reserve_chat: bool,
    preset_args: &str,
    index: usize,
    count: usize,
    layout: Option<&str>,
) -> String {
    mpv_dock_arg_parts(channel, reserve_chat, preset_args, index, count, layout).join(" ")
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

fn mpv_window_title(channel: &str) -> String {
    format!(
        "{PLAYER_WINDOW_PREFIX}-{}",
        sanitize_player_channel(channel)
    )
}

fn legacy_mpv_window_title(channel: &str) -> String {
    format!(
        "{PLAYER_WINDOW_PREFIX_LEGACY}-{}",
        sanitize_player_channel(channel)
    )
}

#[cfg(windows)]
fn find_player_window(channel: &str) -> Option<*mut core::ffi::c_void> {
    find_window_by_title(&mpv_window_title(channel), true)
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
    if hwnd.is_null() {
        return;
    }
    unsafe {
        ShowWindow(hwnd, SW_SHOW);
        SetWindowPos(
            hwnd,
            HWND_TOP as *mut core::ffi::c_void,
            0,
            0,
            0,
            0,
            SWP_NOMOVE | SWP_NOSIZE | SWP_SHOWWINDOW,
        );
        BringWindowToTop(hwnd);
        if foreground {
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
    let mut first = true;
    for channel in channels.iter().take(8) {
        if let Some(hwnd) = find_player_window(channel) {
            raise_hwnd(hwnd, first);
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
            // Chat after video so it ends up in the Z-order next to mpv; don't steal FG from video.
            raise_hwnd(hwnd, false);
        }
    }
    crate::dock::raise_grips();
    raise_poll_overlay();
}

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
        return true;
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
fn process_command_line(pid: u32) -> Option<String> {
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
    }
    const PROCESS_QUERY_LIMITED_INFORMATION: u32 = 0x1000;
    const PROCESS_COMMAND_LINE_INFORMATION: u32 = 60;
    let handle = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid) };
    if handle.is_null() {
        return None;
    }
    let mut buf = vec![0u8; 4096];
    let mut needed = 0u32;
    let status = unsafe {
        NtQueryInformationProcess(
            handle,
            PROCESS_COMMAND_LINE_INFORMATION,
            buf.as_mut_ptr() as *mut _,
            buf.len() as u32,
            &mut needed,
        )
    };
    unsafe {
        let _ = CloseHandle(handle);
    }
    if status < 0 || needed < 8 {
        return None;
    }
    command_line_from_nt_buffer(&buf)
}

/// ProcessCommandLineInformation copies a UNICODE_STRING. Buffer is often a
/// remote pointer, not an address inside `buf` — the WCHAR payload then sits
/// immediately after the 16-byte header.
fn command_line_from_nt_buffer(buf: &[u8]) -> Option<String> {
    if buf.len() < 16 {
        return None;
    }
    let length = u16::from_le_bytes(buf[0..2].try_into().ok()?) as usize;
    if length == 0 || !length.is_multiple_of(2) {
        return None;
    }
    let ptr = usize::from_le_bytes(buf[8..16].try_into().ok()?);
    let base = buf.as_ptr() as usize;
    let bytes = if ptr >= base && ptr.checked_add(length)? <= base.saturating_add(buf.len()) {
        let off = ptr - base;
        buf.get(off..off + length)?
    } else if buf.len() >= 16 + length {
        &buf[16..16 + length]
    } else {
        return None;
    };
    let n = length / 2;
    let mut wide = Vec::with_capacity(n);
    for i in 0..n {
        wide.push(u16::from_le_bytes([bytes[i * 2], bytes[i * 2 + 1]]));
    }
    Some(String::from_utf16_lossy(&wide))
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
fn process_has_dock_env(pid: u32) -> bool {
    let Some(wide) = process_env_block(pid) else {
        return false;
    };
    let needle: Vec<u16> = format!("{CHATTERINO_DOCK_ENV}=").encode_utf16().collect();
    wide.windows(needle.len()).any(|w| w == needle.as_slice())
}

#[cfg(not(windows))]
fn process_has_dock_env(_pid: u32) -> bool {
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
    process_has_dock_env(pid)
        || process_command_line(pid).is_some_and(|cmd| cmd.contains(CHATTERINO_DOCK_ENV))
        || process_env_contains(pid, "chatterino-dock")
}

#[cfg(windows)]
fn process_env_contains(pid: u32, needle: &str) -> bool {
    let Some(wide) = process_env_block(pid) else {
        return false;
    };
    let needle: Vec<u16> = needle.encode_utf16().collect();
    if needle.is_empty() {
        return false;
    }
    wide.windows(needle.len()).any(|w| w == needle.as_slice())
}

#[cfg(not(windows))]
fn process_env_contains(_pid: u32, _needle: &str) -> bool {
    false
}

fn list_rillmux_dock_chatterino_pids() -> Vec<u32> {
    list_chatterino_pids()
        .into_iter()
        .filter(|&pid| process_is_dock_chatterino(pid))
        .collect()
}

/// The dock instance is tagged with `RILLMUX_DOCK=1`. Never returns the user's own Chatterino.
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

fn strip_ansi(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let mut chars = input.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\u{1b}' {
            if chars.peek() == Some(&'[') {
                chars.next();
                for n in chars.by_ref() {
                    if n.is_ascii_alphabetic() {
                        break;
                    }
                }
            }
            continue;
        }
        out.push(c);
    }
    out
}

fn display_status(raw: &str) -> String {
    let cleaned = strip_ansi(raw).trim().to_string();
    // Drop Streamlink log prefixes like [cli][info]
    let mut s = cleaned.as_str();
    while s.starts_with('[') {
        if let Some(end) = s.find(']') {
            s = s[end + 1..].trim_start();
        } else {
            break;
        }
    }
    if s.is_empty() {
        cleaned
    } else {
        s.to_string()
    }
}

/// After the player is up, HLS playlist noise must not wake the UI.
fn should_forward_status(already_ready: bool, phase: &str, _ready: bool) -> bool {
    match phase {
        "ended" | "error" | "ads" => true,
        "ready" => !already_ready,
        _ => !already_ready,
    }
}

fn classify_line(line: &str) -> (&'static str, bool) {
    let lower = line.to_lowercase();
    if lower.contains("pre-roll ads") {
        ("ads", false)
    } else if is_fatal_streamlink_error(line) {
        ("error", false)
    } else if lower.contains("player:")
        || lower.contains("starting player")
        || lower.contains("writing to player")
    {
        // Ready = the player process actually started. "Opening stream" only
        // means Streamlink began fetching — it must NOT mark the session
        // ready (layout, handoff and the missing-window grace all key off it).
        ("ready", true)
    } else if lower.contains("opening stream")
        || lower.contains("available streams")
        || lower.contains("found matching plugin")
    {
        ("starting", false)
    } else {
        ("info", false)
    }
}

fn is_fatal_streamlink_error(line: &str) -> bool {
    let lower = line.to_lowercase();
    if lower.contains("[cli][error]") {
        return true;
    }
    let display = display_status(line).to_lowercase();
    display.starts_with("error:") || display.starts_with("error ")
}

fn update_session_status(state: &StreamingState, id: &str, status: &str, phase: &str, ready: bool) {
    if let Ok(mut map) = state.inner.lock() {
        if let Some(session) = map.get_mut(id) {
            // HLS "Low latency streaming…" / playlist reload errors must not
            // replace Playing or clear ready after the player has started.
            if session.info.ready && !ready && phase != "ended" {
                return;
            }
            session.info.status = status.to_string();
            session.info.phase = phase.to_string();
            if ready && !session.info.ready {
                session.ready_at = Some(Instant::now());
            }
            session.info.ready = ready;
        }
    }
}

fn emit_status(app: &AppHandle, payload: StreamStatusPayload) {
    let _ = app.emit("stream-status", payload);
}

fn schedule_handoff(
    app: AppHandle,
    state: SharedStreaming,
    session_id: String,
    replace_ids: Vec<String>,
    handoff_done: Arc<AtomicBool>,
) {
    if replace_ids.is_empty() {
        return;
    }
    if handoff_done.swap(true, Ordering::SeqCst) {
        return;
    }
    thread::spawn(move || {
        // Match StreamLinkerino: brief overlap for a smoother player swap.
        thread::sleep(Duration::from_millis(600));
        for old_id in replace_ids {
            if old_id == session_id {
                continue;
            }
            let _ = stop_stream(&state, &old_id);
        }
        let _ = app.emit("stream-sessions-changed", ());
    });
}

#[allow(clippy::too_many_arguments)]
fn spawn_output_readers(
    app: AppHandle,
    state: SharedStreaming,
    id: String,
    channel: String,
    stdout: impl std::io::Read + Send + 'static,
    stderr: impl std::io::Read + Send + 'static,
    replace_ids: Vec<String>,
    handoff_done: Arc<AtomicBool>,
    fast: Option<Arc<FastPlayerCtx>>,
) {
    let drain = |pipe: Box<dyn std::io::Read + Send>,
                 app: AppHandle,
                 state: SharedStreaming,
                 id: String,
                 channel: String,
                 replace_ids: Vec<String>,
                 handoff_done: Arc<AtomicBool>,
                 fast: Option<Arc<FastPlayerCtx>>,
                 emit_lines: bool| {
        thread::spawn(move || {
            let reader = BufReader::new(pipe);
            for line in reader.lines().map_while(Result::ok) {
                let trimmed = line.trim();
                if trimmed.is_empty() {
                    continue;
                }
                if !emit_lines {
                    continue;
                }
                // Fast start: Streamlink's local HTTP server is up — attach
                // the pre-launched mpv via IPC and mark the session ready.
                if let Some(fx) = fast.as_ref() {
                    let url = format!("http://127.0.0.1:{}/", fx.port);
                    if trimmed.contains(&url) && !fx.fired.swap(true, Ordering::SeqCst) {
                        let fx = fx.clone();
                        let app2 = app.clone();
                        let state2 = state.clone();
                        let id2 = id.clone();
                        let channel2 = channel.clone();
                        let replace2 = replace_ids.clone();
                        let handoff2 = handoff_done.clone();
                        thread::spawn(move || {
                            // Clear any prior idle/loading playlist state, then
                            // attach the live HTTP stream as a fresh demuxer.
                            let _ =
                                mpv_ipc_command(&fx.pipe, &["stop"], Duration::from_millis(800));
                            let attached = mpv_ipc_command(
                                &fx.pipe,
                                &["loadfile", &url, "replace"],
                                Duration::from_secs(5),
                            )
                            .is_ok();
                            if attached {
                                // Image/idle sessions leave mute/aid in a bad
                                // state (speaker "!"); force audible playback.
                                mpv_ensure_audible(&fx.pipe);
                                // Clear the loading-phase show-text now that
                                // video frames are on screen.
                                let _ = mpv_ipc_command(
                                    &fx.pipe,
                                    &["show-text", "", "1"],
                                    Duration::from_secs(2),
                                );
                            }
                            if !attached {
                                // Fallback: spawn mpv with the URL directly
                                // (no IPC). Title-based closing still finds it.
                                let _ = Command::new(&fx.player_path)
                                    .args(&fx.fallback_argv)
                                    .arg(&url)
                                    .stdin(Stdio::null())
                                    .spawn();
                            }
                            let status = "Playing".to_string();
                            update_session_status(&state2, &id2, &status, "ready", true);
                            emit_status(
                                &app2,
                                StreamStatusPayload {
                                    id: id2.clone(),
                                    channel: channel2,
                                    line: "Starting player: mpv (fast start)".into(),
                                    status,
                                    phase: "ready".into(),
                                    ready: true,
                                },
                            );
                            schedule_handoff(app2, state2, id2, replace2, handoff2);
                        });
                        continue;
                    }
                }
                let status = display_status(trimmed);
                let (phase, ready) = classify_line(trimmed);
                let already_ready = state
                    .inner
                    .lock()
                    .ok()
                    .and_then(|map| map.get(&id).map(|session| session.info.ready))
                    .unwrap_or(false);
                if !should_forward_status(already_ready, phase, ready) {
                    continue;
                }
                // Mirror loading phases ("Waiting for pre-roll ads…",
                // resolving, errors) onto the idle player's OSD — show-text
                // repaints immediately and replaces the previous message.
                if let Some(fx) = fast.as_ref() {
                    if !fx.fired.load(Ordering::SeqCst) && phase != "info" {
                        if let Ok(mut last) = fx.osd.lock() {
                            if *last != status {
                                *last = status.clone();
                                let pipe = fx.pipe.clone();
                                let msg = status.clone();
                                thread::spawn(move || {
                                    let _ = mpv_ipc_command(
                                        &pipe,
                                        &["show-text", msg.as_str(), "600000"],
                                        Duration::from_secs(2),
                                    );
                                });
                            }
                        }
                    }
                }
                update_session_status(&state, &id, &status, phase, ready);
                emit_status(
                    &app,
                    StreamStatusPayload {
                        id: id.clone(),
                        channel: channel.clone(),
                        line: trimmed.to_string(),
                        status: status.clone(),
                        phase: phase.to_string(),
                        ready,
                    },
                );
                if ready {
                    schedule_handoff(
                        app.clone(),
                        state.clone(),
                        id.clone(),
                        replace_ids.clone(),
                        handoff_done.clone(),
                    );
                }
            }
            // Streamlink closed its pipes: the stream ended or it died.
            if let Some(fx) = fast.as_ref() {
                if fx.no_close {
                    // Leave the pre-launched player; prune the Streamlink session.
                } else if !fx.fired.load(Ordering::SeqCst) {
                    // Never attached playback — nothing to show; close now.
                    close_session_player(&state, &id, true);
                } else if !fx.goodbye.swap(true, Ordering::SeqCst) {
                    // Show branded offline screen for a few seconds, then quit.
                    begin_offline_goodbye(
                        app.clone(),
                        state.clone(),
                        id.clone(),
                        channel.clone(),
                        fx.pipe.clone(),
                    );
                    return;
                } else {
                    // Sibling drain thread already started goodbye.
                    return;
                }
            }
            // Prune right away (closes the owned Chatterino) instead of
            // waiting for the watchdog tick. Give the process handle a
            // moment to signal exit after its pipes closed.
            thread::sleep(Duration::from_millis(200));
            if let Ok(true) = prune_dead_sessions(&state) {
                let _ = app.emit("stream-sessions-changed", ());
            }
        });
    };

    // Streamlink 8.x on Windows writes CLI logs to stdout (stderr is empty).
    // Parse both so "Starting player" marks the session ready.
    drain(
        Box::new(stdout),
        app.clone(),
        state.clone(),
        id.clone(),
        channel.clone(),
        replace_ids.clone(),
        handoff_done.clone(),
        fast.clone(),
        true,
    );
    drain(
        Box::new(stderr),
        app,
        state,
        id,
        channel,
        replace_ids,
        handoff_done,
        fast,
        true,
    );
}

/// Grab a free loopback port for Streamlink's external HTTP server.
fn free_loopback_port() -> Result<u16, StreamError> {
    let listener = std::net::TcpListener::bind("127.0.0.1:0")?;
    Ok(listener.local_addr()?.port())
}

pub fn start_stream(
    app: &AppHandle,
    state: &SharedStreaming,
    req: LaunchRequest,
) -> Result<StreamSession, StreamError> {
    let channel = req.channel.trim().trim_start_matches('#').to_lowercase();
    if channel.is_empty() {
        return Err(StreamError::Message("channel is empty".into()));
    }
    // Twitch logins: 1–25 chars of [a-z0-9_]. Reject everything else so the
    // value is always safe to embed in URLs, window titles and player args.
    if channel.len() > 25
        || !channel
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
    {
        return Err(StreamError::Message(format!(
            "invalid channel name: {channel}"
        )));
    }

    let quality = req
        .quality
        .filter(|q| !q.is_empty())
        .unwrap_or_else(|| "best".into());
    // Quality is passed as a bare CLI argument to Streamlink; restrict it to
    // selector characters (e.g. "best", "720p60", "1080p,720p,best") so a
    // malformed settings value can never be interpreted as a flag.
    if quality.len() > 64
        || !quality
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, ',' | '_' | '+' | '-'))
    {
        return Err(StreamError::Message(format!(
            "invalid quality selector: {quality}"
        )));
    }
    let source = req.streamlink_source.as_deref().unwrap_or("bundled");
    let player_id = req.player_id.as_deref().unwrap_or("mpv");

    let (streamlink, _source_label) =
        resolve_streamlink(source, req.streamlink_custom_path.as_deref())?;
    let player = resolve_player(player_id, req.player_custom_path.as_deref())?;

    let title = req.title.clone().unwrap_or_else(|| channel.clone());
    let game = req.game.clone().unwrap_or_default();

    // Fast start (Windows + mpv + stdin pipe): pre-launch mpv idle so the
    // window appears immediately, then serve the stream through Streamlink's
    // loopback HTTP server and attach playback via mpv's IPC pipe (measured:
    // window at ~0.4 s instead of ~2.3 s after clicking watch).
    let reserve_chat = req.reserve_chat.unwrap_or(false);
    let slot_index = req.slot_index.unwrap_or(0) as usize;
    let slot_count = req.slot_count.unwrap_or(1) as usize;
    let preset_player_args = req
        .player_custom_args
        .clone()
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| default_player_args(player_id, &channel, &title, &game));
    let mut fast_player: Option<FastPlayer> = None;
    let mut fast_ctx: Option<Arc<FastPlayerCtx>> = None;
    let mut use_fast = false;
    let mut fast_pid: Option<u32> = None;
    #[cfg(windows)]
    if player_id == "mpv" && req.player_input.as_deref().unwrap_or("default") == "default" {
        if let Some(player_path) = &player {
            let no_close = req.player_no_close.unwrap_or(false);
            let port = free_loopback_port()?;
            let pipe = format!(r"\\.\pipe\rillmux-mpv-{}", Uuid::new_v4().simple());
            let dock_argv = mpv_dock_arg_parts(
                &channel,
                reserve_chat,
                &preset_player_args,
                slot_index,
                slot_count,
                req.layout.as_deref(),
            );
            let mut idle_argv = dock_argv.clone();
            idle_argv.push("--idle=yes".into());
            idle_argv.push(format!("--input-ipc-server={pipe}"));
            // Do NOT play loading.png as the first media file: an image has no
            // audio track, and mpv can then fail to attach sound when loadfile
            // replaces it with the live stream (speaker shows "!"). Branding
            // is the OSD show-text below on a dark idle window instead.
            idle_argv.push("--force-window=immediate".into());
            if let Ok(mpv_child) = Command::new(player_path)
                .args(&idle_argv)
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()
            {
                let job = assign_job(&mpv_child);
                fast_pid = Some(mpv_child.id());
                // Idle window is otherwise empty until the stream attaches.
                // show-text repaints immediately; osd-msg1 does not.
                let osd_pipe = pipe.clone();
                let osd_msg = format!("Starting {channel}…");
                thread::spawn(move || {
                    let _ = mpv_ipc_command(
                        &osd_pipe,
                        &["show-text", osd_msg.as_str(), "600000"],
                        Duration::from_secs(3),
                    );
                });
                use_fast = true;
                fast_ctx = Some(Arc::new(FastPlayerCtx {
                    pipe: pipe.clone(),
                    port,
                    player_path: player_path.clone(),
                    fallback_argv: dock_argv,
                    fired: Arc::new(AtomicBool::new(false)),
                    goodbye: Arc::new(AtomicBool::new(false)),
                    osd: Mutex::new(String::new()),
                    no_close,
                }));
                fast_player = Some(FastPlayer {
                    child: mpv_child,
                    job,
                    pipe,
                    no_close,
                });
            }
        }
    }

    let mut args: Vec<String> = Vec::new();
    if let Some(auth_arg) = crate::twitch_web_auth::streamlink_auth_arg()
        .map_err(|error| StreamError::Message(error.to_string()))?
    {
        args.push(auth_arg);
    }
    if req.low_latency.unwrap_or(false) {
        args.push("--twitch-low-latency".into());
    }
    if req.disable_ads.unwrap_or(false) {
        args.push("--twitch-disable-ads".into());
    }
    if !use_fast {
        match req.player_input.as_deref().unwrap_or("default") {
            "fifo" => args.push("--player-fifo".into()),
            "http" => args.push("--player-continuous-http".into()),
            // "default" = stdin pipe (recommended). Passthrough is intentionally unsupported.
            _ => {}
        }
    }
    if req.webbrowser.unwrap_or(false) {
        args.push("--webbrowser".into());
        args.push("yes".into());
        if req.webbrowser_headless.unwrap_or(true) {
            args.push("--webbrowser-headless".into());
            args.push("yes".into());
        }
        if let Some(exec) = req
            .webbrowser_executable
            .as_deref()
            .filter(|s| !s.is_empty())
        {
            args.push("--webbrowser-executable".into());
            args.push(exec.to_string());
        }
    } else {
        args.push("--webbrowser".into());
        args.push("no".into());
    }
    if let Some(delay) = req.retry_streams {
        args.push("--retry-streams".into());
        args.push(delay.to_string());
    }
    if let Some(max) = req.retry_max {
        args.push("--retry-max".into());
        args.push(max.to_string());
    }
    if req.player_no_close.unwrap_or(false) {
        args.push("--player-no-close".into());
    }
    // Parallel segment fetch + stream data as it arrives (faster first frame).
    args.push("--stream-segment-threads".into());
    args.push("3".into());
    args.push("--hls-segment-stream-data".into());
    if use_fast {
        // Serve the stream on loopback HTTP; the pre-launched mpv attaches
        // via IPC once the watcher sees the printed URL.
        let port = fast_ctx.as_ref().map(|fx| fx.port).unwrap_or(0);
        args.push("--player-external-http".into());
        // Loopback only — never expose the stream on the network.
        args.push("--player-external-http-interface".into());
        args.push("127.0.0.1".into());
        args.push("--player-external-http-port".into());
        args.push(port.to_string());
        // Exit when the stream ends so the player can be cleaned up.
        args.push("--player-external-http-continuous".into());
        args.push("no".into());
    } else {
        // Keep Streamlink's own title short so it doesn't override our mpv --title=.
        args.push("--title".into());
        args.push(mpv_window_title(&channel));
        if let Some(player_path) = &player {
            args.push("--player".into());
            args.push(player_path.to_string_lossy().to_string());
            let player_args = if player_id == "mpv" {
                build_mpv_dock_args(
                    &channel,
                    reserve_chat,
                    &preset_player_args,
                    slot_index,
                    slot_count,
                    req.layout.as_deref(),
                )
            } else {
                preset_player_args.clone()
            };
            if !player_args.is_empty() {
                args.push("--player-args".into());
                args.push(player_args);
            }
        }
    }
    args.push(format!("twitch.tv/{channel}"));
    args.push(quality.clone());

    let replace_existing = req.replace_existing.unwrap_or(false);
    let replace_ids = if replace_existing {
        let map = state
            .inner
            .lock()
            .map_err(|_| StreamError::Message("streaming state poisoned".into()))?;
        map.keys().cloned().collect::<Vec<_>>()
    } else {
        Vec::new()
    };

    let mut child_cmd = Command::new(&streamlink);
    child_cmd
        .args(&args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        child_cmd.creation_flags(CREATE_NO_WINDOW);
    }
    let mut child = match child_cmd.spawn() {
        Ok(c) => c,
        Err(e) => {
            // Don't leave the pre-launched idle player behind.
            close_fast_player(&mut fast_player, false);
            return Err(e.into());
        }
    };

    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| StreamError::Message("failed to capture Streamlink stdout".into()))?;
    let stderr = child
        .stderr
        .take()
        .ok_or_else(|| StreamError::Message("failed to capture Streamlink stderr".into()))?;

    // Chatterino is opened from the frontend (`open_chatterino_chat`) so failures
    // surface in the UI. This worker only starts Streamlink/mpv.
    let id = Uuid::new_v4().to_string();
    let initial_status = if replace_ids.is_empty() {
        "Starting Streamlink…".to_string()
    } else {
        format!("Switching to {channel}…")
    };

    let info = StreamSession {
        id: id.clone(),
        channel: channel.clone(),
        quality,
        title: req.title,
        game: req.game,
        running: true,
        status: initial_status.clone(),
        phase: "starting".into(),
        ready: false,
        muted: false,
    };

    let handoff_done = Arc::new(AtomicBool::new(false));
    {
        // Put the child in a kill-job BEFORE inserting: the player it spawns
        // joins the job, so stop/prune can kill the whole tree.
        let job = assign_job(&child);
        let mut map = state
            .inner
            .lock()
            .map_err(|_| StreamError::Message("streaming state poisoned".into()))?;
        map.insert(
            id.clone(),
            LiveSession {
                info: info.clone(),
                child,
                job,
                player: fast_player,
                ready_at: None,
                mpv_missing_since: None,
                offline_until: None,
            },
        );
    }

    // Close the session (and with it the owned Chatterino) the instant the
    // pre-launched player process exits, e.g. the user closed its window.
    #[cfg(windows)]
    if let Some(pid) = fast_pid {
        watch_player_exit(pid, state.clone(), app.clone());
    }

    spawn_output_readers(
        app.clone(),
        state.clone(),
        id.clone(),
        channel.clone(),
        stdout,
        stderr,
        replace_ids,
        handoff_done,
        fast_ctx,
    );

    emit_status(
        app,
        StreamStatusPayload {
            id: info.id.clone(),
            channel: info.channel.clone(),
            line: initial_status.clone(),
            status: initial_status,
            phase: "starting".into(),
            ready: false,
        },
    );

    Ok(info)
}

pub fn list_sessions(state: &StreamingState) -> Result<Vec<StreamSession>, StreamError> {
    let _ = prune_dead_sessions(state)?;
    let map = state
        .inner
        .lock()
        .map_err(|_| StreamError::Message("streaming state poisoned".into()))?;
    Ok(map.values().map(|s| s.info.clone()).collect())
}

/// Toggle mpv mute for a session (by id). Returns the new muted state.
pub fn toggle_stream_mute(state: &StreamingState, id: &str) -> Result<bool, StreamError> {
    let mut map = state
        .inner
        .lock()
        .map_err(|_| StreamError::Message("streaming state poisoned".into()))?;
    let session = map
        .get_mut(id)
        .ok_or_else(|| StreamError::Message(format!("unknown session {id}")))?;
    let next = !session.info.muted;
    let pipe = session
        .player
        .as_ref()
        .map(|p| p.pipe.clone())
        .ok_or_else(|| {
            StreamError::Message(
                "mute needs a fast-start mpv session (IPC). Restart the stream.".into(),
            )
        })?;
    // JSON boolean — string "yes"/"no" is wrong for flag properties over IPC.
    mpv_ipc_json(
        &pipe,
        vec![
            serde_json::Value::String("set_property".into()),
            serde_json::Value::String("mute".into()),
            serde_json::Value::Bool(next),
        ],
        Duration::from_millis(800),
    )?;
    session.info.muted = next;
    Ok(next)
}

/// Drop sessions whose Streamlink exited or (when ready) whose mpv window is gone.
/// Returns true if any session was removed.
pub fn prune_dead_sessions(state: &StreamingState) -> Result<bool, StreamError> {
    let mut map = state
        .inner
        .lock()
        .map_err(|_| StreamError::Message("streaming state poisoned".into()))?;
    let mut remove: Vec<(String, String)> = Vec::new();
    for (id, session) in map.iter_mut() {
        let child_dead = match session.child.try_wait() {
            Ok(Some(_)) => true,
            Ok(None) => false,
            Err(_) => true,
        };
        // Fast-start sessions own the pre-launched mpv, so its process state
        // is authoritative: closing the player window exits mpv even in
        // --idle mode. Detect that within one watchdog tick instead of
        // waiting out the MPV_MISSING_GRACE window-title timeout.
        let player_dead = session
            .player
            .as_mut()
            .map(|p| !matches!(p.child.try_wait(), Ok(None)))
            .unwrap_or(false);
        // Natural offline goodbye: keep mpv up until offline_until (unless the
        // user closed the player window themselves).
        let in_offline_grace = session
            .offline_until
            .map(|t| Instant::now() < t)
            .unwrap_or(false);
        if in_offline_grace && !player_dead {
            if child_dead {
                session.info.running = false;
                session.info.phase = "ended".into();
            }
            continue;
        }
        // The window-title lookup is a heuristic and can produce false
        // negatives (renamed window, Unicode title, DWM timing). Never kill a
        // stream on a single miss: require the player window to be missing
        // continuously for MPV_MISSING_GRACE before treating it as closed.
        let window_missing = session_title_scan_needed(session.player.is_some())
            && session.info.ready
            && session
                .ready_at
                .map(|t| t.elapsed() > Duration::from_secs(8))
                .unwrap_or(false)
            && !mpv_window_alive(&session.info.channel);
        let mpv_gone = if window_missing {
            let since = session.mpv_missing_since.get_or_insert_with(Instant::now);
            since.elapsed() > MPV_MISSING_GRACE
        } else {
            session.mpv_missing_since = None;
            false
        };
        if child_dead || player_dead || mpv_gone {
            session.info.running = false;
            session.info.phase = "ended".into();
            if session.info.status.is_empty() {
                session.info.status = "Stopped".into();
            }
            remove.push((id.clone(), session.info.channel.clone()));
        }
    }
    if remove.is_empty() {
        return Ok(false);
    }
    for (id, channel) in &remove {
        let mut keep_player = false;
        if let Some(mut session) = map.remove(id) {
            // Natural end: honor --player-no-close and leave a pre-launched
            // player running (it becomes an unowned window of the user).
            keep_player = session.player.as_ref().is_some_and(|p| p.no_close);
            let _ = session.child.kill();
            let _ = session.child.wait();
            // Kill the whole tree (orphaned player included) via the job.
            terminate_job(&mut session.job);
            if keep_player {
                session.player = None;
            } else {
                close_fast_player(&mut session.player, true);
            }
        }
        if !keep_player {
            close_player_windows_for_channel(channel);
        }
    }
    if map.is_empty() {
        drop(map);
        close_owned_chatterino();
        crate::dock::clear_session();
    }
    Ok(true)
}

/// Background poll so closing mpv updates sessions without waiting for the UI refresh.
pub fn start_session_watchdog(app: AppHandle, state: SharedStreaming) {
    thread::spawn(move || loop {
        let count = state.inner.lock().map(|map| map.len()).unwrap_or(0);
        let timeout_ms = session_watchdog_timeout_ms(count);
        #[cfg(windows)]
        wait_session_processes(&state, timeout_ms as u32);
        #[cfg(not(windows))]
        thread::sleep(Duration::from_millis(timeout_ms));
        match prune_dead_sessions(&state) {
            Ok(true) => {
                let _ = app.emit("stream-sessions-changed", ());
            }
            Ok(false) => {}
            Err(_) => {}
        }
    });
}

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
    let empty = state
        .inner
        .lock()
        .map(|map| map.is_empty())
        .unwrap_or(false);
    if empty {
        close_owned_chatterino();
        crate::dock::clear_session();
    }
    Ok(())
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

/// Close mpv/VLC windows whose title starts with the channel name.
fn close_player_windows_for_channel(channel: &str) {
    #[cfg(windows)]
    {
        close_player_windows_for_channel_windows(channel);
    }
    #[cfg(not(windows))]
    {
        let _ = channel;
    }
}

#[cfg(windows)]
fn close_player_windows_for_channel_windows(channel: &str) {
    use std::sync::Mutex;

    #[link(name = "user32")]
    unsafe extern "system" {
        fn EnumWindows(
            cb: unsafe extern "system" fn(*mut core::ffi::c_void, isize) -> i32,
            lparam: isize,
        ) -> i32;
        fn GetWindowTextW(hwnd: *mut core::ffi::c_void, lp: *mut u16, n: i32) -> i32;
        fn GetWindowThreadProcessId(hwnd: *mut core::ffi::c_void, pid: *mut u32) -> u32;
        fn IsWindowVisible(hwnd: *mut core::ffi::c_void) -> i32;
        fn PostMessageW(hwnd: *mut core::ffi::c_void, msg: u32, w: usize, l: isize) -> i32;
    }
    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn OpenProcess(access: u32, inherit: i32, pid: u32) -> *mut core::ffi::c_void;
        fn TerminateProcess(handle: *mut core::ffi::c_void, code: u32) -> i32;
        fn CloseHandle(handle: *mut core::ffi::c_void) -> i32;
    }

    const WM_CLOSE: u32 = 0x0010;
    const PROCESS_TERMINATE: u32 = 0x0001;

    struct Data {
        prefixes: [String; 2],
        pids: Mutex<Vec<u32>>,
    }

    unsafe extern "system" fn enum_cb(hwnd: *mut core::ffi::c_void, lparam: isize) -> i32 {
        let data = &*(lparam as *const Data);
        if IsWindowVisible(hwnd) == 0 {
            return 1;
        }
        let mut buf = [0u16; 512];
        let n = GetWindowTextW(hwnd, buf.as_mut_ptr(), buf.len() as i32);
        if n <= 0 {
            return 1;
        }
        let title = String::from_utf16_lossy(&buf[..n as usize]);
        let lower = title.to_ascii_lowercase();
        let matches_player = data.prefixes.iter().any(|prefix| {
            lower == prefix.as_str()
                || lower.starts_with(&format!("{prefix} -"))
                || lower.starts_with(&format!("{prefix}:"))
        });
        // Player windows we spawn are titled rillmux-<channel> (mpv --title /
        // VLC --input-title-format); VLC appends " - VLC media player".
        // Older builds used stgui-<channel>.
        if !matches_player {
            return 1;
        }
        let mut pid = 0u32;
        GetWindowThreadProcessId(hwnd, &mut pid);
        if pid != 0 {
            if let Ok(mut pids) = data.pids.lock() {
                if !pids.contains(&pid) {
                    pids.push(pid);
                }
            }
        }
        // Ask politely first.
        PostMessageW(hwnd, WM_CLOSE, 0, 0);
        1
    }

    let data = Data {
        prefixes: [mpv_window_title(channel), legacy_mpv_window_title(channel)],
        pids: Mutex::new(Vec::new()),
    };
    unsafe {
        EnumWindows(enum_cb, &data as *const Data as isize);
    }

    // Give WM_CLOSE a moment, then force-kill remaining processes.
    thread::sleep(Duration::from_millis(250));
    let pids = data.pids.lock().map(|g| g.clone()).unwrap_or_default();
    for pid in pids {
        unsafe {
            let handle = OpenProcess(PROCESS_TERMINATE, 0, pid);
            if !handle.is_null() {
                TerminateProcess(handle, 1);
                CloseHandle(handle);
            }
        }
    }
}

pub type SharedStreaming = Arc<StreamingState>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chatterino_reuses_a_live_instance_for_the_same_channels() {
        assert!(chatterino_should_reuse(true, "t:forsen", "t:forsen"));
        assert!(!chatterino_should_reuse(false, "t:forsen", "t:forsen"));
        assert!(!chatterino_should_reuse(true, "", "t:forsen"));
        assert!(!chatterino_should_reuse(true, "t:forsen", "t:forsen;t:xqc"));
        assert_eq!(
            chatterino_launch_plan(true, "t:forsen", "t:forsen"),
            ChatterinoLaunchPlan::Reuse
        );
        assert_eq!(
            chatterino_launch_plan(true, "t:forsen", "t:forsen;t:xqc"),
            ChatterinoLaunchPlan::RestartOwned
        );
        // A Chatterino window the user already had open is not "owned".
        assert_eq!(
            chatterino_launch_plan(false, "", "t:forsen"),
            ChatterinoLaunchPlan::SpawnFresh
        );
        assert_eq!(
            chatterino_launch_plan(true, "", "t:forsen"),
            ChatterinoLaunchPlan::RestartOwned
        );
    }

    #[test]
    fn chatterino_dock_appdata_is_not_the_user_chatterino_folder() {
        let dock = chatterino_dock_appdata();
        let name = dock.file_name().and_then(|s| s.to_str()).unwrap_or("");
        assert_eq!(name, "chatterino-dock");
        assert!(!dock.ends_with("Chatterino2"));
    }

    #[test]
    fn chatterino_picks_the_channels_split_over_a_blank_notebook() {
        let split = chatterino_window_pick_key(true, true, false, 80_000);
        let blank = chatterino_window_pick_key(false, true, false, 400_000);
        let cloaked = chatterino_window_pick_key(false, false, false, 400_000);
        assert!(split > blank);
        assert!(blank > cloaked);
        assert!(chatterino_title_matches_channels(
            "forsen - Chatterino",
            "t:forsen"
        ));
        assert!(!chatterino_title_matches_channels("Chatterino", "t:forsen"));
    }

    #[test]
    fn command_line_from_nt_buffer_reads_payload_after_header_when_pointer_is_foreign() {
        let text = "chatterino.exe --channels=t:forsen";
        let wide: Vec<u16> = text.encode_utf16().collect();
        let length = (wide.len() * 2) as u16;
        let mut buf = vec![0u8; 16 + wide.len() * 2];
        buf[0..2].copy_from_slice(&length.to_le_bytes());
        buf[2..4].copy_from_slice(&length.to_le_bytes());
        // Fake remote pointer — parser must use the packed payload at offset 16.
        buf[8..16].copy_from_slice(&0x7FFF_0000_1234_5678u64.to_le_bytes());
        for (i, unit) in wide.iter().enumerate() {
            let off = 16 + i * 2;
            buf[off..off + 2].copy_from_slice(&unit.to_le_bytes());
        }
        assert_eq!(command_line_from_nt_buffer(&buf).as_deref(), Some(text));
    }

    #[test]
    fn command_line_from_nt_buffer_reads_payload_when_pointer_is_inside_buffer() {
        let text = "chatterino.exe";
        let wide: Vec<u16> = text.encode_utf16().collect();
        let length = (wide.len() * 2) as u16;
        let mut buf = vec![0u8; 16 + wide.len() * 2];
        buf[0..2].copy_from_slice(&length.to_le_bytes());
        buf[2..4].copy_from_slice(&length.to_le_bytes());
        for (i, unit) in wide.iter().enumerate() {
            let off = 16 + i * 2;
            buf[off..off + 2].copy_from_slice(&unit.to_le_bytes());
        }
        let ptr = buf.as_ptr() as u64 + 16;
        buf[8..16].copy_from_slice(&ptr.to_le_bytes());
        assert_eq!(command_line_from_nt_buffer(&buf).as_deref(), Some(text));
    }

    #[test]
    fn opening_stream_is_starting_not_ready() {
        // Regression: "Opening stream" was treated as ready, which started
        // the layout/handoff/missing-window timers before the player existed.
        let (phase, ready) = classify_line("[cli][info] Opening stream: source (hls)");
        assert_eq!(phase, "starting");
        assert!(!ready);
    }

    #[test]
    fn starting_player_marks_ready() {
        let (phase, ready) =
            classify_line("[cli][info] Starting player: C:\\Program Files\\mpv\\mpv.exe");
        assert_eq!(phase, "ready");
        assert!(ready);
    }

    #[test]
    fn low_latency_line_is_info_not_ready() {
        let (phase, ready) = classify_line("[cli][info] Low latency streaming (HLS live edge: 2)");
        assert_eq!(phase, "info");
        assert!(!ready);
    }

    #[test]
    fn hls_reload_error_is_not_fatal() {
        let (phase, ready) =
            classify_line("[stream.hls][error] Failed to reload playlist: Unable to open URL");
        assert_eq!(phase, "info");
        assert!(!ready);
    }

    #[test]
    fn cli_error_is_fatal() {
        let (phase, ready) = classify_line("[cli][error] Failed to start player: mpv");
        assert_eq!(phase, "error");
        assert!(!ready);
    }

    #[test]
    fn error_prefix_is_fatal() {
        let (phase, ready) =
            classify_line("error: No playable streams found on this URL: twitch.tv/foo");
        assert_eq!(phase, "error");
        assert!(!ready);
    }

    #[test]
    fn ready_session_drops_hls_noise() {
        assert!(!should_forward_status(true, "info", false));
        assert!(!should_forward_status(true, "ready", true));
        assert!(should_forward_status(true, "ended", false));
        assert!(should_forward_status(true, "error", false));
        assert!(should_forward_status(false, "info", false));
        assert!(should_forward_status(false, "ready", true));
    }

    #[test]
    fn channel_and_quality_validation() {
        // mpv_window_title strips anything outside [a-z0-9_-].
        assert_eq!(mpv_window_title("Some_Channel-1"), "rillmux-some_channel-1");
        assert_eq!(mpv_window_title("äöü"), "rillmux-stream");
        assert_eq!(
            legacy_mpv_window_title("Some_Channel-1"),
            "stgui-some_channel-1"
        );
    }

    #[test]
    fn dock_watchdog_idles_when_focus_and_layout_are_stable() {
        assert_eq!(dock_watchdog_interval_ms(false, false), 500);
        assert_eq!(dock_watchdog_interval_ms(true, true), 100);
        assert_eq!(dock_watchdog_interval_ms(true, false), 400);
        assert!(dock_watchdog_needs_fast_tick(
            true, false, false, false, false, false
        ));
        assert!(dock_watchdog_needs_fast_tick(
            false, false, true, false, false, false
        ));
        assert!(!dock_watchdog_needs_fast_tick(
            false, false, false, false, false, false
        ));
    }

    #[test]
    fn session_watchdog_skips_title_scans_when_mpv_process_is_owned() {
        assert!(!session_title_scan_needed(true));
        assert!(session_title_scan_needed(false));
        assert_eq!(session_watchdog_timeout_ms(0), 2500);
        assert_eq!(session_watchdog_timeout_ms(3), 1500);
    }

    #[test]
    fn points_hud_channel_from_overlay_label() {
        assert_eq!(
            points_hud_channel_from_label("points-hud-forsen"),
            Some("forsen")
        );
        assert_eq!(points_hud_channel_from_label("raid-overlay"), None);
        assert_eq!(points_hud_channel_from_label("points-hud-"), None);
    }

    #[test]
    fn hud_stacks_just_above_the_player_not_the_desktop() {
        // 0 = HWND_TOP (player is already front-most among peers).
        assert_eq!(hud_z_insert_after(10, 0), Some(0));
        // Already immediately above the player: do not raise over other apps.
        assert_eq!(hud_z_insert_after(10, 10), None);
        assert_eq!(hud_z_insert_after(10, 20), Some(20));
    }

    #[test]
    fn hud_stays_owned_so_the_chip_does_not_vanish() {
        assert!(!hud_needs_detach_owner(1));
        assert!(!hud_needs_detach_owner(0));
    }

    #[test]
    fn hud_never_reowns_to_mpv() {
        assert!(!hud_needs_reown(1, 42));
        assert!(!hud_needs_reown(42, 42));
        assert!(!hud_needs_reown(1, 0));
    }

    #[test]
    fn chatterino_retries_outlast_qt_screen_restore() {
        let ms = chatterino_place_retry_ms();
        assert!(ms.last().copied().unwrap_or(0) >= 3000);
        assert!(ms.len() >= 6);
    }

    #[test]
    fn chatterino_place_redoes_when_qt_snaps_back() {
        let target = OverlayRect {
            x: 1600,
            y: 0,
            width: 320,
            height: 1080,
        };
        let old_monitor = OverlayRect {
            x: 0,
            y: 0,
            width: 320,
            height: 1080,
        };
        assert!(overlay_rect_drifted(
            old_monitor,
            target,
            chatterino_place_slop_px()
        ));
        assert!(!overlay_rect_drifted(
            target,
            target,
            chatterino_place_slop_px()
        ));
    }

    #[test]
    fn chatterino_27px_left_gap_counts_as_drift() {
        let target = OverlayRect {
            x: 1540,
            y: 0,
            width: 380,
            height: 1032,
        };
        let gapped = OverlayRect {
            x: 1567,
            y: 0,
            width: 380,
            height: 1032,
        };
        assert!(overlay_rect_drifted(
            gapped,
            target,
            chatterino_place_slop_px()
        ));
        let aligned = OverlayRect {
            x: 1547,
            y: 0,
            width: 380,
            height: 1032,
        };
        assert!(!overlay_rect_drifted(
            aligned,
            target,
            chatterino_place_slop_px()
        ));
    }

    #[test]
    fn chatterino_retries_when_hwnd_vanishes_but_dock_still_runs() {
        assert!(chatterino_hwnd_lost_needs_retry(false, true));
        assert!(!chatterino_hwnd_lost_needs_retry(true, true));
        assert!(!chatterino_hwnd_lost_needs_retry(false, false));
    }

    #[test]
    fn chatterino_watchdog_places_when_hwnd_is_gone() {
        let target = OverlayRect {
            x: 1540,
            y: 0,
            width: 380,
            height: 1032,
        };
        assert!(chatterino_watchdog_should_place(
            false,
            true,
            None,
            target,
            chatterino_place_slop_px()
        ));
        assert!(!chatterino_watchdog_should_place(
            true,
            true,
            Some(target),
            target,
            chatterino_place_slop_px()
        ));
    }

    #[test]
    fn channel_points_hud_hides_missing_iconic_or_tiny_players() {
        assert_eq!(channel_points_hud_player_rect(None, false), None);
        assert_eq!(
            channel_points_hud_player_rect(
                Some(OverlayRect {
                    x: 0,
                    y: 0,
                    width: 800,
                    height: 450,
                }),
                true,
            ),
            None
        );
        assert_eq!(
            channel_points_hud_player_rect(
                Some(OverlayRect {
                    x: 0,
                    y: 0,
                    width: 199,
                    height: 120,
                }),
                false,
            ),
            None
        );
        assert_eq!(
            channel_points_hud_player_rect(
                Some(OverlayRect {
                    x: 10,
                    y: 20,
                    width: 800,
                    height: 450,
                }),
                false,
            ),
            Some(OverlayRect {
                x: 10,
                y: 20,
                width: 800,
                height: 450,
            })
        );
    }

    #[test]
    fn caption_avoid_matches_scaled_window_controls() {
        let main = OverlayRect {
            x: 100,
            y: 50,
            width: 1280,
            height: 800,
        };
        assert_eq!(
            caption_avoid_from_main(main.clone(), 1.0),
            OverlayRect {
                x: 100 + 1280 - 138,
                y: 50,
                width: 138,
                height: 38,
            }
        );
        assert_eq!(
            caption_avoid_from_main(main, 1.5),
            OverlayRect {
                x: 100 + 1280 - 207,
                y: 50,
                width: 207,
                height: 57,
            }
        );
    }

    #[test]
    fn player_caption_avoid_sits_on_the_stream_tile() {
        let player = OverlayRect {
            x: 0,
            y: 38,
            width: 1000,
            height: 800,
        };
        let avoid = caption_avoid_from_main(player, 1.0);
        assert_eq!(avoid.y, 38);
        assert_eq!(avoid.x, 1000 - 138);
        assert!(overlay_rects_overlap(avoid, player));
    }

    #[test]
    fn union_overlay_covers_plugin_and_dwm_caption_buttons() {
        let plugin = OverlayRect {
            x: 1439,
            y: 75,
            width: 138,
            height: 42,
        };
        let dwm = OverlayRect {
            x: 1432,
            y: 74,
            width: 146,
            height: 30,
        };
        assert_eq!(
            union_overlay_rect(plugin, dwm),
            OverlayRect {
                x: 1432,
                y: 74,
                width: 146,
                height: 43,
            }
        );
    }

    #[test]
    fn overlay_rects_overlap_detects_caption_coverage() {
        let caption = OverlayRect {
            x: 1142,
            y: 50,
            width: 138,
            height: 42,
        };
        let covering = OverlayRect {
            x: 1100,
            y: 40,
            width: 200,
            height: 80,
        };
        let below = OverlayRect {
            x: 1100,
            y: 50 + 42 + 16,
            width: 120,
            height: 36,
        };
        assert!(overlay_rects_overlap(covering, caption.clone()));
        assert!(!overlay_rects_overlap(below, caption));
    }

    #[test]
    fn hud_overlay_moves_when_the_player_jumps_monitors() {
        let old_chip = OverlayRect {
            x: 1199,
            y: 8,
            width: 120,
            height: 36,
        };
        let new_chip = OverlayRect {
            x: 1199,
            y: -1072,
            width: 120,
            height: 36,
        };
        let caption = OverlayRect {
            x: 1782,
            y: 0,
            width: 138,
            height: 38,
        };
        assert!(hud_overlay_should_apply(
            false,
            Some(old_chip),
            new_chip,
            Some(caption),
            12
        ));
        assert!(!hud_overlay_should_apply(
            false,
            Some(new_chip),
            new_chip,
            Some(caption),
            12
        ));
        assert!(hud_overlay_should_apply(
            true,
            Some(new_chip),
            new_chip,
            Some(caption),
            12
        ));
    }

    #[test]
    fn chatterino_watchdog_relaunches_after_the_owned_process_dies() {
        assert!(chatterino_watchdog_should_relaunch(
            false, true, true, 3_000, 2_000
        ));
        assert!(!chatterino_watchdog_should_relaunch(
            true, true, true, 3_000, 2_000
        ));
        assert!(!chatterino_watchdog_should_relaunch(
            false, true, true, 500, 2_000
        ));
        assert!(!chatterino_watchdog_should_relaunch(
            false, true, false, 3_000, 2_000
        ));
        assert!(!chatterino_watchdog_should_relaunch(
            false, false, true, 3_000, 2_000
        ));
    }

    #[test]
    fn chatterino_pid_after_child_exit_adopts_a_surviving_dock_pid() {
        assert_eq!(
            chatterino_pid_after_child_exit(Some(10), 10, &[10, 22]),
            Some(22)
        );
        assert_eq!(chatterino_pid_after_child_exit(Some(10), 10, &[10]), None);
        assert_eq!(chatterino_pid_after_child_exit(Some(7), 10, &[22]), Some(7));
        assert_eq!(chatterino_pid_after_child_exit(None, 10, &[22]), None);
    }

    #[test]
    fn chatterino_close_targets_include_owned_and_other_dock_pids() {
        assert_eq!(chatterino_pids_to_close(Some(10), &[10, 22]), vec![10, 22]);
        assert_eq!(chatterino_pids_to_close(None, &[22]), vec![22]);
        assert_eq!(chatterino_pids_to_close(Some(10), &[]), vec![10]);
        assert!(chatterino_pids_to_close(None, &[]).is_empty());
    }

    #[test]
    fn chatterino_spawn_after_close_is_stale() {
        assert!(chatterino_spawn_is_stale(1, 2));
        assert!(!chatterino_spawn_is_stale(3, 3));
    }

    #[test]
    fn chatterino_picks_a_discovered_dock_pid_when_owned_died() {
        assert_eq!(
            chatterino_pick_owned_pid(Some(10), false, Some(22)),
            Some(22)
        );
        assert_eq!(
            chatterino_pick_owned_pid(Some(10), true, Some(22)),
            Some(10)
        );
        assert_eq!(chatterino_pick_owned_pid(None, false, Some(22)), Some(22));
        assert_eq!(chatterino_pick_owned_pid(None, false, None), None);
    }

    #[test]
    fn chatterino_does_not_wm_close_qt_helper_windows() {
        assert!(
            !chatterino_should_close_duplicate_main(false, false, 80_000, "Chatterino", true),
            "hidden Qt helpers must not receive WM_CLOSE"
        );
        assert!(
            !chatterino_should_close_duplicate_main(false, true, 200, "Chatterino", true),
            "tiny surfaces must not receive WM_CLOSE"
        );
        assert!(
            !chatterino_should_close_duplicate_main(false, true, 80_000, "Chatterino", false),
            "do not close extras until the --channels split exists"
        );
        assert!(
            !chatterino_should_close_duplicate_main(
                true,
                true,
                80_000,
                "eliasn97 - Chatterino",
                true
            ),
            "keep hwnd must stay"
        );
        assert!(chatterino_should_close_duplicate_main(
            false,
            true,
            80_000,
            "Chatterino",
            true
        ));
    }

    #[test]
    #[cfg(windows)]
    fn dock_chatterino_spawn_stays_alive_and_opens_a_window() {
        let Some(path) = find_chatterino_path() else {
            return;
        };
        for pid in list_rillmux_dock_chatterino_pids() {
            terminate_pid(pid);
        }
        thread::sleep(Duration::from_millis(250));
        launch_chatterino_with_path(&path, "t:eliasn97", false, true)
            .expect("dock Chatterino must spawn and stay running");
        let pid = owned_chatterino_pid()
            .lock()
            .ok()
            .and_then(|g| *g)
            .expect("owned dock pid must be tracked");
        let started = Instant::now();
        let mut hwnd = None;
        while started.elapsed() < Duration::from_secs(8) {
            let alive = pid_is_alive(pid) || find_rillmux_dock_chatterino_pid().is_some();
            assert!(
                alive,
                "dock Chatterino pid={pid} died before a window appeared"
            );
            hwnd = find_main_window_for_pid(pid)
                .or_else(|| find_rillmux_dock_chatterino_pid().and_then(find_main_window_for_pid));
            if hwnd.is_some() {
                break;
            }
            thread::sleep(Duration::from_millis(100));
        }
        assert!(
            hwnd.is_some(),
            "dock Chatterino pid={pid} must create a window"
        );
        thread::sleep(Duration::from_secs(3));
        assert!(
            pid_is_alive(pid) || find_rillmux_dock_chatterino_pid().is_some(),
            "dock Chatterino pid={pid} died within 3s (watchdog-loop failure)"
        );
        eprintln!(
            "PROOF dock Chatterino alive pid={pid} hwnd={hwnd:?} after {:?}",
            started.elapsed()
        );
        close_owned_chatterino_wait(Duration::from_secs(2));
        for leftover in list_rillmux_dock_chatterino_pids() {
            terminate_pid(leftover);
        }
    }

    #[test]
    fn raid_overlay_uses_player_inset_and_clamps_narrow_hosts() {
        let wide = overlay_rect_from_host(OverlayRect {
            x: 10,
            y: 20,
            width: 800,
            height: 450,
        });
        assert_eq!(wide.x, 26);
        assert_eq!(wide.y, 36);
        assert_eq!(wide.width, 420);
        assert_eq!(wide.height, 92);

        let narrow = overlay_rect_from_host(OverlayRect {
            x: 810,
            y: 20,
            width: 300,
            height: 450,
        });
        assert_eq!(narrow.width, 268);
    }

    /// Minimized windows report a tiny GetWindowRect (~160x28). The dock
    /// minimize-sync watchdog must still resolve them by title.
    #[test]
    #[cfg(windows)]
    fn find_window_by_title_keeps_iconic_hwnd() {
        #[link(name = "user32")]
        unsafe extern "system" {
            fn RegisterClassExW(c: *const WndClassEx) -> u16;
            fn CreateWindowExW(
                ex: u32,
                class: *const u16,
                name: *const u16,
                style: u32,
                x: i32,
                y: i32,
                w: i32,
                h: i32,
                parent: *mut core::ffi::c_void,
                menu: *mut core::ffi::c_void,
                instance: *mut core::ffi::c_void,
                param: *mut core::ffi::c_void,
            ) -> *mut core::ffi::c_void;
            fn DestroyWindow(hwnd: *mut core::ffi::c_void) -> i32;
            fn ShowWindow(hwnd: *mut core::ffi::c_void, cmd: i32) -> i32;
            fn DefWindowProcW(hwnd: *mut core::ffi::c_void, msg: u32, w: usize, l: isize) -> isize;
            fn GetModuleHandleW(name: *const u16) -> *mut core::ffi::c_void;
        }
        #[repr(C)]
        struct WndClassEx {
            size: u32,
            style: u32,
            wnd_proc: Option<
                unsafe extern "system" fn(*mut core::ffi::c_void, u32, usize, isize) -> isize,
            >,
            cls_extra: i32,
            wnd_extra: i32,
            instance: *mut core::ffi::c_void,
            icon: *mut core::ffi::c_void,
            cursor: *mut core::ffi::c_void,
            background: *mut core::ffi::c_void,
            menu_name: *const u16,
            class_name: *const u16,
            icon_sm: *mut core::ffi::c_void,
        }
        unsafe extern "system" fn wnd_proc(
            hwnd: *mut core::ffi::c_void,
            msg: u32,
            w: usize,
            l: isize,
        ) -> isize {
            DefWindowProcW(hwnd, msg, w, l)
        }
        fn wide(s: &str) -> Vec<u16> {
            s.encode_utf16().chain(std::iter::once(0)).collect()
        }

        let class = wide("StguiIconicFindTest");
        let title = wide("rillmux-iconicfindtest");
        let instance = unsafe { GetModuleHandleW(std::ptr::null()) };
        let wc = WndClassEx {
            size: std::mem::size_of::<WndClassEx>() as u32,
            style: 0,
            wnd_proc: Some(wnd_proc),
            cls_extra: 0,
            wnd_extra: 0,
            instance,
            icon: std::ptr::null_mut(),
            cursor: std::ptr::null_mut(),
            background: std::ptr::null_mut(),
            menu_name: std::ptr::null(),
            class_name: class.as_ptr(),
            icon_sm: std::ptr::null_mut(),
        };
        unsafe {
            RegisterClassExW(&wc);
        }
        // WS_OVERLAPPEDWINDOW | WS_VISIBLE
        const STYLE: u32 = 0x00CF_0000 | 0x1000_0000;
        let hwnd = unsafe {
            CreateWindowExW(
                0,
                class.as_ptr(),
                title.as_ptr(),
                STYLE,
                100,
                100,
                640,
                480,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                instance,
                std::ptr::null_mut(),
            )
        };
        assert!(!hwnd.is_null(), "CreateWindowExW failed");
        #[link(name = "user32")]
        unsafe extern "system" {
            fn PeekMessageW(
                msg: *mut Msg,
                hwnd: *mut core::ffi::c_void,
                min: u32,
                max: u32,
                remove: u32,
            ) -> i32;
            fn TranslateMessage(msg: *const Msg) -> i32;
            fn DispatchMessageW(msg: *const Msg) -> isize;
        }
        #[repr(C)]
        struct Point {
            x: i32,
            y: i32,
        }
        #[repr(C)]
        struct Msg {
            hwnd: *mut core::ffi::c_void,
            message: u32,
            wparam: usize,
            lparam: isize,
            time: u32,
            pt: Point,
        }
        let pump = || unsafe {
            let mut msg = std::mem::zeroed::<Msg>();
            while PeekMessageW(&mut msg, std::ptr::null_mut(), 0, 0, 1) != 0 {
                TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }
        };
        unsafe {
            let _ = ShowWindow(hwnd, 5); // SW_SHOW
        }
        pump();
        assert!(
            find_window_by_title("rillmux-iconicfindtest", true).is_some(),
            "should find restored test window"
        );
        unsafe {
            let _ = ShowWindow(hwnd, 6); // SW_MINIMIZE
        }
        pump();
        std::thread::sleep(Duration::from_millis(100));
        pump();
        assert!(
            is_hwnd_iconic(hwnd),
            "test window should be iconic after minimize"
        );
        let found = find_window_by_title("rillmux-iconicfindtest", true);
        assert!(
            found.is_some(),
            "must still find iconic window (minimize-sync regression)"
        );
        assert_eq!(found.unwrap(), hwnd);
        unsafe {
            let _ = DestroyWindow(hwnd);
        }
        pump();
    }

    #[test]
    #[ignore = "diagnostic: needs a live mpv with IPC pipe (RILLMUX_PROBE_PIPE)"]
    #[cfg(windows)]
    fn probe_mpv_ipc() {
        let pipe = std::env::var("RILLMUX_PROBE_PIPE").expect("RILLMUX_PROBE_PIPE not set");
        let result = mpv_ipc_command(
            &pipe,
            &["get_property", "mpv-version"],
            Duration::from_secs(3),
        );
        println!("EVID ipc get_property: {:?}", result.is_ok());
        assert!(result.is_ok(), "mpv IPC command failed: {result:?}");
    }

    #[test]
    #[ignore = "diagnostic: needs a live mpv probe window (RILLMUX_PROBE_CHANNEL); moves windows"]
    #[cfg(windows)]
    fn probe_layout_evidence() {
        #[link(name = "user32")]
        unsafe extern "system" {
            fn GetWindowRect(hwnd: *mut core::ffi::c_void, rect: *mut WinRect) -> i32;
        }
        fn rect_of(hwnd: *mut core::ffi::c_void) -> Option<WinRect> {
            let mut r = WinRect {
                left: 0,
                top: 0,
                right: 0,
                bottom: 0,
            };
            (unsafe { GetWindowRect(hwnd, &mut r) } != 0).then_some(r)
        }

        let channels: Vec<String> = std::env::var("RILLMUX_PROBE_CHANNEL")
            .unwrap_or_else(|_| "probe".into())
            .split(',')
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect();
        let layout = std::env::var("RILLMUX_PROBE_LAYOUT").unwrap_or_else(|_| "2x2".into());
        let (video, chat) = chat_video_split(true).expect("chat_video_split");
        println!("EVID video area: {video:?}");
        println!("EVID chat area:  {chat:?}");
        println!(
            "EVID effective_layout(count={}, preset={layout}) = {}",
            channels.len(),
            effective_layout(channels.len(), &layout)
        );
        for (i, channel) in channels.iter().enumerate() {
            let title = mpv_window_title(channel);
            println!(
                "EVID launch geometry idx {i}: {:?}",
                mpv_geometry_for_dock(true, i, channels.len(), Some(&layout))
            );
            match find_player_window(channel) {
                Some(hwnd) => println!(
                    "EVID window '{title}' (idx {i}): found, rect before = {:?}",
                    rect_of(hwnd)
                ),
                None => println!("EVID window '{title}' (idx {i}): NOT FOUND"),
            }
        }
        let found = retile_player_windows(&channels, true, &layout);
        println!("EVID retile(layout={layout}) found={found}");
        for channel in &channels {
            let title = mpv_window_title(channel);
            if let Some(hwnd) = find_player_window(channel) {
                println!("EVID window '{title}': rect after = {:?}", rect_of(hwnd));
            }
        }
    }

    #[test]
    #[cfg(windows)]
    fn partially_filled_presets_shrink_to_count_grid() {
        // Regression: one stream under the default "2x2" preset was tiled into
        // the top-left quarter instead of filling the video area.
        assert_eq!(effective_layout(1, "2x2"), "1");
        assert_eq!(effective_layout(2, "2x2"), "2");
        assert_eq!(effective_layout(3, "2x2"), "2x2");
        assert_eq!(effective_layout(4, "4x2"), "2x2");
        assert_eq!(effective_layout(6, "4x2"), "3x2");
        assert_eq!(effective_layout(8, "4x2"), "4x2");
        // 3plus1 keeps its asymmetric main+stack split for 2+ channels.
        assert_eq!(effective_layout(1, "3plus1"), "1");
        assert_eq!(effective_layout(2, "3plus1"), "3plus1");
        // Vertical stack presets keep stacking for 2+ channels.
        assert_eq!(effective_layout(1, "1x2"), "1");
        assert_eq!(effective_layout(2, "1x2"), "1x2");
        assert_eq!(effective_layout(3, "1x3"), "1x3");
        assert_eq!(effective_layout(4, "1x4"), "1x4");
    }

    #[test]
    fn dock_args_keep_custom_extras_but_drop_owned_flags() {
        // Regression: dock mode silently discarded all custom mpv args except
        // --no-keepaspect-window and --loop-*.
        let args = build_mpv_dock_args(
            "chan",
            false,
            "--loop-file=inf --cache=yes --volume=42 --title=\"chan - g - t\" --geometry=50%x50%+0+0 --window-maximized=yes",
            0,
            1,
            Some("2x2"),
        );
        assert!(args.contains("--loop-file=inf"));
        assert!(args.contains("--cache=yes"));
        assert!(args.contains("--volume=42"));
        // Dock owns geometry and window title.
        assert!(!args.contains("--geometry=50%x50%"));
        assert!(!args.contains("--window-maximized"));
        assert!(!args.contains("chan - g - t"));
        assert!(args.contains("--title=rillmux-chan"));
        assert!(args.contains("--force-media-title=rillmux-chan"));
    }
}
