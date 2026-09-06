# Fluent i18n (Steam top-20) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Embed a Fluent catalog of all Steam top-20 languages, route every SuperExplorer and SuperDesktop user-visible string through it, persist and detect locale, and keep plugin JSON locales on the same BCP-47 tags.

**Architecture:** New `explorer-i18n` crate owns `AppLocale`, the embedded `.ftl` bundles, lookup, and fallback. `explorer-model` persists an optional locale on session schema v4. `explorer-ui` / `explorer-app` resolve labels at render time and switch live from Folder Options. SuperDesktop path-depends the same crate. Plugin host only changes locale file selection.

**Tech Stack:** `fluent-templates`, `fluent-bundle`, `unic-langid`, `serde`; Windows `GetUserDefaultLocaleName` in `explorer-app` only.

**Spec:** `docs/superpowers/specs/2026-09-07-fluent-i18n-steam-languages-design.md`

## Global Constraints

- English (`en`) is source of truth and fallback. Never panic on a missing key.
- All 20 `AppLocale` directories ship in the same change; completeness test must pass.
- `ExplorerAction::name()`, widget ids, debug selectors, file names, and Windows Shell menus stay untranslated.
- Existing uitest/UIA fixtures pin `SUPEREXPLORER_LOCALE=zh-TW`.
- `zh-TW` copy matches the current Traditional Chinese UI for migrated strings.
- `es-ES` ≠ `es-419`, `pt-BR` ≠ `pt-PT`, `zh-CN` ≠ `zh-TW`.
- No Crowdin, no runtime `.ftl` overlay, no RTL.

---

### Task 1: `explorer-i18n` crate, `AppLocale`, loader

**Files:**
- Create: `crates/explorer-i18n/Cargo.toml`
- Create: `crates/explorer-i18n/src/lib.rs`
- Create: `crates/explorer-i18n/src/locale.rs`
- Create: `crates/explorer-i18n/src/bundle.rs`
- Create: `crates/explorer-i18n/locales/en/chrome.ftl` (minimal keys to boot tests)
- Modify: `Cargo.toml` (workspace `members` + `workspace.dependencies`)

**Interfaces:**
- Produces: `AppLocale`, `AppLocale::ALL`, `bcp47`, `steamworks`, `native_name`, `from_bcp47`, `from_steamworks`, `negotiate`, `Catalog::{new,locale,with_locale,t,t_args}`

- [ ] Add workspace member `crates/explorer-i18n` and deps `fluent-templates`, `fluent-bundle`, `unic-langid`, `serde` (derive), `tracing`.
- [ ] Implement `AppLocale` with the 20 variants from the spec table. Serde: kebab-case (`zh-cn`, `pt-br`, `es-419`).
- [ ] `negotiate` tests:

```rust
assert_eq!(AppLocale::from_bcp47("zh-TW"), Some(AppLocale::ZhTw));
assert_eq!(AppLocale::negotiate("zh-HK"), AppLocale::ZhTw);
assert_eq!(AppLocale::negotiate("zh-Hans-CN"), AppLocale::ZhCn);
assert_eq!(AppLocale::negotiate("es-MX"), AppLocale::Es419);
assert_eq!(AppLocale::negotiate("es-ES"), AppLocale::EsEs);
assert_eq!(AppLocale::negotiate("pt-BR"), AppLocale::PtBr);
assert_eq!(AppLocale::negotiate("pt-PT"), AppLocale::PtPt);
assert_eq!(AppLocale::negotiate("ru-RU"), AppLocale::Ru);
assert_eq!(AppLocale::negotiate("en-GB"), AppLocale::En);
assert_eq!(AppLocale::negotiate("xx-YY"), AppLocale::En);
assert_eq!(AppLocale::from_steamworks("koreana"), Some(AppLocale::Ko));
assert_eq!(AppLocale::from_steamworks("schinese"), Some(AppLocale::ZhCn));
assert_eq!(AppLocale::Ko.steamworks(), "koreana");
```

- [ ] `static_loader!` with `locales: "./locales"`, `fallback_language: "en"`. Tests set `customise: |b| b.set_use_isolating(false)`.
- [ ] `Catalog::t` falls back en → key. Missing-key test expects the key string, not a panic.
- [ ] `t_args` interpolates `{ $folder }` for `search-in = Search { $folder }`.
- [ ] `cargo test -p explorer-i18n` passes.

---

### Task 2: Complete Fluent catalogs (all 20 locales)

**Files:**
- Create: `crates/explorer-i18n/locales/{en,zh-CN,ru,es-ES,pt-BR,de,ja,fr,pl,ko,zh-TW,tr,th,es-419,uk,it,cs,hu,pt-PT,vi}/{chrome,menus,dialogs,status,settings,a11y,formatting,desktop}.ftl`
- Create: `crates/explorer-i18n/tests/catalog_complete.rs`

**Interfaces:**
- Consumes: loader from Task 1
- Produces: every SuperExplorer/SuperDesktop user-visible message id, present in all 20 locales

