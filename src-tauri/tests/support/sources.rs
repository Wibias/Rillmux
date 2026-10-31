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
    include_str!("../../src/streaming/windows_layout/mpv_and_layout.rs"),
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
