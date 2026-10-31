// Apply callbacks, dock init, and fraction emission.

#[cfg(windows)]
fn apply_dock_layout_inner(raise_after_apply: bool) {
    if DOCK_GROUP_MINIMIZED.load(Ordering::SeqCst) {
        return;
    }
    if crate::dock::has_independent_placements() {
        let _ = place_independent_pairs();
        if raise_after_apply {
            raise_dock_windows(&crate::dock::independent_channels(), false);
        }
        return;
    }
    let cfg = crate::dock::snapshot();
    if cfg.channels.is_empty() {
        return;
    }
    let _ = retile_player_windows(&cfg.channels, cfg.reserve_chat, &cfg.layout);
    if cfg.reserve_chat {
        place_chatterino_window_right(0);
        schedule_chatterino_place();
    }
    if raise_after_apply {
        raise_dock_windows(&cfg.channels, cfg.reserve_chat);
    }
}

fn apply_dock_layout_cb() {
    #[cfg(windows)]
    {
        // A monitor-picker click runs this callback from the grip window-proc
        // thread. Cross-process mpv/Chatterino placement can block, so dispatch
        // monitor changes to a worker and let the grip thread process its queued
        // Sync immediately. Normal divider drags remain synchronous.
        if crate::dock::take_raise_after_apply() {
            thread::spawn(|| apply_dock_layout_inner(true));
            return;
        }
        apply_dock_layout_inner(false);
    }
}

static CHATTERINO_PLACE_GEN: AtomicU64 = AtomicU64::new(0);

#[cfg(windows)]
fn schedule_chatterino_place() {
    let gen = CHATTERINO_PLACE_GEN.fetch_add(1, Ordering::SeqCst) + 1;
    thread::spawn(move || {
        let mut elapsed = 0u64;
        for &at in chatterino_place_retry_ms() {
            if CHATTERINO_PLACE_GEN.load(Ordering::SeqCst) != gen {
                return;
            }
            if at > elapsed {
                thread::sleep(Duration::from_millis(at - elapsed));
                elapsed = at;
            }
            if CHATTERINO_PLACE_GEN.load(Ordering::SeqCst) != gen {
                return;
            }
            let cfg = crate::dock::snapshot();
            if !cfg.reserve_chat || cfg.channels.is_empty() {
                return;
            }
            place_chatterino_window_right(0);
        }
    });
}

static DOCK_APP: OnceLock<AppHandle> = OnceLock::new();

fn emit_dock_fraction(f: f64) {
    if let Some(app) = DOCK_APP.get() {
        let _ = app.emit("dock-chat-fraction", f);
    }
}

/// Register dock callbacks once the Tauri app handle exists.
pub fn init_dock(app: AppHandle) {
    let _ = DOCK_APP.set(app);
    crate::dock::register_apply_layout(apply_dock_layout_cb);
    crate::dock::register_fraction_emit(emit_dock_fraction);
    // Starts Win32 grip thread + global Ctrl+Shift+M (works while mpv focused).
    crate::dock::start_background();
    start_dock_visibility_watchdog();
}
