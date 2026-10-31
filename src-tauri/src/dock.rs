//! Linked dock: shared chat|video fraction, multi-monitor work area, and
//! always-on-top grip windows (Windows) for live resize / move.
//!
//! The module is composed from focused shards that share this namespace.

#![allow(
    clippy::type_complexity,
    clippy::needless_return,
    clippy::needless_range_loop
)]

use std::sync::atomic::{AtomicBool, AtomicIsize, Ordering};
use std::sync::{Mutex, OnceLock};
use std::thread;
use std::time::Duration;

include!("dock/geometry.rs");
include!("dock/config.rs");
include!("dock/session.rs");
include!("dock/win32_monitors.rs");
include!("dock/grip_thread.rs");
include!("dock/grip_api.rs");
include!("dock/grip_model.rs");
include!("dock/grip_proc.rs");
include!("dock/grip_windows.rs");
include!("dock/grip_sync.rs");
include!("dock/tests.rs");
