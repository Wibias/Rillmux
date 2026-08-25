#[test]
fn eventsub_uses_server_reconnect_url_and_keepalive_timeout() {
    let source = include_str!("../src/eventsub.rs");
    assert!(source.contains("reconnect_url"));
    assert!(source.contains("keepalive_timeout_seconds"));
    assert!(source.contains("connect_eventsub"));
    assert!(source.contains("keepalive_deadline"));
    assert!(!source.contains("drop and let supervisor reconnect to the default URL"));
}

#[test]
fn eventsub_auth_rejection_restarts_session_for_fresh_token() {
    let source = include_str!("../src/eventsub.rs");
    assert!(source.contains("subscription_auth_rejected"));
    assert!(source.contains("return Err"));
}
