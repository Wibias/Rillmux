#[test]
fn settings_exposes_and_syncs_debug_categories() {
    let settings = include_str!("../../src/pages/SettingsPage.tsx");
    for marker in [
        "debugOutput",
        "debugWindows",
        "debugPointsCredit",
        "debugPointsClaim",
        "debugRewards",
        "debugPolls",
        "debugRaids",
        "diagnostics_set_debug_categories",
    ] {
        assert!(settings.contains(marker), "missing settings marker {marker}");
    }
}

#[test]
fn stream_and_window_lifecycle_is_instrumented() {
    let frontend = include_str!("../../src/lib/streaming/store.ts");
    for event in [
        "watch.start",
        "chatterino.open.request",
        "layout.request",
        "stream.stop.request",
    ] {
        assert!(frontend.contains(event), "missing frontend window event {event}");
    }

    let native = include_str!("../src/streaming/tools_process.rs");
    assert!(native.contains("chatterino.hwnd"));
    assert!(native.contains("chatterino.post_close.failed"));

    let hud = include_str!("../../src/components/ChannelPointsHud.tsx");
    assert!(hud.contains("hud.place.request"));
    assert!(hud.contains("hud.place.applied"));
}

#[test]
fn channel_points_credit_claim_and_rewards_are_instrumented() {
    let presence = include_str!("../src/viewer_presence.rs");
    for event in ["presence.sync", "worker.start", "minute_watched.result"] {
        assert!(presence.contains(event), "missing presence event {event}");
    }

    let realtime = include_str!("../src/channel_points_realtime.rs");
    for event in ["hermes.connect", "hermes.ready", "hermes.not_ready"] {
        assert!(realtime.contains(event), "missing Hermes event {event}");
    }

    let points = include_str!("../src/channel_points.rs");
    for event in [
        "context.query",
        "balance.snapshot",
        "claim.available",
        "claim.attempt",
        "claim.result",
        "reward.catalog",
        "reward.redeem",
    ] {
        assert!(points.contains(event), "missing Channel Points event {event}");
    }
}

#[test]
fn polls_predictions_and_raids_are_instrumented() {
    let realtime = include_str!("../src/channel_points_realtime.rs");
    assert!(realtime.contains("poll.subscription"));

    let points = include_str!("../src/channel_points.rs");
    assert!(points.contains("poll.vote"));
    assert!(points.contains("prediction.vote"));

    let eventsub = include_str!("../src/eventsub.rs");
    assert!(eventsub.contains("eventsub.sync"));
    assert!(eventsub.contains("raid.received"));

    let frontend = include_str!("../../src/lib/streaming/store.ts");
    assert!(frontend.contains("raid.follow"));
}
