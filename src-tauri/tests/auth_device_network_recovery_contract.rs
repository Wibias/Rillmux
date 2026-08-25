fn function_body<'a>(source: &'a str, signature: &str, next_signature: &str) -> &'a str {
    let start = source.find(signature).expect("function signature");
    let rest = &source[start..];
    let end = rest.find(next_signature).unwrap_or(rest.len());
    &rest[..end]
}

#[test]
fn device_login_transport_failures_use_recovering_http_mapping() {
    let source = include_str!("../src/auth/mod.rs");

    let start = function_body(
        source,
        "pub async fn start_device_flow()",
        "pub enum DevicePoll",
    );
    assert!(start.contains(".send()\n        .await\n        .map_err(map_http)?;"));

    let poll = function_body(
        source,
        "pub async fn poll_device_token(device_code: &str)",
        "async fn refresh_if_needed",
    );
    assert!(poll.contains(".send()\n        .await\n        .map_err(map_http)?;"));
}
