// Watching layout: grouped and independent dock placement.

fn sync_layout_chat_fraction(chat_fraction: Option<f64>) {
    let Some(requested) = chat_fraction else {
        return;
    };
    let requested = crate::dock::clamp_chat_fraction(requested);
    let current = crate::dock::chat_fraction();
    // Ordinary session/layout refreshes always carry the persisted fraction.
    // Re-entering the interactive setter when it is unchanged synchronously
    // runs a full Win32 dock apply against mpv/Chatterino and can block for
    // seconds while Chatterino is restarting for a new multistream channel.
    if (current - requested).abs() >= 0.001 {
        crate::dock::set_chat_fraction(requested);
    }
}

/// Re-tile mpv windows for active channels; optionally leave the right strip for chat.
pub fn layout_watching(
    channels: &[String],
    reserve_chat: bool,
    layout: Option<&str>,
    linked_dock: Option<bool>,
    chat_fraction: Option<f64>,
    main_side: Option<&str>,
) -> Result<(), StreamError> {
    if layout == Some("independent") {
        return layout_independent_streams(channels, chat_fraction);
    }
    crate::dock::clear_independent_placements();
    let cleaned: Vec<String> = channels
        .iter()
        .map(|c| c.trim().trim_start_matches('#').to_lowercase())
        .filter(|c| !c.is_empty())
        .collect();
    if cleaned.is_empty() {
        crate::dock::clear_session();
        return Ok(());
    }
    let layout = normalize_layout(layout);
    sync_layout_chat_fraction(chat_fraction);
    if let Some(side) = main_side {
        crate::dock::set_main_side(side);
    }
    let linked = linked_dock.unwrap_or_else(|| crate::dock::snapshot().linked);
    crate::dock::sync_session(&cleaned, &layout, reserve_chat, linked);
    #[cfg(windows)]
    {
        let cleaned = cleaned.clone();
        // Latest request wins: when several streams become ready at once, the
        // frontend fires layout_watching repeatedly. Older threads exit as
        // soon as they notice a newer generation instead of fighting over
        // window placement.
        let generation = LAYOUT_GENERATION.fetch_add(1, Ordering::SeqCst) + 1;
        let expected = cleaned.len().clamp(1, 8);
        thread::spawn(move || {
            thread::sleep(Duration::from_millis(400));
            let mut streak = 0;
            for _ in 0..28 {
                if LAYOUT_GENERATION.load(Ordering::SeqCst) != generation {
                    return;
                }
                let found = retile_player_windows(&cleaned, reserve_chat, &layout);
                let mut chat_ok = true;
                if reserve_chat {
                    let chat_pid = owned_chatterino_pid()
                        .lock()
                        .ok()
                        .and_then(|g| *g)
                        .unwrap_or(0);
                    chat_ok = chat_pid != 0 && find_main_window_for_pid(chat_pid).is_some();
                    if chat_ok {
                        place_chatterino_window_right(chat_pid);
                    }
                }
                if found >= expected && chat_ok {
                    streak += 1;
                    if streak >= 2 {
                        restack_dock_windows(&cleaned, reserve_chat);
                        return;
                    }
                } else {
                    streak = 0;
                }
                thread::sleep(Duration::from_millis(250));
            }
            restack_dock_windows(&cleaned, reserve_chat);
        });
    }
    #[cfg(not(windows))]
    {
        let _ = (cleaned, reserve_chat, layout);
    }
    Ok(())
}

/// When true, skip retile/place so we don't un-minimize the dock group.
static DOCK_GROUP_MINIMIZED: AtomicBool = AtomicBool::new(false);

fn layout_independent_streams(
    channels: &[String],
    chat_fraction: Option<f64>,
) -> Result<(), StreamError> {
    let cleaned: Vec<String> = channels
        .iter()
        .map(|c| c.trim().trim_start_matches('#').to_lowercase())
        .filter(|c| !c.is_empty())
        .collect();
    sync_layout_chat_fraction(chat_fraction);
    if cleaned.is_empty() {
        crate::dock::clear_session();
        return Ok(());
    }
    crate::dock::sync_independent_session(&cleaned);
    #[cfg(windows)]
    {
        let cleaned = cleaned.clone();
        let generation = LAYOUT_GENERATION.fetch_add(1, Ordering::SeqCst) + 1;
        let expected = cleaned.len().clamp(1, 8);
        thread::spawn(move || {
            thread::sleep(Duration::from_millis(400));
            let mut streak = 0;
            for _ in 0..28 {
                if LAYOUT_GENERATION.load(Ordering::SeqCst) != generation {
                    return;
                }
                let found = place_independent_pairs();
                if found >= expected {
                    streak += 1;
                    if streak >= 2 {
                        restack_dock_windows(&cleaned, false);
                        return;
                    }
                } else {
                    streak = 0;
                }
                thread::sleep(Duration::from_millis(250));
            }
            restack_dock_windows(&cleaned, false);
        });
    }
    Ok(())
}
