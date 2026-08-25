#[test]
fn expected_channels_are_published_before_spawn_placement_starts() {
    let source = include_str!("../src/streaming/tools_process.rs");
    let start = source
        .find("pub fn launch_chatterino_for_channels")
        .expect("missing Chatterino launch entrypoint");
    let body = &source[start..];
    let publish = body
        .find("*guard = list.clone()")
        .expect("expected channel list is not published");
    let launch = body
        .find("launch_chatterino_with_path(&path, &list, true, true)")
        .expect("missing Chatterino spawn call");
    assert!(
        publish < launch,
        "the placement thread must see the new --channels list before it can select a HWND"
    );
    assert!(
        body.contains("*guard = previous_channels"),
        "a failed spawn must restore the previous expected channel list"
    );
}

#[test]
fn an_empty_expected_channel_list_is_never_treated_as_a_verified_split() {
    let source = include_str!("../src/streaming/windows_layout.rs");
    let start = source
        .find("fn chatterino_pid_has_split_window")
        .expect("missing split-window detector");
    let body = &source[start..];
    let end = body
        .find("\n}\n\n#[cfg(not(windows))]")
        .expect("could not bound split-window detector");
    let function = &body[..end];
    assert!(
        function.contains("if channels.is_empty() {\n        return false;"),
        "empty/stale channel state must keep waiting for the real --channels split"
    );
}

#[test]
fn duplicate_cleanup_targets_only_blank_notebooks_not_real_chatterino_dialogs() {
    let source = include_str!("../src/streaming/foundation.rs");
    let start = source
        .find("fn chatterino_should_close_duplicate_main")
        .expect("missing duplicate-window predicate");
    let body = &source[start..];
    let end = body
        .find("\n}\n\n/// `--channels")
        .expect("could not bound duplicate-window predicate");
    let function = &body[..end];
    assert!(function.contains("title.trim().is_empty()"));
    assert!(function.contains("eq_ignore_ascii_case(\"Chatterino\")"));
    assert!(
        !function.contains("contains(\"chatterino\")"),
        "broad title matching can close real Settings/popout windows"
    );
}
