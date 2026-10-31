// Dock visibility watchdog that keeps grips and chat in place.

#[cfg(windows)]
fn start_dock_visibility_watchdog() {
    static STARTED: AtomicBool = AtomicBool::new(false);
    if STARTED.swap(true, Ordering::SeqCst) {
        return;
    }
    thread::spawn(|| {
        // 0 = shown/normal, 1 = minimized as a group
        let mut group_minimized = false;
        let mut seam_suppressed = false;
        // Remember last-known member HWNDs. Once minimized, title/area scans
        // often miss borderless mpv (iconic rect is ~160x28), so without a
        // cache the watchdog never observes IsIconic and never syncs.
        let mut cached: Vec<*mut core::ffi::c_void> = Vec::new();
        let mut last_hwnd_count = 0usize;
        let mut last_focus: Option<DockFocusKind> = None;
        let mut last_popup = false;
        let mut sleep_ms = 100u64;
        loop {
            thread::sleep(Duration::from_millis(sleep_ms));
            let cfg = crate::dock::snapshot();
            // Sync whenever the dock is active (linked grips and/or reserved chat).
            let dock_active = !cfg.channels.is_empty() && (cfg.linked || cfg.reserve_chat);
            let dock_active = dock_active || crate::dock::has_independent_placements();
            if !dock_active {
                if group_minimized {
                    group_minimized = false;
                    DOCK_GROUP_MINIMIZED.store(false, Ordering::SeqCst);
                    crate::dock::show_grips();
                }
                cached.clear();
                last_hwnd_count = 0;
                last_focus = None;
                last_popup = false;
                sleep_ms = dock_watchdog_interval_ms(false, false);
                continue;
            }
            let found = dock_member_hwnds(&cfg.channels, cfg.reserve_chat);
            // Drop destroyed windows from the cache; merge with fresh finds.
            cached.retain(|&h| is_hwnd_alive(h));
            for &h in &found {
                if !h.is_null() && !cached.contains(&h) {
                    cached.push(h);
                }
            }
            // Prefer the union so an iconic mpv still participates.
            let hwnds: Vec<_> = if cached.is_empty() {
                found
            } else {
                cached.clone()
            };
            if hwnds.is_empty() {
                sleep_ms = dock_watchdog_interval_ms(true, true);
                continue;
            }
            let any_iconic = hwnds.iter().any(|&h| is_hwnd_iconic(h));
            let any_zoomed = hwnds.iter().any(|&h| is_hwnd_zoomed(h));
            let any_restored = hwnds.iter().any(|&h| is_hwnd_restored(h));
            let focus = if cfg.linked {
                Some(dock_focus_kind(&hwnds))
            } else {
                None
            };
            let has_popup = cfg.linked && cfg.reserve_chat && chatterino_has_overlay_popup();
            let needs_fast = dock_watchdog_needs_fast_tick(
                group_minimized,
                any_iconic,
                any_zoomed,
                focus != last_focus,
                has_popup != last_popup,
                hwnds.len() != last_hwnd_count,
            );
            last_focus = focus;
            last_popup = has_popup;
            last_hwnd_count = hwnds.len();
            sleep_ms = dock_watchdog_interval_ms(true, needs_fast);

            if !group_minimized && any_iconic {
                DOCK_GROUP_MINIMIZED.store(true, Ordering::SeqCst);
                crate::dock::hide_grips();
                minimize_dock_group(&hwnds);
                if let Some(app) = DOCK_APP.get() {
                    restack_all_points_huds(app);
                }
                group_minimized = true;
                sleep_ms = dock_watchdog_interval_ms(true, true);
                continue;
            }
            if group_minimized {
                if any_restored {
                    DOCK_GROUP_MINIMIZED.store(false, Ordering::SeqCst);
                    restore_dock_group(&cfg.channels, cfg.reserve_chat, &cfg.layout);
                    // Clears grip-minimized latch; Sync no-ops grips when unlinked.
                    crate::dock::show_grips();
                    group_minimized = false;
                    seam_suppressed = false;
                    // Refresh cache from live finds after restore/retile.
                    cached = dock_member_hwnds(&cfg.channels, cfg.reserve_chat);
                    sleep_ms = dock_watchdog_interval_ms(true, true);
                    continue;
                }
                // Chatterino (or a new mpv) may appear after the group was
                // minimized — keep every member iconic.
                let stragglers: Vec<_> = hwnds
                    .iter()
                    .copied()
                    .filter(|&h| is_hwnd_alive(h) && !is_hwnd_iconic(h))
                    .collect();
                if !stragglers.is_empty() {
                    minimize_dock_group(&stragglers);
                    sleep_ms = dock_watchdog_interval_ms(true, true);
                }
                continue;
            }
            // Solo maximize breaks the dock — snap everyone back to tiles.
            if any_zoomed {
                restore_dock_group(&cfg.channels, cfg.reserve_chat, &cfg.layout);
                if cfg.linked {
                    crate::dock::show_grips();
                }
            }

            // Keep grey grips in the normal Z-order immediately above the
            // highest mpv/owned Chatterino member. Foreign apps therefore cover
            // the entire dock naturally; no divider enters the TOPMOST band.
            if cfg.linked {
                if let Some(anchor) = topmost_dock_member(&hwnds) {
                    crate::dock::restack_grips_above(anchor as isize);
                }
                // Preserve the existing poll-overlay focus behavior separately.
                if matches!(
                    focus.unwrap_or(DockFocusKind::Unknown),
                    DockFocusKind::DockOrApp | DockFocusKind::Unknown
                ) {
                    raise_poll_overlay();
                }
            }

            // Chatterino usercards/menus sit above the main chat window; seam
            // grips used to slice through them. Hide seam grips while a
            // secondary Chatterino window is visible.
            if cfg.linked && cfg.reserve_chat {
                if has_popup && !seam_suppressed {
                    crate::dock::suppress_seam_grips();
                    seam_suppressed = true;
                } else if !has_popup && seam_suppressed {
                    crate::dock::restore_seam_grips();
                    seam_suppressed = false;
                }
            } else if seam_suppressed {
                crate::dock::restore_seam_grips();
                seam_suppressed = false;
            }

            if cfg.reserve_chat {
                let pid = owned_chatterino_pid()
                    .lock()
                    .ok()
                    .and_then(|g| *g)
                    .filter(|p| pid_is_alive(*p))
                    .or_else(find_rillmux_dock_chatterino_pid)
                    .unwrap_or(0);
                if pid != 0 && pid_is_alive(pid) {
                    if let Ok(mut g) = owned_chatterino_pid().lock() {
                        *g = Some(pid);
                    }
                    if let Some(target) = overlay_rect_from_reserved_chat() {
                        let hwnd = find_main_window_for_pid(pid);
                        let visible = hwnd.and_then(dwm_visible_overlay_rect);
                        if chatterino_watchdog_should_place(
                            hwnd.is_some(),
                            true,
                            visible,
                            target,
                            chatterino_place_slop_px(),
                        ) {
                            place_chatterino_window_right(pid);
                        }
                    }
                } else {
                    let last_set = last_chatterino_channels()
                        .lock()
                        .ok()
                        .is_some_and(|g| !g.is_empty());
                    let elapsed = last_chatterino_watchdog_relaunch()
                        .lock()
                        .ok()
                        .and_then(|g| *g)
                        .map(|t| t.elapsed().as_millis() as u64)
                        .unwrap_or(u64::MAX);
                    if chatterino_watchdog_should_relaunch(false, true, last_set, elapsed, 2_000) {
                        if let Ok(_launch) = chatterino_launch_lock().try_lock() {
                            if let Ok(mut g) = last_chatterino_watchdog_relaunch().lock() {
                                *g = Some(Instant::now());
                            }
                            let list = last_chatterino_channels()
                                .lock()
                                .ok()
                                .map(|g| g.clone())
                                .unwrap_or_default();
                            if !list.is_empty() {
                                if let Some(path) = cached_chatterino_path() {
                                    crate::diagnostics::log_line("[chatterino] watchdog relaunch");
                                    if let Err(err) =
                                        launch_chatterino_with_path(&path, &list, true, true)
                                    {
                                        crate::diagnostics::log_line(&format!(
                                            "[chatterino] watchdog relaunch failed: {err}"
                                        ));
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    });
}

#[cfg(not(windows))]
fn start_dock_visibility_watchdog() {}
