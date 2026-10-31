//! Twitch EventSub: raid notifications over WebSocket with automatic reconnect.
//!
//! The module is composed from focused shards that share this namespace.

include!("eventsub/status.rs");
include!("eventsub/state.rs");
include!("eventsub/supervisor.rs");
include!("eventsub/session.rs");
include!("eventsub/subscriptions.rs");
include!("eventsub/wire.rs");
include!("eventsub/tests.rs");
