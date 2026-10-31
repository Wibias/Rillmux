// Dock sessions: grouped layout state and independent per-monitor placements.

pub fn remaining_dock_channels(previous: &[String], running: &[String]) -> Vec<String> {
    let running_set: std::collections::HashSet<String> =
        running.iter().map(|c| c.to_ascii_lowercase()).collect();
    let mut out: Vec<String> = previous
        .iter()
        .map(|c| c.to_ascii_lowercase())
        .filter(|c| running_set.contains(c))
        .collect();
    let have: std::collections::HashSet<String> = out.iter().cloned().collect();
    for channel in running {
        let normalized = channel.to_ascii_lowercase();
        if running_set.contains(&normalized) && !have.contains(&normalized) {
            out.push(normalized);
        }
    }
    out
}

/// ◀ ▶ monitor switch on the video|chat seam, always below the title-bar row.
/// Caption buttons sit at the window top-right; the chrome strip is full width,
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct IndependentPlacement {
    pub channel: String,
    pub monitor: usize,
    pub same_monitor_offset: usize,
}

fn independent_placements() -> &'static Mutex<Vec<IndependentPlacement>> {
    static P: OnceLock<Mutex<Vec<IndependentPlacement>>> = OnceLock::new();
    P.get_or_init(|| Mutex::new(Vec::new()))
}

static ARMED_INDEPENDENT_SEAM: AtomicIsize = AtomicIsize::new(-1);

pub fn has_independent_placements() -> bool {
    independent_placements()
        .lock()
        .map(|g| !g.is_empty())
        .unwrap_or(false)
}

pub fn independent_channels() -> Vec<String> {
    independent_placements()
        .lock()
        .map(|g| g.iter().map(|p| p.channel.clone()).collect())
        .unwrap_or_default()
}

pub fn clear_independent_placements() {
    if let Ok(mut g) = independent_placements().lock() {
        g.clear();
    }
    ARMED_INDEPENDENT_SEAM.store(-1, Ordering::SeqCst);
}

pub fn arm_independent_seam(index: usize) {
    ARMED_INDEPENDENT_SEAM.store(index as isize, Ordering::SeqCst);
}

pub fn take_armed_independent_seam() -> Option<usize> {
    let value = ARMED_INDEPENDENT_SEAM.swap(-1, Ordering::SeqCst);
    if value >= 0 {
        Some(value as usize)
    } else {
        None
    }
}

/// Video on the left, chat on the right, sharing the seam and not overlapping.
pub fn split_work_for_chat(work: Rect, fraction: f64) -> (Rect, Rect) {
    let width = work.width().max(2);
    let chat_w = ((width as f64) * clamp_chat_fraction(fraction)).round() as i32;
    let chat_w = chat_w.clamp(1, width - 1);
    let chat = Rect {
        left: work.right - chat_w,
        top: work.top,
        right: work.right,
        bottom: work.bottom,
    };
    let video = Rect {
        left: work.left,
        top: work.top,
        right: chat.left,
        bottom: work.bottom,
    };
    (video, chat)
}

pub fn independent_work_rect(monitor: Rect, cascade: usize) -> Rect {
    if cascade == 0 || monitor.width() < 200 || monitor.height() < 200 {
        return monitor;
    }
    let shift = (cascade as i32) * 48;
    Rect {
        left: monitor.left + shift.min((monitor.width() / 5).max(0)),
        top: monitor.top + shift.min((monitor.height() / 5).max(0)),
        right: monitor.right,
        bottom: monitor.bottom,
    }
}

pub fn next_independent_placements(
    previous: &[IndependentPlacement],
    channels: &[String],
    monitor_count: usize,
) -> Vec<IndependentPlacement> {
    let monitors = monitor_count.max(1);
    let mut next = Vec::new();
    for channel in channels {
        let key = channel.trim().trim_start_matches('#').to_ascii_lowercase();
        if key.is_empty() {
            continue;
        }
        if let Some(existing) = previous.iter().find(|placement| placement.channel == key) {
            next.push(IndependentPlacement {
                channel: key,
                monitor: existing.monitor % monitors,
                same_monitor_offset: existing.same_monitor_offset,
            });
            continue;
        }
        let monitor = (0..monitors)
            .min_by_key(|idx| {
                next.iter()
                    .filter(|placement| placement.monitor == *idx)
                    .count()
            })
            .unwrap_or(0);
        let cascade = next
            .iter()
            .filter(|placement| placement.monitor == monitor)
            .count();
        next.push(IndependentPlacement {
            channel: key,
            monitor,
            same_monitor_offset: cascade,
        });
    }
    next
}

