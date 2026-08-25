from pathlib import Path
import json
import re

ROOT = Path(__file__).resolve().parents[1]

def read(path: str) -> str:
    return (ROOT / path).read_text(encoding="utf-8")

def write(path: str, text: str) -> None:
    p = ROOT / path
    p.parent.mkdir(parents=True, exist_ok=True)
    p.write_text(text, encoding="utf-8", newline="\n")

def replace_once(path: str, old: str, new: str) -> None:
    text = read(path)
    count = text.count(old)
    if count != 1:
        raise SystemExit(f"{path}: expected exactly one match, got {count}")
    write(path, text.replace(old, new, 1))

def remove_regex_once(path: str, pattern: str) -> None:
    text = read(path)
    updated, count = re.subn(pattern, "", text, count=1, flags=re.S)
    if count != 1:
        raise SystemExit(f"{path}: expected exactly one regex match, got {count}")
    write(path, updated)

# Task 3: exact per-Rillmux Chatterino ownership.
replace_once(
    "src-tauri/src/streaming/foundation.rs",
    '''/// Chatterino's QCommandLineParser exits on unknown flags. Tag the dock
/// instance with an env var instead of `--rillmux-dock`.
const CHATTERINO_DOCK_ENV: &str = "RILLMUX_DOCK";

fn chatterino_dock_appdata() -> PathBuf {
    crate::diagnostics::app_data_dir().join("chatterino-dock")
}
''',
    '''/// Chatterino's QCommandLineParser exits on unknown flags. Bind every
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
''',
)

replace_once(
    "src-tauri/src/streaming/windows_layout.rs",
    '    cmd.env(CHATTERINO_DOCK_ENV, "1");\n',
    '    cmd.env(CHATTERINO_DOCK_OWNER_ENV, chatterino_dock_owner());\n',
)

replace_once(
    "src-tauri/src/streaming/windows_layout.rs",
    '''#[cfg(windows)]
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
''',
    '''#[cfg(windows)]
fn process_has_dock_owner(pid: u32) -> bool {
    let Some(wide) = process_env_block(pid) else {
        return false;
    };
    let needle: Vec<u16> = format!(
        "{CHATTERINO_DOCK_OWNER_ENV}={}\\0",
        chatterino_dock_owner()
    )
    .encode_utf16()
    .collect();
    wide.windows(needle.len())
        .any(|window| window == needle.as_slice())
}

#[cfg(not(windows))]
fn process_has_dock_owner(_pid: u32) -> bool {
    false
}
''',
)

replace_once(
    "src-tauri/src/streaming/windows_layout.rs",
    '''fn process_is_dock_chatterino(pid: u32) -> bool {
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
''',
    '''fn process_is_dock_chatterino(pid: u32) -> bool {
    process_has_dock_owner(pid)
}
''',
)

replace_once(
    "src-tauri/src/streaming/windows_layout.rs",
    "/// The dock instance is tagged with `RILLMUX_DOCK=1`. Never returns the user's own Chatterino.\n",
    "/// Only returns Chatterino carrying this Rillmux process' exact owner token.\n",
)

remove_regex_once(
    "src-tauri/src/streaming/windows_layout.rs",
    r'#\[cfg\(windows\)\]\nfn process_command_line\(pid: u32\) -> Option<String> \{.*?\n\}\n\n/// ProcessCommandLineInformation copies a UNICODE_STRING\..*?\nfn command_line_from_nt_buffer\(buf: &\[u8\]\) -> Option<String> \{.*?\n\}\n\n(?=#\[cfg\(windows\)\]\nfn process_env_block)',
)

replace_once(
    "src-tauri/src/streaming/tests.rs",
    '''    #[test]
    fn chatterino_dock_appdata_is_not_the_user_chatterino_folder() {
        let dock = chatterino_dock_appdata();
        let name = dock.file_name().and_then(|s| s.to_str()).unwrap_or("");
        assert_eq!(name, "chatterino-dock");
        assert!(!dock.ends_with("Chatterino2"));
    }
''',
    '''    #[test]
    fn chatterino_dock_appdata_is_not_the_user_chatterino_folder() {
        let dock = chatterino_dock_appdata();
        if cfg!(debug_assertions) {
            let profile = dock
                .parent()
                .and_then(|path| path.file_name())
                .and_then(|name| name.to_str());
            assert_eq!(profile, Some("chatterino-dock-dev"));
        } else {
            assert_eq!(
                dock.file_name().and_then(|name| name.to_str()),
                Some("chatterino-dock")
            );
        }
        assert!(!dock.ends_with("Chatterino2"));
    }
''',
)

remove_regex_once(
    "src-tauri/src/streaming/tests.rs",
    r'    #\[test\]\n    fn command_line_from_nt_buffer_reads_payload_after_header_when_pointer_is_foreign\(\) \{.*?\n    \}\n\n    #\[test\]\n    fn command_line_from_nt_buffer_reads_payload_when_pointer_is_inside_buffer\(\) \{.*?\n    \}\n\n',
)

