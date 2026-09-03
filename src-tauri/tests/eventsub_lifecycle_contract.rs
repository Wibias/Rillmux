#[test]
fn eventsub_uses_server_reconnect_url_and_keepalive_timeout() {
    let source = include_str!("../src/eventsub.rs");
    assert!(source.contains("reconnect_url"));
    assert!(source.contains("keepalive_timeout_seconds"));
    assert!(source.contains("connect_eventsub"));
    assert!(source.contains("keepalive_deadline"));
    assert!(source.contains("timeout(CONNECT_TIMEOUT, connect_async(url))"));
    assert!(!source.contains("drop and let supervisor reconnect to the default URL"));
}

#[test]
fn eventsub_auth_rejection_restarts_session_for_fresh_token() {
    let source = include_str!("../src/eventsub.rs");
    assert!(source.contains("subscription_auth_rejected"));
    let auth_arm_start = source
        .find("Err(CreateSubscriptionError::Auth(error)) => {")
        .expect("subscription auth rejection match arm");
    let auth_arm = &source[auth_arm_start..];
    let auth_arm_end = auth_arm
        .find("Err(CreateSubscriptionError::Other")
        .unwrap_or(auth_arm.len());
    assert!(auth_arm[..auth_arm_end].contains("return Err(error);"));
}
