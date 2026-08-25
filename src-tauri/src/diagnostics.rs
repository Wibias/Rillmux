//! Local logs, panic reports, and an opt-in debug console.

use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, OnceLock};
use std::time::{SystemTime, UNIX_EPOCH};

const MAX_LOG_BYTES: u64 = 10 * 1024 * 1024;
static DEBUG: AtomicBool = AtomicBool::new(false);
#[cfg(windows)]
static OWNED_CONSOLE: AtomicBool = AtomicBool::new(false);

fn log_write_lock() -> &'static Mutex<()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
}

pub fn debug_enabled() -> bool {
    DEBUG.load(Ordering::SeqCst)
}

/// Returns true when `DEBUG` actually flipped, so callers can skip side effects.
fn swap_debug_flag(on: bool) -> bool {
    DEBUG.swap(on, Ordering::SeqCst) != on
}

pub fn set_debug_enabled(on: bool) {
    if !swap_debug_flag(on) {
        return;
    }
    #[cfg(windows)]
    toggle_console(on);
    if on {
        ensure_dirs();
        log_debug("debug mode on");
        log_debug(&format!(
            "log file: {}",
            logs_dir().join("rillmux.log").display()
        ));
    }
}

pub fn app_data_dir() -> PathBuf {
    let root = std::env::var_os("APPDATA")
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir);
    let dest = root.join(crate::branding::APP_DATA_FOLDER);
    let leftover = root.join(crate::branding::APP_DATA_FOLDER_PACKAGE);
    if !dest.exists() && leftover.exists() {
        let _ = fs::rename(&leftover, &dest);
    }
    dest
}

pub fn logs_dir() -> PathBuf {
    app_data_dir().join("logs")
}

pub fn crashes_dir() -> PathBuf {
    app_data_dir().join("crashes")
}

pub fn ensure_dirs() {
    let _ = fs::create_dir_all(logs_dir());
    let _ = fs::create_dir_all(crashes_dir());
}

pub fn install_hooks() {
    ensure_dirs();
    let debug_cli = std::env::args().any(|a| a == "--debug")
        || std::env::var("RILLMUX_DEBUG")
            .map(|v| v != "0" && !v.is_empty())
            .unwrap_or(false);
    if debug_cli {
        set_debug_enabled(true);
    }
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        write_panic_report(info);
        previous(info);
    }));
    #[cfg(windows)]
    install_minidump_filter();
}

pub fn log_debug(msg: &str) {
    if !debug_enabled() {
        return;
    }
    log_line(msg);
}

fn rotate_log_if_needed(path: &Path, incoming_bytes: u64) {
    let current = fs::metadata(path).map(|metadata| metadata.len()).unwrap_or(0);
    if current.saturating_add(incoming_bytes) <= MAX_LOG_BYTES {
        return;
    }
    let rotated = path.with_file_name("rillmux.log.1");
    let _ = fs::remove_file(&rotated);
    if fs::rename(path, &rotated).is_err() {
        let _ = fs::remove_file(path);
    }
}

/// Always append to `rillmux.log` (monitor/chat placement traces), rotating
/// one previous generation before the file grows beyond 10 MiB.
pub fn log_line(msg: &str) {
    ensure_dirs();
    let line = format!("{msg}\n");
    write_debug_output(line.as_bytes());
    let _guard = log_write_lock()
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    let path = logs_dir().join("rillmux.log");
    rotate_log_if_needed(&path, line.len() as u64);
    if let Ok(mut file) = OpenOptions::new().create(true).append(true).open(path) {
        let _ = file.write_all(line.as_bytes());
    }
}

fn write_debug_output(bytes: &[u8]) {
    #[cfg(windows)]
    {
        if OpenOptions::new()
            .write(true)
            .open("CONOUT$")
            .and_then(|mut con| con.write_all(bytes))
            .is_ok()
        {
            return;
        }
    }
    let _ = std::io::stderr().write_all(bytes);
}

