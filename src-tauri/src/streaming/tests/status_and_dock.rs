// Streamlink status classification and dock watchdog behavior.

#[test]
fn hud_stacks_just_above_the_player_not_the_desktop() {
    // 0 = HWND_TOP (player is already front-most among peers).
    assert_eq!(hud_z_insert_after(10, 0), Some(0));
    // Already immediately above the player: do not raise over other apps.
    assert_eq!(hud_z_insert_after(10, 10), None);
    assert_eq!(hud_z_insert_after(10, 20), Some(20));
}

#[test]
fn hud_stays_owned_so_the_chip_does_not_vanish() {
    assert!(!hud_needs_detach_owner(1));
    assert!(!hud_needs_detach_owner(0));
}

#[test]
fn hud_never_reowns_to_mpv() {
    assert!(!hud_needs_reown(1, 42));
    assert!(!hud_needs_reown(42, 42));
    assert!(!hud_needs_reown(1, 0));
}

#[test]
fn chatterino_retries_outlast_qt_screen_restore() {
    let ms = chatterino_place_retry_ms();
    assert!(ms.last().copied().unwrap_or(0) >= 3000);
    assert!(ms.len() >= 6);
}

#[test]
fn chatterino_place_redoes_when_qt_snaps_back() {
    let target = OverlayRect {
        x: 1600,
        y: 0,
        width: 320,
        height: 1080,
    };
    let old_monitor = OverlayRect {
        x: 0,
        y: 0,
        width: 320,
        height: 1080,
    };
    assert!(overlay_rect_drifted(
        old_monitor,
        target,
        chatterino_place_slop_px()
    ));
    assert!(!overlay_rect_drifted(
        target,
        target,
        chatterino_place_slop_px()
    ));
}

#[test]
fn chatterino_27px_left_gap_counts_as_drift() {
    let target = OverlayRect {
        x: 1540,
        y: 0,
        width: 380,
        height: 1032,
    };
    let gapped = OverlayRect {
        x: 1567,
        y: 0,
        width: 380,
        height: 1032,
    };
    assert!(overlay_rect_drifted(
        gapped,
        target,
        chatterino_place_slop_px()
    ));
    let aligned = OverlayRect {
        x: 1547,
        y: 0,
        width: 380,
        height: 1032,
    };
    assert!(!overlay_rect_drifted(
        aligned,
        target,
        chatterino_place_slop_px()
    ));
}

#[test]
fn chatterino_retries_when_hwnd_vanishes_but_dock_still_runs() {
    assert!(chatterino_hwnd_lost_needs_retry(false, true));
    assert!(!chatterino_hwnd_lost_needs_retry(true, true));
    assert!(!chatterino_hwnd_lost_needs_retry(false, false));
}

#[test]
fn chatterino_watchdog_places_when_hwnd_is_gone() {
    let target = OverlayRect {
        x: 1540,
        y: 0,
        width: 380,
        height: 1032,
    };
    assert!(chatterino_watchdog_should_place(
        false,
        true,
        None,
        target,
        chatterino_place_slop_px()
    ));
    assert!(!chatterino_watchdog_should_place(
        true,
        true,
        Some(target),
        target,
        chatterino_place_slop_px()
    ));
}

#[test]
fn channel_points_hud_hides_missing_iconic_or_tiny_players() {
    assert_eq!(channel_points_hud_player_rect(None, false), None);
    assert_eq!(
        channel_points_hud_player_rect(
            Some(OverlayRect {
                x: 0,
                y: 0,
                width: 800,
                height: 450,
            }),
            true,
        ),
        None
    );
    assert_eq!(
        channel_points_hud_player_rect(
            Some(OverlayRect {
                x: 0,
                y: 0,
                width: 199,
                height: 120,
            }),
            false,
        ),
        None
    );
    assert_eq!(
        channel_points_hud_player_rect(
            Some(OverlayRect {
                x: 10,
                y: 20,
                width: 800,
                height: 450,
            }),
            false,
        ),
        Some(OverlayRect {
            x: 10,
            y: 20,
            width: 800,
            height: 450,
        })
    );
    let overlays = include_str!("../overlays/hud.rs");
    assert!(
        overlays.contains("if is_hwnd_iconic(player)"),
        "HUD restack must hide the chip when the player is minimized"
    );
    assert!(
        overlays.contains("hidden: true"),
        "minimized players must be reported as hidden so the frontend skips the miss grace"
    );
    let dock = include_str!("../dock/watchdog.rs");
    assert!(
        dock.contains("restack_all_points_huds(app)"),
        "minimizing a stream must restack/hide Channel Points HUDs with the dock group"
    );
}

