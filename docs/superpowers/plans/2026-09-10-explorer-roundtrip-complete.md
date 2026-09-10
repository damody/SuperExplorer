# Explorer Import And Reverse Handoff Completion Plan

> **For agentic workers:** Inline execution in this session. The user forbade further confirmation.

**Goal:** First SuperExplorer start recreates every open File Explorer window and every tab; File / More → 轉換回檔案總管 recreates those windows/tabs in File Explorer and closes SuperExplorer; isolated tests never steal the user's live Explorers.

**Architecture:** Snapshot via IShellWindows when it has the window; otherwise read Win11 breadcrumbs (and address-bar Invoke as fallback) so factory-process / orphan Explorers still import. Open reverse-handoff windows with `IShellDispatch.Explore` (new CabinetWClass registered in IShellWindows), extra tabs with WM_COMMAND 0xA21B then `IWebBrowser2.Navigate`. Close only CabinetWClass with SC_CLOSE/WM_CLOSE loops. Never `IWebBrowser2.Quit`, never kill `explorer.exe`.

**Tech Stack:** Rust, Win32 COM (`IShellWindows`, `IShellDispatch`, `IWebBrowser2`), UI Automation (`CUIAutomation`), GPUI SuperExplorer process model.

**Spec:** Locked product UX from this session plus `docs/superpowers/plans/2026-09-10-import-explorer-tabs-and-win-e.md`.

## Global Constraints

- One File Explorer window → one SuperExplorer process/window (and the reverse).
- Skip remote/virtual tabs; if a window has no openable tabs use This PC.
- Do not kill `explorer.exe`. Do not call `IWebBrowser2.Quit`.
- Isolated checks use `D:\se-rt-*` (or `D:\se-live-*`) plus `SUPEREXPLORER_IMPORT_PATH_PREFIX`; isolated `LOCALAPPDATA` / `EXPLORER_LOG_DIR`.
- Do not `Stop-Process` installed SuperExplorer; only `target\debug\SuperExplorer.exe` started by the test.
- Prefix filter: a window is imported only when **every** tab path is under the prefix (case-insensitive string prefix). Mixed user+test windows are skipped, never closed.
- Win+E unchanged: exclusive mutex, This PC on top while SuperExplorer runs.

## Root cause (verified 2026-09-10)

This machine has "Launch folder windows in a separate process" on (`explorer.exe /factory,{75dff2b7-...} -Embedding`). After a shell restart, 23 CabinetWClass windows stay visible but `IShellWindows` lists 1. Snapshot, prefix filter, `explorer_is_showing_target`, and the isolated round-trip all used COM, so they treated live UE5/omfue and the test folders as missing.

UI Automation breadcrumbs reconstruct real paths when the drive crumb contains `(D:)` / `(C:)` and following crumbs are filesystem names (`D:\UE5.8`, `D:\code\omoba\omfue`). Localized crumbs like `使用者` fail `Path::exists`; then Invoke `PART_AutoSuggestBox` yields the real path (`D:\se-probeA-...` verified).

