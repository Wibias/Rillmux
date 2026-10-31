//! Production source text for string-contract tests.
//!
//! Several large modules are split into focused shards composed with `include!`.
//! The contract tests search the production text, so they read those shards in
//! composition order instead of the thin wrapper file.
#![allow(dead_code)]

pub const CHANNEL_POINTS: &str = concat!(
    include_str!("../../src/channel_points/types_cache.rs"),
    "\n",
    include_str!("../../src/channel_points/gql_parse.rs"),
    "\n",
    include_str!("../../src/channel_points/realtime_fetch.rs"),
    "\n",
    include_str!("../../src/channel_points/commands.rs"),
);

pub const WINDOWS_LAYOUT: &str = concat!(
    include_str!("../../src/streaming/windows_layout/chatterino_spawn.rs"),
    "\n",
    include_str!("../../src/streaming/windows_layout/mpv/args.rs"),
    "\n",
    include_str!("../../src/streaming/windows_layout/mpv/player_window.rs"),
    "\n",
    include_str!("../../src/streaming/windows_layout/mpv/window_move.rs"),
    "\n",
    include_str!("../../src/streaming/windows_layout/mpv/raise.rs"),
    "\n",
    include_str!("../../src/streaming/windows_layout/process_ownership.rs"),
);

pub const DOCK: &str = concat!(
    include_str!("../../src/dock/geometry.rs"),
    "\n",
    include_str!("../../src/dock/config.rs"),
    "\n",
    include_str!("../../src/dock/session.rs"),
    "\n",
    include_str!("../../src/dock/win32_monitors.rs"),
    "\n",
    include_str!("../../src/dock/grip_thread.rs"),
    "\n",
    include_str!("../../src/dock/grip_api.rs"),
    "\n",
    include_str!("../../src/dock/grip_model.rs"),
    "\n",
    include_str!("../../src/dock/grip_proc.rs"),
    "\n",
    include_str!("../../src/dock/grip_windows.rs"),
    "\n",
    include_str!("../../src/dock/grip_sync.rs"),
);

pub const CHANNEL_POINTS_REALTIME: &str = concat!(
    include_str!("../../src/channel_points_realtime/state.rs"),
    "\n",
    include_str!("../../src/channel_points_realtime/sync_api.rs"),
    "\n",
    include_str!("../../src/channel_points_realtime/session.rs"),
    "\n",
    include_str!("../../src/channel_points_realtime/raids.rs"),
    "\n",
    include_str!("../../src/channel_points_realtime/socket.rs"),
);

pub const VIEWER_PRESENCE: &str = concat!(
    include_str!("../../src/viewer_presence/state.rs"),
    "\n",
    include_str!("../../src/viewer_presence/sync.rs"),
    "\n",
    include_str!("../../src/viewer_presence/worker.rs"),
    "\n",
    include_str!("../../src/viewer_presence/playback.rs"),
    "\n",
    include_str!("../../src/viewer_presence/telemetry.rs"),
);

pub const STREAMING_RUNTIME: &str = concat!(
    include_str!("../../src/streaming/runtime/status.rs"),
    "\n",
    include_str!("../../src/streaming/runtime/handoff.rs"),
    "\n",
    include_str!("../../src/streaming/runtime/start.rs"),
    "\n",
    include_str!("../../src/streaming/runtime/sessions.rs"),
    "\n",
    include_str!("../../src/streaming/runtime/teardown.rs"),
    "\n",
    include_str!("../../src/streaming/runtime/player_windows.rs"),
);

pub const LIB: &str = concat!(
    include_str!("../../src/lib.rs"),
    "\n",
    include_str!("../../src/app/commands_account.rs"),
    "\n",
    include_str!("../../src/app/commands_channel_points.rs"),
    "\n",
    include_str!("../../src/app/commands_stream.rs"),
    "\n",
    include_str!("../../src/app/commands_diagnostics.rs"),
    "\n",
    include_str!("../../src/app/overlays.rs"),
    "\n",
    include_str!("../../src/app/lifecycle.rs"),
    "\n",
    include_str!("../../src/app/run.rs"),
);

pub const OVERLAYS: &str = concat!(
    include_str!("../../src/streaming/overlays/rect.rs"),
    "\n",
    include_str!("../../src/streaming/overlays/caption.rs"),
    "\n",
    include_str!("../../src/streaming/overlays/policy.rs"),
    "\n",
    include_str!("../../src/streaming/overlays/hud.rs"),
    "\n",
    include_str!("../../src/streaming/overlays/window.rs"),
    "\n",
    include_str!("../../src/streaming/overlays/layout_watch.rs"),
);

pub const EVENTSUB: &str = concat!(
    include_str!("../../src/eventsub/status.rs"),
    "\n",
    include_str!("../../src/eventsub/state.rs"),
    "\n",
    include_str!("../../src/eventsub/supervisor.rs"),
    "\n",
    include_str!("../../src/eventsub/session.rs"),
    "\n",
    include_str!("../../src/eventsub/subscriptions.rs"),
    "\n",
    include_str!("../../src/eventsub/wire.rs"),
);

pub const TWITCH_WEB_AUTH: &str = concat!(
    include_str!("../../src/twitch_web_auth/model.rs"),
    "\n",
    include_str!("../../src/twitch_web_auth/runtime.rs"),
    "\n",
    include_str!("../../src/twitch_web_auth/store.rs"),
    "\n",
    include_str!("../../src/twitch_web_auth/streamlink.rs"),
    "\n",
    include_str!("../../src/twitch_web_auth/status.rs"),
);

pub const STREAMING_DOCK: &str = concat!(
    include_str!("../../src/streaming/dock/layout.rs"),
    "\n",
    include_str!("../../src/streaming/dock/apply.rs"),
    "\n",
    include_str!("../../src/streaming/dock/watchdog.rs"),
    "\n",
    include_str!("../../src/streaming/dock/focus.rs"),
    "\n",
    include_str!("../../src/streaming/dock/members.rs"),
    "\n",
    include_str!("../../src/streaming/dock/commands.rs"),
);

pub const STREAMING_STORE_TS: &str = concat!(
    include_str!("../../../src/lib/streaming/store.ts"),
    "\n",
    include_str!("../../../src/lib/streaming/store/types.ts"),
    "\n",
    include_str!("../../../src/lib/streaming/store/state.ts"),
    "\n",
    include_str!("../../../src/lib/streaming/store/presenceSync.ts"),
    "\n",
    include_str!("../../../src/lib/streaming/store/layoutChannels.ts"),
    "\n",
    include_str!("../../../src/lib/streaming/store/chatterinoStoreSync.ts"),
    "\n",
    include_str!("../../../src/lib/streaming/store/layoutSchedule.ts"),
    "\n",
    include_str!("../../../src/lib/streaming/store/listeners.ts"),
);
