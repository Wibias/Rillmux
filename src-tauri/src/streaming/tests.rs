#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chatterino_reuses_a_live_instance_for_the_same_channels() {
        assert!(chatterino_should_reuse(true, "t:forsen", "t:forsen"));
        assert!(!chatterino_should_reuse(false, "t:forsen", "t:forsen"));
        assert!(!chatterino_should_reuse(true, "", "t:forsen"));
        assert!(!chatterino_should_reuse(true, "t:forsen", "t:forsen;t:xqc"));
        assert_eq!(
            chatterino_launch_plan(true, "t:forsen", "t:forsen"),
            ChatterinoLaunchPlan::Reuse
        );
        assert_eq!(
            chatterino_launch_plan(true, "t:forsen", "t:forsen;t:xqc"),
            ChatterinoLaunchPlan::RestartOwned
        );
        // A Chatterino window the user already had open is not "owned".
        assert_eq!(
            chatterino_launch_plan(false, "", "t:forsen"),
            ChatterinoLaunchPlan::SpawnFresh
        );
        assert_eq!(
            chatterino_launch_plan(true, "", "t:forsen"),
            ChatterinoLaunchPlan::RestartOwned
        );
    }

    #[test]
    fn chatterino_dock_appdata_is_not_the_user_chatterino_folder() {
        let dock = chatterino_dock_appdata();
        let name = dock.file_name().and_then(|s| s.to_str()).unwrap_or("");
        assert_eq!(name, "chatterino-dock");
        assert!(!dock.ends_with("Chatterino2"));
    }

    #[test]
    fn chatterino_picks_the_channels_split_over_a_blank_notebook() {
        let split = chatterino_window_pick_key(true, true, false, 80_000);
        let blank = chatterino_window_pick_key(false, true, false, 400_000);
        let cloaked = chatterino_window_pick_key(false, false, false, 400_000);
        assert!(split > blank);
        assert!(blank > cloaked);
        assert!(chatterino_title_matches_channels(
            "forsen - Chatterino",
            "t:forsen"
        ));
        assert!(!chatterino_title_matches_channels("Chatterino", "t:forsen"));
    }

    #[test]
    fn command_line_from_nt_buffer_reads_payload_after_header_when_pointer_is_foreign() {
        let text = "chatterino.exe --channels=t:forsen";
        let wide: Vec<u16> = text.encode_utf16().collect();
        let length = (wide.len() * 2) as u16;
        let mut buf = vec![0u8; 16 + wide.len() * 2];
        buf[0..2].copy_from_slice(&length.to_le_bytes());
        buf[2..4].copy_from_slice(&length.to_le_bytes());
        // Fake remote pointer — parser must use the packed payload at offset 16.
        buf[8..16].copy_from_slice(&0x7FFF_0000_1234_5678u64.to_le_bytes());
        for (i, unit) in wide.iter().enumerate() {
            let off = 16 + i * 2;
            buf[off..off + 2].copy_from_slice(&unit.to_le_bytes());
        }
        assert_eq!(command_line_from_nt_buffer(&buf).as_deref(), Some(text));
    }

    #[test]
    fn command_line_from_nt_buffer_reads_payload_when_pointer_is_inside_buffer() {
        let text = "chatterino.exe";
        let wide: Vec<u16> = text.encode_utf16().collect();
        let length = (wide.len() * 2) as u16;
        let mut buf = vec![0u8; 16 + wide.len() * 2];
        buf[0..2].copy_from_slice(&length.to_le_bytes());
        buf[2..4].copy_from_slice(&length.to_le_bytes());
        for (i, unit) in wide.iter().enumerate() {
            let off = 16 + i * 2;
            buf[off..off + 2].copy_from_slice(&unit.to_le_bytes());
        }
        let ptr = buf.as_ptr() as u64 + 16;
        buf[8..16].copy_from_slice(&ptr.to_le_bytes());
        assert_eq!(command_line_from_nt_buffer(&buf).as_deref(), Some(text));
    }

    #[test]
    fn opening_stream_is_starting_not_ready() {
        // Regression: "Opening stream" was treated as ready, which started
        // the layout/handoff/missing-window timers before the player existed.
        let (phase, ready) = classify_line("[cli][info] Opening stream: source (hls)");
        assert_eq!(phase, "starting");
        assert!(!ready);
    }

    #[test]
    fn starting_player_marks_ready() {
        let (phase, ready) =
            classify_line("[cli][info] Starting player: C:\\Program Files\\mpv\\mpv.exe");
        assert_eq!(phase, "ready");
        assert!(ready);
    }

    #[test]
    fn low_latency_line_is_info_not_ready() {
        let (phase, ready) = classify_line("[cli][info] Low latency streaming (HLS live edge: 2)");
        assert_eq!(phase, "info");
        assert!(!ready);
    }

    #[test]
    fn hls_reload_error_is_not_fatal() {
        let (phase, ready) =
            classify_line("[stream.hls][error] Failed to reload playlist: Unable to open URL");
        assert_eq!(phase, "info");
        assert!(!ready);
    }

    #[test]
    fn cli_error_is_fatal() {
        let (phase, ready) = classify_line("[cli][error] Failed to start player: mpv");
        assert_eq!(phase, "error");
        assert!(!ready);
    }

    #[test]
    fn error_prefix_is_fatal() {
        let (phase, ready) =
            classify_line("error: No playable streams found on this URL: twitch.tv/foo");
        assert_eq!(phase, "error");
        assert!(!ready);
    }

    #[test]
    fn ready_session_drops_hls_noise() {
        assert!(!should_forward_status(true, "info", false));
        assert!(!should_forward_status(true, "ready", true));
        assert!(should_forward_status(true, "ended", false));
        assert!(should_forward_status(true, "error", false));
        assert!(should_forward_status(false, "info", false));
        assert!(should_forward_status(false, "ready", true));
    }

    #[test]
    fn channel_and_quality_validation() {
        // mpv_window_title strips anything outside [a-z0-9_-].
        assert_eq!(mpv_window_title("Some_Channel-1"), "rillmux-some_channel-1");
        assert_eq!(mpv_window_title("äöü"), "rillmux-stream");
        assert_eq!(
            legacy_mpv_window_title("Some_Channel-1"),
            "stgui-some_channel-1"
        );
    }

    #[test]
    fn dock_watchdog_idles_when_focus_and_layout_are_stable() {
        assert_eq!(dock_watchdog_interval_ms(false, false), 500);
        assert_eq!(dock_watchdog_interval_ms(true, true), 100);
        assert_eq!(dock_watchdog_interval_ms(true, false), 400);
        assert!(dock_watchdog_needs_fast_tick(
            true, false, false, false, false, false
        ));
        assert!(dock_watchdog_needs_fast_tick(
            false, false, true, false, false, false
        ));
        assert!(!dock_watchdog_needs_fast_tick(
            false, false, false, false, false, false
        ));
    }

    #[test]
    fn session_watchdog_skips_title_scans_when_mpv_process_is_owned() {
        assert!(!session_title_scan_needed(true));
        assert!(session_title_scan_needed(false));
        assert_eq!(session_watchdog_timeout_ms(0), 2500);
        assert_eq!(session_watchdog_timeout_ms(3), 1500);
    }

    #[test]
    fn points_hud_channel_from_overlay_label() {
        assert_eq!(
            points_hud_channel_from_label("points-hud-forsen"),
            Some("forsen")
        );
        assert_eq!(points_hud_channel_from_label("raid-overlay"), None);
        assert_eq!(points_hud_channel_from_label("points-hud-"), None);
    }

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
            caption_avoid_from_main(main.clone(), 1.0),
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
        assert!(overlay_rects_overlap(covering, caption.clone()));
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
            !chatterino_should_close_duplicate_main(
                true,
                true,
                80_000,
                "eliasn97 - Chatterino",
                true
            ),
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

    /// Minimized windows report a tiny GetWindowRect (~160x28). The dock
    /// minimize-sync watchdog must still resolve them by title.
    #[test]
    #[cfg(windows)]
    fn find_window_by_title_keeps_iconic_hwnd() {
        #[link(name = "user32")]
        unsafe extern "system" {
            fn RegisterClassExW(c: *const WndClassEx) -> u16;
            fn CreateWindowExW(
                ex: u32,
                class: *const u16,
                name: *const u16,
                style: u32,
                x: i32,
                y: i32,
                w: i32,
                h: i32,
                parent: *mut core::ffi::c_void,
                menu: *mut core::ffi::c_void,
                instance: *mut core::ffi::c_void,
                param: *mut core::ffi::c_void,
            ) -> *mut core::ffi::c_void;
            fn DestroyWindow(hwnd: *mut core::ffi::c_void) -> i32;
            fn ShowWindow(hwnd: *mut core::ffi::c_void, cmd: i32) -> i32;
            fn DefWindowProcW(hwnd: *mut core::ffi::c_void, msg: u32, w: usize, l: isize) -> isize;
            fn GetModuleHandleW(name: *const u16) -> *mut core::ffi::c_void;
        }
        #[repr(C)]
        struct WndClassEx {
            size: u32,
            style: u32,
            wnd_proc: Option<
                unsafe extern "system" fn(*mut core::ffi::c_void, u32, usize, isize) -> isize,
            >,
            cls_extra: i32,
            wnd_extra: i32,
            instance: *mut core::ffi::c_void,
            icon: *mut core::ffi::c_void,
            cursor: *mut core::ffi::c_void,
            background: *mut core::ffi::c_void,
            menu_name: *const u16,
            class_name: *const u16,
            icon_sm: *mut core::ffi::c_void,
        }
        unsafe extern "system" fn wnd_proc(
            hwnd: *mut core::ffi::c_void,
            msg: u32,
            w: usize,
            l: isize,
        ) -> isize {
            DefWindowProcW(hwnd, msg, w, l)
        }
        fn wide(s: &str) -> Vec<u16> {
            s.encode_utf16().chain(std::iter::once(0)).collect()
        }

        let class = wide("StguiIconicFindTest");
        let title = wide("rillmux-iconicfindtest");
        let instance = unsafe { GetModuleHandleW(std::ptr::null()) };
        let wc = WndClassEx {
            size: std::mem::size_of::<WndClassEx>() as u32,
            style: 0,
            wnd_proc: Some(wnd_proc),
            cls_extra: 0,
            wnd_extra: 0,
            instance,
            icon: std::ptr::null_mut(),
            cursor: std::ptr::null_mut(),
            background: std::ptr::null_mut(),
            menu_name: std::ptr::null(),
            class_name: class.as_ptr(),
            icon_sm: std::ptr::null_mut(),
        };
        unsafe {
            RegisterClassExW(&wc);
        }
        // WS_OVERLAPPEDWINDOW | WS_VISIBLE
        const STYLE: u32 = 0x00CF_0000 | 0x1000_0000;
        let hwnd = unsafe {
            CreateWindowExW(
                0,
                class.as_ptr(),
                title.as_ptr(),
                STYLE,
                100,
                100,
                640,
                480,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                instance,
                std::ptr::null_mut(),
            )
        };
        assert!(!hwnd.is_null(), "CreateWindowExW failed");
        #[link(name = "user32")]
        unsafe extern "system" {
            fn PeekMessageW(
                msg: *mut Msg,
                hwnd: *mut core::ffi::c_void,
                min: u32,
                max: u32,
                remove: u32,
            ) -> i32;
            fn TranslateMessage(msg: *const Msg) -> i32;
            fn DispatchMessageW(msg: *const Msg) -> isize;
        }
        #[repr(C)]
        struct Point {
            x: i32,
            y: i32,
        }
        #[repr(C)]
        struct Msg {
            hwnd: *mut core::ffi::c_void,
            message: u32,
            wparam: usize,
            lparam: isize,
            time: u32,
            pt: Point,
        }
        let pump = || unsafe {
            let mut msg = std::mem::zeroed::<Msg>();
            while PeekMessageW(&mut msg, std::ptr::null_mut(), 0, 0, 1) != 0 {
                TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }
        };
        unsafe {
            let _ = ShowWindow(hwnd, 5); // SW_SHOW
        }
        pump();
        assert!(
            find_window_by_title("rillmux-iconicfindtest", true).is_some(),
            "should find restored test window"
        );
        unsafe {
            let _ = ShowWindow(hwnd, 6); // SW_MINIMIZE
        }
        pump();
        std::thread::sleep(Duration::from_millis(100));
        pump();
        assert!(
            is_hwnd_iconic(hwnd),
            "test window should be iconic after minimize"
        );
        let found = find_window_by_title("rillmux-iconicfindtest", true);
        assert!(
            found.is_some(),
            "must still find iconic window (minimize-sync regression)"
        );
        assert_eq!(found.unwrap(), hwnd);
        unsafe {
            let _ = DestroyWindow(hwnd);
        }
        pump();
    }

    #[test]
    #[ignore = "diagnostic: needs a live mpv with IPC pipe (RILLMUX_PROBE_PIPE)"]
    #[cfg(windows)]
    fn probe_mpv_ipc() {
        let pipe = std::env::var("RILLMUX_PROBE_PIPE").expect("RILLMUX_PROBE_PIPE not set");
        let result = mpv_ipc_command(
            &pipe,
            &["get_property", "mpv-version"],
            Duration::from_secs(3),
        );
        println!("EVID ipc get_property: {:?}", result.is_ok());
        assert!(result.is_ok(), "mpv IPC command failed: {result:?}");
    }

    #[test]
    #[ignore = "diagnostic: needs a live mpv probe window (RILLMUX_PROBE_CHANNEL); moves windows"]
    #[cfg(windows)]
    fn probe_layout_evidence() {
        #[link(name = "user32")]
        unsafe extern "system" {
            fn GetWindowRect(hwnd: *mut core::ffi::c_void, rect: *mut WinRect) -> i32;
        }
        fn rect_of(hwnd: *mut core::ffi::c_void) -> Option<WinRect> {
            let mut r = WinRect {
                left: 0,
                top: 0,
                right: 0,
                bottom: 0,
            };
            (unsafe { GetWindowRect(hwnd, &mut r) } != 0).then_some(r)
        }

        let channels: Vec<String> = std::env::var("RILLMUX_PROBE_CHANNEL")
            .unwrap_or_else(|_| "probe".into())
            .split(',')
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect();
        let layout = std::env::var("RILLMUX_PROBE_LAYOUT").unwrap_or_else(|_| "2x2".into());
        let (video, chat) = chat_video_split(true).expect("chat_video_split");
        println!("EVID video area: {video:?}");
        println!("EVID chat area:  {chat:?}");
        println!(
            "EVID effective_layout(count={}, preset={layout}) = {}",
            channels.len(),
            effective_layout(channels.len(), &layout)
        );
        for (i, channel) in channels.iter().enumerate() {
            let title = mpv_window_title(channel);
            println!(
                "EVID launch geometry idx {i}: {:?}",
                mpv_geometry_for_dock(true, i, channels.len(), Some(&layout))
            );
            match find_player_window(channel) {
                Some(hwnd) => println!(
                    "EVID window '{title}' (idx {i}): found, rect before = {:?}",
                    rect_of(hwnd)
                ),
                None => println!("EVID window '{title}' (idx {i}): NOT FOUND"),
            }
        }
        let found = retile_player_windows(&channels, true, &layout);
        println!("EVID retile(layout={layout}) found={found}");
        for channel in &channels {
            let title = mpv_window_title(channel);
            if let Some(hwnd) = find_player_window(channel) {
                println!("EVID window '{title}': rect after = {:?}", rect_of(hwnd));
            }
        }
    }

    #[test]
    #[cfg(windows)]
    fn partially_filled_presets_shrink_to_count_grid() {
        // Regression: one stream under the default "2x2" preset was tiled into
        // the top-left quarter instead of filling the video area.
        assert_eq!(effective_layout(1, "2x2"), "1");
        assert_eq!(effective_layout(2, "2x2"), "2");
        assert_eq!(effective_layout(3, "2x2"), "2x2");
        assert_eq!(effective_layout(4, "4x2"), "2x2");
        assert_eq!(effective_layout(6, "4x2"), "3x2");
        assert_eq!(effective_layout(8, "4x2"), "4x2");
        // 3plus1 keeps its asymmetric main+stack split for 2+ channels.
        assert_eq!(effective_layout(1, "3plus1"), "1");
        assert_eq!(effective_layout(2, "3plus1"), "3plus1");
        // Vertical stack presets keep stacking for 2+ channels.
        assert_eq!(effective_layout(1, "1x2"), "1");
        assert_eq!(effective_layout(2, "1x2"), "1x2");
        assert_eq!(effective_layout(3, "1x3"), "1x3");
        assert_eq!(effective_layout(4, "1x4"), "1x4");
    }

    #[test]
    fn dock_args_keep_custom_extras_but_drop_owned_flags() {
        // Regression: dock mode silently discarded all custom mpv args except
        // --no-keepaspect-window and --loop-*.
        let args = build_mpv_dock_args(
            "chan",
            false,
            "--loop-file=inf --cache=yes --volume=42 --title=\"chan - g - t\" --geometry=50%x50%+0+0 --window-maximized=yes",
            0,
            1,
            Some("2x2"),
        );
        assert!(args.contains("--loop-file=inf"));
        assert!(args.contains("--cache=yes"));
        assert!(args.contains("--volume=42"));
        // Dock owns geometry and window title.
        assert!(!args.contains("--geometry=50%x50%"));
        assert!(!args.contains("--window-maximized"));
        assert!(!args.contains("chan - g - t"));
        assert!(args.contains("--title=rillmux-chan"));
        assert!(args.contains("--force-media-title=rillmux-chan"));
    }
}
