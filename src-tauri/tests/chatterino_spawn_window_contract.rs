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
fn current_chatterino_main_window_is_selected_by_version_title_not_channel_name() {
    let foundation = include_str!("../src/streaming/foundation.rs");
    assert!(
        foundation.contains("fn chatterino_title_is_main_window"),
        "current Chatterino Windows main windows need a version-title classifier"
    );

    let windows = include_str!("../src/streaming/windows_layout.rs");
    let select_start = windows
        .find("fn find_main_window_for_pid(pid: u32)")
        .expect("missing Chatterino main-window selector");
    let select_body = &windows[select_start..];
    let select_end = select_body
        .find("fn top_level_windows_for_pid")
        .expect("missing selector boundary");
    let selector = &select_body[..select_end];
    assert!(
        selector.contains("chatterino_title_is_main_window"),
        "the HWND selector must prefer Chatterino's current version-titled main window"
    );

    let ready_start = windows
        .find("fn chatterino_pid_has_split_window(pid: u32) -> bool")
        .expect("missing split readiness detector");
    let ready_body = &windows[ready_start..];
    let ready_end = ready_body
        .find("fn chatterino_pid_has_split_window(_pid: u32) -> bool")
        .expect("missing non-Windows readiness boundary");
    let readiness = &ready_body[..ready_end];
    assert!(readiness.contains("chatterino_title_is_main_window"));
    assert!(
        !readiness.contains("chatterino_title_matches_channels"),
        "current Chatterino Windows titles do not contain the Twitch channel"
    );
}
