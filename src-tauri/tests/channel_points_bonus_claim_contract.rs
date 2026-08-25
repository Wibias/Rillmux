#[test]
fn bonus_claim_uses_tv_identity_without_browser_integrity() {
    let source = include_str!("../src/channel_points.rs");
    let operations = include_str!("../src/twitch_gql_operations.rs");

    assert!(source.contains("ClaimCommunityPoints"));
    assert!(source.contains("CLAIM_COMMUNITY_POINTS_HASH"));
    assert!(operations.contains("46aaeebe02c99afdf4fc97c7c0cba964124bf6b0af229395f1f6d1feed05b3d0"));
    assert!(source.contains("async fn claim_bonus("));
    assert!(source.contains("channel_points_claim_auth::TV_CLIENT_ID"));
    assert!(source.contains("channel_points_claim_auth::load_session"));
    assert!(source.contains("crate::channel_points_realtime::is_ready()"));

    assert!(!source.contains("https://gql.twitch.tv/integrity"));
    assert!(!source.contains("\"Client-Integrity\""));
    assert!(!source.contains("fetch_integrity_token"));
}