- [ ] Extract every user-visible literal from `explorer-ui` (`chrome.rs`, `navigation_pane.rs`, `folder_options_window.rs`, bookmark_* windows, `transfer_center_window.rs`, `remote_*`, `file_view.rs`, `icons.rs` a11y, `lib.rs` dialogs, `state.rs` operation messages) and SuperDesktop `localized()` call sites. Assign kebab-case ids.
- [ ] Write `en/` as source of truth.
- [ ] Write `zh-TW/` to match current Traditional Chinese UI exactly for those strings.
- [ ] Write the other 18 locales in full. Plurals: `ru`, `uk`, `pl`, `cs` must use CLDR categories, not English `one/other` only.
- [ ] Completeness test walks `locales/en/**/*.ftl` message ids and asserts each other locale defines the same set (no missing, no extras). Also `Catalog::new(locale).t(id)` is non-empty and not equal to `id` for every pair.
- [ ] Plural spot checks: `copy-items` with counts 1, 2, 5 for `en`, `ru`, `pl`.
- [ ] `cargo test -p explorer-i18n --test catalog_complete` passes.

---

### Task 3: Persist locale on session schema v4

**Files:**
- Modify: `crates/explorer-model/src/session.rs` (`SESSION_SCHEMA_VERSION`, `PersistedSessionPayload`, migration)
- Modify: `crates/explorer-model/src/lib.rs` (re-export `AppLocale` or depend on `explorer-i18n`)
- Modify: `crates/explorer-model/Cargo.toml`

**Interfaces:**
- Consumes: `AppLocale`
- Produces: `PersistedSessionPayload.locale: Option<AppLocale>` (`None` = follow Windows)

- [ ] Depend on `explorer-i18n`. Bump `SESSION_SCHEMA_VERSION` to 4.
- [ ] Add `locale: Option<AppLocale>` with `#[serde(default, skip_serializing_if = "Option::is_none")]`.
- [ ] Migration: v3 payload without `locale` loads as `None`.
- [ ] Tests: round-trip `Some(AppLocale::Ru)`; v3 fixture without the field succeeds; unknown locale string fails validation the same way as other bad payload fields.
- [ ] `cargo test -p explorer-model --lib session` passes.

---

### Task 4: App detection, env override, live catalog on window state

**Files:**
- Modify: `crates/explorer-app/src/application.rs` (and the session restore path that hydrates UI state)
- Create or modify: a small Windows locale helper next to existing Win32 calls (`GetUserDefaultLocaleName`)
- Modify: `crates/explorer-ui/src/state.rs` (store `AppLocale`)
- Modify: `crates/explorer-ui/src/lib.rs` / harness constructors
- Modify: `crates/explorer-ui/Cargo.toml`, `crates/explorer-app/Cargo.toml`

**Interfaces:**
- Consumes: `AppLocale::negotiate`, session `locale`, env `SUPEREXPLORER_LOCALE`
- Produces: `ExplorerState::locale()` / `set_locale()`, resolved at startup as env > session `Some` > Windows negotiate > `En`

- [ ] Resolve order tests with a fake Windows tag and env:

```rust
// env zh-CN wins over session ru and Windows ja
// session Some(Ru) wins over Windows ja when env unset
// session None + Windows zh-HK => ZhTw
// invalid env ignored
```

- [ ] Wire resolved locale into every new `ExplorerRoot` / window state.
- [ ] `set_locale` updates state and is visible on the next render (no process restart).
- [ ] Default uitest/harness locale is `ZhTw` so existing Chinese assertions can migrate incrementally in later tasks; document that production default is negotiated.

---

### Task 5: Folder Options language picker

**Files:**
- Modify: `crates/explorer-ui/src/folder_options_window.rs`
- Modify: `crates/explorer-ui/src/state.rs` (`FolderOptionsDraft`, apply snapshot)
- Modify: `crates/explorer-ui/src/actions.rs` (if a dedicated action is cleaner than stuffing locale into `ViewSettings` — do **not** put locale in `ViewSettings`)
- Modify: `crates/explorer-ui/src/chrome.rs` (General page content)
- Modify: `crates/explorer-app` apply path so locale is written to session

**Interfaces:**
- Draft field `locale_choice: LocaleChoice` where `FollowWindows` or `Explicit(AppLocale)`
- Apply writes `PersistedSessionPayload.locale` (`None` or `Some`) and `state.set_locale(resolved)`

- [ ] General page: label `settings-language`, combo of “Windows display language ({native})” plus `AppLocale::ALL` native names in spec order.
- [ ] Changing the combo dirties the draft; Cancel restores; Apply/OK switches every live window.
- [ ] Tests: draft dirty on change; Apply with Explicit(Ja) sets state locale to `Ja` and session to `Some(Ja)`; Apply with FollowWindows sets session `None` and state to negotiated.
- [ ] Window title `dialogs-folder-options` is translated; English and zh-TW both covered.

---

### Task 6: SuperExplorer chrome, menus, navigation, search

