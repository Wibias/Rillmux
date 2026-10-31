// Window procedure for the dock grip windows.

unsafe extern "system" fn wnd_proc(
    hwnd: *mut core::ffi::c_void,
    msg: u32,
    wparam: usize,
    lparam: isize,
) -> isize {
    let kind = kind_from_code(GetWindowLongPtrW(hwnd, GWLP_USERDATA));
    match msg {
        WM_PAINT => {
            let mut ps = std::mem::zeroed::<PaintStruct>();
            let hdc = BeginPaint(hwnd, &mut ps);
            let mut rc = Rect::default();
            GetClientRect(hwnd, &mut rc);
            if matches!(kind, GripKind::Move | GripKind::IndependentMove(_)) {
                let hover = MOVER_HOVER.load(Ordering::Relaxed);
                let brush = CreateSolidBrush(if hover { 0x00_60_60_70 } else { 0x00_40_40_48 });
                FillRect(hdc, &rc, brush);
                DeleteObject(brush);
                SetBkMode(hdc, TRANSPARENT);
                SetTextColor(hdc, if hover { 0x00_FF_FF_FF } else { 0x00_DD_DD_DD });
                let label = wide("◀  ▶");
                DrawTextW(
                    hdc,
                    label.as_ptr(),
                    -1,
                    &mut rc,
                    DT_CENTER | DT_VCENTER | DT_SINGLELINE,
                );
            } else if let GripKind::Identify(idx) = kind {
                let brush = CreateSolidBrush(0x00_20_20_28);
                FillRect(hdc, &rc, brush);
                DeleteObject(brush);
                SetBkMode(hdc, TRANSPARENT);
                SetTextColor(hdc, 0x00_FF_FF_FF);
                let face = wide("Segoe UI");
                let font = CreateFontW(
                    -((rc.height() as f64) * 0.55).round() as i32,
                    0,
                    0,
                    0,
                    FW_BOLD,
                    0,
                    0,
                    0,
                    0,
                    0,
                    0,
                    0,
                    0,
                    face.as_ptr(),
                );
                let old = if !font.is_null() {
                    SelectObject(hdc, font)
                } else {
                    std::ptr::null_mut()
                };
                let label = wide(&format!("{}", idx + 1));
                DrawTextW(
                    hdc,
                    label.as_ptr(),
                    -1,
                    &mut rc,
                    DT_CENTER | DT_VCENTER | DT_SINGLELINE,
                );
                if !font.is_null() {
                    SelectObject(hdc, old);
                    DeleteObject(font);
                }
            } else {
                let brush = CreateSolidBrush(0x00_88_88_88);
                FillRect(hdc, &rc, brush);
                DeleteObject(brush);
            }
            EndPaint(hwnd, &ps);
            0
        }
        WM_SETCURSOR => {
            let side = snapshot().main_side;
            let cursor_id = match kind {
                GripKind::Move | GripKind::IndependentMove(_) | GripKind::Identify(_) => IDC_HAND,
                GripKind::Chat | GripKind::IndependentChat(_) | GripKind::Col(_) => IDC_SIZEWE,
                GripKind::Row(_) => IDC_SIZENS,
                GripKind::Main => {
                    if side == "top" || side == "bottom" {
                        IDC_SIZENS
                    } else {
                        IDC_SIZEWE
                    }
                }
                GripKind::Stack(_) => {
                    if side == "top" || side == "bottom" {
                        IDC_SIZEWE
                    } else {
                        IDC_SIZENS
                    }
                }
            };
            let cur = LoadCursorW(std::ptr::null_mut(), cursor_id);
            SetCursor(cur);
            1
        }
        WM_MOUSEMOVE
            if matches!(kind, GripKind::Move | GripKind::IndependentMove(_))
                && drag().lock().map(|g| g.is_none()).unwrap_or(true) =>
        {
            if !MOVER_HOVER.swap(true, Ordering::Relaxed) {
                SetLayeredWindowAttributes(hwnd, 0, ALPHA_HOVER, LWA_ALPHA);
                InvalidateRect(hwnd, std::ptr::null(), 1);
            }
            let mut tme = TrackMouseEvent {
                cb_size: std::mem::size_of::<TrackMouseEvent>() as u32,
                dw_flags: TME_LEAVE,
                hwnd,
                hover_time: 0,
            };
            TrackMouseEvent(&mut tme);
            0
        }
        WM_MOUSEMOVE
            if divider_kind(kind) && drag().lock().map(|g| g.is_none()).unwrap_or(true) =>
        {
            let mut hover = divider_hover().lock().unwrap_or_else(|e| e.into_inner());
            let hwnd_key = hwnd as isize;
            if *hover != Some(hwnd_key) {
                if let Some(prev) = *hover {
                    if prev != hwnd_key {
                        let prev_hwnd = prev as *mut core::ffi::c_void;
                        let prev_kind = kind_from_code(GetWindowLongPtrW(prev_hwnd, GWLP_USERDATA));
                        resize_divider_hwnd(prev_hwnd, prev_kind, false);
                    }
                }
                *hover = Some(hwnd_key);
                drop(hover);
                resize_divider_hwnd(hwnd, kind, true);
            }
            let mut tme = TrackMouseEvent {
                cb_size: std::mem::size_of::<TrackMouseEvent>() as u32,
                dw_flags: TME_LEAVE,
                hwnd,
                hover_time: 0,
            };
            TrackMouseEvent(&mut tme);
            0
        }
        WM_MOUSELEAVE if matches!(kind, GripKind::Move | GripKind::IndependentMove(_)) => {
            MOVER_HOVER.store(false, Ordering::Relaxed);
            SetLayeredWindowAttributes(hwnd, 0, ALPHA_REST, LWA_ALPHA);
            InvalidateRect(hwnd, std::ptr::null(), 1);
            0
        }
        WM_MOUSELEAVE if divider_kind(kind) => {
            let mut hover = divider_hover().lock().unwrap_or_else(|e| e.into_inner());
            if *hover == Some(hwnd as isize) {
                *hover = None;
            }
            drop(hover);
            resize_divider_hwnd(hwnd, kind, false);
            0
        }
        WM_LBUTTONDOWN => {
            if matches!(kind, GripKind::Move) {
                post_cmd(DockCmd::PickMonitor);
                return 0;
            }
            if let GripKind::IndependentMove(index) = kind {
                arm_independent_seam(index);
                post_cmd(DockCmd::PickMonitor);
                return 0;
            }
            if let GripKind::Identify(idx) = kind {
                dismiss_monitor_picker();
                if let Some(seam) = take_armed_independent_seam() {
                    move_independent_seam_to_monitor(seam, idx);
                    request_raise_after_apply();
                    run_apply();
                    post_cmd(DockCmd::Sync);
                    return 0;
                }
                if let Ok(mut g) = dock().lock() {
                    g.monitor_index = idx;
                }
                request_raise_after_apply();
                run_apply();
                post_cmd(DockCmd::Sync);
                return 0;
            }
            let mut pt = Point { x: 0, y: 0 };
            GetCursorPos(&mut pt);
            let cfg = snapshot();
            *drag().lock().unwrap_or_else(|e| e.into_inner()) = Some(DragState {
                kind,
                start_x: pt.x,
                start_y: pt.y,
                start_frac: cfg.chat_fraction,
                start_main: cfg.main_frac,
                start_cols: cfg.col_weights.clone(),
                start_rows: cfg.row_weights.clone(),
                start_stack: cfg.stack_weights,
                start_monitor: cfg.monitor_index,
                moved: false,
            });
            SetCapture(hwnd);
            0
        }
        WM_MOUSEMOVE => {
            let mut pt = Point { x: 0, y: 0 };
            GetCursorPos(&mut pt);
            let mut guard = drag().lock().unwrap_or_else(|e| e.into_inner());
            let Some(d) = guard.as_mut() else {
                return DefWindowProcW(hwnd, msg, wparam, lparam);
            };
            if (pt.x - d.start_x).abs() > 4 || (pt.y - d.start_y).abs() > 4 {
                d.moved = true;
            }
            let work = active_work_rect().unwrap_or(Rect {
                left: 0,
                top: 0,
                right: 1920,
                bottom: 1080,
            });
            match d.kind {
                GripKind::Move | GripKind::IndependentMove(_) | GripKind::Identify(_) => 0,
                GripKind::IndependentChat(index) => {
                    let seam = independent_seam_work(index).unwrap_or(work);
                    let w = seam.width().max(1) as f64;
                    let chat_w = (seam.right - pt.x).max(0) as f64;
                    let frac = clamp_chat_fraction(chat_w / w);
                    drop(guard);
                    if let Ok(mut g) = dock().lock() {
                        g.chat_fraction = frac;
                    }
                    emit_fraction(frac);
                    run_apply();
                    reposition_all_grips_static();
                    return 0;
                }
                GripKind::Chat => {
                    let w = work.width().max(1) as f64;
                    let chat_w = (work.right - pt.x).max(0) as f64;
                    let frac = clamp_chat_fraction(chat_w / w);
                    drop(guard);
                    if let Ok(mut g) = dock().lock() {
                        g.chat_fraction = frac;
                    }
                    emit_fraction(frac);
                    run_apply();
                    reposition_all_grips_static();
                    return 0;
                }
                GripKind::Main => {
                    let video = chat_video_split(snapshot().reserve_chat)
                        .map(|(v, _)| v)
                        .unwrap_or(work);
                    let side = snapshot().main_side;
                    let main = match side.as_str() {
                        "top" => {
                            let h = video.height().max(1) as f64;
                            ((pt.y - video.top) as f64 / h).clamp(0.4, 0.85)
                        }
                        "bottom" => {
                            let h = video.height().max(1) as f64;
                            ((video.bottom - pt.y) as f64 / h).clamp(0.4, 0.85)
                        }
                        "right" => {
                            let w = video.width().max(1) as f64;
                            ((video.right - pt.x) as f64 / w).clamp(0.4, 0.85)
                        }
                        _ => {
                            let w = video.width().max(1) as f64;
                            ((pt.x - video.left) as f64 / w).clamp(0.4, 0.85)
                        }
                    };
                    drop(guard);
                    if let Ok(mut g) = dock().lock() {
                        g.main_frac = main;
                    }
                    run_apply();
                    reposition_all_grips_static();
                    return 0;
                }
                GripKind::Col(seam) => {
                    let (cols, _) = grid_dims(&snapshot().layout);
                    if cols < 2 || seam + 1 >= cols {
                        return 0;
                    }
                    let video = chat_video_split(snapshot().reserve_chat)
                        .map(|(v, _)| v)
                        .unwrap_or(work);
                    let mut weights = normalize_weights(&d.start_cols, cols);
                    let left_edge = {
                        let mut x = video.left as f64;
                        for w in weights.iter().take(seam) {
                            x += video.width() as f64 * w;
                        }
                        x
                    };
                    let pair = weights[seam] + weights[seam + 1];
                    let local = ((pt.x as f64 - left_edge) / (video.width() as f64))
                        .clamp(0.05, pair - 0.05);
                    weights[seam] = local;
                    weights[seam + 1] = pair - local;
                    drop(guard);
                    if let Ok(mut g) = dock().lock() {
                        g.col_weights = weights;
                    }
                    run_apply();
                    reposition_all_grips_static();
                    return 0;
                }
                GripKind::Row(seam) => {
                    let (_, rows) = grid_dims(&snapshot().layout);
                    if rows < 2 || seam + 1 >= rows {
                        return 0;
                    }
                    let video = chat_video_split(snapshot().reserve_chat)
                        .map(|(v, _)| v)
                        .unwrap_or(work);
                    let mut weights = normalize_weights(&d.start_rows, rows);
                    let top_edge = {
                        let mut y = video.top as f64;
                        for w in weights.iter().take(seam) {
                            y += video.height() as f64 * w;
                        }
                        y
                    };
                    let pair = weights[seam] + weights[seam + 1];
                    let local = ((pt.y as f64 - top_edge) / (video.height() as f64))
                        .clamp(0.05, pair - 0.05);
                    weights[seam] = local;
                    weights[seam + 1] = pair - local;
                    drop(guard);
                    if let Ok(mut g) = dock().lock() {
                        g.row_weights = weights;
                    }
                    run_apply();
                    reposition_all_grips_static();
                    return 0;
                }
                GripKind::Stack(seam) => {
                    let cfg = snapshot();
                    let stack_n = if cfg.layout == "2plus1" { 2 } else { 3 };
                    if seam + 1 >= stack_n {
                        return 0;
                    }
                    let video = chat_video_split(cfg.reserve_chat)
                        .map(|(v, _)| v)
                        .unwrap_or(work);
                    let side = cfg.main_side.as_str();
                    let mut weights = normalize_weights(&d.start_stack[..stack_n], stack_n);
                    if side == "top" || side == "bottom" {
                        let stack_w = video.width().max(1) as f64;
                        let left_edge = {
                            let mut x = video.left as f64;
                            for w in weights.iter().take(seam) {
                                x += stack_w * w;
                            }
                            x
                        };
                        let pair = weights[seam] + weights[seam + 1];
                        let local = ((pt.x as f64 - left_edge) / stack_w).clamp(0.05, pair - 0.05);
                        weights[seam] = local;
                        weights[seam + 1] = pair - local;
                    } else {
                        let stack_h = video.height().max(1) as f64;
                        let top_edge = {
                            let mut y = video.top as f64;
                            for w in weights.iter().take(seam) {
                                y += stack_h * w;
                            }
                            y
                        };
                        let pair = weights[seam] + weights[seam + 1];
                        let local = ((pt.y as f64 - top_edge) / stack_h).clamp(0.05, pair - 0.05);
                        weights[seam] = local;
                        weights[seam + 1] = pair - local;
                    }
                    let mut arr = [1.0_f64, 1.0, 1.0];
                    for (i, w) in weights.iter().enumerate().take(3) {
                        arr[i] = *w;
                    }
                    drop(guard);
                    if let Ok(mut g) = dock().lock() {
                        g.stack_weights = arr;
                    }
                    run_apply();
                    reposition_all_grips_static();
                    return 0;
                }
            }
        }
        WM_LBUTTONUP => {
            ReleaseCapture();
            let _ = drag().lock().unwrap_or_else(|e| e.into_inner()).take();
            post_cmd(DockCmd::Sync);
            0
        }
        WM_DESTROY => 0,
        _ => DefWindowProcW(hwnd, msg, wparam, lparam),
    }
}
