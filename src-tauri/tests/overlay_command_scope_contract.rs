#[test]
fn overlay_self_placement_cannot_target_an_arbitrary_label() {
    let lib = include_str!("../src/lib.rs");
    let start = lib.find("fn overlay_place_hud").expect("overlay command");
    let body = &lib[start..lib.find("fn poll_overlay_place").expect("next command")];
    assert!(body.contains("window: tauri::WebviewWindow"));
    assert!(!body.contains("label: String"));
    assert!(body.contains("window.label()"));
}

#[test]
fn main_hud_placement_derives_the_window_label_from_channel() {
    let lib = include_str!("../src/lib.rs");
    assert!(lib.contains("fn points_hud_place_window"));
    assert!(lib.contains("format!(\"points-hud-{}\""));
}
