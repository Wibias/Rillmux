//! Twitch website authentication: keyring-backed session plus ephemeral
//! Streamlink auth config.
//!
//! The module is composed from focused shards that share this namespace.

include!("twitch_web_auth/model.rs");
include!("twitch_web_auth/runtime.rs");
include!("twitch_web_auth/store.rs");
include!("twitch_web_auth/streamlink.rs");
include!("twitch_web_auth/status.rs");
include!("twitch_web_auth/tests.rs");
