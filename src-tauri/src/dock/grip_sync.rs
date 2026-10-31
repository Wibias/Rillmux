// Grip reconstruction from the current dock session.

fn place_independent_seam_grips(g: &GripWindows, splits: &[(Rect, Rect)]) {
    const THICK: i32 = DIVIDER_REST_THICK;
    unsafe {
        if let Some((video, chat)) = splits.first() {
            place_divider_grip(
                g.chat,
                vertical_divider_rect(chat.left, video.top, video.bottom, THICK),
            );
            place_grip(g.mover, mover_rect(*video, Some(*chat)));
        }
        for (offset, (video, chat)) in splits.iter().enumerate().skip(1) {
            let Some((chat_hwnd, mover)) = g.extra.get(offset - 1) else {
                continue;
            };
            place_divider_grip(
                *chat_hwnd,
                vertical_divider_rect(chat.left, video.top, video.bottom, THICK),
            );
            place_grip(*mover, mover_rect(*video, Some(*chat)));
        }
    }
}

fn reposition_grips_inner() -> Result<(), ()> {
    if crate::dock::has_independent_placements() {
        let splits = crate::dock::independent_grip_splits();
        let g = grips().lock().map_err(|_| ())?;
        place_independent_seam_grips(&g, &splits);
        return Ok(());
    }
    let cfg = snapshot();
    let mut g = grips().lock().map_err(|_| ())?;
    if !cfg.linked || cfg.channels.is_empty() {
        unsafe {
            destroy_grip(g.chat);
            g.chat = std::ptr::null_mut();
            destroy_grip(g.mover);
            g.mover = std::ptr::null_mut();
            for h in g.tiles.drain(..) {
                destroy_grip(h);
            }
            for h in g.identifies.drain(..) {
                destroy_grip(h);
            }
        }
        PICKER_OPEN.store(false, Ordering::SeqCst);
        return Ok(());
    }
    let Some((video, chat_opt)) = chat_video_split(cfg.reserve_chat) else {
        return Ok(());
    };
    const THICK: i32 = DIVIDER_REST_THICK;
    if cfg.reserve_chat {
        if let Some(chat) = chat_opt {
            let seam = Rect {
                left: chat.left - THICK / 2,
                top: video.top,
                right: chat.left + THICK / 2,
                bottom: video.bottom,
            };
            unsafe { place_divider_grip(g.chat, seam) };
        }
    } else if !g.chat.is_null() {
        unsafe {
            destroy_grip(g.chat);
            g.chat = std::ptr::null_mut();
            destroy_grip(g.mover);
            g.mover = std::ptr::null_mut();
        }
    }
    // Live-move tile greys with the streams while dragging.
    place_existing_tiles(&g, video, &cfg);
    // Mover always last so it stays above the chat/tile grips.
    if cfg.reserve_chat && !g.mover.is_null() {
        unsafe { place_grip(g.mover, mover_rect(video, chat_opt)) };
    }
    Ok(())
}
fn hide_all_grips() {
    let Ok(g) = grips().lock() else {
        return;
    };
    unsafe {
        hide_grip(g.chat);
        hide_grip(g.mover);
        for (chat, mover) in &g.extra {
            hide_grip(*chat);
            hide_grip(*mover);
        }
        for &h in &g.tiles {
            hide_grip(h);
        }
        for &h in &g.identifies {
            hide_grip(h);
        }
    }
}

fn suppress_seam_grips_inner() {
    let Ok(g) = grips().lock() else {
        return;
    };
    unsafe {
        hide_grip(g.chat);
        hide_grip(g.mover);
        for (chat, mover) in &g.extra {
            hide_grip(*chat);
            hide_grip(*mover);
        }
    }
}

fn restore_seam_grips_inner() {
    let cfg = snapshot();
    if !cfg.linked || cfg.channels.is_empty() || !cfg.reserve_chat {
        return;
    }
    let Some((video, chat_opt)) = chat_video_split(true) else {
        return;
    };
    let Ok(g) = grips().lock() else {
        return;
    };
    const THICK: i32 = DIVIDER_REST_THICK;
    unsafe {
        if let Some(chat) = chat_opt {
            if !g.chat.is_null() {
                place_grip(
                    g.chat,
                    Rect {
                        left: chat.left - THICK / 2,
                        top: video.top,
                        right: chat.left + THICK / 2,
                        bottom: video.bottom,
                    },
                );
            }
            if !g.mover.is_null() {
                place_grip(g.mover, mover_rect(video, Some(chat)));
            }
        }
    }
}