#[test]
fn caption_avoid_matches_scaled_window_controls() {
    let main = OverlayRect {
        x: 100,
        y: 50,
        width: 1280,
        height: 800,
    };
    assert_eq!(
        caption_avoid_from_main(main, 1.0),
        OverlayRect {
            x: 100 + 1280 - 138,
            y: 50,
            width: 138,
            height: 38,
        }
    );
    assert_eq!(
        caption_avoid_from_main(main, 1.5),
        OverlayRect {
            x: 100 + 1280 - 207,
            y: 50,
            width: 207,
            height: 57,
        }
    );
}

#[test]
fn player_caption_avoid_sits_on_the_stream_tile() {
    let player = OverlayRect {
        x: 0,
        y: 38,
        width: 1000,
        height: 800,
    };
    let avoid = caption_avoid_from_main(player, 1.0);
    assert_eq!(avoid.y, 38);
    assert_eq!(avoid.x, 1000 - 138);
    assert!(overlay_rects_overlap(avoid, player));
}

#[test]
fn union_overlay_covers_plugin_and_dwm_caption_buttons() {
    let plugin = OverlayRect {
        x: 1439,
        y: 75,
        width: 138,
        height: 42,
    };
    let dwm = OverlayRect {
        x: 1432,
        y: 74,
        width: 146,
        height: 30,
    };
    assert_eq!(
        union_overlay_rect(plugin, dwm),
        OverlayRect {
            x: 1432,
            y: 74,
            width: 146,
            height: 43,
        }
    );
}

#[test]
fn overlay_rects_overlap_detects_caption_coverage() {
    let caption = OverlayRect {
        x: 1142,
        y: 50,
        width: 138,
        height: 42,
    };
    let covering = OverlayRect {
        x: 1100,
        y: 40,
        width: 200,
        height: 80,
    };
    let below = OverlayRect {
        x: 1100,
        y: 50 + 42 + 16,
        width: 120,
        height: 36,
    };
    assert!(overlay_rects_overlap(covering, caption));
    assert!(!overlay_rects_overlap(below, caption));
}

#[test]
fn hud_overlay_moves_when_the_player_jumps_monitors() {
    let old_chip = OverlayRect {
        x: 1199,
        y: 8,
        width: 120,
        height: 36,
    };
    let new_chip = OverlayRect {
        x: 1199,
        y: -1072,
        width: 120,
        height: 36,
    };
    let caption = OverlayRect {
        x: 1782,
        y: 0,
        width: 138,
        height: 38,
    };
    assert!(hud_overlay_should_apply(
        false,
        Some(old_chip),
        new_chip,
        Some(caption),
        12
    ));
    assert!(!hud_overlay_should_apply(
        false,
        Some(new_chip),
        new_chip,
        Some(caption),
        12
    ));
    assert!(hud_overlay_should_apply(
        true,
        Some(new_chip),
        new_chip,
        Some(caption),
        12
    ));
}

#[test]
fn chatterino_watchdog_relaunches_after_the_owned_process_dies() {
    assert!(chatterino_watchdog_should_relaunch(
        false, true, true, 3_000, 2_000
    ));
    assert!(!chatterino_watchdog_should_relaunch(
        true, true, true, 3_000, 2_000
    ));
    assert!(!chatterino_watchdog_should_relaunch(
        false, true, true, 500, 2_000
    ));
    assert!(!chatterino_watchdog_should_relaunch(
        false, true, false, 3_000, 2_000
    ));
    assert!(!chatterino_watchdog_should_relaunch(
        false, false, true, 3_000, 2_000
    ));
}

#[test]
fn chatterino_pid_after_child_exit_adopts_a_surviving_dock_pid() {
    assert_eq!(
        chatterino_pid_after_child_exit(Some(10), 10, &[10, 22]),
        Some(22)
    );
    assert_eq!(chatterino_pid_after_child_exit(Some(10), 10, &[10]), None);
    assert_eq!(chatterino_pid_after_child_exit(Some(7), 10, &[22]), Some(7));
    assert_eq!(chatterino_pid_after_child_exit(None, 10, &[22]), None);
}

