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
    /// Opt-in: raise mpv speed while the stream is behind live, then restore.
    pub auto_catch_up: Option<bool>,
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
    /// Randomized config carrying website OAuth outside the process argv.
    /// Keep it alive for the Streamlink child lifetime; Drop removes the file.
    _streamlink_auth_config: Option<crate::twitch_web_auth::StreamlinkAuthConfig>,
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
    /// Opt-in live-drift catch-up for this session (mpv fast start only).
    catch_up: bool,
    /// Whether mpv is currently sped up for catch-up (avoids redundant IPC).
    catch_up_active: bool,
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
    // Remember the level the user left the player at so the next stream
    // (switch, raid follow, or a later watch) reopens at their volume.
    capture_player_volume(&p.pipe);
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
    /// `--volume=` target from the composed args (booster aware).
    volume: f64,
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

/// After attaching a live stream, force mute off and the configured volume.
/// Important: mpv's JSON IPC needs a real boolean for flags — the string
/// `"no"` is truthy and would *enable* mute (speaker shows "!").
fn mpv_ensure_audible(pipe: &str, volume: f64) {
    let _ = mpv_ipc_json(
        pipe,
        vec![
            serde_json::Value::String("set_property".into()),
            serde_json::Value::String("mute".into()),
            serde_json::Value::Bool(false),
        ],
        Duration::from_millis(800),
    );
    // mpv clamps `volume` to `volume-max` (default 130), and an opted-in
    // booster is only audible once that ceiling is raised to match.
    if volume > 100.0 {
        let _ = mpv_ipc_json(
            pipe,
            vec![
                serde_json::Value::String("set_property".into()),
                serde_json::Value::String("volume-max".into()),
                serde_json::Value::from(volume),
            ],
            Duration::from_millis(800),
        );
    }
    let _ = mpv_ipc_json(
        pipe,
        vec![
            serde_json::Value::String("set_property".into()),
            serde_json::Value::String("volume".into()),
            serde_json::Value::from(volume),
        ],
        Duration::from_millis(800),
    );
    let _ = mpv_ipc_command(
        pipe,
        &["set_property", "aid", "auto"],
        Duration::from_millis(800),
    );
}

/// Read a numeric mpv property over IPC (e.g. volume, demuxer-cache-time).
/// Returns None when the pipe is unavailable or mpv reports no number.
#[cfg(windows)]
fn mpv_get_property_f64(pipe: &str, property: &str) -> Option<f64> {
    use std::fs::OpenOptions;
    use std::io::{BufRead, BufReader, Write};
    let mut file = OpenOptions::new().read(true).write(true).open(pipe).ok()?;
    let msg = serde_json::json!({ "command": ["get_property", property] }).to_string() + "\n";
    file.write_all(msg.as_bytes()).ok()?;
    let mut reader = BufReader::new(file);
    let mut line = String::new();
    for _ in 0..20 {
        line.clear();
        if reader.read_line(&mut line).ok()? == 0 {
            break;
        }
        let trimmed = line.trim();
        if !trimmed.contains("\"error\"") {
            continue;
        }
        let value: serde_json::Value = serde_json::from_str(trimmed).ok()?;
        if value.get("error").and_then(|e| e.as_str()) != Some("success") {
            return None;
        }
        return value.get("data").and_then(|d| d.as_f64());
    }
    None
}

#[cfg(not(windows))]
fn mpv_get_property_f64(_pipe: &str, _property: &str) -> Option<f64> {
    None
}

/// Last mpv volume the user settled on, so switching streams (or following a
/// raid) keeps their level instead of resetting to the configured default.
/// Process-local: a fresh app start returns to the configured volume.
static LAST_MPV_VOLUME: OnceLock<Mutex<Option<f64>>> = OnceLock::new();

fn volume_is_usable(value: f64) -> bool {
    value.is_finite() && value > 0.0
}

fn remembered_volume() -> Option<f64> {
    LAST_MPV_VOLUME
        .get_or_init(|| Mutex::new(None))
        .lock()
        .ok()
        .and_then(|slot| *slot)
        .filter(|value| volume_is_usable(*value))
}

