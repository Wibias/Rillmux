// Dock geometry: shared rectangles, divider seams, and tile layout.

pub const DEFAULT_CHAT_FRACTION: f64 = 0.18;
pub const MIN_CHAT_FRACTION: f64 = 0.12;
pub const MAX_CHAT_FRACTION: f64 = 0.45;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[repr(C)]
pub struct Rect {
    pub left: i32,
    pub top: i32,
    pub right: i32,
    pub bottom: i32,
}

impl Rect {
    pub fn width(self) -> i32 {
        (self.right - self.left).max(0)
    }
    pub fn height(self) -> i32 {
        (self.bottom - self.top).max(0)
    }
}

// Match the hover target up front so near-seam clicks cannot fall through to
// mpv's own resize border.
pub const DIVIDER_REST_THICK: i32 = 8;
pub const DIVIDER_HOVER_THICK: i32 = 8;
pub const MONITOR_SWITCH_WIDTH: i32 = 56;
pub const MONITOR_SWITCH_HEIGHT: i32 = 28;
pub const MONITOR_SWITCH_CAPTION_GAP: i32 = 8;
/// Matches the frameless Rillmux chrome (`body` / `.shell__titlebar` / `#tbo-controls`).
pub const TITLE_BAR_HEIGHT_CSS: i32 = 38;

pub fn divider_thickness(hover: bool) -> i32 {
    if hover {
        DIVIDER_HOVER_THICK
    } else {
        DIVIDER_REST_THICK
    }
}

pub fn vertical_divider_rect(seam_x: i32, top: i32, bottom: i32, thick: i32) -> Rect {
    let half = thick / 2;
    Rect {
        left: seam_x - half,
        top,
        right: seam_x - half + thick,
        bottom,
    }
}

pub fn horizontal_divider_rect(seam_y: i32, left: i32, right: i32, thick: i32) -> Rect {
    let half = thick / 2;
    Rect {
        left,
        top: seam_y - half,
        right,
        bottom: seam_y - half + thick,
    }
}

#[cfg(test)]
pub fn rects_overlap(a: Rect, b: Rect) -> bool {
    a.left < b.right && a.right > b.left && a.top < b.bottom && a.bottom > b.top
}

/// Keep slot order; drop closed channels; append any newly running ones.
pub fn monitor_switch_rect(video: Rect, chat: Option<Rect>, caption: Option<Rect>) -> Rect {
    const W: i32 = MONITOR_SWITCH_WIDTH;
    const H: i32 = MONITOR_SWITCH_HEIGHT;
    const GAP: i32 = MONITOR_SWITCH_CAPTION_GAP;
    let seam_x = chat.map(|c| c.left).unwrap_or(video.right);
    let title_bottom = caption
        .map(|cap| cap.bottom)
        .unwrap_or(video.top + TITLE_BAR_HEIGHT_CSS);
    let min_top = video.top.max(title_bottom) + GAP;
    let top = min_top.min((video.bottom - H).max(video.top));
    Rect {
        left: seam_x - W / 2,
        top,
        right: seam_x + W / 2,
        bottom: top + H,
    }
}
fn normalize_weights(weights: &[f64], n: usize) -> Vec<f64> {
    if n == 0 {
        return Vec::new();
    }
    let mut w = if weights.len() == n && weights.iter().all(|x| *x > 0.0) {
        weights.to_vec()
    } else {
        vec![1.0; n]
    };
    let sum: f64 = w.iter().sum();
    if sum <= 0.0 {
        w = vec![1.0; n];
        let s = n as f64;
        for x in &mut w {
            *x /= s;
        }
    } else {
        for x in &mut w {
            *x /= sum;
        }
    }
    w
}

