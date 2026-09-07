//! Windows display-language detection and startup locale resolution.

use explorer_i18n::{AppLocale, Catalog};

/// Process override used by uitest and fixtures. Invalid values are ignored.
pub const LOCALE_ENV_VAR: &str = "SUPEREXPLORER_LOCALE";

/// Resolves the active UI locale.
///
/// Order: valid `env_override` → `session_locale` `Some` → Windows negotiate → `En`.
#[must_use]
pub fn resolve_app_locale(
    env_override: Option<&str>,
    session_locale: Option<AppLocale>,
    windows_tag: Option<&str>,
) -> AppLocale {
    if let Some(raw) = env_override.map(str::trim).filter(|value| !value.is_empty()) {
        match AppLocale::from_bcp47(raw) {
            Some(locale) => return locale,
            None => tracing::warn!(
                override = raw,
                env = LOCALE_ENV_VAR,
                "ignoring invalid SUPEREXPLORER_LOCALE; continuing with session/Windows"
            ),
        }
    }
    if let Some(locale) = session_locale {
        return locale;
    }
    windows_tag
        .map(AppLocale::negotiate)
        .unwrap_or(AppLocale::En)
}

/// Reads [`LOCALE_ENV_VAR`] and resolves against the session preference and Windows tag.
#[must_use]
pub fn resolve_app_locale_from_process(
    session_locale: Option<AppLocale>,
    windows_tag: Option<&str>,
) -> AppLocale {
    let env_override = std::env::var(LOCALE_ENV_VAR).ok();
    resolve_app_locale(env_override.as_deref(), session_locale, windows_tag)
}

/// Catalog for app-owned prompts that run outside a live Explorer window.
#[must_use]
pub fn live_catalog() -> Catalog {
    Catalog::new(resolve_app_locale_from_process(
        None,
        windows_display_locale_tag().as_deref(),
    ))
}

/// Returns the Windows user-default locale name (`GetUserDefaultLocaleName`), if available.
#[must_use]
#[cfg(windows)]
pub fn windows_display_locale_tag() -> Option<String> {
    windows_display_locale_tag_impl()
}

#[must_use]
#[cfg(not(windows))]
pub fn windows_display_locale_tag() -> Option<String> {
    None
}

#[cfg(windows)]
#[allow(
    unsafe_code,
    reason = "reading the Windows display language requires GetUserDefaultLocaleName"
)]
fn windows_display_locale_tag_impl() -> Option<String> {
    use windows::Win32::{
        Globalization::GetUserDefaultLocaleName, System::SystemServices::LOCALE_NAME_MAX_LENGTH,
    };

    let mut buffer = [0u16; LOCALE_NAME_MAX_LENGTH as usize];
    // SAFETY: `buffer` is a writable LOCALE_NAME_MAX_LENGTH UTF-16 array owned for the
    // synchronous GetUserDefaultLocaleName call; the returned length includes the NUL.
    let written = unsafe { GetUserDefaultLocaleName(&mut buffer) };
    if written <= 1 {
        return None;
    }
    let len = usize::try_from(written).ok()?.saturating_sub(1);
    let tag = String::from_utf16(&buffer[..len]).ok()?;
    if tag.is_empty() {
        None
    } else {
        Some(tag)
    }
}

#[cfg(test)]
mod tests {
    use explorer_i18n::AppLocale;

    use super::resolve_app_locale;

    #[test]
    fn env_wins_over_session_and_windows() {
        assert_eq!(
            resolve_app_locale(Some("zh-CN"), Some(AppLocale::Ru), Some("ja")),
            AppLocale::ZhCn
        );
    }

    #[test]
    fn session_some_wins_over_windows_when_env_unset() {
        assert_eq!(
            resolve_app_locale(None, Some(AppLocale::Ru), Some("ja")),
            AppLocale::Ru
        );
    }

    #[test]
    fn session_none_negotiates_windows_tag() {
        assert_eq!(
            resolve_app_locale(None, None, Some("zh-HK")),
            AppLocale::ZhTw
        );
    }

    #[test]
    fn invalid_env_is_ignored() {
        assert_eq!(
            resolve_app_locale(Some("klingon"), Some(AppLocale::Ru), Some("ja")),
            AppLocale::Ru
        );
        assert_eq!(
            resolve_app_locale(Some("not-a-locale"), None, Some("ja")),
            AppLocale::Ja
        );
        assert_eq!(
            resolve_app_locale(Some("   "), None, None),
            AppLocale::En
        );
    }
}
