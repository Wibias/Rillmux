// Grip identity and drag/hover state for the dock divider windows.

#[derive(Clone, Copy)]
enum GripKind {
    Chat,
    Move,
    IndependentChat(usize),
    IndependentMove(usize),
    Col(usize),
    Row(usize),
    Main,
    Stack(usize),
    Identify(usize),
}

struct DragState {
    kind: GripKind,
    start_x: i32,
    start_y: i32,
    #[allow(dead_code)]
    start_frac: f64,
    #[allow(dead_code)]
    start_main: f64,
    start_cols: Vec<f64>,
    start_rows: Vec<f64>,
    start_stack: [f64; 3],
    #[allow(dead_code)]
    start_monitor: usize,
    moved: bool,
}

static DRAG: OnceLock<Mutex<Option<DragState>>> = OnceLock::new();
static MOVER_HOVER: AtomicBool = AtomicBool::new(false);
static DIVIDER_HOVER: OnceLock<Mutex<Option<isize>>> = OnceLock::new();
fn divider_hover() -> &'static Mutex<Option<isize>> {
    DIVIDER_HOVER.get_or_init(|| Mutex::new(None))
}
fn kind_from_code(n: isize) -> GripKind {
    match n {
        1 => GripKind::Chat,
        2 => GripKind::Main,
        10 => GripKind::Move,
        n if (400..500).contains(&n) => GripKind::IndependentChat((n - 400) as usize),
        n if (500..600).contains(&n) => GripKind::IndependentMove((n - 500) as usize),
        n if (100..200).contains(&n) => GripKind::Col((n - 100) as usize),
        n if (200..300).contains(&n) => GripKind::Row((n - 200) as usize),
        n if (300..400).contains(&n) => GripKind::Stack((n - 300) as usize),
        n if (1000..1100).contains(&n) => GripKind::Identify((n - 1000) as usize),
        _ => GripKind::Chat,
    }
}
fn divider_kind(kind: GripKind) -> bool {
    matches!(
        kind,
        GripKind::Chat
            | GripKind::IndependentChat(_)
            | GripKind::Col(_)
            | GripKind::Row(_)
            | GripKind::Main
            | GripKind::Stack(_)
    )
}
fn divider_is_vertical(kind: GripKind) -> bool {
    let side = snapshot().main_side;
    match kind {
        GripKind::Chat | GripKind::IndependentChat(_) | GripKind::Col(_) => true,
        GripKind::Row(_) => false,
        GripKind::Main => side != "top" && side != "bottom",
        GripKind::Stack(_) => side == "top" || side == "bottom",
        _ => true,
    }
}
unsafe fn resize_divider_hwnd(hwnd: *mut core::ffi::c_void, kind: GripKind, hover: bool) {
    if hwnd.is_null() {
        return;
    }
    let mut rc = Rect::default();
    if GetWindowRect(hwnd, &mut rc) == 0 {
        return;
    }
    let thick = divider_thickness(hover);
    let next = if divider_is_vertical(kind) {
        let seam = rc.left + rc.width() / 2;
        vertical_divider_rect(seam, rc.top, rc.bottom, thick)
    } else {
        let seam = rc.top + rc.height() / 2;
        horizontal_divider_rect(seam, rc.left, rc.right, thick)
    };
    place_grip(hwnd, next);
}
fn divider_is_hovered(hwnd: *mut core::ffi::c_void) -> bool {
    divider_hover()
        .lock()
        .map(|g| *g == Some(hwnd as isize))
        .unwrap_or(false)
}
unsafe fn place_divider_grip(hwnd: *mut core::ffi::c_void, rest: Rect) {
    if hwnd.is_null() {
        return;
    }
    let kind = kind_from_code(GetWindowLongPtrW(hwnd, GWLP_USERDATA));
    if divider_kind(kind) && divider_is_hovered(hwnd) {
        let thick = divider_thickness(true);
        let next = if divider_is_vertical(kind) {
            let seam = rest.left + rest.width() / 2;
            vertical_divider_rect(seam, rest.top, rest.bottom, thick)
        } else {
            let seam = rest.top + rest.height() / 2;
            horizontal_divider_rect(seam, rest.left, rest.right, thick)
        };
        place_grip(hwnd, next);
    } else {
        place_grip(hwnd, rest);
    }
}
static PICKER_OPEN: AtomicBool = AtomicBool::new(false);
fn drag() -> &'static Mutex<Option<DragState>> {
    DRAG.get_or_init(|| Mutex::new(None))
}
