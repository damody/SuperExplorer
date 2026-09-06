# Fluent i18n for SuperExplorer and SuperDesktop

Date: 2026-09-07
Status: draft, pending review
Platform: Windows 11 x64

## 1. Goal

Ship SuperExplorer and SuperDesktop with a Fluent-based localization system that covers every user-visible string, all Steam Hardware Survey top-20 interface languages, live language switching, Windows locale detection, persisted preference, locale-aware counts/sizes, and plugin locale selection under the same BCP-47 tags.

This is not a staged MVP. First delivery includes complete catalogs for all 20 locales (every English key present in every locale file), full UI extraction, settings UI, persistence, tests, SuperDesktop, and plugin host resolution.

## 2. Confirmed decisions

- Localization format: Mozilla Fluent (`.ftl`).
- Loader: `fluent-templates` compile-time embed plus a thin `explorer-i18n` crate.
- Language identifiers in-app: BCP-47. Steamworks store codes are a mapping table only.
- English (`en`) is the source of truth and the fallback locale.
- Current Traditional Chinese UI copy becomes the `zh-TW` catalog.
- SuperDesktop uses the same crate and the same `AppLocale` enum; it does not keep a parallel zh/en `if` helper.
- Plugin `.sepack` locale JSON stays JSON. The host selects the JSON file with the same BCP-47 tag and the same fallback chain. Plugin authors are not required to migrate to Fluent in this change.
- Native Windows Shell menus, Shell column values that Windows already formats, file names, paths, action type names, and diagnostic logs are not translated by this system.

## 3. Scope

### In scope

- New workspace crate `explorer-i18n`.
- All user-visible SuperExplorer strings in `explorer-ui` and `explorer-app` (chrome, menus, dialogs, status, errors shown to users, AccessKit names, Folder Options, About, bookmarks, transfer center, remote windows, navigation pane section titles).
- All user-visible SuperDesktop strings currently using the zh/en `localized()` helper (taskbar flyouts, taskbar settings, start/search copy the crate owns).
- Complete `.ftl` catalogs for the 20 locales listed in §4. Every locale has every English message id.
- Folder Options → General: language list, native names, Apply/OK live switch without restart.
- Session persistence of the chosen locale (schema bump).
- Windows display-language detection and negotiation when the user has not chosen a language.
- Locale-aware item counts, operation summaries, and file-size unit labels.
- Plugin host resolution of existing `locales/*.json` by `AppLocale`.
- Tests: crate unit tests, catalog completeness, UI tests pinned to `zh-TW` or keys, live-switch, persistence, Windows tag negotiation.
- `SUPEREXPLORER_LOCALE` override for uitest and fixtures.

### Out of scope

- Arabic / RTL layout (Steam interface share is 0%).
- Crowdin/Weblate accounts or translator portals.
- Translating README/EULA/docs (already separately tri-lingual).
- Translating Windows-owned Shell context menus, property-sheet text, or IFileOperation dialogs.
- Changing action identifiers (`ExplorerAction::name()` stays English).
- Shipping extra fonts; Windows Segoe UI font-link covers CJK, Thai, Korean, Japanese.

## 4. Locale catalog

`AppLocale` is a closed enum of 20 values. Picker order follows Steam rank.

| Rank | Variant | BCP-47 | Steamworks | Native name |
|------|---------|--------|------------|-------------|
| 1 | `En` | `en` | `english` | English |
| 2 | `ZhCn` | `zh-CN` | `schinese` | 简体中文 |
| 3 | `Ru` | `ru` | `russian` | Русский |
| 4 | `EsEs` | `es-ES` | `spanish` | Español |
| 5 | `PtBr` | `pt-BR` | `brazilian` | Português (Brasil) |
| 6 | `De` | `de` | `german` | Deutsch |
| 7 | `Ja` | `ja` | `japanese` | 日本語 |
| 8 | `Fr` | `fr` | `french` | Français |
| 9 | `Pl` | `pl` | `polish` | Polski |
| 10 | `Ko` | `ko` | `koreana` | 한국어 |
| 11 | `ZhTw` | `zh-TW` | `tchinese` | 繁體中文 |
| 12 | `Tr` | `tr` | `turkish` | Türkçe |
| 13 | `Th` | `th` | `thai` | ไทย |
| 14 | `Es419` | `es-419` | `latam` | Español (Latinoamérica) |
| 15 | `Uk` | `uk` | `ukrainian` | Українська |
| 16 | `It` | `it` | `italian` | Italiano |
| 17 | `Cs` | `cs` | `czech` | Čeština |
| 18 | `Hu` | `hu` | `hungarian` | Magyar |
| 19 | `PtPt` | `pt-PT` | `portuguese` | Português (Portugal) |
| 20 | `Vi` | `vi` | `vietnamese` | Tiếng Việt |

Negotiation from a Windows tag (`GetUserDefaultLocaleName`):

