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

    let auth_start = auth
        .find("pub(crate) fn streamlink_auth_config()")
        .expect("streamlink auth config function");
    let auth_end = auth[auth_start..]
        .find("pub(crate) fn load_session")
        .map(|offset| auth_start + offset)
        .expect("load_session after auth config");
    let auth_block = &auth[auth_start..auth_end];
    let cleanup = auth_block
        .find("remove_streamlink_auth()")
        .expect("legacy managed auth cleanup");
    let token_load = auth_block.find("load_token()").expect("token load");
    assert!(cleanup < token_load);

    let runtime_start = streaming
        .find("let streamlink_auth_config = match")
        .expect("streamlink auth config error handling");
    let runtime_end = streaming[runtime_start..]
        .find("let mut args")
        .map(|offset| runtime_start + offset)
        .expect("args after auth config");
    let runtime_block = &streaming[runtime_start..runtime_end];
    assert!(runtime_block.contains("close_fast_player(&mut fast_player, false);"));
}
