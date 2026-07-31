//! Platform-neutral pointer-capture boundary used by GPUI drag interactions.

use std::sync::Arc;

use gpui::Window;
use raw_window_handle::{HasWindowHandle, RawWindowHandle};

/// An acquired native pointer-capture session.
pub trait PointerCaptureSession {
    /// Whether this session still owns native capture.
    fn is_owned(&self) -> bool;

    /// Current cursor position in signed window-client coordinates.
    fn cursor_client_position(&self) -> Option<(f32, f32)>;

    /// Whether the physical secondary mouse button is currently pressed.
    fn secondary_button_pressed(&self) -> bool;
}

/// Composition-root factory for acquiring native capture from an HWND identity.
pub type PointerCaptureFactory =
    Arc<dyn Fn(isize) -> Option<Box<dyn PointerCaptureSession>> + Send + Sync>;

pub(crate) fn window_handle_value(window: &Window) -> Option<isize> {
    let raw = HasWindowHandle::window_handle(window).ok()?.as_raw();
    let RawWindowHandle::Win32(handle) = raw else {
        return None;
    };
    Some(handle.hwnd.get())
}
