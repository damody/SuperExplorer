//! Closed set of app locales and Windows/Steam tag negotiation.

use serde::{Deserialize, Serialize};

/// Steam Hardware Survey top-20 interface languages supported by the app.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum AppLocale {
    En,
    ZhCn,
    Ru,
    EsEs,
    PtBr,
    De,
    Ja,
    Fr,
    Pl,
    Ko,
    ZhTw,
    Tr,
    Th,
    #[serde(rename = "es-419")]
    Es419,
    Uk,
    It,
    Cs,
    Hu,
    PtPt,
    Vi,
}

impl AppLocale {
    /// Picker order follows Steam interface-language rank.
    pub const ALL: [Self; 20] = [
        Self::En,
        Self::ZhCn,
        Self::Ru,
        Self::EsEs,
        Self::PtBr,
        Self::De,
        Self::Ja,
        Self::Fr,
        Self::Pl,
        Self::Ko,
        Self::ZhTw,
        Self::Tr,
        Self::Th,
        Self::Es419,
        Self::Uk,
        Self::It,
        Self::Cs,
        Self::Hu,
        Self::PtPt,
        Self::Vi,
    ];

    /// Canonical BCP-47 tag used for catalogs and plugin locale files.
    #[must_use]
    pub const fn bcp47(self) -> &'static str {
        match self {
            Self::En => "en",
            Self::ZhCn => "zh-CN",
            Self::Ru => "ru",
            Self::EsEs => "es-ES",
            Self::PtBr => "pt-BR",
            Self::De => "de",
            Self::Ja => "ja",
            Self::Fr => "fr",
            Self::Pl => "pl",
            Self::Ko => "ko",
            Self::ZhTw => "zh-TW",
            Self::Tr => "tr",
            Self::Th => "th",
            Self::Es419 => "es-419",
            Self::Uk => "uk",
            Self::It => "it",
            Self::Cs => "cs",
            Self::Hu => "hu",
            Self::PtPt => "pt-PT",
            Self::Vi => "vi",
        }
    }

    /// Steamworks API language code for store/page mapping.
    #[must_use]
    pub const fn steamworks(self) -> &'static str {
        match self {
            Self::En => "english",
            Self::ZhCn => "schinese",
            Self::Ru => "russian",
            Self::EsEs => "spanish",
            Self::PtBr => "brazilian",
            Self::De => "german",
            Self::Ja => "japanese",
            Self::Fr => "french",
            Self::Pl => "polish",
            Self::Ko => "koreana",
            Self::ZhTw => "tchinese",
            Self::Tr => "turkish",
            Self::Th => "thai",
            Self::Es419 => "latam",
            Self::Uk => "ukrainian",
            Self::It => "italian",
            Self::Cs => "czech",
            Self::Hu => "hungarian",
            Self::PtPt => "portuguese",
            Self::Vi => "vietnamese",
        }
    }

    /// Native endonym shown in the language picker.
    #[must_use]
    pub const fn native_name(self) -> &'static str {
        match self {
            Self::En => "English",
            Self::ZhCn => "简体中文",
            Self::Ru => "Русский",
            Self::EsEs => "Español",
            Self::PtBr => "Português (Brasil)",
            Self::De => "Deutsch",
            Self::Ja => "日本語",
            Self::Fr => "Français",
            Self::Pl => "Polski",
            Self::Ko => "한국어",
            Self::ZhTw => "繁體中文",
            Self::Tr => "Türkçe",
            Self::Th => "ไทย",
            Self::Es419 => "Español (Latinoamérica)",
            Self::Uk => "Українська",
            Self::It => "Italiano",
            Self::Cs => "Čeština",
            Self::Hu => "Magyar",
            Self::PtPt => "Português (Portugal)",
            Self::Vi => "Tiếng Việt",
        }
    }

    /// Exact BCP-47 match after `_` → `-` and ASCII case fold. No aliasing.
    #[must_use]
    pub fn from_bcp47(tag: &str) -> Option<Self> {
        let normalized = normalize_tag(tag);
        Self::ALL
            .into_iter()
            .find(|locale| locale.bcp47().eq_ignore_ascii_case(&normalized))
    }

    /// Exact Steamworks language-code match.
    #[must_use]
    pub fn from_steamworks(code: &str) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|locale| locale.steamworks() == code)
    }

    /// Map a Windows display-language tag to a catalog member.
    #[must_use]
    pub fn negotiate(windows_tag: &str) -> Self {
        let normalized = normalize_tag(windows_tag);
        if let Some(exact) = Self::from_bcp47(&normalized) {
            return exact;
        }
        if let Some(aliased) = negotiate_alias(&normalized) {
            return aliased;
        }
        if let Some(primary) = unique_primary(&normalized) {
            return primary;
        }
        Self::En
    }
}

