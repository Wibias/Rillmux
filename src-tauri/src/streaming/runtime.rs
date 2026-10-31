// Stream runtime: Streamlink + mpv session lifecycle.
//
// The module is composed from focused shards that share the parent streaming
// namespace.

include!("runtime/status.rs");
include!("runtime/handoff.rs");
include!("runtime/start.rs");
include!("runtime/sessions.rs");
include!("runtime/teardown.rs");
include!("runtime/player_windows.rs");

pub type SharedStreaming = Arc<StreamingState>;
