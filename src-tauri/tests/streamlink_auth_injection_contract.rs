#[test]
fn website_token_is_passed_via_ephemeral_config_not_process_command_line() {
    let auth = include_str!("../src/twitch_web_auth.rs");
    let streaming = include_str!("../src/streaming/runtime.rs");

    assert!(auth.contains("streamlink_auth_config"));
    assert!(auth.contains("twitch-api-header=Authorization=OAuth"));
    assert!(!auth.contains("--twitch-api-header=Authorization=OAuth"));
    assert!(!auth.contains("fn streamlink_auth_arg"));
    assert!(streaming.contains("--config"));
    assert!(streaming.contains("streamlink_auth_config"));
}
