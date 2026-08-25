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

#[test]
fn channel_points_context_prefers_current_known_good_queries() {
    let operations = include_str!("../src/twitch_gql_operations.rs");
    let rewards_context = "1530a003a7d374b0380b79db0be0534f30ff46e61cffa2bc0e2468a909fbc024";
    let miner_context = "9988086babc615a918a1e9a722ff41d98847acac822645209ac7379eecb27152";
    let legacy_context = "7fe050e3761eb2cf258d70ee1a21cbd76fa8cf3d7e7b12fc437e7029d446b5e3";
    let alternate_legacy_context =
        "374314de591e69925fce3ddc2bcf085796f56ebb8cad67a0daa3165c03adc345";

    let rewards_index = operations
        .find(rewards_context)
        .expect("reward-capable ChannelPointsContext hash must be registered");
    let miner_index = operations
        .find(miner_context)
        .expect("current ChannelPointsContext fallback must be registered");
    let legacy_index = operations
        .find(legacy_context)
        .expect("legacy ChannelPointsContext fallback must stay registered");
    let alternate_legacy_index = operations
        .find(alternate_legacy_context)
        .expect("alternate legacy ChannelPointsContext fallback must stay registered");

    assert!(rewards_index < miner_index);
    assert!(miner_index < legacy_index);
    assert!(legacy_index < alternate_legacy_index);
}
