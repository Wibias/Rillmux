fn function_body<'a>(source: &'a str, signature: &str, next_signature: &str) -> &'a str {
    let start = source.find(signature).expect("function signature");
    let rest = &source[start..];
    let end = rest.find(next_signature).unwrap_or(rest.len());
    &rest[..end]
}

#[test]
fn mutable_token_entry_points_share_one_async_owner_gate() {
    let source = include_str!("../src/auth/mod.rs");
    assert!(
        source.contains("fn token_state_gate() -> &'static tokio::sync::Mutex<()>"),
        "auth token state needs one process-wide async ownership gate"
    );

    let poll = function_body(
        source,
        "pub async fn poll_device_token(device_code: &str)",
        "async fn refresh_if_needed",
    );
    assert!(
        poll.contains("token_state_gate().lock().await"),
        "successful device-token persistence must serialize with refresh/logout"
    );

    let get_session = function_body(
        source,
        "pub async fn get_session()",
        "pub async fn logout()",
    );
    assert!(
        get_session.contains("token_state_gate().lock().await"),
        "session restore must not race a shared one-time refresh token"
    );

    let logout = function_body(source, "pub async fn logout()", "pub struct ApiCredentials");
    assert!(
        logout.contains("token_state_gate().lock().await"),
        "logout must serialize against refresh persistence"
    );

    let credentials = function_body(
        source,
        "pub async fn credentials_for_api()",
        "pub fn public_client_id()",
    );
    assert!(
        credentials.contains("token_state_gate().lock().await"),
        "Helix/EventSub credential acquisition must share the refresh owner"
    );
}

#[test]
fn legacy_client_identity_is_recovered_before_proactive_refresh() {
    let source = include_str!("../src/auth/mod.rs");

    for (signature, next_signature) in [
        (
            "async fn session_from_tokens(mut tokens: StoredTokens)",
            "pub async fn get_session()",
        ),
        (
            "pub async fn credentials_for_api()",
            "pub fn public_client_id()",
        ),
    ] {
        let body = function_body(source, signature, next_signature);
        let recover = body
            .find("recover_legacy_client_id_if_possible(&mut tokens).await?")
            .expect("legacy client-id recovery");
        let refresh = body
            .find("refresh_if_needed(tokens).await?")
            .expect("proactive refresh");
        assert!(
            recover < refresh,
            "legacy token identity must be learned while its access token is still valid"
        );
    }
}

#[test]
fn fallback_client_refresh_rejection_does_not_destroy_shared_legacy_tokens() {
    let source = include_str!("../src/auth/mod.rs");
    let refresh = function_body(
        source,
        "async fn refresh_if_needed(mut tokens: StoredTokens)",
        "async fn validate_access_token",
    );

    assert!(
        refresh.contains(
            "let client_id_known = stored_token_client_id(tokens.client_id.as_deref()).is_some();"
        ),
        "refresh must distinguish a persisted token identity from an app-id fallback"
    );
    assert!(
        refresh.contains("if should_clear_tokens_after_refresh_rejection(client_id_known)"),
        "400/401 may clear tokens only when the refresh used the known issuing client id"
    );
}