fn write_independent_dock(placements: &[IndependentPlacement]) {
    if let Ok(mut g) = dock().lock() {
        g.channels = placements
            .iter()
            .map(|placement| placement.channel.clone())
            .collect();
        g.layout = "independent".into();
        g.reserve_chat = false;
        g.linked = !placements.is_empty();
    }
}

pub fn sync_independent_session(channels: &[String]) {
    let previous = independent_placements()
        .lock()
        .map(|g| g.clone())
        .unwrap_or_default();
    let next = next_independent_placements(&previous, channels, monitor_count());
    if let Ok(mut g) = independent_placements().lock() {
        *g = next.clone();
    }
    write_independent_dock(&next);
    ensure_grip_thread();
    #[cfg(windows)]
    post_cmd(DockCmd::Sync);
}

pub fn move_independent_seam_to_monitor(index: usize, monitor: usize) {
    let monitors = monitor_count();
    let monitor = monitor % monitors;
    let Ok(mut placements) = independent_placements().lock() else {
        return;
    };
    if index >= placements.len() {
        return;
    }
    let cascade = placements
        .iter()
        .enumerate()
        .filter(|(other, placement)| *other != index && placement.monitor == monitor)
        .count();
    placements[index].monitor = monitor;
    placements[index].same_monitor_offset = cascade;
    let next = placements.clone();
    write_independent_dock(&next);
}

#[cfg(windows)]
pub fn monitor_work_rects() -> Vec<Rect> {
    list_monitor_work()
}

#[cfg(not(windows))]
pub fn monitor_work_rects() -> Vec<Rect> {
    Vec::new()
}

pub fn monitor_count() -> usize {
    monitor_work_rects().len().max(1)
}

pub fn independent_seam_work(index: usize) -> Option<Rect> {
    let monitors = monitor_work_rects();
    if monitors.is_empty() {
        return None;
    }
    let placements = independent_placements().lock().ok()?;
    let placement = placements.get(index)?;
    let monitor = monitors.get(placement.monitor % monitors.len()).copied()?;
    Some(independent_work_rect(
        monitor,
        placement.same_monitor_offset,
    ))
}

pub fn independent_window_splits() -> Vec<(String, Rect, Rect)> {
    let fraction = chat_fraction();
    let monitors = monitor_work_rects();
    if monitors.is_empty() {
        return Vec::new();
    }
    let Ok(placements) = independent_placements().lock() else {
        return Vec::new();
    };
    placements
        .iter()
        .filter_map(|placement| {
            let monitor = monitors.get(placement.monitor % monitors.len()).copied()?;
            let work = independent_work_rect(monitor, placement.same_monitor_offset);
            let (video, chat) = split_work_for_chat(work, fraction);
            Some((placement.channel.clone(), video, chat))
        })
        .collect()
}

#[cfg(windows)]
pub fn independent_grip_splits() -> Vec<(Rect, Rect)> {
    independent_window_splits()
        .into_iter()
        .map(|(_, video, chat)| (video, chat))
        .collect()
}
pub fn sync_session(channels: &[String], layout: &str, reserve_chat: bool, linked: bool) {
    let layout_changed = dock().lock().map(|g| g.layout != layout).unwrap_or(true);
    if let Ok(mut g) = dock().lock() {
        g.channels = channels.to_vec();
        if layout_changed {
            g.layout = layout.to_string();
            g.col_weights.clear();
            g.row_weights.clear();
            g.main_frac = 2.0 / 3.0;
            g.stack_weights = [1.0, 1.0, 1.0];
        }
        g.reserve_chat = reserve_chat;
        g.linked = linked;
    }
    ensure_grip_thread();
    #[cfg(windows)]
    post_cmd(DockCmd::Sync);
}

/// Drop closed streams from the dock snapshot and retile the rest in place.
pub fn drop_closed_channels(running: &[String]) {
    if snapshot().layout == "independent" {
        sync_independent_session(running);
        if !has_independent_placements() {
            clear_session();
            return;
        }
        #[cfg(windows)]
        run_apply();
        return;
    }
    let cfg = snapshot();
    let next = remaining_dock_channels(&cfg.channels, running);
    if next.is_empty() {
        clear_session();
        return;
    }
    if next == cfg.channels {
        #[cfg(windows)]
        run_apply();
        return;
    }
    sync_session(&next, &cfg.layout, cfg.reserve_chat, cfg.linked);
    #[cfg(windows)]
    run_apply();
}

pub fn clear_session() {
    clear_independent_placements();
    if let Ok(mut g) = dock().lock() {
        g.channels.clear();
        if g.layout == "independent" {
            g.layout = "2x2".into();
            g.linked = false;
            g.reserve_chat = false;
        }
    }
    #[cfg(windows)]
    post_cmd(DockCmd::Sync);
}
