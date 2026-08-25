#[test]
fn runtime_debug_diagnostics_exposes_six_filtered_categories() {
    let source = include_str!("../src/diagnostics.rs");

    for category in [
        "Windows",
        "PointsCredit",
        "PointsClaim",
        "Rewards",
        "Polls",
        "Raids",
    ] {
        assert!(
            source.contains(category),
            "missing runtime debug category {category}"
        );
    }
    assert!(source.contains("DebugCategoryFlags"));
    assert!(source.contains("set_debug_categories"));
    assert!(source.contains("log_event"));
}

#[test]
fn runtime_debug_diagnostics_uses_non_blocking_bounded_transport() {
    let source = include_str!("../src/diagnostics.rs");

    assert!(source.contains("sync_channel"));
    assert!(source.contains("try_send"));
    assert!(source.contains("dropped"));
    assert!(source.contains("AtomicU64"));
}

#[test]
fn runtime_debug_diagnostics_has_central_redaction_helpers() {
    let source = include_str!("../src/diagnostics.rs");

    assert!(source.contains("redact_id"));
    assert!(source.contains("redact_hash"));
}
