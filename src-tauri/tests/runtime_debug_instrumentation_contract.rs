#[test]
fn settings_exposes_and_syncs_debug_categories() {
    let app = include_str!("../../src/App.tsx");
    assert!(app.contains("DebugOutputSettings"));
    assert!(app.contains("DebugDiagnosticsBootstrap"));

    let settings = include_str!("../../src/components/DebugOutputSettings.tsx");
    for marker in [
        "debugOutput",
        "debugWindows",
        "debugPointsCredit",
        "debugPointsClaim",
        "debugRewards",
        "debugPolls",
        "debugRaids",
    ] {
        assert!(
            settings.contains(marker),
            "missing settings marker {marker}"
        );
    }

    let bridge = include_str!("../../src/components/DebugDiagnosticsBootstrap.tsx");
    assert!(bridge.contains("diagnostics_set_debug_categories"));
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
        assert!(
            frontend.contains(event),
            "missing frontend window event {event}"
        );
    }

    let native = include_str!("../src/lib.rs");
    for event in [
        "chatterino.open.native",
        "chatterino.close.native.result",
        "hud.place.request",
        "hud.place.applied",
    ] {
        assert!(
            native.contains(event),
            "missing native window event {event}"
        );
    }

    let hwnd = include_str!("../src/streaming/debug_observability.rs");
    for marker in [
        "chatterino.hwnd",
        "IsWindow",
        "GetWindowRect",
        "GetLastError",
    ] {
        assert!(hwnd.contains(marker), "missing HWND evidence {marker}");
    }
}

#[test]
fn channel_points_credit_claim_and_rewards_are_instrumented() {
    let native = include_str!("../src/lib.rs");
    for event in [
        "presence.sync",
        "worker.start",
        "minute_watched.result",
        "context.query",
        "context.candidate.configured",
        "balance.snapshot",
        "claim.available",
        "claim.attempt",
        "claim.result",
        "reward.catalog",
        "reward.redeem",
    ] {
        assert!(
            native.contains(event),
            "missing Channel Points event {event}"
        );
    }
    assert!(native.contains("delta={balance_delta:?}"));
    assert!(native.contains("diagnostics::redact_hash(hash)"));

    let realtime = include_str!("../src/channel_points_realtime.rs");
    for event in [
        "hermes.connect.attempt",
        "hermes.auth.ack",
        "hermes.subscription.ack",
        "hermes.ready",
        "hermes.not_ready",
        "hermes.reconnect",
    ] {
        assert!(realtime.contains(event), "missing Hermes event {event}");
    }
}

#[test]
fn polls_predictions_and_raids_are_instrumented() {
    let native = include_str!("../src/lib.rs");
    for event in [
        "poll.subscription",
        "poll.vote",
        "prediction.vote",
        "eventsub.sync",
    ] {
        assert!(native.contains(event), "missing poll/raid event {event}");
    }

    let realtime = include_str!("../src/channel_points_realtime.rs");
    for event in [
        "poll.subscription.request",
        "poll.subscription.ack",
        "poll.subscription.fallback",
        "poll.pubsub",
    ] {
        assert!(realtime.contains(event), "missing poll runtime event {event}");
    }

    let eventsub = include_str!("../src/eventsub.rs");
    for event in [
        "eventsub.welcome",
        "eventsub.subscription.created",
        "eventsub.reconnect.complete",
        "raid.received.native",
    ] {
        assert!(eventsub.contains(event), "missing EventSub event {event}");
    }

    let raid = include_str!("../../src/components/RaidBanner.tsx");
    assert!(raid.contains("raid.received"));

    let frontend = include_str!("../../src/lib/streaming/store.ts");
    assert!(frontend.contains("raid.follow"));
}