pub fn tile_rect(video: Rect, index: usize, layout: &str) -> Rect {
    let cfg = snapshot();
    let vw = video.width();
    let vh = video.height();
    if layout == "3plus1" || layout == "2plus1" {
        let stack_n = if layout == "2plus1" { 2 } else { 3 };
        let main_frac = cfg.main_frac.clamp(0.4, 0.85);
        let side = cfg.main_side.as_str();
        if side == "top" || side == "bottom" {
            let main_h = ((vh as f64) * main_frac).round() as i32;
            let stack = normalize_weights(&cfg.stack_weights[..stack_n], stack_n);
            if index == 0 {
                return if side == "top" {
                    Rect {
                        left: video.left,
                        top: video.top,
                        right: video.right,
                        bottom: video.top + main_h,
                    }
                } else {
                    Rect {
                        left: video.left,
                        top: video.bottom - main_h,
                        right: video.right,
                        bottom: video.bottom,
                    }
                };
            }
            let stack_top = if side == "top" {
                video.top + main_h
            } else {
                video.top
            };
            let stack_bottom = if side == "top" {
                video.bottom
            } else {
                video.bottom - main_h
            };
            let stack_h = (stack_bottom - stack_top).max(1);
            let slot = (index - 1).min(stack_n - 1);
            let mut x = video.left;
            for (i, w) in stack.iter().enumerate() {
                let cw = if i + 1 == stack_n {
                    video.right - x
                } else {
                    ((vw as f64) * w).round() as i32
                };
                if i == slot {
                    return Rect {
                        left: x,
                        top: stack_top,
                        right: x + cw.max(1),
                        bottom: stack_top + stack_h,
                    };
                }
                x += cw;
            }
            return Rect {
                left: video.left,
                top: stack_top,
                right: video.right,
                bottom: stack_bottom,
            };
        }
        // left / right
        let main_w = ((vw as f64) * main_frac).round() as i32;
        let stack = normalize_weights(&cfg.stack_weights[..stack_n], stack_n);
        if index == 0 {
            return if side == "right" {
                Rect {
                    left: video.right - main_w,
                    top: video.top,
                    right: video.right,
                    bottom: video.bottom,
                }
            } else {
                Rect {
                    left: video.left,
                    top: video.top,
                    right: video.left + main_w,
                    bottom: video.bottom,
                }
            };
        }
        let stack_left = if side == "right" {
            video.left
        } else {
            video.left + main_w
        };
        let stack_right = if side == "right" {
            video.right - main_w
        } else {
            video.right
        };
        let slot = (index - 1).min(stack_n - 1);
        let mut y = video.top;
        for (i, w) in stack.iter().enumerate() {
            let h = if i + 1 == stack_n {
                video.bottom - y
            } else {
                ((vh as f64) * w).round() as i32
            };
            if i == slot {
                return Rect {
                    left: stack_left,
                    top: y,
                    right: stack_right,
                    bottom: y + h.max(1),
                };
            }
            y += h;
        }
        return Rect {
            left: stack_left,
            top: video.top,
            right: stack_right,
            bottom: video.bottom,
        };
    }
    let (cols, rows) = match layout {
        "1" => (1, 1),
        "2" => (2, 1),
        "1x2" => (1, 2),
        "1x3" => (1, 3),
        "1x4" => (1, 4),
        "2x2" => (2, 2),
        "3x2" => (3, 2),
        "4x2" => (4, 2),
        "8x1" => (8, 1),
        _ => (2, 2),
    };
    let max_i = (cols * rows).max(1) - 1;
    let i = index.min(max_i);
    let col = i % cols;
    let row = i / cols;
    let col_w = normalize_weights(&cfg.col_weights, cols);
    let row_w = normalize_weights(&cfg.row_weights, rows);
    let mut x = video.left;
    for (c, w) in col_w.iter().enumerate() {
        let cw = if c == cols - 1 {
            video.right - x
        } else {
            ((vw as f64) * w).round() as i32
        };
        if c == col {
            let mut y = video.top;
            for (r, rw) in row_w.iter().enumerate() {
                let rh = if r == rows - 1 {
                    video.bottom - y
                } else {
                    ((vh as f64) * rw).round() as i32
                };
                if r == row {
                    return Rect {
                        left: x,
                        top: y,
                        right: x + cw.max(1),
                        bottom: y + rh.max(1),
                    };
                }
                y += rh;
            }
        }
        x += cw;
    }
    video
}

fn grid_dims(layout: &str) -> (usize, usize) {
    match layout {
        "1" => (1, 1),
        "2" => (2, 1),
        "1x2" => (1, 2),
        "1x3" => (1, 3),
        "1x4" => (1, 4),
        "2x2" => (2, 2),
        "3x2" => (3, 2),
        "4x2" => (4, 2),
        "8x1" => (8, 1),
        "3plus1" => (2, 3),
        "2plus1" => (2, 2),
        _ => (2, 2),
    }
}

