// Tauri commands for diagnostics and EventSub.

#[tauri::command]
fn diagnostics_set_debug(enabled: bool) {
    diagnostics::set_debug_enabled(enabled);
}

#[tauri::command]
fn diagnostics_set_debug_categories(categories: diagnostics::DebugCategoryFlags) {
    diagnostics::set_debug_categories(categories);
}

#[tauri::command]
fn diagnostics_log_event(category: String, event: String, fields: Option<String>) {
    let event = event.trim();
    if event.is_empty() || event.len() > 80 {
        return;
    }
    let fields = fields.unwrap_or_default();
    if fields.len() > 2048 {
        return;
    }
    if let Some(category) = diagnostics::DebugCategory::parse(&category) {
        diagnostics::log_event(category, event, &fields);
    }
}

#[tauri::command]
fn diagnostics_set_sentry_enabled(enabled: bool) {
    set_native_sentry_enabled(enabled);
}

#[tauri::command]
fn diagnostics_open_logs() -> Result<(), String> {
    diagnostics::ensure_dirs();
    open_folder(&diagnostics::logs_dir())
}

#[tauri::command]
fn diagnostics_open_crashes() -> Result<(), String> {
    diagnostics::ensure_dirs();
    open_folder(&diagnostics::crashes_dir())
}

fn open_folder(path: &std::path::Path) -> Result<(), String> {
    #[cfg(windows)]
    {
        std::process::Command::new("explorer")
            .arg(path)
            .spawn()
            .map_err(|e| e.to_string())?;
        Ok(())
    }
    #[cfg(not(windows))]
    {
        let _ = path;
        Err("opening the logs folder is only supported on Windows".into())
    }
}

#[tauri::command]
fn eventsub_sync(enabled: bool, channels: Vec<String>) -> Result<(), String> {
    diagnostics::log_event(
        diagnostics::DebugCategory::Raids,
        "eventsub.sync",
        &format!("enabled={enabled} channels={}", channels.join(",")),
    );
    eventsub::sync(enabled, channels);
    Ok(())
}

#[tauri::command]
fn eventsub_status() -> Result<eventsub::EventSubStatus, String> {
    eventsub::status()
}
