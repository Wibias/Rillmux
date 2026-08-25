#[test]
fn ready_transition_resyncs_channel_points_presence() {
    let store = include_str!("../../src/lib/streaming/store.ts");
    let start = store
        .find("applyStatus: (payload) =>")
        .expect("stream status handler");
    let end = store[start..]
        .find("watchStream: async")
        .map(|offset| start + offset)
        .expect("watchStream after status handler");
    let body = &store[start..end];
    let ready = body
        .find("if (becameReady)")
        .expect("ready transition branch");
    let ready_body = &body[ready..body.len().min(ready + 240)];

    assert!(
        ready_body.contains("syncViewerPresence(true)"),
        "a session excluded while starting must be resynced as soon as it becomes ready"
    );
}

#[test]
fn stable_hud_host_state_does_not_log_every_watchdog_tick() {
    let lib = include_str!("../src/lib.rs");
    let helper = lib
        .find("fn hud_host_debug_changed")
        .expect("HUD host debug state-change helper");
    let helper_body = &lib[helper..lib.len().min(helper + 700)];

    assert!(helper_body.contains("previous != Some(host_found)"));

    let start = lib
        .find("fn channel_points_hud_place")
        .expect("channel_points_hud_place command");
    let end = lib[start..]
        .find("fn points_hud_place_window")
        .map(|offset| start + offset)
        .expect("points_hud_place_window after host command");
    let body = &lib[start..end];

    assert!(
        body.contains("hud_host_debug_changed(&channel, host_found)"),
        "host polling should emit diagnostics only when found/lost state changes"
    );
}
