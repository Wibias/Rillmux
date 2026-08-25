#[test]
fn private_gql_operations_live_in_registry_and_vote_poll_has_no_fake_hash() {
    let channel_points = include_str!("../src/channel_points.rs");
    let lib = include_str!("../src/lib.rs");

    assert!(lib.contains("mod twitch_gql_operations;"));
    assert!(channel_points.contains("twitch_gql_operations"));
    assert!(!channel_points.contains("VOTE_POLL_HASH"));
    assert!(
        !channel_points.contains("6b21d6e5c8c6c8d6f0c0c6e6a0b0d0e0f0a1b2c3d4e5f60718293a4b5c6d7e8")
    );
}
