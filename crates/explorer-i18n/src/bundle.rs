//! Embedded Fluent catalogs and lookup helpers.

use std::borrow::Cow;
use std::collections::HashMap;

use fluent_bundle::{FluentArgs, FluentValue};
use fluent_templates::Loader;
use unic_langid::{langid, LanguageIdentifier};

use crate::locale::AppLocale;

fluent_templates::static_loader! {
    static LOCALES = {
        locales: "./locales",
        fallback_language: "en",
        // Isolating marks stay on in production; tests disable them for readable asserts.
        customise: |bundle| {
            if cfg!(test) {
                bundle.set_use_isolating(false);
            }
        },
    };
}

/// Thin wrapper around the embedded Fluent loader for one active locale.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub struct Catalog {
    locale: AppLocale,
}

impl Catalog {
    /// Creates a catalog bound to `locale`.
    #[must_use]
    pub const fn new(locale: AppLocale) -> Self {
        Self { locale }
    }

    /// Returns the active locale.
    #[must_use]
    pub const fn locale(self) -> AppLocale {
        self.locale
    }

    /// Returns a catalog using `locale` instead.
    #[must_use]
    pub const fn with_locale(self, locale: AppLocale) -> Self {
        Self { locale }
    }

    /// Looks up `key` for the active locale, then English, then returns `key`.
    #[must_use]
    pub fn t(self, key: &str) -> String {
        self.resolve(key, None)
    }

    /// Looks up `key` with Fluent arguments, using the same fallback chain as [`Self::t`].
    #[must_use]
    pub fn t_args(self, key: &str, args: &FluentArgs<'_>) -> String {
        self.resolve(key, Some(args))
    }

    fn resolve(self, key: &str, args: Option<&FluentArgs<'_>>) -> String {
        let lang = language_id(self.locale);
        let owned_args = args.map(fluent_args_to_map);
        let found = match owned_args.as_ref() {
            Some(map) => LOCALES.try_lookup_with_args(&lang, key, map),
            None => LOCALES.try_lookup(&lang, key),
        };
        if let Some(value) = found {
            return value;
        }

        tracing::warn!(
            key,
            locale = self.locale.bcp47(),
            "missing fluent message after locale and English fallback"
        );
        key.to_owned()
    }
}

fn language_id(locale: AppLocale) -> LanguageIdentifier {
    match locale {
        AppLocale::En => langid!("en"),
        AppLocale::ZhCn => langid!("zh-CN"),
        AppLocale::Ru => langid!("ru"),
        AppLocale::EsEs => langid!("es-ES"),
        AppLocale::PtBr => langid!("pt-BR"),
        AppLocale::De => langid!("de"),
        AppLocale::Ja => langid!("ja"),
        AppLocale::Fr => langid!("fr"),
        AppLocale::Pl => langid!("pl"),
        AppLocale::Ko => langid!("ko"),
        AppLocale::ZhTw => langid!("zh-TW"),
        AppLocale::Tr => langid!("tr"),
        AppLocale::Th => langid!("th"),
        AppLocale::Es419 => langid!("es-419"),
        AppLocale::Uk => langid!("uk"),
        AppLocale::It => langid!("it"),
        AppLocale::Cs => langid!("cs"),
        AppLocale::Hu => langid!("hu"),
        AppLocale::PtPt => langid!("pt-PT"),
        AppLocale::Vi => langid!("vi"),
    }
}

fn fluent_args_to_map<'a>(
    args: &'a FluentArgs<'_>,
) -> HashMap<Cow<'static, str>, FluentValue<'a>> {
    args.iter()
        .map(|(key, value)| (Cow::Owned(key.to_owned()), value.clone()))
        .collect()
}

#[cfg(test)]
mod tests {
    use fluent_bundle::FluentArgs;

    use super::Catalog;
    use crate::locale::AppLocale;

    #[test]
    fn missing_key_returns_key_not_panic() {
        let catalog = Catalog::new(AppLocale::En);
        assert_eq!(catalog.t("definitely-missing-key"), "definitely-missing-key");

        let zh = Catalog::new(AppLocale::ZhTw);
        assert_eq!(zh.t("definitely-missing-key"), "definitely-missing-key");
    }

    #[test]
    fn t_args_interpolates_folder() {
        let catalog = Catalog::new(AppLocale::En);
        let mut args = FluentArgs::new();
        args.set("folder", "Documents");
        assert_eq!(catalog.t_args("search-in", &args), "Search Documents");
    }

    #[test]
    fn english_message_available_via_other_locale_fallback() {
        let catalog = Catalog::new(AppLocale::ZhCn);
        let mut args = FluentArgs::new();
        args.set("folder", "Downloads");
        assert_eq!(catalog.t_args("search-in", &args), "搜索 Downloads");
    }

    #[test]
    fn with_locale_switches_active_locale() {
        let catalog = Catalog::new(AppLocale::En).with_locale(AppLocale::Ko);
        assert_eq!(catalog.locale(), AppLocale::Ko);
    }
}
