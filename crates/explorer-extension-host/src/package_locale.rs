//! Select a `.sepack` locale JSON resource for the active [`AppLocale`].
//!
//! Plugin locale files stay JSON. Matching follows the same BCP-47 fallback
//! chain as the app catalogs: exact tag → negotiate aliases → primary subtag →
//! `en` / `en-US` → manifest display-name fallback. An empty `locales` array
//! does not fail package load; callers use the manifest display name instead.

use std::collections::BTreeMap;

use explorer_i18n::AppLocale;
use serde::Deserialize;

use crate::LocaleResourceV1;

/// Outcome of picking a package locale file for one [`AppLocale`].
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PackageLocaleResolutionV1<'a> {
    /// Selected `locales[]` entry.
    File(&'a LocaleResourceV1),
    /// No locale file matched; use the package manifest display-name fallback.
    ManifestDisplayName,
}

/// Locale JSON bytes captured at package admit for Folder Options chrome.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DiscoveredPackageChromeV1 {
    package_id: String,
    locales: Vec<LocaleResourceV1>,
    locale_bytes: BTreeMap<String, Vec<u8>>,
}

impl DiscoveredPackageChromeV1 {
    pub(crate) fn from_parts(
        package_id: String,
        locales: Vec<LocaleResourceV1>,
        locale_bytes: BTreeMap<String, Vec<u8>>,
    ) -> Self {
        Self {
            package_id,
            locales,
            locale_bytes,
        }
    }

    /// Package identity used as the manifest display-name fallback.
    #[must_use]
    pub fn package_id(&self) -> &str {
        &self.package_id
    }

    /// Resolves the package display name for `app_locale`.
    #[must_use]
    pub fn display_name(&self, app_locale: AppLocale) -> String {
        resolve_package_display_name(
            app_locale,
            &self.locales,
            |resource| self.locale_bytes.get(&resource.path).cloned(),
            &self.package_id,
        )
    }
}

/// Subset of plugin locale JSON the host reads for package chrome.
#[derive(Clone, Debug, Eq, PartialEq, Deserialize)]
pub struct PackageLocaleJsonV1 {
    #[serde(default)]
    pub plugin: Option<String>,
    pub display_name: String,
    #[serde(default)]
    pub description: Option<String>,
}

/// Selects the best `locales[]` entry for `app_locale`.
///
/// Empty `locales` yields [`PackageLocaleResolutionV1::ManifestDisplayName`]
/// without error so package load can continue.
#[must_use]
pub fn resolve_package_locale<'a>(
    app_locale: AppLocale,
    locales: &'a [LocaleResourceV1],
) -> PackageLocaleResolutionV1<'a> {
    if locales.is_empty() {
        return PackageLocaleResolutionV1::ManifestDisplayName;
    }
    if let Some(resource) = find_exact(app_locale, locales) {
        return PackageLocaleResolutionV1::File(resource);
    }
    if let Some(resource) = find_negotiate_alias(app_locale, locales) {
        return PackageLocaleResolutionV1::File(resource);
    }
    if let Some(resource) = find_primary_subtag(app_locale, locales) {
        return PackageLocaleResolutionV1::File(resource);
    }
    if let Some(resource) = find_english_fallback(locales) {
        return PackageLocaleResolutionV1::File(resource);
    }
    PackageLocaleResolutionV1::ManifestDisplayName
}

/// Parses plugin locale JSON bytes. Unknown fields are ignored.
///
/// # Errors
///
/// Returns a serde error when the document is not an object with a string
/// `display_name`.
pub fn parse_package_locale_json(bytes: &[u8]) -> Result<PackageLocaleJsonV1, serde_json::Error> {
    serde_json::from_slice(bytes)
}

/// Resolves the package display name for `app_locale`.
///
/// When a locale file is selected, `read_locale` supplies its bytes. Missing,
/// unreadable, or invalid JSON falls through to `manifest_display_name`.
#[must_use]
pub fn resolve_package_display_name<'a, F>(
    app_locale: AppLocale,
    locales: &'a [LocaleResourceV1],
    mut read_locale: F,
    manifest_display_name: &str,
) -> String
where
    F: FnMut(&'a LocaleResourceV1) -> Option<Vec<u8>>,
{
    match resolve_package_locale(app_locale, locales) {
        PackageLocaleResolutionV1::File(resource) => read_locale(resource)
            .and_then(|bytes| parse_package_locale_json(&bytes).ok())
            .map(|json| json.display_name)
            .filter(|name| !name.trim().is_empty())
            .unwrap_or_else(|| manifest_display_name.to_owned()),
        PackageLocaleResolutionV1::ManifestDisplayName => manifest_display_name.to_owned(),
    }
}

fn find_exact<'a>(
    app_locale: AppLocale,
    locales: &'a [LocaleResourceV1],
) -> Option<&'a LocaleResourceV1> {
    let want = normalize_tag(app_locale.bcp47());
    locales
        .iter()
        .find(|resource| normalize_tag(&resource.locale) == want)
}

