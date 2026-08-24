const COMMANDS: &[&str] = &[
    "get_doctor_report",
    "get_twitch_client_id",
    "auth_get_session",
    "auth_start_device_login",
    "auth_poll_device_login",
    "auth_logout",
    "twitch_web_auth_status",
    "twitch_web_auth_save",
    "twitch_web_auth_clear",
    "channel_points_claim_auth_status",
    "channel_points_claim_auth_start_device_login",
    "channel_points_claim_auth_poll_device_login",
    "channel_points_claim_auth_clear",
    "viewer_presence_sync",
    "viewer_presence_status",
    "channel_points_refresh",
    "channel_points_cached",
    "channel_points_vote_poll",
    "channel_points_vote_prediction",
    "channel_points_redeem_reward",
    "helix_fetch",
    "stream_start",
    "stream_list",
    "stream_stop",
    "stream_stop_all",
    "stream_toggle_mute",
    "open_chatterino_chat",
    "close_owned_chatterino",
    "layout_watching",
    "dock_set_linked",
    "dock_set_chat_fraction",
    "dock_cycle_monitor",
    "diagnostics_set_debug",
    "diagnostics_open_logs",
    "diagnostics_open_crashes",
    "eventsub_sync",
    "raid_overlay_place",
    "channel_points_hud_place",
    "overlay_fit_webview",
    "overlay_place_hud",
    "poll_overlay_place",
    "poll_overlay_raise",
    "app_quit",
    "enable_title_bar_overlay",
];

fn main() {
    #[cfg(target_os = "windows")]
    {
        println!("cargo:rustc-link-lib=dbghelp");
    }

    tauri_build::try_build(
        tauri_build::Attributes::new()
            .app_manifest(tauri_build::AppManifest::new().commands(COMMANDS)),
    )
    .expect("failed to build Tauri application manifest");
}