**Files:**
- Modify: `crates/explorer-ui/src/chrome.rs`
- Modify: `crates/explorer-ui/src/navigation_pane.rs`
- Modify: `crates/explorer-ui/src/file_view.rs` (any user-visible literals)

Replace every user-visible literal with `catalog.t` / `t_args`. Keep action enums unchanged.

- [ ] Command labels: 新增資料夾 / 開啟 / 剪下 / 複製 / 貼上 / 重新命名 / 內容 / … → `menus.ftl`.
- [ ] Search placeholder `search-in` with `$folder`; bookmark search `search-bookmarks`.
- [ ] Nav section titles: 手機, SFTP, FTP, Google Drive, 連線 Google Drive (Drive/SFTP/FTP stay as brand names where the spec catalog marks them untranslated; the Chinese “手機” is translated).
- [ ] Tests that currently `assert_eq!(label, "貼上")` construct state at `ZhTw` and keep passing; add an `En` assertion for `Paste`.
- [ ] `cargo test -p explorer-ui` focused on chrome/navigation.

---

### Task 7: Dialogs, status, a11y, formatting, remaining windows

**Files:**
- Modify: bookmark_* windows, `folder_options_window.rs` remaining copy, `remote_properties_window.rs`, `remote_symlink_window.rs`, `transfer_center_window.rs`, `lib.rs` (About, lock-owner, delete confirm, plugin safe mode)
- Modify: `crates/explorer-ui/src/formatting.rs`
- Modify: `crates/explorer-ui/src/state.rs` operation message builders
- Modify: `crates/explorer-ui/src/icons.rs` accessible labels

- [ ] `format_file_size(bytes, locale)`: En still `"1.0 KB"`; unit strings from `formatting.ftl`.
- [ ] Operation messages (`copy-items`, preparing/copying/complete) use Fluent plurals.
- [ ] AccessKit names from `a11y.ftl`.
- [ ] All remaining Chinese/English UI literals in `explorer-ui` gone (search the crate for CJK and for known English chrome words like `"New tab"`).
- [ ] Tests updated; `cargo test -p explorer-ui` passes.

---

### Task 8: Plugin locale resolution

**Files:**
- Modify: `crates/explorer-extension-host` locale loading (package locale JSON selection)
- Modify: tests/fixtures that already ship `en-US.json` / `zh-TW.json`

- [ ] Resolver: exact tag → negotiate aliases → primary subtag → `en`/`en-US` → manifest display name.
- [ ] Tests: app `ZhTw` picks `zh-TW.json`; app `En` picks `en-US.json`; app `Ja` with only en+zh-TW falls back to `en-US.json`; missing locales array still loads the package.
- [ ] No manifest schema change.

---

### Task 9: SuperDesktop

**Files:**
- Modify: `SuperDesktop/Cargo.toml` (path dep)
- Modify: `SuperDesktop/crates/taskbar-ui/src/system_flyout.rs` (delete `localized()`)
- Modify: `SuperDesktop/crates/taskbar-ui/src/taskbar_settings.rs`
- Modify: SuperDesktop settings-store schema to persist `Option<AppLocale>`
- Modify: SuperDesktop startup to negotiate Windows locale

- [ ] Path dep `explorer-i18n = { path = "../crates/explorer-i18n" }`.
- [ ] Every `localized(presentation, zh, en)` becomes `catalog.t(key)`.
- [ ] Settings persist independent of Explorer `session.json`.
- [ ] Tests for zh-TW and en flyout strings via catalog; completeness already covered by Task 2 `desktop.ftl`.
- [ ] SuperDesktop workspace `cargo test` for the touched crates.

---

### Task 10: Uitest pin, workspace gates, leftover sweep

**Files:**
- Modify: uitest launchers / `explorer-uitest` / scripts that start the app for UIA
- Modify: any test helpers in `explorer-ui/src/harness.rs`

- [ ] Default headful/uitest process env `SUPEREXPLORER_LOCALE=zh-TW` unless a test opts into another locale.
- [ ] Repo sweep: no user-visible CJK or English chrome literals left in `explorer-ui` / SuperDesktop UI crates except inside `.ftl`, tests that pin a locale, and brand names (SFTP, FTP, Google Drive, SuperExplorer).
- [ ] `cargo test -p explorer-i18n`
- [ ] `cargo test -p explorer-model --lib session`
- [ ] `cargo test -p explorer-ui`
- [ ] `cargo test -p explorer-extension-host` (locale selection)
- [ ] `cargo clippy -p explorer-i18n -p explorer-ui -p explorer-model -p explorer-app --all-targets -- -D warnings`

---

## Execution notes

Task 1–2 are the foundation; 3–5 can overlap after 1; 6–7 are the large UI grind and must keep `ZhTw` snapshots green; 8 is independent after 1; 9 needs Task 2 `desktop.ftl`; 10 is the gate.

Do not commit locale files that are missing keys. The completeness test is the release gate, not a later cleanup.