fn tile_grip_plan(video: Rect, cfg: &DockConfig, layout: &str) -> Vec<(isize, Rect)> {
    const THICK: i32 = DIVIDER_REST_THICK;
    let mut out = Vec::new();
    if layout == "3plus1" || layout == "2plus1" {
        let stack_n = if layout == "2plus1" { 2 } else { 3 };
        let main_frac = cfg.main_frac.clamp(0.4, 0.85);
        let side = cfg.main_side.as_str();
        let stack = normalize_weights(&cfg.stack_weights[..stack_n], stack_n);
        if side == "top" || side == "bottom" {
            let main_h = ((video.height() as f64) * main_frac).round() as i32;
            let seam_y = if side == "top" {
                video.top + main_h
            } else {
                video.bottom - main_h
            };
            out.push((
                2,
                Rect {
                    left: video.left,
                    top: seam_y - THICK / 2,
                    right: video.right,
                    bottom: seam_y + THICK / 2,
                },
            ));
            let stack_top = if side == "top" {
                video.top + main_h
            } else {
                video.top
            };
            let stack_bottom = if side == "top" {
                video.bottom
            } else {
                video.bottom - main_h
            };
            let mut x = video.left as f64;
            for seam in 0..stack_n.saturating_sub(1) {
                x += video.width() as f64 * stack[seam];
                out.push((
                    300 + seam as isize,
                    Rect {
                        left: x as i32 - THICK / 2,
                        top: stack_top,
                        right: x as i32 + THICK / 2,
                        bottom: stack_bottom,
                    },
                ));
            }
        } else {
            let main_w = ((video.width() as f64) * main_frac).round() as i32;
            let seam_x = if side == "right" {
                video.right - main_w
            } else {
                video.left + main_w
            };
            out.push((
                2,
                Rect {
                    left: seam_x - THICK / 2,
                    top: video.top,
                    right: seam_x + THICK / 2,
                    bottom: video.bottom,
                },
            ));
            let stack_left = if side == "right" {
                video.left
            } else {
                video.left + main_w
            };
            let stack_right = if side == "right" {
                video.right - main_w
            } else {
                video.right
            };
            let mut y = video.top as f64;
            for seam in 0..stack_n.saturating_sub(1) {
                y += video.height() as f64 * stack[seam];
                out.push((
                    300 + seam as isize,
                    Rect {
                        left: stack_left,
                        top: y as i32 - THICK / 2,
                        right: stack_right,
                        bottom: y as i32 + THICK / 2,
                    },
                ));
            }
        }
        return out;
    }
    let (cols, rows) = grid_dims(layout);
    let col_w = normalize_weights(&cfg.col_weights, cols);
    let row_w = normalize_weights(&cfg.row_weights, rows);
    let mut x = video.left as f64;
    for seam in 0..cols.saturating_sub(1) {
        x += video.width() as f64 * col_w[seam];
        out.push((
            100 + seam as isize,
            Rect {
                left: x as i32 - THICK / 2,
                top: video.top,
                right: x as i32 + THICK / 2,
                bottom: video.bottom,
            },
        ));
    }
    let mut y = video.top as f64;
    for seam in 0..rows.saturating_sub(1) {
        y += video.height() as f64 * row_w[seam];
        out.push((
            200 + seam as isize,
            Rect {
                left: video.left,
                top: y as i32 - THICK / 2,
                right: video.right,
                bottom: y as i32 + THICK / 2,
            },
        ));
    }
    out
}
fn effective_layout(count: usize, preset: &str) -> &str {
    if preset == "3plus1" && count >= 2 {
        return "3plus1";
    }
    if preset == "2plus1" && count >= 2 {
        return "2plus1";
    }
    if preset == "8x1" && count >= 2 {
        return "8x1";
    }
    if (preset == "1x2" || preset == "1x3" || preset == "1x4") && count >= 2 {
        return preset;
    }
    match count {
        0 | 1 => "1",
        2 => "2",
        3 | 4 => "2x2",
        5 | 6 => "3x2",
        _ => "4x2",
    }
}
