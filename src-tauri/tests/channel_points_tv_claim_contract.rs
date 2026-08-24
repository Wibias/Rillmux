#[test]
fn bonus_claim_uses_tv_identity_without_changing_watch_identity() {
    let points = include_str!("../src/channel_points.rs");
    let claim_auth = include_str!("../src/channel_points_claim_auth.rs");
    let lib = include_str!("../src/lib.rs");
    let presence = include_str!("../src/viewer_presence.rs");
    let realtime = include_str!("../src/channel_points_realtime.rs");

    assert!(lib.contains("mod channel_points_claim_auth;"));
    assert!(points.contains("ClaimCommunityPoints"));
    assert!(points.contains("channel_points_claim_auth::TV_CLIENT_ID"));
    assert!(points.contains("channel_points_claim_auth::load_session"));
    assert!(!points.contains("Client-Integrity"));

    // Bonus claiming needs an authenticated identity, not account mutation
    // privileges. Do not regress to the miner's broad legacy write scopes.
    assert!(claim_auth.contains("const SCOPES: &str = \"user_read\";"));
    assert!(!claim_auth.contains("user_blocks_edit"));
    assert!(!claim_auth.contains("user_follows_edit"));

    // Passive WATCH stays on the proven Website-authenticated transport.
    assert!(presence.contains("twitch_web_auth::load_session"));
    assert!(realtime.contains("twitch_web_auth::load_session"));
    assert!(!presence.contains("channel_points_claim_auth::"));
    assert!(!realtime.contains("channel_points_claim_auth::"));
}
