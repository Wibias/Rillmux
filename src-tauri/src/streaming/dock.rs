// Streaming dock orchestration.
//
// The module is composed from focused shards that share the streaming
// namespace.

include!("dock/layout.rs");
include!("dock/apply.rs");
include!("dock/watchdog.rs");
include!("dock/focus.rs");
include!("dock/members.rs");
include!("dock/commands.rs");
