#[test]
fn streaming_is_composed_from_ordered_focused_shards() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    assert!(!root.join("src/streaming.rs").exists());

    let module = include_str!("../src/streaming/mod.rs");
    let expected = [
        "foundation.rs",
        "types_player.rs",
        "tools_process.rs",
        "dock.rs",
        "overlays.rs",
        "windows_layout.rs",
        "runtime.rs",
        "tests.rs",
    ];
    let mut cursor = 0;
    for shard in expected {
        assert!(
            root.join("src/streaming").join(shard).is_file(),
            "missing {shard}"
        );
        let needle = format!("include!(\"{shard}\");");
        let relative = module[cursor..]
            .find(&needle)
            .unwrap_or_else(|| panic!("missing or out-of-order include for {shard}"));
        cursor += relative + needle.len();
    }
}
