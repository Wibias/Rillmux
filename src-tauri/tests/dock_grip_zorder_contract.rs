#[test]
fn regular_dock_grips_never_enter_global_topmost_band() {
    let dock = include_str!("../src/dock.rs");
    let streaming = include_str!("../src/streaming/dock.rs");
    let windows_layout = include_str!("../src/streaming/windows_layout.rs");

    assert!(
        !dock.contains("DockCmd::ElevateGrips"),
        "regular dock grips must never enter the global TOPMOST band"
    );
    assert!(
        !dock.contains("DockCmd::DemoteGrips"),
        "divider stacking must not depend on focus-driven TOPMOST demotion"
    );
    assert!(
        !dock.contains("GRIPS_ELEVATED"),
        "regular grip placement must not carry global TOPMOST state"
    );
    assert!(
        !dock.contains("HWND_TOPMOST as *mut core::ffi::c_void"),
        "regular grips must never be promoted with HWND_TOPMOST"
    );
    assert!(!dock.contains("elevate_grips_inner()"));
    assert!(!dock.contains("pub fn raise_grips()"));
    assert!(!dock.contains("pub fn demote_grips()"));
    assert!(dock.contains("RestackGrips(isize)"));
    assert!(dock.contains("restack_grips_inner(anchor: isize)"));
    assert!(dock.contains("GetWindow(anchor, GW_HWNDPREV)"));
    assert!(streaming.contains("topmost_dock_member(&hwnds)"));
    assert!(streaming.contains("crate::dock::restack_grips_above(anchor as isize)"));
    assert!(!windows_layout.contains("crate::dock::raise_grips()"));
    assert!(windows_layout.contains("topmost_dock_member(&members)"));
    assert!(windows_layout.contains("crate::dock::restack_grips_above(anchor as isize)"));
}

#[test]
fn monitor_identify_overlay_remains_the_only_permanent_topmost_grip() {
    let dock = include_str!("../src/dock.rs");
    assert!(dock.contains("if is_identify { WS_EX_TOPMOST } else { 0 }"));
}
