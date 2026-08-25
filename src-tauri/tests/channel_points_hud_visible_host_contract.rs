#[test]
fn channel_points_hud_rejects_transient_offscreen_player_windows() {
    let overlays = include_str!("../src/streaming/overlays.rs");
    let start = overlays
        .find("pub fn channel_points_hud_host")
        .expect("channel points HUD host function");
    let body = &overlays[start..overlays.len().min(start + 1800)];

    assert!(
        body.contains("IsWindowVisible") || body.contains("is_hwnd_visible"),
        "HUD host must reject hidden/transitional player HWNDs"
    );
    assert!(
        body.contains("MonitorFromWindow") || body.contains("hwnd_has_monitor"),
        "HUD host must reject parked HWNDs that do not intersect a real monitor"
    );
    assert!(
        overlays.contains("MONITOR_DEFAULTTONULL"),
        "monitor membership must use MONITOR_DEFAULTTONULL instead of snapping to a nearby monitor"
    );

    // Negative desktop coordinates are valid for monitors left/above the primary
    // display, so this regression must be solved by monitor membership, not by
    // rejecting negative x/y coordinates.
    assert!(!body.contains("rect.x < 0"));
    assert!(!body.contains("rect.y < 0"));
}
