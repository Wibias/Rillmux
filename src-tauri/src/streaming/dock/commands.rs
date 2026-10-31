// Dock commands exposed to the Tauri layer.

pub fn dock_set_linked(enabled: bool) {
    crate::dock::set_linked(enabled);
}

pub fn dock_set_chat_fraction(f: f64) {
    crate::dock::set_chat_fraction(f);
}

pub fn dock_cycle_monitor() {
    crate::dock::cycle_monitor();
}
