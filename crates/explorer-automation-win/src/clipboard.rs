//! Read-only Unicode clipboard adapter.

#![allow(unsafe_code)]

use explorer_automation::{AutomationError, AutomationErrorKind, AutomationFuture, ClipboardHost};
use windows::Win32::{
    Foundation::HGLOBAL,
    System::{
        DataExchange::{
            CloseClipboard, GetClipboardData, IsClipboardFormatAvailable, OpenClipboard,
        },
        Memory::{GlobalLock, GlobalSize, GlobalUnlock},
        Ole::CF_UNICODETEXT,
    },
};

#[derive(Clone, Copy, Debug, Default)]
pub struct WindowsClipboardHost;

impl ClipboardHost for WindowsClipboardHost {
    fn read_text(&self) -> AutomationFuture<Option<String>> {
        Box::pin(async { read_clipboard_text() })
    }
}

struct ClipboardGuard;

impl Drop for ClipboardGuard {
    fn drop(&mut self) {
        // SAFETY: this guard exists only after a successful OpenClipboard call.
        let _ = unsafe { CloseClipboard() };
    }
}

fn read_clipboard_text() -> Result<Option<String>, AutomationError> {
    let format = u32::from(CF_UNICODETEXT.0);
    // SAFETY: this only queries availability and does not retain pointers.
    if unsafe { IsClipboardFormatAvailable(format) }.is_err() {
        return Ok(None);
    }
    // SAFETY: no owner HWND is required for a read; guard closes on every exit path.
    unsafe { OpenClipboard(None) }.map_err(|_| clipboard_error())?;
    let _guard = ClipboardGuard;
    // SAFETY: clipboard is open and the requested format was reported available.
    let handle = unsafe { GetClipboardData(format) }.map_err(|_| clipboard_error())?;
    let global = HGLOBAL(handle.0);
    // SAFETY: handle belongs to the open clipboard; size bounds the temporary slice.
    let byte_size = unsafe { GlobalSize(global) };
    if byte_size < 2 {
        return Ok(Some(String::new()));
    }
    // SAFETY: handle belongs to the open clipboard and remains locked until conversion completes.
    let pointer = unsafe { GlobalLock(global) }.cast::<u16>();
    if pointer.is_null() {
        return Err(clipboard_error());
    }
    let units = byte_size / 2;
    // SAFETY: GlobalSize provides the allocation bound and pointer is locked above.
    let slice = unsafe { std::slice::from_raw_parts(pointer, units) };
    let length = slice.iter().position(|unit| *unit == 0).unwrap_or(units);
    let text = String::from_utf16(&slice[..length]).map_err(|_| clipboard_error());
    // SAFETY: this balances the successful GlobalLock call.
    let _ = unsafe { GlobalUnlock(global) };
    text.map(Some)
}

fn clipboard_error() -> AutomationError {
    AutomationError::new(
        AutomationErrorKind::Unavailable,
        "clipboard.read_text",
        true,
        "Clipboard text is currently unavailable",
    )
}
