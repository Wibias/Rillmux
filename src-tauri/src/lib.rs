// Rillmux Tauri application root.
//
// The command surface and lifecycle wiring live in focused shards under
// src/app/, which share this crate-root namespace.

mod auth;
mod branding;
mod channel_points;
mod channel_points_claim_auth;
mod channel_points_realtime;
mod diagnostics;
mod dock;
mod doctor;
mod eventsub;
mod helix;
mod http;
mod overlay;
mod streaming;
mod twitch_gql_operations;
mod twitch_web_auth;
mod viewer_presence;

use auth::{AuthSession, DeviceCodeResponse};
use doctor::DoctorReport;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex, OnceLock};
use streaming::{
    ChannelPointsHudPlace, LaunchRequest, OverlayRect, SharedStreaming, StreamSession,
    StreamingState,
};
use tauri::{AppHandle, Emitter, Manager};
use tauri_plugin_decorum::WebviewWindowExt;

include!("app/commands_account.rs");
include!("app/commands_channel_points.rs");
include!("app/commands_stream.rs");
include!("app/commands_diagnostics.rs");
include!("app/overlays.rs");
include!("app/lifecycle.rs");
include!("app/run.rs");
