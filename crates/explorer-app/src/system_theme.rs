//! Windows high-contrast discovery and semantic system-color resolution.

use std::mem::size_of;

use anyhow::{Context as _, Result};
use explorer_ui::{
    UiTokens,
    theme::{Rgba8, SystemColorRole, ThemeTokens},
};
use windows::Win32::{
    Graphics::Gdi::{
        COLOR_BTNFACE, COLOR_GRAYTEXT, COLOR_HIGHLIGHT, COLOR_HIGHLIGHTTEXT, COLOR_HOTLIGHT,
        COLOR_WINDOW, COLOR_WINDOWTEXT, GetSysColor,
    },
    UI::{
        Accessibility::{HCF_HIGHCONTRASTON, HIGHCONTRASTW},
        WindowsAndMessaging::{
            SPI_GETHIGHCONTRAST, SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS, SystemParametersInfoW,
        },
    },
};

/// Returns the active system high-contrast palette, or `None` when Windows
/// high contrast is disabled.
///
/// # Errors
///
/// Returns an error when Windows rejects the high-contrast state query or the
/// platform structure size cannot be represented by the Win32 API.
#[allow(
    unsafe_code,
    reason = "reading Windows high-contrast state requires a synchronous Win32 pointer API"
)]
pub fn high_contrast_tokens() -> Result<Option<UiTokens>> {
    let mut settings = HIGHCONTRASTW {
        cbSize: u32::try_from(size_of::<HIGHCONTRASTW>())
            .context("HIGHCONTRASTW size does not fit u32")?,
        ..Default::default()
    };
    // SAFETY: `settings` is a correctly sized writable HIGHCONTRASTW for the
    // synchronous SPI_GETHIGHCONTRAST call and remains live for its duration.
    unsafe {
        SystemParametersInfoW(
            SPI_GETHIGHCONTRAST,
            settings.cbSize,
            Some(std::ptr::from_mut(&mut settings).cast()),
            SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS::default(),
        )
    }
    .context("query Windows high-contrast state")?;
    if settings.dwFlags.0 & HCF_HIGHCONTRASTON.0 == 0 {
        return Ok(None);
    }
    Ok(Some(UiTokens {
        theme: ThemeTokens::windows_high_contrast(resolve_system_color),
        ..UiTokens::default()
    }))
}

#[allow(
    unsafe_code,
    reason = "resolving a documented value-only Windows system color requires GetSysColor"
)]
fn resolve_system_color(role: SystemColorRole) -> Rgba8 {
    let index = match role {
        SystemColorRole::Window => COLOR_WINDOW,
        SystemColorRole::WindowText => COLOR_WINDOWTEXT,
        SystemColorRole::ButtonFace => COLOR_BTNFACE,
        SystemColorRole::GrayText => COLOR_GRAYTEXT,
        SystemColorRole::Highlight => COLOR_HIGHLIGHT,
        SystemColorRole::HighlightText => COLOR_HIGHLIGHTTEXT,
        SystemColorRole::Hotlight => COLOR_HOTLIGHT,
    };
    // SAFETY: GetSysColor accepts the documented COLOR_* index and returns a
    // value-only COLORREF with no ownership or lifetime requirements.
    let color = unsafe { GetSysColor(index) };
    Rgba8::opaque(
        (color & 0xff) as u8,
        ((color >> 8) & 0xff) as u8,
        ((color >> 16) & 0xff) as u8,
    )
}