fn sync_grips_full(instance: *mut core::ffi::c_void, class: *const u16) {
    unsafe fn restore_shared_grip_kinds(g: &mut GripWindows) {
        if !g.chat.is_null() {
            SetWindowLongPtrW(g.chat, GWLP_USERDATA, 1);
        }
        if !g.mover.is_null() {
            SetWindowLongPtrW(g.mover, GWLP_USERDATA, 10);
        }
        for (chat, mover) in g.extra.drain(..) {
            destroy_grip(chat);
            destroy_grip(mover);
        }
    }

    unsafe fn show_independent_grips(
        instance: *mut core::ffi::c_void,
        class: *const u16,
        g: &mut GripWindows,
    ) {
        let splits = crate::dock::independent_grip_splits();
        for hwnd in g.tiles.drain(..) {
            destroy_grip(hwnd);
        }
        if splits.is_empty() {
            restore_shared_grip_kinds(g);
            return;
        }
        if g.chat.is_null() {
            g.chat = create_grip(instance, class, 400);
        } else {
            SetWindowLongPtrW(g.chat, GWLP_USERDATA, 400);
        }
        if g.mover.is_null() {
            g.mover = create_grip(instance, class, 500);
        } else {
            SetWindowLongPtrW(g.mover, GWLP_USERDATA, 500);
        }
        while g.extra.len() + 1 < splits.len() {
            let index = (g.extra.len() + 1) as isize;
            g.extra.push((
                create_grip(instance, class, 400 + index),
                create_grip(instance, class, 500 + index),
            ));
        }
        while g.extra.len() + 1 > splits.len() {
            if let Some((chat, mover)) = g.extra.pop() {
                destroy_grip(chat);
                destroy_grip(mover);
            }
        }
        place_independent_seam_grips(g, &splits);
    }

    if GRIPS_GROUP_MINIMIZED.load(Ordering::SeqCst) {
        hide_all_grips();
        return;
    }
    if crate::dock::has_independent_placements() {
        let Ok(mut g) = grips().lock() else {
            return;
        };
        unsafe {
            show_independent_grips(instance, class, &mut g);
        }
        return;
    }
    let cfg = snapshot();
    let Ok(mut g) = grips().lock() else {
        return;
    };
    unsafe {
        restore_shared_grip_kinds(&mut g);
        for h in g.tiles.drain(..) {
            destroy_grip(h);
        }
        if !cfg.linked || cfg.channels.is_empty() {
            destroy_grip(g.chat);
            g.chat = std::ptr::null_mut();
            destroy_grip(g.mover);
            g.mover = std::ptr::null_mut();
            for h in g.identifies.drain(..) {
                destroy_grip(h);
            }
            PICKER_OPEN.store(false, Ordering::SeqCst);
            return;
        }
        if g.chat.is_null() {
            g.chat = create_grip(instance, class, 1);
        }
        if g.mover.is_null() {
            g.mover = create_grip(instance, class, 10);
        }
        let Some((video, chat_opt)) = chat_video_split(cfg.reserve_chat) else {
            return;
        };
        const THICK: i32 = DIVIDER_REST_THICK;
        if cfg.reserve_chat {
            if let Some(chat) = chat_opt {
                place_grip(
                    g.chat,
                    Rect {
                        left: chat.left - THICK / 2,
                        top: video.top,
                        right: chat.left + THICK / 2,
                        bottom: video.bottom,
                    },
                );
            }
        } else {
            hide_grip(g.chat);
            hide_grip(g.mover);
        }

        let n = cfg.channels.len().clamp(1, 8);
        let layout = effective_layout(n, &cfg.layout);
        for (code, rect) in tile_grip_plan(video, &cfg, layout) {
            let hwnd = create_grip(instance, class, code);
            place_grip(hwnd, rect);
            g.tiles.push(hwnd);
        }
        // Always restack the move handle above every other grip.
        if cfg.reserve_chat {
            if let Some(chat) = chat_opt {
                place_grip(g.mover, mover_rect(video, Some(chat)));
            }
        }
    }
    // Sync owns grip creation/geometry only. The streaming watchdog owns
    // relative Z-order because it has the live mpv/Chatterino HWNDs.
    drop(g);
}