fn remember_volume(value: f64) {
    if !volume_is_usable(value) {
        return;
    }
    if let Ok(mut slot) = LAST_MPV_VOLUME.get_or_init(|| Mutex::new(None)).lock() {
        *slot = Some(value);
    }
}

/// Capture the volume of an owned player just before it closes.
fn capture_player_volume(pipe: &str) {
    if let Some(volume) = mpv_get_property_f64(pipe, "volume") {
        remember_volume(volume);
    }
}

/// Volume of a still-live owned player. A seamless switch launches the new
/// player while the old one is still playing, so read it before it is closed.
fn live_player_volume(state: &StreamingState) -> Option<f64> {
    let pipe = {
        let map = state.inner.lock().ok()?;
        map.values()
            .find_map(|session| session.player.as_ref().map(|player| player.pipe.clone()))
    }?;
    mpv_get_property_f64(&pipe, "volume")
}

/// New sessions start at the volume the user had on the stream they came from,
/// falling back to the composed --volume= value (the opt-in volume boost).
fn inherited_or_initial(inherited: Option<f64>, initial: f64) -> f64 {
    inherited
        .filter(|value| volume_is_usable(*value))
        .unwrap_or(initial)
}

/// Playback speed mpv is raised to while auto catch-up drains a lag.
const CATCH_UP_SPEED: f64 = 1.1;
/// Buffered lead (seconds) above which playback counts as behind live.
const CATCH_UP_ENTER_SECS: f64 = 6.0;
/// Buffered lead (seconds) below which playback returns to normal speed.
const CATCH_UP_EXIT_SECS: f64 = 2.0;

/// Hysteresis so the speed does not flap around the enter threshold.
fn catch_up_should_engage(active: bool, lead_secs: f64) -> bool {
    if !lead_secs.is_finite() {
        return false;
    }
    let threshold = if active {
        CATCH_UP_EXIT_SECS
    } else {
        CATCH_UP_ENTER_SECS
    };
    lead_secs > threshold
}

/// Speed up owned players that have drifted behind live, and drop back to
/// normal once the buffered lead drains. Opt-in per session.
fn apply_catch_up(state: &StreamingState) {
    let targets: Vec<(String, String, bool)> = {
        let Ok(map) = state.inner.lock() else {
            return;
        };
        map.iter()
            .filter_map(|(id, session)| {
                if !session.catch_up || !session.info.ready {
                    return None;
                }
                let pipe = session.player.as_ref()?.pipe.clone();
                Some((id.clone(), pipe, session.catch_up_active))
            })
            .collect()
    };
    for (id, pipe, active) in targets {
        // Missing lead means "not applicable"; treat it as caught up.
        let lead = mpv_cached_lead(&pipe).unwrap_or(0.0);
        let next = catch_up_should_engage(active, lead);
        if next == active {
            continue;
        }
        let speed = if next { CATCH_UP_SPEED } else { 1.0 };
        let applied = mpv_ipc_json(
            &pipe,
            vec![
                serde_json::Value::String("set_property".into()),
                serde_json::Value::String("speed".into()),
                serde_json::Value::from(speed),
            ],
            Duration::from_millis(500),
        )
        .is_ok();
        if applied {
            if let Ok(mut map) = state.inner.lock() {
                if let Some(session) = map.get_mut(&id) {
                    session.catch_up_active = next;
                }
            }
        }
    }
}

/// Forward-buffered media (seconds) mpv has downloaded ahead of playback. A
/// large lead means playback is behind the live edge. Tries the current mpv
/// property name first, then the older alias, skipping "not applicable" (-1).
fn mpv_cached_lead(pipe: &str) -> Option<f64> {
    for property in ["demuxer-cache-duration", "demuxer-cache-time"] {
        if let Some(value) = mpv_get_property_f64(pipe, property) {
            if value >= 0.0 {
                return Some(value);
            }
        }
    }
    None
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
