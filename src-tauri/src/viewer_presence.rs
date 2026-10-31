// Viewer presence: keeps a per-channel minute-watched worker alive for Channel
// Points credit, using the website-authenticated playback session.
//
// The module is composed from focused shards that share this namespace.

include!("viewer_presence/state.rs");
include!("viewer_presence/sync.rs");
include!("viewer_presence/worker.rs");
include!("viewer_presence/playback.rs");
include!("viewer_presence/telemetry.rs");
include!("viewer_presence/tests.rs");