`Shell.Application.Explore` and `Start-Process` of a `D:\` folder both create a **new** CabinetWClass that **does** register with IShellWindows after ~3s.

## Files

- Modify: `crates/explorer-shell-win/src/explorer_tabs.rs` — breadcrumb path helper, UIA location, snapshot fallback, live isolated snapshot test.
- Modify: `crates/explorer-shell-win/src/explorer_handoff.rs` — Explore new window, Navigate extra tabs, wait until the location is visible.
- Modify: `crates/explorer-app/src/explorer_import.rs` — keep prefix filter; live isolated round-trip test launching debug SuperExplorer.
- Unchanged product: close loop, handoff dump/quit, Win+E, menu i18n.

---

### Task 1: Breadcrumb path reconstruction (pure)

**Files:**
- Modify: `crates/explorer-shell-win/src/explorer_tabs.rs`

- [ ] **Step 1: Write failing tests for drive crumb + components**

```rust
assert_eq!(
    path_from_breadcrumb_names(&["此電腦".into(), "新增磁碟區 (D:)".into(), "UE5.8".into()]),
    Some(r"D:\UE5.8".into())
);
assert_eq!(
    path_from_breadcrumb_names(&[
        "此電腦".into(),
        "新增磁碟區 (D:)".into(),
        "code".into(),
        "omoba".into(),
        "omfue".into()
    ]),
    Some(r"D:\code\omoba\omfue".into())
);
assert_eq!(
    path_from_breadcrumb_names(&["此電腦".into(), "本機磁碟 (C:)".into(), "使用者".into()]),
    None // localized component; real path is C:\Users
);
```

`path_from_breadcrumb_names` returns `Some` only when the joined path exists **or** (in unit tests) when we split the exists check: implement `drive_path_from_breadcrumb_names` that always joins, and `path_from_breadcrumb_names` that requires `Path::exists`.

Unit tests assert the join, not exists, via `drive_path_from_breadcrumb_names`.

- [ ] **Step 2: Implement `extract_drive_letter` + `drive_path_from_breadcrumb_names`**

Drive token is `(X:)` anywhere in a crumb. Crumbs after that drive become path components. Empty rest → `X:\`.

- [ ] **Step 3: Run `cargo test -p explorer-shell-win --lib -- explorer_tabs::tests::breadcrumb`**

Expected: PASS

---

### Task 2: Snapshot UIA fallback

**Files:**
- Modify: `crates/explorer-shell-win/src/explorer_tabs.rs`

- [ ] **Step 1: `location_from_uia(parent_hwnd)`**

STA assumed. `CoCreateInstance(CUIAutomation)` → `ElementFromHandle` → find `PART_BreadcrumbBar` → items with class `FileExplorerExtensions.BreadcrumbBarItemControl` → `drive_path_from_breadcrumb_names`. If that path exists, use it. Else Invoke `PART_AutoSuggestBox`, read `IUIAutomationValuePattern::CurrentValue`, PostMessage Escape to restore breadcrumbs.

- [ ] **Step 2: `resolve_tab_location` falls back to UIA on the parent Cabinet hwnd**

COM `collect_shell_tabs` first; UIA second (active tab only). Multi-tab windows still cycle `0xA221` then resolve.

- [ ] **Step 3: Live test opens `D:\se-live-{pid}\alpha`, waits until snapshot contains that path, closes only that hwnd**

Do not use `%TEMP%`. Do not `Quit`.

- [ ] **Step 4: Run `cargo test -p explorer-shell-win --lib -- explorer_tabs explorer_handoff`**

Expected: PASS

---

### Task 3: Reverse handoff opens real new windows

**Files:**
- Modify: `crates/explorer-shell-win/src/explorer_handoff.rs`

- [ ] **Step 1: `launch_explorer(target, new_window=true)` uses `IShellDispatch.Explore`**

`CoCreateInstance(&Shell)` → `Explore(VARIANT from BSTR path)`. Fallback `ShellExecuteW` open if Explore fails.

- [ ] **Step 2: Extra tabs**

`0xA21B` on the new cabinet, wait until `ShellTabWindowClass` count grows, `IWebBrowser2.Navigate` the new tab. Do not Explore extra tabs (that would spawn more windows).

- [ ] **Step 3: Wait until `explorer_is_showing_target` (condition-based, 8s cap) after the first tab**

Replace the old 2s cabinet wait as the success condition.

- [ ] **Step 4: `live_open_creates_explorer_window_then_closes_it` uses `D:\se-handoff-{pid}` not temp**

- [ ] **Step 5: Run handoff tests**

Expected: PASS; no leftover This PC for that path.

---

### Task 4: Isolated user-perspective round-trip test

**Files:**
- Modify: `crates/explorer-app/src/explorer_import.rs` tests

- [ ] **Step 1: Live test `live_isolated_round_trip_under_prefix`**

1. Kill only leftover `target\debug\SuperExplorer.exe` matching this build's image.
2. Create `D:\se-rt-{pid}\alpha|beta|gamma` with a marker file each.
3. Record user cabinet titles that do **not** match `se-rt-`.
4. `open_file_explorer_windows` three windows.
5. Poll `snapshot_open_explorer_windows` until three windows exist whose tabs are all under the prefix (15s).
6. Launch `target/debug/SuperExplorer.exe` with `SUPEREXPLORER_IMPORT_PATH_PREFIX`, `SUPEREXPLORER_UITEST_HANDOFF_AFTER_MS=8000`, isolated `LOCALAPPDATA` and `EXPLORER_LOG_DIR`.
7. Wait until prefix Explorer windows close (imported) and at least one debug SuperExplorer is running.
8. Wait until those debug SuperExplorer processes exit (60s).
9. Snapshot: alpha, beta, gamma present; previously recorded user titles still present.
10. Close only prefix hwnds; delete `D:\se-rt-{pid}`.

Fail (do not kill user Explorers) if prefix windows never appear, SuperExplorer never starts, user titles vanish, or folders are missing after handoff.

- [ ] **Step 2: Run the test; on failure, fix and rerun until pass**

---

### Task 5: Automated + user-perspective verification loop

- [ ] `cargo test -p explorer-shell-win --lib -- explorer_handoff explorer_tabs::tests`
- [ ] `cargo test -p explorer-app --lib -- explorer_import explorer_handoff`
- [ ] `cargo build -p explorer-app --bin SuperExplorer`
- [ ] Isolated round-trip (Task 4) from the user's point of view: three folders leave File Explorer, appear in SuperExplorer, return to File Explorer; UE5/omfue/SuperExplorer windows stay.

On any failure: continue in this session, do not ask the user.