fn find_negotiate_alias<'a>(
    app_locale: AppLocale,
    locales: &'a [LocaleResourceV1],
) -> Option<&'a LocaleResourceV1> {
    locales.iter().find(|resource| {
        let normalized = normalize_tag(&resource.locale);
        if normalize_tag(app_locale.bcp47()) == normalized {
            return false;
        }
        // `AppLocale::negotiate` defaults unknown tags to `En`. Only accept a
        // match when the tag actually negotiates to `app_locale` via exact,
        // alias, or unique-primary rules — never via that final default alone.
        let negotiated = AppLocale::negotiate(&normalized);
        if negotiated != app_locale {
            return false;
        }
        if app_locale == AppLocale::En {
            primary_subtag(&normalized) == "en"
        } else {
            true
        }
    })
}

fn find_primary_subtag<'a>(
    app_locale: AppLocale,
    locales: &'a [LocaleResourceV1],
) -> Option<&'a LocaleResourceV1> {
    let want_tag = normalize_tag(app_locale.bcp47());
    let want_primary = primary_subtag(&want_tag).to_owned();
    let matches: Vec<&'a LocaleResourceV1> = locales
        .iter()
        .filter(|resource| primary_subtag(&normalize_tag(&resource.locale)) == want_primary)
        .collect();
    if matches.is_empty() {
        return None;
    }
    match app_locale {
        AppLocale::ZhTw => matches
            .iter()
            .copied()
            .find(|resource| is_hant_like(&normalize_tag(&resource.locale)))
            .or_else(|| matches.first().copied()),
        AppLocale::ZhCn => matches
            .iter()
            .copied()
            .find(|resource| is_hans_like(&normalize_tag(&resource.locale)))
            .or_else(|| matches.first().copied()),
        _ => matches.first().copied(),
    }
}

fn find_english_fallback(locales: &[LocaleResourceV1]) -> Option<&LocaleResourceV1> {
    locales
        .iter()
        .find(|resource| normalize_tag(&resource.locale) == "en")
        .or_else(|| {
            locales
                .iter()
                .find(|resource| normalize_tag(&resource.locale) == "en-us")
        })
}

fn normalize_tag(tag: &str) -> String {
    tag.replace('_', "-").to_ascii_lowercase()
}

fn primary_subtag(normalized: &str) -> &str {
    normalized.split('-').next().unwrap_or(normalized)
}

fn is_hant_like(normalized: &str) -> bool {
    normalized
        .split('-')
        .any(|part| matches!(part, "hant" | "tw" | "hk" | "mo"))
}

fn is_hans_like(normalized: &str) -> bool {
    normalized
        .split('-')
        .any(|part| matches!(part, "hans" | "cn" | "sg"))
}

#[cfg(test)]
mod tests {
    use super::{
        DiscoveredPackageChromeV1, PackageLocaleResolutionV1, parse_package_locale_json,
        resolve_package_display_name, resolve_package_locale,
    };
    use crate::LocaleResourceV1;
    use explorer_i18n::AppLocale;

    fn resource(locale: &str, path: &str) -> LocaleResourceV1 {
        LocaleResourceV1 {
            locale: locale.to_owned(),
            path: path.to_owned(),
            sha256: "0".repeat(64),
        }
    }

    fn en_zh_locales() -> Vec<LocaleResourceV1> {
        vec![
            resource("en-US", "locales/en-US.json"),
            resource("zh-TW", "locales/zh-TW.json"),
        ]
    }

    #[test]
    fn zh_tw_picks_zh_tw_json() {
        let locales = en_zh_locales();
        match resolve_package_locale(AppLocale::ZhTw, &locales) {
            PackageLocaleResolutionV1::File(selected) => {
                assert_eq!(selected.locale, "zh-TW");
                assert_eq!(selected.path, "locales/zh-TW.json");
            }
            PackageLocaleResolutionV1::ManifestDisplayName => {
                panic!("expected zh-TW.json")
            }
        }
    }

