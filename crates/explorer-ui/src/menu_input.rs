//! Use English menu mnemonics without installing or changing system input methods.
#![allow(
    unsafe_code,
    reason = "the menu boundary updates this thread's Win32 input context"
)]

use windows::Win32::{
    Foundation::HWND,
    UI::Input::{
        Ime::{
            IME_CONVERSION_MODE, IME_SENTENCE_MODE, ImmGetContext, ImmGetConversionStatus,
            ImmGetOpenStatus, ImmReleaseContext, ImmSetConversionStatus, ImmSetOpenStatus,
        },
        KeyboardAndMouse::{
            ActivateKeyboardLayout, GetKeyboardLayout, GetKeyboardLayoutList, HKL, KLF_ACTIVATE,
        },
    },
};

pub(crate) struct MenuInputGuard {
    owner: u64,
    layout: usize,
    ime: Option<(bool, Option<(IME_CONVERSION_MODE, IME_SENTENCE_MODE)>)>,
}

impl Drop for MenuInputGuard {
    fn drop(&mut self) {
        // SAFETY: the guard is created and released on the window's UI thread.
        let _ = unsafe { ActivateKeyboardLayout(HKL(self.layout as *mut _), KLF_ACTIVATE) };
        if let Some((open, conversion)) = self.ime {
            let hwnd = HWND(self.owner as usize as *mut _);
            let context = unsafe { ImmGetContext(hwnd) };
            if !context.0.is_null() {
                unsafe {
                    if let Some((mode, sentence)) = conversion {
                        let _ = ImmSetConversionStatus(context, mode, sentence);
                    }
                    let _ = ImmSetOpenStatus(context, open);
                    let _ = ImmReleaseContext(hwnd, context);
                }
            }
        }
    }
}

pub(crate) fn prepare(owner: u64) -> MenuInputGuard {
    let mut guard = MenuInputGuard {
        owner,
        layout: unsafe { GetKeyboardLayout(0) }.0 as usize,
        ime: None,
    };
    if owner != 0 {
        let hwnd = HWND(owner as usize as *mut _);
        // Capture the original IME before changing the keyboard layout.
        let context = unsafe { ImmGetContext(hwnd) };
        if !context.0.is_null() {
            let mut mode = IME_CONVERSION_MODE::default();
            let mut sentence = IME_SENTENCE_MODE::default();
            let conversion =
                unsafe { ImmGetConversionStatus(context, Some(&mut mode), Some(&mut sentence)) }
                    .as_bool();
            guard.ime = Some((
                unsafe { ImmGetOpenStatus(context) }.as_bool(),
                conversion.then_some((mode, sentence)),
            ));
            let _ = unsafe { ImmReleaseContext(hwnd, context) };
        }
    }
    // SAFETY: the first call obtains the count; the second fills the live slice.
    let count = unsafe { GetKeyboardLayoutList(None) }.clamp(0, 256) as usize;
    let mut layouts = vec![HKL::default(); count];
    let count = unsafe { GetKeyboardLayoutList(Some(&mut layouts)) }.max(0) as usize;
    if let Some(layout) = layouts.iter().take(count).find(|layout| {
        // The low LANGID's primary language identifies all English variants.
        (layout.0 as usize & 0x3ff) == 0x09
    }) {
        // SAFETY: the HKL was returned by Windows; only the calling UI thread changes.
        let _ = unsafe { ActivateKeyboardLayout(*layout, KLF_ACTIVATE) };
    }
    if owner != 0 {
        let hwnd = HWND(owner as usize as *mut core::ffi::c_void);
        // SAFETY: an invalid/stale HWND simply has no input context. Every acquired
        // context is released, including when an English keyboard was unavailable.
        let context = unsafe { ImmGetContext(hwnd) };
        if !context.0.is_null() {
            unsafe {
                let _ = ImmSetOpenStatus(context, false);
                let _ = ImmReleaseContext(hwnd, context);
            }
        }
    }
    guard
}

#[cfg(test)]
mod tests {
    #[test]
    fn menu_uses_an_installed_english_layout_on_its_own_thread() {
        std::thread::spawn(|| {
            use windows::Win32::UI::Input::KeyboardAndMouse::*;
            // SAFETY: this test owns the thread and restores its original layout.
            let cleanup = super::MenuInputGuard {
                owner: 0,
                layout: unsafe { GetKeyboardLayout(0) }.0 as usize,
                ime: None,
            };
            let mut layouts = [HKL::default(); 256];
            let count = unsafe { GetKeyboardLayoutList(Some(&mut layouts)) }.max(0) as usize;
            if let Some(layout) = layouts
                .iter()
                .take(count)
                .find(|layout| (layout.0 as usize & 0x3ff) != 9)
            {
                let _ = unsafe { ActivateKeyboardLayout(*layout, KLF_ACTIVATE) };
            }
            let original = unsafe { GetKeyboardLayout(0) };
            let guard = super::prepare(0);
            if layouts
                .iter()
                .take(count)
                .any(|layout| (layout.0 as usize & 0x3ff) == 9)
            {
                assert_eq!(unsafe { GetKeyboardLayout(0) }.0 as usize & 0x3ff, 9);
            }
            drop(guard);
            assert_eq!(unsafe { GetKeyboardLayout(0) }, original);
            drop(cleanup);
        })
        .join()
        .unwrap();
    }
}
