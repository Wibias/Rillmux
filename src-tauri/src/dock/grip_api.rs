// Win32 bindings, message constants, and FFI structs used by the grip thread.

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
    fn SetWindowPos(
        hwnd: *mut core::ffi::c_void,
        after: *mut core::ffi::c_void,
        x: i32,
        y: i32,
        cx: i32,
        cy: i32,
        flags: u32,
    ) -> i32;
    fn TranslateMessage(msg: *const Msg) -> i32;
    fn DispatchMessageW(msg: *const Msg) -> isize;
    fn DefWindowProcW(hwnd: *mut core::ffi::c_void, msg: u32, w: usize, l: isize) -> isize;
    fn SetCapture(hwnd: *mut core::ffi::c_void) -> *mut core::ffi::c_void;
    fn ReleaseCapture() -> i32;
    fn GetCursorPos(pt: *mut Point) -> i32;
    fn LoadCursorW(instance: *mut core::ffi::c_void, name: usize) -> *mut core::ffi::c_void;
    fn SetCursor(cursor: *mut core::ffi::c_void) -> *mut core::ffi::c_void;
    fn FillRect(
        hdc: *mut core::ffi::c_void,
        rect: *const Rect,
        brush: *mut core::ffi::c_void,
    ) -> i32;
    fn BeginPaint(hwnd: *mut core::ffi::c_void, ps: *mut PaintStruct) -> *mut core::ffi::c_void;
    fn EndPaint(hwnd: *mut core::ffi::c_void, ps: *const PaintStruct) -> i32;
    fn GetClientRect(hwnd: *mut core::ffi::c_void, rect: *mut Rect) -> i32;
    fn GetWindowRect(hwnd: *mut core::ffi::c_void, rect: *mut Rect) -> i32;
    fn CreateSolidBrush(color: u32) -> *mut core::ffi::c_void;
    fn DeleteObject(obj: *mut core::ffi::c_void) -> i32;
    fn SetWindowLongPtrW(hwnd: *mut core::ffi::c_void, index: i32, value: isize) -> isize;
    fn GetWindowLongPtrW(hwnd: *mut core::ffi::c_void, index: i32) -> isize;
    fn IsWindow(hwnd: *mut core::ffi::c_void) -> i32;
    fn PeekMessageW(
        msg: *mut Msg,
        hwnd: *mut core::ffi::c_void,
        min: u32,
        max: u32,
        remove: u32,
    ) -> i32;
    fn SetLayeredWindowAttributes(
        hwnd: *mut core::ffi::c_void,
        key: u32,
        alpha: u8,
        flags: u32,
    ) -> i32;
    fn TrackMouseEvent(tme: *mut TrackMouseEvent) -> i32;
    fn InvalidateRect(hwnd: *mut core::ffi::c_void, rect: *const Rect, erase: i32) -> i32;
    fn SetTextColor(hdc: *mut core::ffi::c_void, color: u32) -> u32;
    fn SetBkMode(hdc: *mut core::ffi::c_void, mode: i32) -> i32;
    fn DrawTextW(
        hdc: *mut core::ffi::c_void,
        text: *const u16,
        count: i32,
        rect: *mut Rect,
        format: u32,
    ) -> i32;
    fn RegisterHotKey(hwnd: *mut core::ffi::c_void, id: i32, modifiers: u32, vk: u32) -> i32;
    fn GetAsyncKeyState(vk: i32) -> i16;
}
#[link(name = "gdi32")]
unsafe extern "system" {
    fn CreateFontW(
        height: i32,
        width: i32,
        escapement: i32,
        orientation: i32,
        weight: i32,
        italic: u32,
        underline: u32,
        strike_out: u32,
        char_set: u32,
        out_precision: u32,
        clip_precision: u32,
        quality: u32,
        pitch_and_family: u32,
        face: *const u16,
    ) -> *mut core::ffi::c_void;
    fn SelectObject(
        hdc: *mut core::ffi::c_void,
        obj: *mut core::ffi::c_void,
    ) -> *mut core::ffi::c_void;
}
#[link(name = "kernel32")]
unsafe extern "system" {
    fn GetModuleHandleW(name: *const u16) -> *mut core::ffi::c_void;
}

#[repr(C)]
struct TrackMouseEvent {
    cb_size: u32,
    dw_flags: u32,
    hwnd: *mut core::ffi::c_void,
    hover_time: u32,
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
#[repr(C)]
struct PaintStruct {
    hdc: *mut core::ffi::c_void,
    erase: i32,
    rc: Rect,
    restore: i32,
    inc_update: i32,
    rgb: [u8; 32],
}
#[repr(C)]
struct WndClassEx {
    cb_size: u32,
    style: u32,
    wnd_proc: Option<unsafe extern "system" fn(*mut core::ffi::c_void, u32, usize, isize) -> isize>,
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

const WM_PAINT: u32 = 0x000F;
const WM_DESTROY: u32 = 0x0002;
const WM_LBUTTONDOWN: u32 = 0x0201;
const WM_LBUTTONUP: u32 = 0x0202;
const WM_MOUSEMOVE: u32 = 0x0200;
const WM_MOUSELEAVE: u32 = 0x02A3;
const WM_SETCURSOR: u32 = 0x0020;
const WM_HOTKEY: u32 = 0x0312;
const WS_POPUP: u32 = 0x8000_0000;
const WS_VISIBLE: u32 = 0x1000_0000;
// Regular grips always stay in the normal Z-order. Only the brief monitor
// identify overlays are TOPMOST.
const WS_EX_TOOLWINDOW: u32 = 0x0000_0080;
const WS_EX_NOACTIVATE: u32 = 0x0800_0000;
const WS_EX_LAYERED: u32 = 0x0008_0000;
/// Only monitor-number overlays use permanent TOPMOST (brief picker UX).
const WS_EX_TOPMOST: u32 = 0x0000_0008;
const HWND_NOTOPMOST: isize = -2;
const SWP_NOACTIVATE: u32 = 0x0010;
const SWP_SHOWWINDOW: u32 = 0x0040;
const SWP_NOMOVE: u32 = 0x0002;
const SWP_NOSIZE: u32 = 0x0001;
const SWP_NOZORDER: u32 = 0x0004;
const SW_SHOWNOACTIVATE: i32 = 4;
const IDC_SIZEWE: usize = 32644;
const IDC_SIZENS: usize = 32645;
const IDC_HAND: usize = 32649;
const GWLP_USERDATA: i32 = -21;
const PM_REMOVE: u32 = 0x0001;
const LWA_ALPHA: u32 = 0x0000_0002;
const TME_LEAVE: u32 = 0x0000_0002;
const DT_CENTER: u32 = 0x0000_0001;
const DT_VCENTER: u32 = 0x0000_0004;
const DT_SINGLELINE: u32 = 0x0000_0020;
const TRANSPARENT: i32 = 1;
const MOD_CONTROL: u32 = 0x0002;
const MOD_SHIFT: u32 = 0x0004;
const MOD_NOREPEAT: u32 = 0x4000;
const HOTKEY_CYCLE: i32 = 1;
const VK_M: u32 = 0x4D;
const VK_ESCAPE: i32 = 0x1B;
const FW_BOLD: i32 = 700;
/// ~15% visible at rest, ~55% on hover (user asked ~85% transparent).
const ALPHA_REST: u8 = 38;
const ALPHA_HOVER: u8 = 140;
const ALPHA_IDENTIFY: u8 = 210;
