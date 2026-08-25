#[test]
fn website_token_is_injected_at_streamlink_launch_not_persisted() {
    let auth = include_str!("../src/twitch_web_auth.rs");
    let streaming = include_str!("../src/streaming.rs");

    assert!(auth.contains("streamlink_auth_arg"));
    assert!(auth.contains("--twitch-api-header=Authorization=OAuth"));
    assert!(!auth.contains("fn write_streamlink_auth"));
    assert!(!auth.contains("upsert_managed_block"));
    assert!(streaming.contains("twitch_web_auth::streamlink_auth_arg"));
}
