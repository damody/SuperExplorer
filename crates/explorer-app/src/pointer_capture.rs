//! Audited Win32 mouse capture implementation owned by the application composition root.
#![allow(
    unsafe_code,
    reason = "Win32 SetCapture/GetCapture/ReleaseCapture require an audited HWND boundary"
)]

use std::ffi::c_void;

use explorer_ui::PointerCaptureSession;
use windows::Win32::{
    Foundation::{HWND, POINT},
    Graphics::Gdi::ScreenToClient,
    UI::Input::KeyboardAndMouse::{
        GetAsyncKeyState, GetCapture, ReleaseCapture, SetCapture, VK_RBUTTON,
    },
    UI::WindowsAndMessaging::GetCursorPos,
};

pub(crate) struct NativePointerCapture {
    hwnd: HWND,
}

impl NativePointerCapture {
    pub(crate) fn acquire(hwnd_value: isize) -> Option<Box<dyn PointerCaptureSession>> {
        let hwnd = HWND(hwnd_value as *mut c_void);
        // SAFETY: the HWND identity comes from the live GPUI Window. Capture borrows it and does
        // not transfer ownership.
        unsafe { SetCapture(hwnd) };
        // SAFETY: GetCapture reads process-global input state and transfers no ownership.
        (unsafe { GetCapture() } == hwnd)
            .then(|| Box::new(Self { hwnd }) as Box<dyn PointerCaptureSession>)
    }

    fn release(&mut self) -> bool {
        if !self.is_owned() {
            return false;
        }
        // SAFETY: this session verified that its HWND owns capture. ReleaseCapture transfers no
        // resources and this Drop path is the session's single terminal operation.
        unsafe { ReleaseCapture() }.is_ok()
    }
}

impl PointerCaptureSession for NativePointerCapture {
    fn is_owned(&self) -> bool {
        // SAFETY: GetCapture is a non-owning query and `self.hwnd` is only an identity value.
        (unsafe { GetCapture() }) == self.hwnd
    }

    fn cursor_client_position(&self) -> Option<(f32, f32)> {
        if !self.is_owned() {
            return None;
        }
        let mut point = POINT::default();
        // SAFETY: `point` is valid writable storage and both calls borrow the captured live HWND.
        unsafe {
            GetCursorPos(&raw mut point).ok()?;
            if !ScreenToClient(self.hwnd, &raw mut point).as_bool() {
                return None;
            }
        }
        Some(point_as_f32(point))
    }

    fn secondary_button_pressed(&self) -> bool {
        // SAFETY: GetAsyncKeyState reads process-global input state and transfers no ownership.
        (unsafe { GetAsyncKeyState(i32::from(VK_RBUTTON.0)) }) < 0
    }
}

#[allow(
    clippy::cast_precision_loss,
    reason = "Win32 client coordinates are bounded by practical desktop dimensions and GPUI pointer coordinates are f32"
)]
fn point_as_f32(point: POINT) -> (f32, f32) {
    (point.x as f32, point.y as f32)
}

impl Drop for NativePointerCapture {
    fn drop(&mut self) {
        let _ = self.release();
    }
}