1. Exact BCP-47 match after `_` → `-` and case fold (`zh-TW`, `pt-BR`).
2. Script/region aliases: `zh-Hant` / `zh-HK` / `zh-MO` → `zh-TW`; `zh-Hans` / `zh-SG` → `zh-CN`; `es-MX` and other `es-*` except `es-ES` → `es-419`; `pt-BR` stays Brazilian; other `pt-*` → `pt-PT`.
3. Primary subtag match when only one catalog member has that primary (`ja`, `ko`, `ru`, …).
4. Otherwise `en`.

Steamworks mapping is `AppLocale::steamworks()` / `AppLocale::from_steamworks()`. It is not used at runtime unless a future Steam install sets the language; the table exists so store pages and in-app locales cannot drift.

## 5. Architecture

```
crates/explorer-i18n
  src/lib.rs          AppLocale, Catalog, t(), negotiate()
  src/locale.rs       enum, BCP-47, Steamworks, Windows aliases
  src/bundle.rs       fluent-templates loader, fallback, args
  locales/{bcp47}/*.ftl

explorer-model        persist AppLocale on the session payload
explorer-ui           every visible string goes through Catalog
explorer-app          detect Windows locale, persist, env override
explorer-extension-host   pick plugin JSON by AppLocale
SuperDesktop          path-dep explorer-i18n; replace localized()
```

`explorer-i18n` has no GPUI, no filesystem I/O at runtime, no Windows dependency. Windows detection lives in `explorer-app`. SuperDesktop detection lives in `superdesktop-app` / `platform-win`.

UI never stores translated strings in the model. Labels are resolved at render / command-build time from the current `AppLocale`.

## 6. Crate API

```rust
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum AppLocale { En, ZhCn, Ru, EsEs, PtBr, De, Ja, Fr, Pl, Ko, ZhTw, Tr, Th, Es419, Uk, It, Cs, Hu, PtPt, Vi }

impl AppLocale {
    pub const ALL: [Self; 20];
    pub fn bcp47(self) -> &'static str;
    pub fn steamworks(self) -> &'static str;
    pub fn native_name(self) -> &'static str;
    pub fn from_bcp47(tag: &str) -> Option<Self>;
    pub fn from_steamworks(code: &str) -> Option<Self>;
    pub fn negotiate(windows_tag: &str) -> Self;
}

pub struct Catalog { locale: AppLocale }

impl Catalog {
    pub fn new(locale: AppLocale) -> Self;
    pub fn locale(self) -> AppLocale;
    pub fn with_locale(self, locale: AppLocale) -> Self;
    pub fn t(self, key: &str) -> String;
    pub fn t_args(self, key: &str, args: &FluentArgs<'_>) -> String;
}
```

Missing-key policy: look up `locale`, then `en`, then return the key itself and `tracing::warn!`. Never panic in production.

`fluent-templates::static_loader!` embeds `crates/explorer-i18n/locales`. Fallback language is `en`. Isolating marks stay on (default) except in tests, where they are disabled so assertions stay readable.

Tests and uitest pin locale explicitly. They do not depend on the developer machine's Windows language.

## 7. Resource layout

```
crates/explorer-i18n/locales/en/
  chrome.ftl
  menus.ftl
  dialogs.ftl
  status.ftl
  settings.ftl
  a11y.ftl
  formatting.ftl
  desktop.ftl
```

The same file names exist under every other locale directory. `desktop.ftl` is SuperDesktop copy; SuperExplorer may load the whole bundle (unused keys are harmless) so there is one loader.

Message ids are stable kebab-case, namespaced by surface:

```
menu-copy = Copy
menu-cut = Cut
search-in = Search { $folder }
copy-items = Copy { $count } items
file-size = { $value } { $unit }
file-size-unit-kb = KB
language-label = Language
language-follow-windows = Windows display language ({ $name })
```

Plurals use Fluent selectors. Russian, Ukrainian, Polish, Czech must use the correct CLDR categories (`one` / `few` / `many` / `other` as required). English can use `one` / `other`.

`zh-TW` strings match the current hardcoded Traditional Chinese UI. `zh-CN` is Simplified, not a copy of `zh-TW`. `es-ES` and `es-419` are distinct. `pt-BR` and `pt-PT` are distinct.

## 8. Persistence and detection

Session schema becomes version 4. `PersistedSessionPayload` gains:

```rust
#[serde(default, skip_serializing_if = "Option::is_none")]
pub locale: Option<AppLocale>,
```

- `None` means “follow Windows”. Startup calls `AppLocale::negotiate(windows_tag)`.
- `Some(locale)` means the user picked that language in Folder Options. Startup uses it even if Windows changes.
- Folder Options General shows a dropdown: first row “Windows display language ({native name of negotiated})”, then the 20 native names in Steam rank order.
- Apply/OK writes the payload and updates every live Explorer window immediately. No restart.
- Unknown future serde values: `deny_unknown_fields` stays. Adding `locale` with `default` keeps old session files loadable through the existing v3 → v4 migration path.
- Env `SUPEREXPLORER_LOCALE` (BCP-47) overrides both session and Windows, used by uitest. Invalid values are ignored with a warning.

Language is app-global, not per-tab and not inside `ViewSettings`.

