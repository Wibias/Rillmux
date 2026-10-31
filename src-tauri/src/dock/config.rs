// Live dock configuration: linked flag, chat fraction, main pane side.

#[derive(Clone, Debug)]
pub struct DockConfig {
    pub linked: bool,
    pub chat_fraction: f64,
    pub monitor_index: usize,
    pub channels: Vec<String>,
    pub layout: String,
    pub reserve_chat: bool,
    /// Relative column widths (sum normalized). Empty = equal.
    pub col_weights: Vec<f64>,
    /// Relative row heights (sum normalized). Empty = equal.
    pub row_weights: Vec<f64>,
    /// 3plus1/2plus1: fraction of video along the main split axis.
    pub main_frac: f64,
    /// 3plus1/2plus1: relative sizes of the stacked panes.
    pub stack_weights: [f64; 3],
    /// Where the large pane sits: left|right|top|bottom.
    pub main_side: String,
}

impl Default for DockConfig {
    fn default() -> Self {
        Self {
            linked: true,
            chat_fraction: DEFAULT_CHAT_FRACTION,
            monitor_index: 0,
            channels: Vec::new(),
            layout: "2x2".into(),
            reserve_chat: false,
            col_weights: Vec::new(),
            row_weights: Vec::new(),
            main_frac: 2.0 / 3.0,
            stack_weights: [1.0, 1.0, 1.0],
            main_side: "left".into(),
        }
    }
}

fn dock() -> &'static Mutex<DockConfig> {
    static D: OnceLock<Mutex<DockConfig>> = OnceLock::new();
    D.get_or_init(|| Mutex::new(DockConfig::default()))
}

static APPLY_LAYOUT: OnceLock<Mutex<Option<fn()>>> = OnceLock::new();
static FRACTION_EMIT: OnceLock<Mutex<Option<fn(f64)>>> = OnceLock::new();
static GRIP_THREAD_STARTED: AtomicBool = AtomicBool::new(false);
/// After a monitor change, ask apply to raise mpv + chat to the foreground.
static RAISE_AFTER_APPLY: AtomicBool = AtomicBool::new(false);

pub fn take_raise_after_apply() -> bool {
    RAISE_AFTER_APPLY.swap(false, Ordering::SeqCst)
}

fn request_raise_after_apply() {
    RAISE_AFTER_APPLY.store(true, Ordering::SeqCst);
}

pub fn register_apply_layout(f: fn()) {
    *APPLY_LAYOUT
        .get_or_init(|| Mutex::new(None))
        .lock()
        .unwrap_or_else(|e| e.into_inner()) = Some(f);
}

pub fn register_fraction_emit(f: fn(f64)) {
    *FRACTION_EMIT
        .get_or_init(|| Mutex::new(None))
        .lock()
        .unwrap_or_else(|e| e.into_inner()) = Some(f);
}

/// Start the Windows grip/hotkey thread (idempotent). Call once from app setup.
pub fn start_background() {
    ensure_grip_thread();
}

fn run_apply() {
    if let Ok(guard) = APPLY_LAYOUT.get_or_init(|| Mutex::new(None)).lock() {
        if let Some(f) = *guard {
            f();
        }
    }
}

fn emit_fraction(f: f64) {
    if let Ok(guard) = FRACTION_EMIT.get_or_init(|| Mutex::new(None)).lock() {
        if let Some(cb) = *guard {
            cb(f);
        }
    }
}

pub fn clamp_chat_fraction(f: f64) -> f64 {
    f.clamp(MIN_CHAT_FRACTION, MAX_CHAT_FRACTION)
}

pub fn snapshot() -> DockConfig {
    dock().lock().map(|g| g.clone()).unwrap_or_default()
}

pub fn set_linked(enabled: bool) {
    if let Ok(mut g) = dock().lock() {
        g.linked = enabled;
    }
    ensure_grip_thread();
    #[cfg(windows)]
    post_cmd(DockCmd::Sync);
    #[cfg(not(windows))]
    let _ = enabled;
    run_apply();
}

pub fn set_chat_fraction(f: f64) {
    let f = clamp_chat_fraction(f);
    if let Ok(mut g) = dock().lock() {
        g.chat_fraction = f;
    }
    emit_fraction(f);
    run_apply();
    #[cfg(windows)]
    post_cmd(DockCmd::Sync);
}

pub fn chat_fraction() -> f64 {
    dock()
        .lock()
        .map(|g| clamp_chat_fraction(g.chat_fraction))
        .unwrap_or(DEFAULT_CHAT_FRACTION)
}

/// Update where the large pane sits for 2+1 / 3+1. Does not retile by itself —
/// call `run_apply` / `layout_watching` after.
pub fn set_main_side(side: &str) {
    let side = match side {
        "right" | "top" | "bottom" => side,
        _ => "left",
    };
    if let Ok(mut g) = dock().lock() {
        if g.main_side == side {
            return;
        }
        g.main_side = side.to_string();
        g.main_frac = 2.0 / 3.0;
        g.stack_weights = [1.0, 1.0, 1.0];
    }
}

/// Show Windows-style monitor numbers and pick one (replaces drag-to-move).
pub fn open_monitor_picker() {
    #[cfg(windows)]
    post_cmd(DockCmd::PickMonitor);
}
pub fn cycle_monitor() {
    // Prefer the identify-style picker over silent cycling.
    open_monitor_picker();
}

#[allow(dead_code)]
pub fn cycle_monitor_by(delta: i32) {
    #[cfg(windows)]
    {
        let n = list_monitor_work().len() as i32;
        if n <= 0 {
            return;
        }
        if let Ok(mut g) = dock().lock() {
            let cur = g.monitor_index as i32;
            let next = (cur + delta).rem_euclid(n) as usize;
            g.monitor_index = next;
        }
        request_raise_after_apply();
        run_apply();
        post_cmd(DockCmd::Sync);
    }
    #[cfg(not(windows))]
    {
        let _ = delta;
    }
}

pub fn active_work_rect() -> Option<Rect> {
    #[cfg(windows)]
    {
        let monitors = list_monitor_work();
        if monitors.is_empty() {
            return primary_work();
        }
        let idx = dock()
            .lock()
            .map(|g| g.monitor_index.min(monitors.len() - 1))
            .unwrap_or(0);
        Some(monitors[idx])
    }
    #[cfg(not(windows))]
    {
        None
    }
}

/// Split active work area into video (left) + optional chat (right).
pub fn chat_video_split(reserve_chat: bool) -> Option<(Rect, Option<Rect>)> {
    let work = active_work_rect()?;
    if !reserve_chat {
        return Some((work, None));
    }
    let frac = chat_fraction();
    let chat_w = ((work.width() as f64) * frac).round() as i32;
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
    Some((video, Some(chat)))
}