fn write_panic_report(info: &std::panic::PanicHookInfo<'_>) {
    ensure_dirs();
    let ts = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let path = crashes_dir().join(format!("panic-{ts}.txt"));
    let backtrace = std::backtrace::Backtrace::force_capture();
    let body = format!("Rillmux panic\n{info}\n\n{backtrace}\n");
    if let Ok(mut file) = fs::File::create(&path) {
        let _ = file.write_all(body.as_bytes());
    }
    eprintln!("{body}");
}

#[cfg(windows)]
fn toggle_console(on: bool) {
    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn AllocConsole() -> i32;
        fn FreeConsole() -> i32;
        fn GetConsoleWindow() -> *mut core::ffi::c_void;
        fn SetStdHandle(kind: u32, handle: *mut core::ffi::c_void) -> i32;
        fn SetConsoleTitleW(title: *const u16) -> i32;
        fn CreateFileW(
            name: *const u16,
            access: u32,
            share: u32,
            sec: *mut core::ffi::c_void,
            create: u32,
            flags: u32,
            template: *mut core::ffi::c_void,
        ) -> *mut core::ffi::c_void;
    }
    const GENERIC_READ: u32 = 0x8000_0000;
    const GENERIC_WRITE: u32 = 0x4000_0000;
    const FILE_SHARE_READ: u32 = 1;
    const FILE_SHARE_WRITE: u32 = 2;
    const OPEN_EXISTING: u32 = 3;
    const STD_INPUT_HANDLE: u32 = 0xFFFF_FFF6;
    const STD_OUTPUT_HANDLE: u32 = 0xFFFF_FFF5;
    const STD_ERROR_HANDLE: u32 = 0xFFFF_FFF4;
    unsafe {
        if on {
            if GetConsoleWindow().is_null() && AllocConsole() != 0 {
                OWNED_CONSOLE.store(true, Ordering::SeqCst);
                let conout: Vec<u16> = "CONOUT$\0".encode_utf16().collect();
                let conin: Vec<u16> = "CONIN$\0".encode_utf16().collect();
                let out = CreateFileW(
                    conout.as_ptr(),
                    GENERIC_READ | GENERIC_WRITE,
                    FILE_SHARE_READ | FILE_SHARE_WRITE,
                    std::ptr::null_mut(),
                    OPEN_EXISTING,
                    0,
                    std::ptr::null_mut(),
                );
                let input = CreateFileW(
                    conin.as_ptr(),
                    GENERIC_READ | GENERIC_WRITE,
                    FILE_SHARE_READ | FILE_SHARE_WRITE,
                    std::ptr::null_mut(),
                    OPEN_EXISTING,
                    0,
                    std::ptr::null_mut(),
                );
                if !out.is_null() && out != (-1isize as *mut core::ffi::c_void) {
                    let _ = SetStdHandle(STD_OUTPUT_HANDLE, out);
                    let _ = SetStdHandle(STD_ERROR_HANDLE, out);
                }
                if !input.is_null() && input != (-1isize as *mut core::ffi::c_void) {
                    let _ = SetStdHandle(STD_INPUT_HANDLE, input);
                }
                let title: Vec<u16> = "Rillmux debug\0".encode_utf16().collect();
                let _ = SetConsoleTitleW(title.as_ptr());
            }
        } else if OWNED_CONSOLE.swap(false, Ordering::SeqCst) {
            let _ = FreeConsole();
        }
    }
}

#[cfg(windows)]
fn install_minidump_filter() {
    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn SetUnhandledExceptionFilter(
            handler: Option<unsafe extern "system" fn(*mut ExceptionPointers) -> i32>,
        ) -> Option<unsafe extern "system" fn(*mut ExceptionPointers) -> i32>;
    }
    unsafe {
        let _ = SetUnhandledExceptionFilter(Some(unhandled_exception));
    }
}

#[cfg(windows)]
#[repr(C)]
struct ExceptionPointers {
    exception_record: *mut core::ffi::c_void,
    context_record: *mut core::ffi::c_void,
}

