// Overlay geometry, Channel Points HUD, and caption avoidance.


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
            let title = mpv_window_title(channel, channel);
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
            let title = mpv_window_title(channel, channel);
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
            "Just Chatting",
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
        assert!(args.contains("--title=chan-just_chatting"));
        assert!(args.contains("--force-media-title=chan-just_chatting"));
    }

    #[test]
    fn dock_args_keep_an_opt_in_volume_boost() {
        // The booster is composed in the frontend; dock mode must not drop it,
        // or the stream would attach at mpv's default 100 % ceiling.
        let args = build_mpv_dock_args(
            "chan",
            "Just Chatting",
            false,
            "--no-border --volume-max=200 --volume=200",
            0,
            1,
            Some("2x2"),
        );
        assert!(args.contains("--volume-max=200"));
        assert!(args.contains("--volume=200"));
    }

    #[test]
    fn attached_volume_follows_the_composed_args() {
        assert_eq!(
            mpv_initial_volume("--no-border --volume-max=150 --volume=150"),
            150.0
        );
        // No booster, or garbage: mpv's normal 100 %.
        assert_eq!(mpv_initial_volume("--no-border --volume-max=130"), 100.0);
        assert_eq!(mpv_initial_volume("--volume=loud"), 100.0);
        assert_eq!(mpv_initial_volume(""), 100.0);
    }

    #[test]
    fn new_sessions_inherit_the_previous_volume_over_the_default() {
        // The level left on the old stream carries over a switch / raid follow.
        assert_eq!(inherited_or_initial(Some(35.0), 100.0), 35.0);
        assert_eq!(inherited_or_initial(Some(35.0), 200.0), 35.0);
        // Without a remembered level, the composed --volume (booster) wins.
        assert_eq!(inherited_or_initial(None, 200.0), 200.0);
        // Junk readings must never silence the stream.
        assert_eq!(inherited_or_initial(Some(0.0), 100.0), 100.0);
        assert_eq!(inherited_or_initial(Some(f64::NAN), 100.0), 100.0);
    }

    #[test]
    fn selected_volume_respects_mode_and_ceiling() {
        assert_eq!(
            selected_player_volume("max", 80.0, 150.0, Some(35.0), 100.0),
            150.0
        );
        assert_eq!(
            selected_player_volume("custom", 175.0, 200.0, Some(35.0), 100.0),
            175.0
        );
        assert_eq!(
            selected_player_volume("custom", 250.0, 200.0, Some(35.0), 100.0),
            200.0
        );
        assert_eq!(
            selected_player_volume("last", 100.0, 130.0, Some(180.0), 100.0),
            130.0
        );
        assert_eq!(
            selected_player_volume("last", 100.0, 200.0, Some(35.0), 100.0),
            35.0
        );
    }

    #[test]
    fn catch_up_uses_hysteresis_around_the_buffered_lead() {
        // Engage only once the lead clearly exceeds the enter threshold.
        assert!(!catch_up_should_engage(false, 5.9));
        assert!(catch_up_should_engage(false, 6.1));
        // Once engaged, keep going until the lead drains below the exit band.
        assert!(catch_up_should_engage(true, 2.5));
        assert!(!catch_up_should_engage(true, 1.9));
        // mpv reports -1 / NaN when no lead is available: never engage.
        assert!(!catch_up_should_engage(false, -1.0));
        assert!(!catch_up_should_engage(true, f64::NAN));
    }
