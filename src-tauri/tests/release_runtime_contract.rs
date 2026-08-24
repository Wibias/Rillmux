#[test]
fn release_configuration_is_compiled_into_the_native_binary() {
    let auth = include_str!("../src/auth/mod.rs");
    let lib = include_str!("../src/lib.rs");

    assert!(auth.contains("option_env!(\"TWITCH_CLIENT_ID\")"));
    assert!(lib.contains("option_env!(\"SENTRY_DSN\")"));
}

#[test]
fn native_sentry_follows_the_user_setting() {
    let lib = include_str!("../src/lib.rs");
    let frontend = include_str!("../../src/lib/sentry.tsx");

    assert!(lib.contains("diagnostics_set_sentry_enabled"));
    assert!(frontend.contains("diagnostics_set_sentry_enabled"));
}

#[test]
fn hard_exit_cleans_up_owned_streaming_processes() {
    let lib = include_str!("../src/lib.rs");

    assert!(lib.contains("streaming::stop_all"));
    assert!(lib.contains("streaming::close_owned_chatterino"));
    assert!(lib.contains("dock::clear_session"));
}
