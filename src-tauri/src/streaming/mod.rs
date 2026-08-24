// Structural shards intentionally share the `streaming` module namespace.
// Keep this order stable unless a follow-up refactor also changes interfaces.
include!("foundation.rs");
include!("types_player.rs");
include!("tools_process.rs");
include!("dock.rs");
include!("overlays.rs");
include!("windows_layout.rs");
include!("runtime.rs");
include!("tests.rs");