#[test]
fn chatterino_close_targets_include_owned_and_other_dock_pids() {
    assert_eq!(chatterino_pids_to_close(Some(10), &[10, 22]), vec![10, 22]);
    assert_eq!(chatterino_pids_to_close(None, &[22]), vec![22]);
    assert_eq!(chatterino_pids_to_close(Some(10), &[]), vec![10]);
    assert!(chatterino_pids_to_close(None, &[]).is_empty());
}

#[test]
fn chatterino_spawn_after_close_is_stale() {
    assert!(chatterino_spawn_is_stale(1, 2));
    assert!(!chatterino_spawn_is_stale(3, 3));
}

#[test]
fn chatterino_picks_a_discovered_dock_pid_when_owned_died() {
    assert_eq!(
        chatterino_pick_owned_pid(Some(10), false, Some(22)),
        Some(22)
    );
    assert_eq!(
        chatterino_pick_owned_pid(Some(10), true, Some(22)),
        Some(10)
    );
    assert_eq!(chatterino_pick_owned_pid(None, false, Some(22)), Some(22));
    assert_eq!(chatterino_pick_owned_pid(None, false, None), None);
}

#[test]
fn chatterino_does_not_wm_close_qt_helper_windows() {
    assert!(
        !chatterino_should_close_duplicate_main(false, false, 80_000, "Chatterino", true),
        "hidden Qt helpers must not receive WM_CLOSE"
    );
    assert!(
        !chatterino_should_close_duplicate_main(false, true, 200, "Chatterino", true),
        "tiny surfaces must not receive WM_CLOSE"
    );
    assert!(
        !chatterino_should_close_duplicate_main(false, true, 80_000, "Chatterino", false),
        "do not close extras until the --channels split exists"
    );
    assert!(
        !chatterino_should_close_duplicate_main(true, true, 80_000, "eliasn97 - Chatterino", true),
        "keep hwnd must stay"
    );
    assert!(chatterino_should_close_duplicate_main(
        false,
        true,
        80_000,
        "Chatterino",
        true
    ));
}

#[test]
#[cfg(windows)]
fn dock_chatterino_spawn_stays_alive_and_opens_a_window() {
    let Some(path) = find_chatterino_path() else {
        return;
    };
    for pid in list_rillmux_dock_chatterino_pids() {
        terminate_pid(pid);
    }
    thread::sleep(Duration::from_millis(250));
    launch_chatterino_with_path(&path, "t:eliasn97", false, true)
        .expect("dock Chatterino must spawn and stay running");
    let pid = owned_chatterino_pid()
        .lock()
        .ok()
        .and_then(|g| *g)
        .expect("owned dock pid must be tracked");
    let started = Instant::now();
    let mut hwnd = None;
    while started.elapsed() < Duration::from_secs(8) {
        let alive = pid_is_alive(pid) || find_rillmux_dock_chatterino_pid().is_some();
        assert!(
            alive,
            "dock Chatterino pid={pid} died before a window appeared"
        );
        hwnd = find_main_window_for_pid(pid)
            .or_else(|| find_rillmux_dock_chatterino_pid().and_then(find_main_window_for_pid));
        if hwnd.is_some() {
            break;
        }
        thread::sleep(Duration::from_millis(100));
    }
    assert!(
        hwnd.is_some(),
        "dock Chatterino pid={pid} must create a window"
    );
    thread::sleep(Duration::from_secs(3));
    assert!(
        pid_is_alive(pid) || find_rillmux_dock_chatterino_pid().is_some(),
        "dock Chatterino pid={pid} died within 3s (watchdog-loop failure)"
    );
    eprintln!(
        "PROOF dock Chatterino alive pid={pid} hwnd={hwnd:?} after {:?}",
        started.elapsed()
    );
    close_owned_chatterino_wait(Duration::from_secs(2));
    for leftover in list_rillmux_dock_chatterino_pids() {
        terminate_pid(leftover);
    }
}

#[test]
fn raid_overlay_uses_player_inset_and_clamps_narrow_hosts() {
    let wide = overlay_rect_from_host(OverlayRect {
        x: 10,
        y: 20,
        width: 800,
        height: 450,
    });
    assert_eq!(wide.x, 26);
    assert_eq!(wide.y, 36);
    assert_eq!(wide.width, 420);
    assert_eq!(wide.height, 92);

    let narrow = overlay_rect_from_host(OverlayRect {
        x: 810,
        y: 20,
        width: 300,
        height: 450,
    });
    assert_eq!(narrow.width, 268);
}
