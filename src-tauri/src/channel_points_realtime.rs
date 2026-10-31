//! Authenticated Twitch Hermes presence for Channel Points watch credit.
//!
//! Twitch accepts minute-watched Spade telemetry without necessarily crediting
//! Channel Points. The working browser-authenticated miner also maintains the
//! private viewer + channel PubSub topics over Hermes, so viewer presence is
//! established here before the existing minute-watched worker is started.
//!
//! The module is composed from focused shards that share this namespace.

include!("channel_points_realtime/state.rs");
include!("channel_points_realtime/sync_api.rs");
include!("channel_points_realtime/session.rs");
include!("channel_points_realtime/raids.rs");
include!("channel_points_realtime/socket.rs");
include!("channel_points_realtime/tests.rs");
