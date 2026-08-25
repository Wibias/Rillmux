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

#[test]
fn other_shared_http_entry_points_recover_transport_failures() {
    let helix = include_str!("../src/helix.rs");
    assert!(helix.contains(".await\n        .map_err(reset_on_transport)?;"));

    let website_auth = include_str!("../src/twitch_web_auth.rs");
    let validate = function_body(
        website_auth,
        "async fn validate_token(token: &str)",
        "pub async fn save(raw_token: &str)",
    );
    assert!(validate.contains(".await\n        .map_err(reset_on_transport)?;"));

    let claim_auth = include_str!("../src/channel_points_claim_auth.rs");
    assert_eq!(claim_auth.matches(".map_err(reset_on_transport)?;").count(), 3);

    let eventsub = include_str!("../src/eventsub.rs");
    let supervisor = function_body(
        eventsub,
        "async fn run_supervisor(app: AppHandle)",
        "async fn connect_eventsub",
    );
    assert!(supervisor.contains("crate::http::reset_shared_client();"));
}