#[cfg(windows)]
unsafe extern "system" fn unhandled_exception(info: *mut ExceptionPointers) -> i32 {
    write_minidump(info);
    0
}

#[cfg(windows)]
fn write_minidump(info: *mut ExceptionPointers) {
    ensure_dirs();
    let ts = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let path = crashes_dir().join(format!("crash-{ts}.dmp"));
    let wide: Vec<u16> = path
        .to_string_lossy()
        .encode_utf16()
        .chain(std::iter::once(0))
        .collect();
    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn GetCurrentProcess() -> *mut core::ffi::c_void;
        fn GetCurrentProcessId() -> u32;
        fn CreateFileW(
            name: *const u16,
            access: u32,
            share: u32,
            sec: *mut core::ffi::c_void,
            create: u32,
            flags: u32,
            template: *mut core::ffi::c_void,
        ) -> *mut core::ffi::c_void;
        fn CloseHandle(handle: *mut core::ffi::c_void) -> i32;
    }
    #[link(name = "dbghelp")]
    unsafe extern "system" {
        fn MiniDumpWriteDump(
            process: *mut core::ffi::c_void,
            pid: u32,
            file: *mut core::ffi::c_void,
            dump_type: u32,
            param: *mut MiniDumpExceptionParam,
            user: *mut core::ffi::c_void,
            callback: *mut core::ffi::c_void,
        ) -> i32;
    }
    #[repr(C)]
    struct MiniDumpExceptionParam {
        thread_id: u32,
        exception_pointers: *mut ExceptionPointers,
        client_pointers: i32,
    }
    const GENERIC_WRITE: u32 = 0x4000_0000;
    const CREATE_ALWAYS: u32 = 2;
    const FILE_ATTRIBUTE_NORMAL: u32 = 0x80;
    const MINI_DUMP_WITH_DATA_SEGS: u32 = 0x0001;
    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn GetCurrentThreadId() -> u32;
    }
    unsafe {
        let file = CreateFileW(
            wide.as_ptr(),
            GENERIC_WRITE,
            0,
            std::ptr::null_mut(),
            CREATE_ALWAYS,
            FILE_ATTRIBUTE_NORMAL,
            std::ptr::null_mut(),
        );
        if file.is_null() || file == (-1isize as *mut core::ffi::c_void) {
            return;
        }
        let mut param = MiniDumpExceptionParam {
            thread_id: GetCurrentThreadId(),
            exception_pointers: info,
            client_pointers: 0,
        };
        let _ = MiniDumpWriteDump(
            GetCurrentProcess(),
            GetCurrentProcessId(),
            file,
            MINI_DUMP_WITH_DATA_SEGS,
            if info.is_null() {
                std::ptr::null_mut()
            } else {
                &mut param
            },
            std::ptr::null_mut(),
            std::ptr::null_mut(),
        );
        let _ = CloseHandle(file);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn log_and_crash_dirs_share_appdata_root() {
        assert!(logs_dir().ends_with("logs"));
        assert!(crashes_dir().ends_with("crashes"));
        assert_eq!(logs_dir().parent(), crashes_dir().parent());
        assert!(app_data_dir().ends_with(crate::branding::APP_DATA_FOLDER));
    }

    #[test]
    fn debug_flag_only_reports_a_real_change() {
        DEBUG.store(false, Ordering::SeqCst);
        assert!(swap_debug_flag(true));
        assert!(!swap_debug_flag(true));
        assert!(swap_debug_flag(false));
        assert!(!swap_debug_flag(false));
        DEBUG.store(false, Ordering::SeqCst);
    }

    #[test]
    fn log_rotation_threshold_is_bounded() {
        assert_eq!(MAX_LOG_BYTES, 10 * 1024 * 1024);
        assert!(MAX_LOG_BYTES.saturating_add(1) > MAX_LOG_BYTES);
    }
}