fn normalize_tag(tag: &str) -> String {
    tag.replace('_', "-").to_ascii_lowercase()
}

fn primary_subtag(normalized: &str) -> &str {
    normalized.split('-').next().unwrap_or(normalized)
}

fn negotiate_alias(normalized: &str) -> Option<AppLocale> {
    let mut parts = normalized.split('-');
    let primary = parts.next()?;
    match primary {
        "zh" => {
            let rest: Vec<&str> = parts.collect();
            if rest
                .iter()
                .any(|part| matches!(*part, "hant" | "hk" | "mo"))
            {
                Some(AppLocale::ZhTw)
            } else if rest
                .iter()
                .any(|part| matches!(*part, "hans" | "sg"))
            {
                Some(AppLocale::ZhCn)
            } else {
                None
            }
        }
        "es" => {
            if parts.next().is_some() {
                Some(AppLocale::Es419)
            } else {
                None
            }
        }
        "pt" => match parts.next() {
            Some("br") => Some(AppLocale::PtBr),
            Some(_) => Some(AppLocale::PtPt),
            None => None,
        },
        _ => None,
    }
}

fn unique_primary(normalized: &str) -> Option<AppLocale> {
    let primary = primary_subtag(normalized);
    let mut matches = AppLocale::ALL.into_iter().filter(|locale| {
        primary_subtag(&normalize_tag(locale.bcp47())) == primary
    });
    match (matches.next(), matches.next()) {
        (Some(only), None) => Some(only),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::AppLocale;

    #[test]
    fn from_bcp47_is_exact_only() {
        assert_eq!(AppLocale::from_bcp47("zh-TW"), Some(AppLocale::ZhTw));
        assert_eq!(AppLocale::from_bcp47("zh_tw"), Some(AppLocale::ZhTw));
        assert_eq!(AppLocale::from_bcp47("en-GB"), None);
        assert_eq!(AppLocale::from_bcp47("zh-HK"), None);
    }

    #[test]
    fn negotiate_windows_tags() {
        assert_eq!(AppLocale::negotiate("zh-HK"), AppLocale::ZhTw);
        assert_eq!(AppLocale::negotiate("zh-Hans-CN"), AppLocale::ZhCn);
        assert_eq!(AppLocale::negotiate("es-MX"), AppLocale::Es419);
        assert_eq!(AppLocale::negotiate("es-ES"), AppLocale::EsEs);
        assert_eq!(AppLocale::negotiate("pt-BR"), AppLocale::PtBr);
        assert_eq!(AppLocale::negotiate("pt-PT"), AppLocale::PtPt);
        assert_eq!(AppLocale::negotiate("ru-RU"), AppLocale::Ru);
        assert_eq!(AppLocale::negotiate("en-GB"), AppLocale::En);
        assert_eq!(AppLocale::negotiate("xx-YY"), AppLocale::En);
    }

    #[test]
    fn steamworks_round_trip_samples() {
        assert_eq!(AppLocale::from_steamworks("koreana"), Some(AppLocale::Ko));
        assert_eq!(
            AppLocale::from_steamworks("schinese"),
            Some(AppLocale::ZhCn)
        );
        assert_eq!(AppLocale::Ko.steamworks(), "koreana");
    }

    #[test]
    fn serde_uses_kebab_case() {
        assert_eq!(
            serde_json::to_string(&AppLocale::ZhCn).expect("serialize"),
            "\"zh-cn\""
        );
        assert_eq!(
            serde_json::to_string(&AppLocale::Es419).expect("serialize"),
            "\"es-419\""
        );
        assert_eq!(
            serde_json::to_string(&AppLocale::PtBr).expect("serialize"),
            "\"pt-br\""
        );
        assert_eq!(
            serde_json::from_str::<AppLocale>("\"zh-tw\"").expect("deserialize"),
            AppLocale::ZhTw
        );
    }

    #[test]
    fn all_follows_steam_rank() {
        assert_eq!(AppLocale::ALL[0], AppLocale::En);
        assert_eq!(AppLocale::ALL[1], AppLocale::ZhCn);
        assert_eq!(AppLocale::ALL[19], AppLocale::Vi);
        assert_eq!(AppLocale::ALL.len(), 20);
    }
}
