//! Logs native exceptions that would otherwise terminate the process silently.
#![allow(
    unsafe_code,
    reason = "SetUnhandledExceptionFilter is the Win32 boundary for native crashes"
)]

use std::sync::atomic::{AtomicBool, Ordering};

use explorer_common::{ErrorSeverity, record_process_error_message};

const EXCEPTION_CONTINUE_SEARCH: i32 = 0;

#[repr(C)]
struct ExceptionPointers {
    exception_record: *mut ExceptionRecord,
    context_record: *mut core::ffi::c_void,
}

#[repr(C)]
struct ExceptionRecord {
    exception_code: u32,
    exception_flags: u32,
    nested: *mut ExceptionRecord,
    exception_address: *mut core::ffi::c_void,
    number_parameters: u32,
    exception_information: [usize; 15],
}

#[link(name = "kernel32")]
unsafe extern "system" {
    fn SetUnhandledExceptionFilter(
        filter: Option<unsafe extern "system" fn(*const ExceptionPointers) -> i32>,
    ) -> Option<unsafe extern "system" fn(*const ExceptionPointers) -> i32>;
}

/// Installs a process filter that appends native crash details to `error.log`.
///
/// Rust panics are contained by the panic hook and window-procedure boundary.
/// An unhandled native exception still ends the process after this filter
/// returns; the log is the record of why it ended.
pub fn install_native_crash_log() {
    static INSTALLED: AtomicBool = AtomicBool::new(false);
    if INSTALLED.swap(true, Ordering::AcqRel) {
        return;
    }
    // SAFETY: the filter is a static function with no captured pointers.
    // Windows keeps it for the process lifetime.
    unsafe {
        SetUnhandledExceptionFilter(Some(unhandled_exception_filter));
    }
}

unsafe extern "system" fn unhandled_exception_filter(info: *const ExceptionPointers) -> i32 {
    static LOGGING: AtomicBool = AtomicBool::new(false);
    if LOGGING.swap(true, Ordering::AcqRel) {
        return EXCEPTION_CONTINUE_SEARCH;
    }
    let (code, address) = exception_site(info);
    record_process_error_message(
        ErrorSeverity::Critical,
        "process",
        "unhandled_native_exception",
        &format!("native exception code=0x{code:08X} address=0x{address:X}"),
        Some(file!()),
    );
    EXCEPTION_CONTINUE_SEARCH
}

fn exception_site(info: *const ExceptionPointers) -> (u32, usize) {
    if info.is_null() {
        return (0, 0);
    }
    // SAFETY: a non-null filter argument points at EXCEPTION_POINTERS allocated
    // by the system for this callback. The record pointer is checked.
    let record = unsafe { (*info).exception_record };
    if record.is_null() {
        return (0, 0);
    }
    let record = unsafe { &*record };
    (record.exception_code, record.exception_address as usize)
}