# Task 5: least-privilege overlay capabilities and caller-bound HUD placement.
replace_once(
    "src-tauri/build.rs",
    '    "channel_points_hud_place",\n    "overlay_fit_webview",\n',
    '    "channel_points_hud_place",\n    "points_hud_place_window",\n    "overlay_fit_webview",\n',
)

replace_once(
    "src-tauri/src/lib.rs",
    '''/// Force the overlay HWND and its WebView2 child to the physical size.
/// Transparent windows often keep the old child size after `setSize`.
#[tauri::command]
fn overlay_fit_webview(window: tauri::WebviewWindow, width: i32, height: i32) {
    streaming::fit_overlay_webview(&window, width, height);
}

#[tauri::command]
fn overlay_place_hud(
    app: AppHandle,
    label: String,
    x: i32,
    y: i32,
    width: i32,
    height: i32,
    force: bool,
) {
    streaming::place_hud_overlay(&app, &label, x, y, width, height, force);
}
''',
    '''#[tauri::command]
fn points_hud_place_window(
    app: AppHandle,
    channel_login: String,
    x: i32,
    y: i32,
    width: i32,
    height: i32,
    force: bool,
) {
    let channel = channel_login.trim().to_ascii_lowercase();
    if channel.is_empty()
        || channel.len() > 25
        || !channel
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || character == '_')
    {
        return;
    }
    let label = format!("points-hud-{}", channel);
    streaming::place_hud_overlay(&app, &label, x, y, width, height, force);
}

/// Force the overlay HWND and its WebView2 child to the physical size.
/// Transparent windows often keep the old child size after `setSize`.
#[tauri::command]
fn overlay_fit_webview(window: tauri::WebviewWindow, width: i32, height: i32) {
    streaming::fit_overlay_webview(&window, width, height);
}

#[tauri::command]
fn overlay_place_hud(
    window: tauri::WebviewWindow,
    x: i32,
    y: i32,
    width: i32,
    height: i32,
    force: bool,
) {
    if streaming::points_hud_channel_from_label(window.label()).is_none() {
        return;
    }
    streaming::place_hud_overlay(
        window.app_handle(),
        window.label(),
        x,
        y,
        width,
        height,
        force,
    );
}
''',
)

replace_once(
    "src-tauri/src/lib.rs",
    '''            raid_overlay_place,
            channel_points_hud_place,
            overlay_fit_webview,
''',
    '''            raid_overlay_place,
            channel_points_hud_place,
            points_hud_place_window,
            overlay_fit_webview,
''',
)

replace_once(
    "src/components/ChannelPointsHudSync.tsx",
    '''  await invoke("overlay_place_hud", {
    label: pointsHudLabel(channel),
''',
    '''  await invoke("points_hud_place_window", {
    channelLogin: channel,
''',
)

replace_once(
    "src/components/ChannelPointsHud.tsx",
    '''    await invoke("overlay_place_hud", {
      label: win.label,
      x: Math.round(rect.x),
''',
    '''    await invoke("overlay_place_hud", {
      x: Math.round(rect.x),
''',
)

default_cap = json.loads(read("src-tauri/capabilities/default.json"))
default_perms = default_cap["permissions"]
for permission in ["allow-overlay-fit-webview", "allow-overlay-place-hud"]:
    if permission not in default_perms:
        raise SystemExit(f"default capability missing expected {permission}")
    default_perms.remove(permission)
anchor = default_perms.index("allow-channel-points-hud-place") + 1
default_perms.insert(anchor, "allow-points-hud-place-window")
write("src-tauri/capabilities/default.json", json.dumps(default_cap, indent=2) + "\n")

points_cap = {
    "$schema": "../gen/schemas/desktop-schema.json",
    "identifier": "overlay",
    "description": "Least-privilege capability for Channel Points HUD windows",
    "windows": ["points-hud-*"],
    "permissions": [
        "core:event:default",
        "core:window:default",
        "core:window:allow-close",
        "core:window:allow-set-position",
        "core:window:allow-set-size",
        "core:window:allow-set-ignore-cursor-events",
        "store:default",
        "allow-channel-points-refresh",
        "allow-channel-points-cached",
        "allow-channel-points-redeem-reward",
        "allow-channel-points-hud-place",
        "allow-overlay-fit-webview",
        "allow-overlay-place-hud",
    ],
}
write("src-tauri/capabilities/overlay.json", json.dumps(points_cap, indent=2) + "\n")

poll_cap = {
    "$schema": "../gen/schemas/desktop-schema.json",
    "identifier": "poll-overlay",
    "description": "Least-privilege capability for the poll and prediction overlay",
    "windows": ["poll-overlay"],
    "permissions": [
        "core:event:default",
        "core:window:default",
        "core:window:allow-close",
        "core:window:allow-set-position",
        "core:window:allow-set-size",
        "core:window:allow-set-ignore-cursor-events",
        "store:default",
        "allow-channel-points-vote-poll",
        "allow-channel-points-vote-prediction",
    ],
}
write("src-tauri/capabilities/poll-overlay.json", json.dumps(poll_cap, indent=2) + "\n")

