#[test]
fn channel_points_uses_authenticated_hermes_without_gating_watch_credit() {
    let realtime = include_str!("../src/channel_points_realtime.rs");
    let lib = include_str!("../src/lib.rs");

    assert!(realtime.contains("wss://hermes.twitch.tv/v1?clientId="));
    assert!(realtime.contains("https://www.twitch.tv"));
    assert!(realtime.contains("community-points-user-v1."));
    assert!(realtime.contains("video-playback-by-id."));
    assert!(realtime.contains("\"type\": \"authenticate\""));
    assert!(realtime.contains("\"token\": token"));

    let start = lib
        .find("async fn viewer_presence_sync")
        .expect("viewer_presence_sync command");
    let end = lib[start..]
        .find("fn viewer_presence_status")
        .map(|offset| start + offset)
        .expect("viewer_presence_status command after sync");
    let body = &lib[start..end];

    assert!(body.contains("channel_points_realtime::sync"));
    assert!(body.contains("viewer_presence::sync"));
    assert!(!body.contains("viewer_presence::cancel_all"));
    assert!(!body.contains("channel_points_realtime::is_ready"));
    assert!(!body.contains("waiting for Twitch realtime presence"));
}

#[test]
fn bonus_claim_falls_back_to_polled_context_without_realtime_gate() {
    let channel_points = include_str!("../src/channel_points.rs");
    let start = channel_points
        .find("pub async fn refresh")
        .expect("Channel Points refresh");
    let end = channel_points[start..]
        .find("pub async fn vote_poll")
        .map(|offset| start + offset)
        .expect("vote_poll after refresh");
    let body = &channel_points[start..end];

    assert!(body.contains("if let Some(claim_id) = context.claim_id.clone()"));
    assert!(!body.contains("channel_points_realtime::is_ready"));
}
