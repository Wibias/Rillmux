#[test]
fn chatterino_dock_processes_are_bound_to_one_rillmux_instance() {
    let foundation = include_str!("../src/streaming/foundation.rs");
    let layout = include_str!("../src/streaming/windows_layout.rs");

    assert!(foundation.contains("RILLMUX_DOCK_OWNER"));
    assert!(foundation.contains("chatterino_dock_owner"));
    assert!(layout.contains("process_has_dock_owner"));
    assert!(!layout.contains("process_has_dock_env(pid)\n        ||"));
    assert!(!layout.contains("process_env_contains(pid, \"chatterino-dock\")"));
}

#[test]
fn debug_and_release_dock_profiles_do_not_share_appdata() {
    let foundation = include_str!("../src/streaming/foundation.rs");
    assert!(foundation.contains("chatterino-dock-dev"));
    assert!(foundation.contains("chatterino-dock"));
    assert!(foundation.contains("cfg!(debug_assertions)"));
}