raid_cap = {
    "$schema": "../gen/schemas/desktop-schema.json",
    "identifier": "raid-overlay",
    "description": "Least-privilege capability for the raid prompt overlay",
    "windows": ["raid-overlay"],
    "permissions": [
        "core:event:default",
        "core:window:default",
        "core:window:allow-close",
        "core:window:allow-set-position",
        "core:window:allow-set-size",
        "core:window:allow-set-ignore-cursor-events",
        "store:default",
    ],
}
write("src-tauri/capabilities/raid-overlay.json", json.dumps(raid_cap, indent=2) + "\n")

# Task 6: keep Twitch OAuth out of process argv by using a randomized config file.
replace_once(
    "src-tauri/src/twitch_web_auth.rs",
    '''use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::OnceLock;
''',
    '''use serde::{Deserialize, Serialize};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;
''',
)

replace_once(
    "src-tauri/src/twitch_web_auth.rs",
    '''#[allow(dead_code)]
pub(crate) fn load_token() -> Result<Option<String>, TwitchWebAuthError> {
    Ok(load_auth()?.map(|auth| auth.token))
}

fn streamlink_auth_arg_for(token: &str) -> String {
    format!("--twitch-api-header=Authorization=OAuth {token}")
}

pub(crate) fn streamlink_auth_arg() -> Result<Option<String>, TwitchWebAuthError> {
    Ok(load_token()?.map(|token| streamlink_auth_arg_for(&token)))
}
''',
    '''#[allow(dead_code)]
pub(crate) fn load_token() -> Result<Option<String>, TwitchWebAuthError> {
    Ok(load_auth()?.map(|auth| auth.token))
}

pub(crate) struct StreamlinkAuthConfig {
    path: PathBuf,
}

impl StreamlinkAuthConfig {
    pub(crate) fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for StreamlinkAuthConfig {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
    }
}

fn streamlink_auth_config_for(token: &str) -> Result<StreamlinkAuthConfig, TwitchWebAuthError> {
    let path = std::env::temp_dir().join(format!(
        "rillmux-streamlink-auth-{}-{}.conf",
        std::process::id(),
        Uuid::new_v4().simple()
    ));
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options.open(&path)?;
    writeln!(file, "twitch-api-header=Authorization=OAuth {token}")?;
    file.flush()?;
    Ok(StreamlinkAuthConfig { path })
}

pub(crate) fn streamlink_auth_config(
) -> Result<Option<StreamlinkAuthConfig>, TwitchWebAuthError> {
    let Some(token) = load_token()? else {
        return Ok(None);
    };
    streamlink_auth_config_for(&token).map(Some)
}
''',
)

replace_once(
    "src-tauri/src/twitch_web_auth.rs",
    '''    #[test]
    fn formats_streamlink_auth_as_one_cli_argument() {
        let arg = streamlink_auth_arg_for(TOKEN);
        assert_eq!(
            arg,
            format!("--twitch-api-header=Authorization=OAuth {TOKEN}")
        );
        assert!(!arg.contains(char::from(10)));
    }
''',
    '''    #[test]
    fn ephemeral_streamlink_auth_config_is_removed_on_drop() {
        let config = streamlink_auth_config_for(TOKEN).unwrap();
        let path = config.path().to_path_buf();
        assert_eq!(
            std::fs::read_to_string(&path).unwrap(),
            format!("twitch-api-header=Authorization=OAuth {TOKEN}\\n")
        );
        drop(config);
        assert!(!path.exists());
    }
''',
)

replace_once(
    "src-tauri/src/streaming/types_player.rs",
    '''    /// Windows Job containing the Streamlink child (and transitively the
    /// player). Terminating it kills the whole tree.
    job: JobSlot,
''',
    '''    /// Windows Job containing the Streamlink child (and transitively the
    /// player). Terminating it kills the whole tree.
    job: JobSlot,
    /// Randomized config carrying website OAuth outside the process argv.
    /// Keep it alive for the Streamlink child lifetime; Drop removes the file.
    _streamlink_auth_config: Option<crate::twitch_web_auth::StreamlinkAuthConfig>,
''',
)

replace_once(
    "src-tauri/src/streaming/runtime.rs",
    '''    let mut args: Vec<String> = Vec::new();
    if let Some(auth_arg) = crate::twitch_web_auth::streamlink_auth_arg()
        .map_err(|error| StreamError::Message(error.to_string()))?
    {
        args.push(auth_arg);
    }
''',
    '''    let streamlink_auth_config = crate::twitch_web_auth::streamlink_auth_config()
        .map_err(|error| StreamError::Message(error.to_string()))?;
    let mut args: Vec<String> = Vec::new();
    if let Some(config) = streamlink_auth_config.as_ref() {
        args.push("--config".into());
        args.push(config.path().to_string_lossy().into_owned());
    }
''',
)

replace_once(
    "src-tauri/src/streaming/runtime.rs",
    '''                child,
                job,
                player: fast_player,
''',
    '''                child,
                job,
                _streamlink_auth_config: streamlink_auth_config,
                player: fast_player,
''',
)

print("remaining full-review source fixes applied")
