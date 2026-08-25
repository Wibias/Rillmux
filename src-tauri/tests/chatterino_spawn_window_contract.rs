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
        .find("fn chatterino_pid_has_split_window(pid: u32) -> bool")
        .expect("missing Windows split-window detector");
    let body = &source[start..];
    let end = body
        .find("fn chatterino_pid_has_split_window(_pid: u32) -> bool")
        .expect("missing non-Windows split-window detector boundary");
    let function = &body[..end];
    let empty_guard = function
        .find("if channels.is_empty()")
        .expect("missing empty expected-channel guard");
    let guard_tail = &function[empty_guard..];
    let return_false = guard_tail
        .find("return false;")
        .expect("empty expected-channel guard must reject the split");
    let win32_lookup = guard_tail
        .find("#[link(name = \"user32\")]")
        .expect("missing Win32 title lookup after empty-channel guard");
    assert!(
        return_false < win32_lookup,
        "empty/stale channel state must keep waiting before any HWND is treated as the real split"
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
        .find("fn chatterino_title_matches_channels")
        .expect("missing title-match helper boundary");
    let function = &body[..end];
    assert!(function.contains("title.trim().is_empty()"));
    assert!(function.contains("eq_ignore_ascii_case(\"Chatterino\")"));
    assert!(
        !function.contains("contains(\"chatterino\")"),
        "broad title matching can close real Settings/popout windows"
    );
}

#[test]
fn current_chatterino_version_title_is_accepted_as_the_expected_main_window() {
    let source = include_str!("../src/streaming/foundation.rs");
    let classifier = source
        .find("fn chatterino_title_is_main_window")
        .expect("current Chatterino Windows main windows need a version-title classifier");
    let classifier_body = &source[classifier..];
    assert!(
        classifier_body.contains("starts_with(\"Chatterino \")"),
        "Chatterino fullVersion titles are `Chatterino <version>` / `Chatterino Nightly <version>`"
    );

    let matcher = source
        .find("fn chatterino_title_matches_channels")
        .expect("missing Chatterino title matcher");
    let matcher_body = &source[matcher..];
    assert!(
        matcher_body.contains("chatterino_title_is_main_window(title)"),
        "the existing HWND selector/readiness path must recognize current version-titled main windows"
    );

    let windows = include_str!("../src/streaming/windows_layout.rs");
    assert!(
        windows.matches("chatterino_title_matches_channels").count() >= 2,
        "both HWND selection and split readiness must use the corrected matcher"
    );
}