## 9. UI integration

`ExplorerRoot` / window state holds `Catalog` (or `AppLocale` and constructs `Catalog` on the stack). Every label currently written as a Chinese or English literal becomes `catalog.t("…")` at the point of use.

Command builders in `chrome.rs` that today return `label: "複製"` return `label: catalog.t("menu-copy")`. Tests that assert those labels either:

- construct state with `AppLocale::ZhTw` and keep the Chinese expected strings, or
- assert message ids / action types instead of glyphs.

AccessKit `aria_label` values are translated. Debug selectors and widget ids stay English.

Layout: buttons and menu rows size to content. Do not introduce pixel widths derived from Chinese or English metrics.

Live switch: changing locale updates state and requests a full window refresh. Open Folder Options title bar, dialogs, and SuperDesktop flyouts re-read the catalog on the next frame.

## 10. Formatting

`explorer-ui` `format_file_size` becomes locale-aware:

- Numeric magnitude stays the current 1-decimal binary-step algorithm (Explorer-like).
- Unit suffix comes from Fluent (`file-size-unit-kb` … `tb`).
- Tests pin `AppLocale::En` for the existing `"1.0 KB"` assertions, plus one `zh-TW` / `ru` case.

Operation summaries (`複製 {n} 個項目`) become Fluent messages with a `count` argument and plural selectors.

App-owned dates/times that we format ourselves use the active locale. Values that Windows already formatted (native Shell details dates) stay as returned.

## 11. Plugins

`.sepack` `locales[]` entries keep `{ locale, path, sha256 }`. Host matching:

1. Exact BCP-47 (`zh-TW`).
2. `AppLocale::negotiate` aliases (`zh-HK` file is not required; `zh-TW` matches).
3. Primary subtag (`zh` → `zh-TW` or `zh-CN` by script if present, else the first `zh-*` in the package).
4. `en` / `en-US`.
5. Package `display_name` from the manifest if no locale file matches.

Existing example plugins already ship `en-US` and `zh-TW`. `en-US` matches `AppLocale::En`. No manifest schema change.

## 12. SuperDesktop

SuperDesktop is a second workspace. It path-depends `explorer-i18n`:

```toml
explorer-i18n = { path = "../crates/explorer-i18n" }
```

Replace `fn localized(..., zh_tw, en)` with `catalog.t(key)`. Persist the locale in SuperDesktop's settings-store (not Explorer's session.json). First-run negotiation uses the same `AppLocale::negotiate`. If SuperExplorer already stored a locale, SuperDesktop does not read that file; the two products keep independent preferences unless a later settings-sync change is specified.

`desktop.ftl` lives in `explorer-i18n` so both products share one catalog completeness test.

## 13. Error handling

| Case | Behavior |
|------|----------|
| Missing key in current locale | English fallback, then the key, plus `warn` |
| Missing entire locale directory | Treat as English; compile fails if a listed `AppLocale` has no folder |
| Malformed `.ftl` | Compile error from `fluent-templates` |
| Corrupt session locale field | Migration rejects that artifact the same way as other payload errors; start with Windows negotiation |
| Invalid `SUPEREXPLORER_LOCALE` | Ignore, warn, continue with session/Windows |
| Plugin locale JSON missing | Fall back per §11; do not fail package load |

## 14. Testing

- `explorer-i18n`: parse BCP-47, Steamworks round-trip, Windows alias table, fallback, args, plural samples for `en` / `ru` / `pl`, missing-key fallback.
- Catalog completeness: every locale directory contains every message id from `en/`. Fail CI if any locale is missing a key or has an extra unknown key.
- Fluent parse: every `.ftl` file loads.
- `explorer-model`: session v3 file without `locale` loads as `None`; v4 with `locale: "zh-tw"` round-trips.
- `explorer-ui`: command labels under `ZhTw` still match today's Chinese strings for a snapshot of core commands; `En` returns English.
- Folder Options: selecting a language updates draft; Apply updates live catalog.
- `explorer-app`: env override wins; Windows tag `zh-HK` negotiates `ZhTw`.
- SuperDesktop: flyout copy follows catalog; zh-TW and en both covered.
- Existing uitest: set `SUPEREXPLORER_LOCALE=zh-TW` so UIA names stay stable.

## 15. Non-goals that stay explicit

Do not translate `ExplorerAction::name()`. Do not change GPUI action identity. Do not wrap Windows `IContextMenu` verbs. Do not add a language pack installer or runtime `.ftl` overlay. Catalogs are embedded.

## 16. Success

- Switching Folder Options language to any of the 20 locales redraws SuperExplorer chrome, menus, dialogs, status, and a11y names in that language.
- A machine with Windows display language `ru-RU` and no saved preference opens in Russian.
- `zh-TW` matches the current product copy for the migrated strings.
- `cargo test -p explorer-i18n` fails if any of the 20 locales drops a key.
- SuperDesktop flyouts and settings use the same 20 locales.
- Plugins with `en-US`/`zh-TW` JSON keep working and follow the app locale.
