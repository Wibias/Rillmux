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

#[test]
fn win32_version_info_pointer_is_validated_before_dereference() {
    let source = include_str!("../src/streaming/windows_layout.rs");
    let query = source
        .find("VerQueryValueW")
        .expect("missing Win32 version-info query");
    let dereference = source[query..]
        .find("let info = &*info_ptr;")
        .map(|offset| query + offset)
        .expect("missing validated VS_FIXEDFILEINFO dereference");
    let guard = &source[query..dereference];

    assert!(guard.contains("checked_add(buf.len())?"));
    assert!(guard.contains("checked_add(std::mem::size_of::<VsFixedFileInfo>())?"));
    assert!(guard.contains("info_addr < buf_start"));
    assert!(guard.contains("info_end > buf_end"));
    assert!(guard.contains("is_multiple_of(std::mem::align_of::<VsFixedFileInfo>())"));
}
