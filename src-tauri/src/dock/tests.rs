#[cfg(all(test, windows))]
mod grip_zorder_behavior_tests {
    use super::restack_grip_windows;
    use std::collections::HashMap;

    #[link(name = "user32")]
    unsafe extern "system" {
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
        fn SetWindowPos(
            hwnd: *mut core::ffi::c_void,
            after: *mut core::ffi::c_void,
            x: i32,
            y: i32,
            cx: i32,
            cy: i32,
            flags: u32,
        ) -> i32;
        fn GetTopWindow(hwnd: *mut core::ffi::c_void) -> *mut core::ffi::c_void;
        fn GetWindow(hwnd: *mut core::ffi::c_void, cmd: u32) -> *mut core::ffi::c_void;
        fn GetWindowLongPtrW(hwnd: *mut core::ffi::c_void, index: i32) -> isize;
    }

    const WS_POPUP: u32 = 0x8000_0000;
    const WS_EX_TOPMOST: u32 = 0x0000_0008;
    const SWP_NOMOVE: u32 = 0x0002;
    const SWP_NOSIZE: u32 = 0x0001;
    const SWP_NOACTIVATE: u32 = 0x0010;
    const SWP_SHOWWINDOW: u32 = 0x0040;
    const HWND_TOP: isize = 0;
    const GWL_EXSTYLE: i32 = -20;
    const GW_HWNDNEXT: u32 = 2;

    struct TestWindow(*mut core::ffi::c_void);

    impl TestWindow {
        fn new(ex_style: u32) -> Self {
            let class: Vec<u16> = "STATIC".encode_utf16().chain(std::iter::once(0)).collect();
            let title: Vec<u16> = "Rillmux dock test"
                .encode_utf16()
                .chain(std::iter::once(0))
                .collect();
            let hwnd = unsafe {
                CreateWindowExW(
                    ex_style,
                    class.as_ptr(),
                    title.as_ptr(),
                    WS_POPUP,
                    0,
                    0,
                    1,
                    1,
                    std::ptr::null_mut(),
                    std::ptr::null_mut(),
                    std::ptr::null_mut(),
                    std::ptr::null_mut(),
                )
            };
            assert!(!hwnd.is_null(), "could not create a Win32 test window");
            Self(hwnd)
        }

        fn raise(&self) {
            let ok = unsafe {
                SetWindowPos(
                    self.0,
                    HWND_TOP as *mut core::ffi::c_void,
                    0,
                    0,
                    0,
                    0,
                    SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE | SWP_SHOWWINDOW,
                )
            };
            assert_ne!(ok, 0, "could not arrange a Win32 test window");
        }

        fn is_topmost(&self) -> bool {
            let style = unsafe { GetWindowLongPtrW(self.0, GWL_EXSTYLE) as u32 };
            style & WS_EX_TOPMOST != 0
        }
    }

    impl Drop for TestWindow {
        fn drop(&mut self) {
            unsafe {
                DestroyWindow(self.0);
            }
        }
    }

    fn positions(windows: &[&TestWindow]) -> HashMap<usize, usize> {
        let mut order = Vec::new();
        let mut hwnd = unsafe { GetTopWindow(std::ptr::null_mut()) };
        while !hwnd.is_null() {
            order.push(hwnd as usize);
            hwnd = unsafe { GetWindow(hwnd, GW_HWNDNEXT) };
        }
        windows
            .iter()
            .enumerate()
            .map(|(index, window)| {
                let position = order
                    .iter()
                    .position(|hwnd| *hwnd == window.0 as usize)
                    .unwrap_or_else(|| panic!("window {index} disappeared from the Z-order"));
                (index, position)
            })
            .collect()
    }

    #[test]
    fn restack_places_grips_above_the_dock_and_below_unrelated_windows() {
        let anchor = TestWindow::new(0);
        let seam = TestWindow::new(0);
        let mover = TestWindow::new(0);
        let unrelated = TestWindow::new(0);

        anchor.raise();
        seam.raise();
        mover.raise();
        unrelated.raise();
        unsafe { restack_grip_windows(anchor.0, &[seam.0, mover.0]) };

        let order = positions(&[&anchor, &seam, &mover, &unrelated]);
        assert!(
            order[&3] < order[&2],
            "unrelated window must stay above the grip"
        );
        assert!(order[&2] < order[&1], "mover must stay above the seam grip");
        assert!(
            order[&1] < order[&0],
            "seam grip must stay above the dock member"
        );
    }

    #[test]
    fn restack_demotes_legacy_topmost_grips_before_placing_them() {
        let anchor = TestWindow::new(0);
        let legacy_grip = TestWindow::new(WS_EX_TOPMOST);

        anchor.raise();
        legacy_grip.raise();
        assert!(legacy_grip.is_topmost());

        unsafe { restack_grip_windows(anchor.0, &[legacy_grip.0]) };

        assert!(!legacy_grip.is_topmost());
        let order = positions(&[&anchor, &legacy_grip]);
        assert!(order[&1] < order[&0]);
    }
}

#[cfg(test)]
mod leftover_grip_tests {
    use super::*;

    #[test]
    fn independent_split_puts_chat_beside_video_without_overlap() {
        let work = Rect {
            left: 0,
            top: 0,
            right: 1000,
            bottom: 800,
        };
        let (video, chat) = split_work_for_chat(work, 0.2);
        assert_eq!(video.left, 0);
        assert_eq!(video.top, 0);
        assert_eq!(video.bottom, 800);
        assert_eq!(chat.top, 0);
        assert_eq!(chat.right, 1000);
        assert_eq!(chat.bottom, 800);
        assert_eq!(video.right, chat.left);
        assert!(!rects_overlap(video, chat));
        assert!(chat.width() > 0);
        assert!(video.width() > chat.width());
    }

