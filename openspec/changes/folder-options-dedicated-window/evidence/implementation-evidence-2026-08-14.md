# Folder Options dedicated window implementation evidence — 2026-08-14

## Baseline and source map

- Workspace: `D:\SuperExplorer`; baseline was intentionally dirty and unrelated user/change artifacts were preserved.
- GPUI checkout: `vendor/gpui-ce` at `b3d978d34361087fbb6a9fb92565970943831b4d`.
- Window/controller: `crates/explorer-app/src/application.rs`.
- Dedicated entity and scrolling: `crates/explorer-ui/src/folder_options_window.rs`.
- Page composition: `crates/explorer-ui/src/chrome.rs`.
- Typed draft/reducers: `crates/explorer-ui/src/state.rs` and `crates/explorer-ui/src/actions.rs`.
- Persistence bridge/action integration: `crates/explorer-ui/src/lib.rs`.
- Registered headful entry point: `scripts/smoke_folder_options_extensions_scroll_escape.ps1` and `uitest/manifest.json` case `folder-options-extensions-scroll-escape`.
- The historical overlay-red assertion was superseded because the captured source already had the dedicated shell. The permanent regression assertion now requires `folder-options-overlay` to be absent and all three pages plus OK/Cancel/Apply to remain rendered.

## Current implementation hashes

| File | SHA-256 |
| --- | --- |
| `crates/explorer-ui/src/folder_options_window.rs` | `9B95CB4A0CD900144B811029A3E08D03B025916E942AAFFB05C55695C26B5398` |
| `crates/explorer-ui/src/state.rs` | `76C0A7720BFB502F0FDC48844DD4D9C013EB0287B4B58B48B16E9EAF18C9381E` |
| `crates/explorer-ui/src/chrome.rs` | `F7DB1A620E509086B7BA0C56843C4FF44866A9636AB26506AEEC7A7766FB6C43` |
| `crates/explorer-ui/src/lib.rs` | `DA2179187617D61DD347578DBC968270773D82B1037FCF121A06C27F06026A37` |
| `crates/explorer-app/src/application.rs` | `0B98EE52FACA5D9BE4F07F26CED2B9863CC75AFCBA5A805A754A2FA915F31907` |
| `scripts/smoke_folder_options_extensions_scroll_escape.ps1` | `DD17ADF8DF0CF9464C654412549B1E52668C7D62E1F71C97C125541B0C781FE1` |
| `scripts/UitestHeadful.psm1` | `1927E690B8F21B888E758FF7B47BD5C41169199F2F588DF214AF3D1436A9B739` |

## Focused gates

- `cargo test -p explorer-ui folder_options --lib`: pass.
- `cargo test -p explorer-app folder_options_controller_is_single_instance_retryable_and_idempotently_closed --lib`: pass.
- `cargo check -p explorer-app`: pass; pre-existing warnings remain non-fatal.
- Production binary build: `cargo build -p explorer-app --bin SuperExplorer`: pass.
- Headful command: `powershell.exe -NoProfile -ExecutionPolicy Bypass -File scripts/smoke_folder_options_extensions_scroll_escape.ps1 -OutputDirectory openspec/changes/folder-options-dedicated-window/evidence/headful-2026-08-14-final4 -SkipBuild`: pass.

An additional unscoped `cargo test -p explorer-ui --lib` diagnostic run completed 329 tests successfully and exposed seven pre-existing failures outside Folder Options (details filter/popup source assertions, pre-layout height, This PC localized capacity, Unicode placeholder inventory, icon-cache budget, and stale thumbnail demand). None of the 13 Folder Options tests failed; the change gate remains the focused suite defined above.

The final headful report is `headful-2026-08-14-final4/report.json` (SHA-256 `CE02DC3EB2D6B6BE09DBEDAA1DCACFBFA24F7243B6894F451DDEA4287C36884B`). It records one 168-DPI/175% display, one live options HWND at a time, 960×760 logical and 1680×1330 physical bounds, distinct replacement HWNDs after OK/Cancel/title-close, all keyboard and pointer scroll gestures, fixed footer, modeless owner interaction, and zero background file-view scroll delta.

The available runner has one display fixed at 175%. Physical 100/125/150/200% display switching and multi-monitor movement are therefore evidence-backed not-applicable on this host. Pure coordinate tests cover 100/125/150/200%, while the real headful gate covers the available 175% scale without double conversion.

## Requirement traceability

| Requirement/scenario | Tasks/gate | Evidence |
| --- | --- | --- |
| Dedicated modeless singleton, retry, stale recovery | 1.2.3, 2.1.1–2.1.2, G3 | Controller lifecycle test; final4 HWND sequence and singleton oracle |
| Typed draft, revision, Apply/OK/Cancel failure semantics | 1.2.2–1.2.4, 2.1.3–2.1.6, G2/G3 | State reducer tests; persistence preflight; two-window revision adoption test; final4 action oracles |
| Fixed shell, no overlay, keyboard focus/actions | 2.2.1–2.2.6, G4 | Chrome render assertion; entity keystroke observer scoped to the options HWND; minimum 680×480 window-options test |
| Page-local visible scrolling and DPI safety | 3.1.1–3.1.6, G5 | Shared scrollbar geometry tests; keyboard clamp/fit/resize tests; final4 wheel/track/thumb/Home/End/PageUp/PageDown/per-page offsets |
| Input isolation and capture termination | 3.2.1–3.2.4, G6 | Idempotent terminal test; final4 background delta `0`; modeless owner and Escape/title-close oracles |
| Headful lifecycle and evidence inventory | 4.1.1–4.1.6, G7 | `headful-2026-08-14-final4`; screenshots, logs, report, HWNDs, bounds, DPI, offsets, terminal PASS |
| Strict validation and final traceability | 4.2.1–4.2.6, G8 | Focused commands above, this table, strict OpenSpec validation record |

No failed or unexecuted software leaf remains. The only unavailable matrix dimensions are physical display scales other than 175% and multi-monitor placement, explicitly allowed as not-applicable when the runner cannot set those display states.
