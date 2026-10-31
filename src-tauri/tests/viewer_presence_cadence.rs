#[path = "support/sources.rs"]
mod sources;

#[test]
fn successful_watch_heartbeat_matches_known_good_miner_cadence() {
    let source = sources::VIEWER_PRESENCE;
    assert!(source.contains("const MIN_SUCCESS_INTERVAL_SECS: u64 = 55;"));
    assert!(source.contains("const MAX_SUCCESS_INTERVAL_SECS: u64 = 70;"));
    assert!(source.contains("fn success_interval() -> Duration"));
    assert!(!source.contains("const SUCCESS_INTERVAL: Duration = Duration::from_secs(20);"));
}