    #[test]
    fn en_picks_en_us_json() {
        let locales = en_zh_locales();
        match resolve_package_locale(AppLocale::En, &locales) {
            PackageLocaleResolutionV1::File(selected) => {
                assert_eq!(selected.locale, "en-US");
                assert_eq!(selected.path, "locales/en-US.json");
            }
            PackageLocaleResolutionV1::ManifestDisplayName => {
                panic!("expected en-US.json")
            }
        }
    }

    #[test]
    fn ja_with_only_en_and_zh_tw_falls_back_to_en_us() {
        let locales = en_zh_locales();
        match resolve_package_locale(AppLocale::Ja, &locales) {
            PackageLocaleResolutionV1::File(selected) => {
                assert_eq!(selected.locale, "en-US");
                assert_eq!(selected.path, "locales/en-US.json");
            }
            PackageLocaleResolutionV1::ManifestDisplayName => {
                panic!("expected en-US.json fallback")
            }
        }
    }

    #[test]
    fn missing_locales_array_uses_manifest_display_name_without_failing() {
        let locales: Vec<LocaleResourceV1> = Vec::new();
        assert_eq!(
            resolve_package_locale(AppLocale::ZhTw, &locales),
            PackageLocaleResolutionV1::ManifestDisplayName
        );
        let name = resolve_package_display_name(
            AppLocale::ZhTw,
            &locales,
            |_| panic!("must not read locale files when locales are empty"),
            "example.package",
        );
        assert_eq!(name, "example.package");
    }

    #[test]
    fn zh_hk_package_tag_matches_zh_tw_app_locale() {
        let locales = vec![
            resource("en-US", "locales/en-US.json"),
            resource("zh-HK", "locales/zh-HK.json"),
        ];
        match resolve_package_locale(AppLocale::ZhTw, &locales) {
            PackageLocaleResolutionV1::File(selected) => {
                assert_eq!(selected.locale, "zh-HK");
            }
            PackageLocaleResolutionV1::ManifestDisplayName => {
                panic!("expected zh-HK via negotiate aliases")
            }
        }
    }

    #[test]
    fn discovered_chrome_follows_app_locale() {
        let locales = en_zh_locales();
        let mut locale_bytes = std::collections::BTreeMap::new();
        locale_bytes.insert(
            "locales/zh-TW.json".to_owned(),
            r#"{"display_name":"資料夾大小圖"}"#.as_bytes().to_vec(),
        );
        locale_bytes.insert(
            "locales/en-US.json".to_owned(),
            r#"{"display_name":"Folder Size Map"}"#.as_bytes().to_vec(),
        );
        let chrome = DiscoveredPackageChromeV1::from_parts(
            "third-party.size-map".to_owned(),
            locales,
            locale_bytes,
        );
        assert_eq!(chrome.display_name(AppLocale::ZhTw), "資料夾大小圖");
        assert_eq!(chrome.display_name(AppLocale::En), "Folder Size Map");
        assert_eq!(chrome.display_name(AppLocale::Ja), "Folder Size Map");
    }

    #[test]
    fn display_name_reads_selected_locale_json() {
        let locales = en_zh_locales();
        let name = resolve_package_display_name(
            AppLocale::ZhTw,
            &locales,
            |resource| {
                assert_eq!(resource.path, "locales/zh-TW.json");
                Some(
                    r#"{
                      "plugin": "rust-folder-size-map-view",
                      "display_name": "Folder Size Map TW",
                      "description": "sample"
                    }"#
                    .as_bytes()
                    .to_vec(),
                )
            },
            "rust-folder-size-map-view",
        );
        assert_eq!(name, "Folder Size Map TW");
    }

    #[test]
    fn parse_locale_json_requires_display_name() {
        let parsed = parse_package_locale_json(
            br#"{"plugin":"demo","display_name":"Demo","description":"d"}"#,
        )
        .expect("valid locale json");
        assert_eq!(parsed.display_name, "Demo");
        assert_eq!(parsed.plugin.as_deref(), Some("demo"));
        assert!(parse_package_locale_json(br#"{"plugin":"demo"}"#).is_err());
    }
}