    #[test]
    fn independent_streams_take_separate_monitors_before_cascading() {
        let next = next_independent_placements(&[], &["alpha".into(), "beta".into()], 2);
        assert_eq!(next[0].channel, "alpha");
        assert_eq!(next[0].monitor, 0);
        assert_eq!(next[0].same_monitor_offset, 0);
        assert_eq!(next[1].channel, "beta");
        assert_eq!(next[1].monitor, 1);
        assert_eq!(next[1].same_monitor_offset, 0);
    }

    #[test]
    fn second_independent_stream_on_one_monitor_is_offset_not_tiled() {
        let next = next_independent_placements(&[], &["alpha".into(), "beta".into()], 1);
        assert_eq!(next[0].monitor, 0);
        assert_eq!(next[1].monitor, 0);
        assert_eq!(next[0].same_monitor_offset, 0);
        assert_eq!(next[1].same_monitor_offset, 1);
        let monitor = Rect {
            left: 0,
            top: 0,
            right: 1920,
            bottom: 1080,
        };
        let first = independent_work_rect(monitor, next[0].same_monitor_offset);
        let second = independent_work_rect(monitor, next[1].same_monitor_offset);
        assert_eq!(first, monitor);
        assert!(second.left > first.left);
        assert!(second.top > first.top);
        assert_eq!(second.right, monitor.right);
        assert_eq!(second.bottom, monitor.bottom);
    }

    #[test]
    fn existing_independent_monitor_is_kept_when_another_stream_opens() {
        let previous = vec![IndependentPlacement {
            channel: "beta".into(),
            monitor: 1,
            same_monitor_offset: 0,
        }];
        let next = next_independent_placements(&previous, &["beta".into(), "alpha".into()], 2);
        assert_eq!(next[0].channel, "beta");
        assert_eq!(next[0].monitor, 1);
        assert_eq!(next[1].channel, "alpha");
        assert_eq!(next[1].monitor, 0);
    }

    #[test]
    fn empty_session_destroys_leftover_grips() {
        let source = include_str!("grip_sync.rs");
        assert!(source.contains("if !cfg.linked || cfg.channels.is_empty()"));
        assert!(source.contains("destroy_grip(g.chat)"));
        assert!(source.contains("destroy_grip(g.mover)"));
        assert!(source.contains("for h in g.identifies.drain(..)"));
        assert!(source.contains("g.chat = std::ptr::null_mut()"));
    }

    #[test]
    fn remaining_dock_channels_drops_closed_streams_and_keeps_order() {
        let previous = vec!["forsen".into(), "xqc".into(), "lirik".into()];
        let running = vec!["lirik".into(), "forsen".into()];
        assert_eq!(
            remaining_dock_channels(&previous, &running),
            vec!["forsen".to_string(), "lirik".to_string()]
        );
        assert!(remaining_dock_channels(&previous, &[]).is_empty());
    }

    #[test]
    fn one_by_two_stack_has_easy_horizontal_divider_hit_target() {
        let video = Rect {
            left: 0,
            top: 0,
            right: 1000,
            bottom: 800,
        };
        let cfg = DockConfig {
            layout: "1x2".into(),
            ..DockConfig::default()
        };

        let plan = tile_grip_plan(video, &cfg, "1x2");
        let (_, divider) = plan
            .iter()
            .find(|(code, _)| *code == 200)
            .expect("1x2 must have a row divider");

        assert_eq!(divider.left, 0);
        assert_eq!(divider.top, 396);
        assert_eq!(divider.right, 1000);
        assert_eq!(divider.bottom, 404);
    }

    #[test]
    fn monitor_switch_sits_below_caption_buttons() {
        let video = Rect {
            left: 0,
            top: 0,
            right: 1540,
            bottom: 1080,
        };
        let chat = Rect {
            left: 1540,
            top: 0,
            right: 1920,
            bottom: 1080,
        };
        let caption = Rect {
            left: 1540 - 138,
            top: 0,
            right: 1540,
            bottom: 38,
        };
        let handle = monitor_switch_rect(video, Some(chat), Some(caption));
        assert!(!rects_overlap(handle, caption));
        assert!(handle.top >= caption.bottom);
        assert_eq!(handle.left + handle.width() / 2, chat.left);
    }

    #[test]
    fn monitor_switch_clears_the_title_bar_even_when_buttons_are_far_right() {
        let video = Rect {
            left: 0,
            top: 0,
            right: 1540,
            bottom: 1080,
        };
        let chat = Rect {
            left: 1540,
            top: 0,
            right: 1920,
            bottom: 1080,
        };
        let title_bar = Rect {
            left: 0,
            top: 0,
            right: 1920,
            bottom: 38,
        };
        let buttons = Rect {
            left: 1920 - 138,
            top: 0,
            right: 1920,
            bottom: 38,
        };
        let handle = monitor_switch_rect(video, Some(chat), Some(title_bar));
        assert!(!rects_overlap(handle, title_bar));
        assert!(!rects_overlap(handle, buttons));
        assert!(handle.top >= title_bar.bottom + MONITOR_SWITCH_CAPTION_GAP);
    }
}
