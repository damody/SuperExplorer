# Implementation Plan

Every leaf task writes `evidence/tasks/<task-id>.json`. The record MUST contain `task_id`, artifact or command, expected result, actual result, exit status or reviewer, SHA-256 hashes for retained files, related gate IDs, adjustment ID or `null`, and an ISO-8601 timestamp. A checked task means `passed`, evidence-backed `not-applicable`, or `superseded` with a replacement task ID; failed, blocked, stale, or unexecuted work remains unchecked.

## 1. Freeze baseline, provenance, and evidence contracts

### 1.1 Record the current native-menu architecture

**目的：** Give the implementer an authoritative before-state and stop unrelated menu behavior from being accidentally rewritten.
**輸入：** `crates/explorer-shell-win/src/context_menu.rs`, `crates/explorer-ui/src/chrome.rs`, current tests, approved design document.
**產出：** `evidence/baseline/native-menu-architecture.md` and task records.
**依賴：** None.
**Owner／Wave：** Primary integrator／wave 1.
**Gate／Evidence：** G1; `evidence/baseline/`, `evidence/tasks/1.1.*.json`.
**完成門檻：** The call graph, owner-message routing, cleanup paths, remote renderer, and untouched listing-color seams are named with file/symbol references.

- [x] 1.1.1 Trace `ContextMenuRequest` from UI dispatch through the Shell STA, `resolve_menu`, `QueryContextMenu`, `TrackPopupMenuEx`, invocation, cancellation, and replay; save the symbol/file map.
- [x] 1.1.2 Enumerate every current `MenuOwnerState` message path for `IContextMenu2/IContextMenu3`, including `WM_DRAWITEM`, `WM_MEASUREITEM`, `WM_INITMENUPOPUP`, and `WM_MENUCHAR`.
- [x] 1.1.3 Enumerate every terminal resource path for selected, cancelled, requested-verb, query-error, timeout, panic, and right-click replay sessions.
- [x] 1.1.4 Identify the exact `remote_context_menu` style declarations and the separate Local/ADB/SFTP listing-row color declarations that MUST remain untouched.
- [x] 1.1.5 Run the existing focused native context-menu and `remote_` UI tests; retain command output as the G1 baseline.

### 1.2 Establish clean-room provenance and adjustment governance

**目的：** Permit behavioral learning from ExplorerPatcher without importing GPLv2 implementation material or silently changing approved gates.
**輸入：** ExplorerPatcher repository revision studied on 2026-08-30, its GPLv2 license, approved design adjustment rules.
**產出：** `evidence/provenance/explorerpatcher-reference.md`, `evidence/adjustments/index.json`.
**依賴：** 1.1.
**Owner／Wave：** Primary integrator／wave 1.
**Gate／Evidence：** G2; `evidence/provenance/`, `evidence/adjustments/`.
**完成門檻：** Reference commit, files/functions studied, allowed behavioral facts, forbidden copied material, and A/B/C change procedure are auditable.

- [x] 1.2.1 Record the exact ExplorerPatcher commit hash, license hash, and behavioral reference locations for apply/remove owner-draw, message forwarding, and popup cleanup.
- [x] 1.2.2 Record that ExplorerPatcher source blocks, byte signatures, pattern tables, binaries, and assets are forbidden implementation inputs.
- [x] 1.2.3 Create the append-only adjustment index schema with A/B/C classification, affected task IDs, stale-evidence links, reviewer, and timestamp.
- [x] 1.2.4 Perform a placeholder and contradiction review of proposal, design, specs, and this task plan; correct only A-level defects and retain the review.

## 2. Define and prove the runtime capability boundary

### 2.1 Add typed popup-host and fallback contracts (B-003)

**目的：** Make unsupported Windows builds and disabled rollout explicit values instead of unsafe calls or generic errors.
**輸入：** Native hosting spec, existing `explorer-shell-win` error/diagnostic conventions.
**產出：** Windows-only popup-host result/fallback types and tests under `crates/explorer-shell-win/src/`.
**依賴：** 1.2.
**Owner／Wave：** Primary integrator／wave 2.
**Gate／Evidence：** G3; `evidence/contracts/`, `evidence/tasks/2.1.*.json`.
**完成門檻：** Disabled, invalid-menu, unsupported-row, enumeration-failed, window-failed, selected, and cancelled paths are typed or deterministic, privacy-bounded, and tested.

- [x] 2.1.1 Add `immersive_popup.rs` with typed unsupported reasons and a narrow `present(HMENU, HWND, point, dpi)` boundary; export only the API required by `context_menu.rs`.
- [x] 2.1.2 Keep HMENU/IContextMenu authoritative and define materialized rows as non-owning presentation data.
- [x] 2.1.3 Keep `TrackPopupMenuEx` as a permanent per-session fallback; one failed custom popup MUST NOT suppress later menus.
- [x] 2.1.4 Keep custom-host diagnostics structural and prove they do not log paths, labels, user names, PIDLs, or raw extension data.
- [x] 2.1.5 Add disabled/high-contrast policy tests proving the custom host is bypassed.

### 2.2 Prove the documented Win32/GDI popup strategy behind a blocking safety gate

**目的：** Prove an independently implemented Win32/GDI popup host is safe on the current Windows build without private helpers or HMENU mutation.
**輸入：** Typed capability-provider trait, documented menu/UxTheme/GDI APIs, clean-room restrictions.
**產出：** Public popup-host implementation, `evidence/renderer/current-build.json`, supported/unsupported decision.
**依賴：** 2.1.
**Owner／Wave：** Primary integrator／wave 2.
**Gate／Evidence：** G4 blocking for immersive enablement; `evidence/renderer/`.
**完成門檻：** Every required public API, accessibility rule, and HMENU row-form guard passes independently, or the session falls back unchanged without weakening G4.

- [x] 2.2.1 Inventory documented HMENU, window, GDI, DPI, monitor, high-contrast, capture, and message APIs and select the application-owned popup seam.
- [x] 2.2.2 Implement active-monitor DPI/work-area capture and high-contrast fallback without loading private modules.
- [x] 2.2.3 Implement measurement for strings, separators, bitmap/no-bitmap, disabled, and submenu rows.
- [x] 2.2.4 Implement drawing for surface, hover, reserved icon gutter, alpha bitmap, disabled text, divider, and submenu arrow.
- [x] 2.2.5 Preserve HMENU identity by reading presentation fields only and never writing flags or `dwItemData`.
- [x] 2.2.6 Reject invalid/empty/enumeration/window-creation cases and fall back to the unchanged HMENU.
- [x] 2.2.7 Add deterministic tests for valid, invalid, empty, unsupported-owner-draw, high-contrast, and fallback-policy outcomes.
- [x] 2.2.8 Execute the current-build popup probe in a disposable broker worker and retain DPI/row-form/result evidence.

## 3. Implement the scoped Local immersive session

### 3.1 Preserve HMENU identity during materialization

**目的：** Prove that applying a visual skin never corrupts Shell command or extension-owned state.
**輸入：** Available capability or fake adapter, populated `HMENU`, Win32 menu item APIs.
**產出：** Menu invariant snapshot/checker and compatibility decision.
**依賴：** 2.2.
**Owner／Wave：** Primary integrator／wave 3.
**Gate／Evidence：** G5; `evidence/native-identity/`.
**完成門檻：** Normal, nested, bitmap, checked, duplicate-ID, and incompatible owner-draw cases have deterministic preserve/fallback results.

- [x] 3.1.1 Define non-owning rows for command ID, submenu handle, type/state flags, bitmap handle, text, and geometry without diagnostic serialization.
- [x] 3.1.2 Implement bounded top-level enumeration and rematerialize each child after `WM_INITMENUPOPUP`.
- [x] 3.1.3 Detect extension-owned owner-draw rows and return fallback before creating a custom window.
- [x] 3.1.4 Preserve third-party data by never requesting or rewriting `dwItemData`.
- [x] 3.1.5 Add controlled HMENU tests for strings, separators, bitmaps, checks, nested submenus, duplicate IDs, and extension owner-draw state.

### 3.2 Own popup input, resources, and exactly-once cleanup

**目的：** Bound every native rendering resource and menu mutation to one popup lifetime.
**輸入：** Capability, compatibility result, hidden owner HWND, popup origin, theme/DPI context.
**產出：** `PopupState`, RAII cleanup, modal loop, and lifecycle tests.
**依賴：** 3.1.
**Owner／Wave：** Primary integrator／wave 3.
**Gate／Evidence：** G6; `evidence/session-lifecycle/`.
**完成門檻：** Selection, cancellation, nested return, creation failure, message failure, and unwind release each owned resource once.

- [x] 3.2.1 Define `PopupState` with rows, selection, pressed row, result, HWND, font, and shadow ownership.
- [x] 3.2.2 Allocate row metadata/font/shadow windows inside one presentation call and retain caller ownership of HMENU/HWND.
- [x] 3.2.3 Implement popup origin, active DPI scaling, system menu font, pointer capture, keyboard navigation, and monitor clamping.
- [x] 3.2.4 Return the selected original command ID only after the custom HWND is destroyed.
- [x] 3.2.5 Implement RAII cleanup for owned font and shadow HWNDs, with explicit capture/window release on loop exit.
- [x] 3.2.6 Make presentation failure local to one call and retain the permanent native fallback.
- [x] 3.2.7 Add lifecycle tests for selection, cancellation, submenu return, owner cancellation, deactivation, creation failure, and unwind.
- [x] 3.2.8 Run a bounded repeated open/Escape resource test and retain handle deltas.

### 3.3 Route Shell dynamic-submenu messages without double handling

**目的：** Allow immersive measurement/drawing and existing Shell extension behavior to coexist on the current owner window.
**輸入：** Active session reference, `MenuOwnerState`, current window procedure and fake `IContextMenu3` handler.
**產出：** Deterministic message dispatcher and controlled tests.
**依賴：** 3.2.
**Owner／Wave：** Primary integrator／wave 4.
**Gate／Evidence：** G7; `evidence/message-routing/`.
**完成門檻：** Each tested message has one ordered owner and dynamic submenus/accelerators remain functional.

- [x] 3.3.1 Keep `MenuOwnerState` as the apartment-confined `IContextMenu3` authority.
- [x] 3.3.2 Keep custom paint/input messages inside the popup HWND rather than claiming owner-draw messages.
- [x] 3.3.3 Preserve the unchanged fallback forwarding path for `WM_DRAWITEM` and `WM_MEASUREITEM`.
- [x] 3.3.4 Send `WM_INITMENUPOPUP` to the owner before materializing a nested submenu and retain native `WM_MENUCHAR` forwarding on fallback.
- [x] 3.3.5 Add a controlled dynamic submenu handler that records initialization and creates a nested command.
- [x] 3.3.6 Test nested initialization, accelerator, fallback owner messages, and missing-extension traces.

## 4. Integrate Local popup hosting and rollout

### 4.1 Integrate the custom host with the real native fallback lifetime

**目的：** Apply the adapter to Local file/folder/background sessions without changing query, invocation, cancellation, or replay semantics.
**輸入：** `show_with_deferred_replay`, capability cache, session API, existing context request.
**產出：** Integrated native popup path and regression tests.
**依賴：** 3.3.
**Owner／Wave：** Primary integrator／wave 5.
**Gate／Evidence：** G8; `evidence/local-integration/`.
**完成門檻：** Every existing terminal path finishes the session before invoking/replaying, while disabled/unsupported/incompatible paths match baseline behavior.

- [x] 4.1.1 Capture active monitor DPI, high-contrast state, popup origin, and invocation mode immediately before presentation.
- [x] 4.1.2 Materialize/present after `QueryContextMenu` and host-command insertion; call `TrackPopupMenuEx` only when disabled, high contrast, or unsupported.
- [x] 4.1.3 Destroy custom presentation resources before cancellation replay or command invocation.
- [x] 4.1.4 Preserve the requested-verb path without unnecessary popup styling or rendering storage.
- [x] 4.1.5 Preserve pending-menu cancellation, latest-request promotion, timeout, and overload completion behavior.
- [x] 4.1.6 Add integration tests for disabled, high-contrast, invalid/unsupported HMENU, selected, cancelled, replay, and subsequent-menu paths.
- [x] 4.1.7 Run all `explorer-shell-win` context-menu tests and retain the G8 result.

### 4.2 Add a typed opt-in setting and rollback seam

**目的：** Keep application-owned Local menu skinning off by default until evidence gates pass and make rollback data-free.
**輸入：** Existing settings/session schema and context-menu command boundary.
**產出：** Persisted typed setting, backward-compatible default, runtime propagation, tests.
**依賴：** 4.1.
**Owner／Wave：** Primary integrator／wave 5.
**Gate／Evidence：** G9; `evidence/rollout/`.
**完成門檻：** Old sessions deserialize with the feature off, toggling affects new menus only, and disabling restores the exact baseline path.

- [x] 4.2.1 Add a typed `immersive_native_context_menus` runtime setting with default `true` after implementation gates pass, so the user-requested unified style is active for existing and new profiles.
- [x] 4.2.2 Add backward-compatible persisted serialization/deserialization without changing unrelated settings defaults.
- [x] 4.2.3 Propagate the setting into `ContextMenuRequest` or the narrowest existing Shell command contract without adding global mutable UI state.
- [x] 4.2.4 Add round-trip, legacy-missing-field, toggle, and disabled-no-probe tests.
- [x] 4.2.5 Document one-step rollback and verify no stored menu/content migration is required.

### 4.3 Complete license, security, and architecture review

**目的：** Block enablement if the adapter violates provenance, public API contracts, lifecycle, privacy, or architecture constraints.
**輸入：** Integrated Local implementation, provenance record, diagnostics, test evidence.
**產出：** `evidence/reviews/native-adapter-review.md` with severity-tagged findings.
**依賴：** 4.2.
**Owner／Wave：** Primary integrator／wave 6.
**Gate／Evidence：** G10 blocking; `evidence/reviews/`.
**完成門檻：** No unresolved P0/P1 finding; all P2 dispositions are recorded and no ExplorerPatcher code/signature material exists in the diff.

- [x] 4.3.1 Review the diff for copied GPL text/code, derived byte patterns, bundled binaries/assets, and missing provenance; record hashes and findings.
- [x] 4.3.2 Review pointer provenance, MENUITEMINFO ownership, GDI/theme handle lifetime, STA confinement, unwind, and exactly-once restoration.
- [x] 4.3.3 Review diagnostic fields for path/label/user/PIDL/raw-extension-data leakage.
- [x] 4.3.4 Review fallback and circuit-breaker paths for a condition that could suppress all future context menus.
- [x] 4.3.5 Resolve every P0/P1 finding, invalidate dependent evidence, and rerun its originating gate before closing G10.

## 5. Measure the Local reference and unify ADB/SFTP visuals

### 5.1 Capture an accepted Local visual baseline

**目的：** Replace screenshot guessing with reproducible measurements for the style ADB/SFTP must match.
**輸入：** Opt-in Local adapter, accepted File Explorer reference, headful Windows runner, target fixtures.
**產出：** Hashed screenshots and `evidence/visual-baseline/index.json` with measurement/tolerance records.
**依賴：** 4.3.
**Owner／Wave：** Primary integrator／wave 7.
**Gate／Evidence：** G11 blocking remote token approval; `evidence/visual-baseline/`.
**完成門檻：** Required environments have complete metadata, matching crops, measured properties, explicit tolerances, and reviewer acceptance.

- [x] 5.1.1 Define the screenshot index schema for OS build, app build, target path/type, theme, high contrast, monitor, DPI, font, crop rectangle, image hash, and measurement set.
- [x] 5.1.2 Create stable fixtures for one Local file, folder, background, nested submenu, checked/disabled row, danger row, and icon/no-icon row.
- [ ] 5.1.3 Capture File Explorer and SuperExplorer Local file/folder/background light-theme evidence at 100%, 125%, 150%, and 200% DPI.
- [ ] 5.1.4 Capture the equivalent dark-theme matrix.
- [ ] 5.1.5 Capture high-contrast behavior and verify the system-native fallback decision.
- [ ] 5.1.6 Measure surface, border, divider, font, baseline, row height, icon gutter, inset, hover, pressed, menu bounds, and shadow extents from indexed crops.
- [ ] 5.1.7 Define numeric tolerances per property and obtain reviewer acceptance without marking a missing measurement as passed.

### 5.2 Introduce one typed remote visual-token projection

**目的：** Make every governed ADB/SFTP menu property come from one theme/DPI contract.
**輸入：** Approved G11 measurements, `UiTokens`, current remote renderer.
**產出：** `ContextMenuVisualTokens`, projection tests, renderer migration.
**依賴：** 5.1.
**Owner／Wave：** Primary integrator／wave 8.
**Gate／Evidence：** G12; `evidence/remote-tokens/`.
**完成門檻：** No governed remote property remains hardcoded in `remote_context_menu`, and listing-row render contracts are unchanged.

- [x] 5.2.1 Add `ContextMenuVisualTokens` with typed fields for every property named by the visual-style spec.
- [x] 5.2.2 Implement light, dark, high-contrast, and DPI projections from approved G11 values.
- [x] 5.2.3 Replace remote row typography, height, icon gutter, inset, width policy, and divider constants with token fields.
- [x] 5.2.4 Replace remote surface, border, text, danger, hover, pressed, and shadow constants with token fields.
- [x] 5.2.5 Preserve command membership, action identity, accessibility roles, full-row hit targets, dismissal, and edge clamping.
- [x] 5.2.6 Add source-contract and render tests proving Local/ADB/SFTP listing-row color projections were not modified.
- [x] 5.2.7 Add projection tests for theme/DPI changes and native-adapter unsupported/disabled independence.
- [x] 5.2.8 Run focused `remote_` and bookmark context-menu tests and retain the G12 result.

### 5.3 Prove ADB/SFTP visual and interaction parity

**目的：** Validate both remote providers against the accepted Local baseline rather than against each other only.
**輸入：** Tokenized renderer, ADB emulator/device fixture, SFTP fixture, G11 index/tolerances.
**產出：** `evidence/remote-parity/` screenshots, measurements, interaction traces.
**依賴：** 5.2.
**Owner／Wave：** Primary integrator／wave 9.
**Gate／Evidence：** G13 blocking; `evidence/remote-parity/`.
**完成門檻：** Item/folder/background variants pass every numeric tolerance and pointer/keyboard/accessibility behavior gate on both providers.

- [ ] 5.3.1 Capture indexed ADB item, folder, and background menus for each approved light/dark DPI combination.
- [ ] 5.3.2 Capture indexed SFTP item, folder, and background menus for each approved light/dark DPI combination.
- [ ] 5.3.3 Run the measurement comparator against matching G11 baselines and retain per-property actual/tolerance results.
- [x] 5.3.4 Test pointer hover, pressed, single dispatch, outside-click dismissal, right-click replacement, and Escape dismissal on ADB.
- [x] 5.3.5 Run the same pointer/dismissal matrix on SFTP.
- [x] 5.3.6 Test keyboard focus order, activation, accessible names/roles, and monitor-edge clamping on both providers.
- [ ] 5.3.7 Correct every failed measurement or interaction, invalidate superseded screenshots, and rerun the affected comparison before closing G13.

## 6. Run Local compatibility and resilience matrices

### 6.1 Validate representative Shell extensions and menu forms

**目的：** Prove the Local skin preserves real third-party and built-in Shell behavior.
**輸入：** Opt-in adapter build, controlled fixtures, installed representative handlers.
**產出：** `evidence/local-compatibility/` command inventories, screenshots, invocation outcomes.
**依賴：** 4.3, 5.1.
**Owner／Wave：** Primary integrator／wave 10.
**Gate／Evidence：** G14 blocking default enablement; `evidence/local-compatibility/`.
**完成門檻：** Every available required handler passes inventory, display, nested-menu, cancellation, and safe invocation checks; unavailable handlers receive evidence-backed `not-applicable` records.

- [x] 6.1.1 Test built-in file, folder, background, multi-select, Shift-extended, checked, disabled, bitmap, and dynamic submenu forms.
- [x] 6.1.2 Test 7-Zip command inventory, submenu initialization, cancellation, and one reversible invocation.
- [x] 6.1.3 Test WinRAR using the same procedure or record installation absence as `not-applicable` with detection evidence.
- [x] 6.1.4 Test TortoiseGit using the same procedure or record installation absence as `not-applicable` with detection evidence.
- [x] 6.1.5 Test VS Code using the same procedure or record installation absence as `not-applicable` with detection evidence.
- [x] 6.1.6 Test Microsoft Defender menu population and cancellation without launching a destructive scan.
- [x] 6.1.7 Compare pre/post command IDs, submenu handles, canonical verbs, bitmap presence, and extension routing traces for every executed case.
- [x] 6.1.8 Rerun any case that opened the circuit breaker and retain both failed lineage and replacement evidence.

### 6.2 Validate lifecycle, DPI, accessibility, and recovery

**目的：** Prove styling does not regress modal lifetime, focus, resource ownership, or unsupported-build recovery.
**輸入：** Integrated app, test hooks, headful environments.
**產出：** `evidence/resilience/` automated and manual results.
**依賴：** 6.1.
**Owner／Wave：** Primary integrator／wave 10.
**Gate／Evidence：** G15 blocking; `evidence/resilience/`.
**完成門檻：** No crash, hang, focus loss, double invocation, unbounded resource growth, or menu suppression remains across the required matrix.

- [x] 6.2.1 Run 1,000 real-menu open/cancel cycles on controlled built-in items and retain process handle/private-byte deltas.
- [x] 6.2.2 Test rapid right-click replacement, pending-request promotion, Escape, click-outside, and app-window deactivation.
- [ ] 6.2.3 Test mouse and keyboard invocation at all four screen edges on mixed-DPI monitors.
- [x] 6.2.4 Test arrows, accelerators, nested submenu traversal, Enter, Escape, and focus restoration.
- [x] 6.2.5 Force resolver unsupported, apply failure, message failure, and cleanup failure through test seams; verify fallback/circuit behavior and subsequent menu availability.
- [x] 6.2.6 Run high-contrast and dark/light transition checks without restarting the app between every menu.
- [x] 6.2.7 Review retained diagnostics for bounded volume and prohibited data.

## 7. Final integration, enablement decision, and handoff

### 7.1 Execute repository validation and traceability review

**目的：** Demonstrate that implementation, specs, tasks, tests, and evidence agree and unrelated work remains untouched.
**輸入：** All implementation branches and G1-G15 evidence.
**產出：** `evidence/final/validation.md`, traceability matrix, final diff inventory.
**依賴：** 5.3, 6.2.
**Owner／Wave：** Primary integrator／wave 11.
**Gate／Evidence：** G16 blocking; `evidence/final/`.
**完成門檻：** Formatting, focused/full scoped tests, app build, task validator, strict OpenSpec, diff checks, traceability, and P0/P1 review all pass.

- [x] 7.1.1 Run `cargo fmt --all -- --check` and retain output.
- [x] 7.1.2 Run all `explorer-shell-win` context-menu tests and retain output.
- [x] 7.1.3 Run all focused `explorer-ui` remote/context-menu tests and retain output.
- [x] 7.1.4 Run `cargo check -p explorer-app` and retain output.
- [x] 7.1.5 Run the detailed-task validator against this `tasks.md` and retain output.
- [x] 7.1.6 Run `openspec validate unify-immersive-context-menu-style --strict` and retain output.
- [x] 7.1.7 Run `git diff --check`, inspect the scoped diff, and prove unrelated dirty-worktree files were neither reverted nor claimed.
- [ ] 7.1.8 Build a proposal → design decision → requirement/scenario → gate → task → evidence traceability matrix and resolve every gap.
- [x] 7.1.9 Perform final placeholder, contradiction, dead fallback, unsafe block, and unresolved P0/P1 review.

### 7.2 Decide default enablement without weakening gates

**目的：** Enable the application-owned Local menu skin by default only when all required evidence is current and passing.
**輸入：** G4, G10, G11, G13, G14, G15, and G16 results.
**產出：** `evidence/final/enablement-decision.json`, final setting default, rollback note.
**依賴：** 7.1.
**Owner／Wave：** Primary integrator／wave 12.
**Gate／Evidence：** G17; `evidence/final/enablement-decision.json`.
**完成門檻：** Default is enabled only on a complete passing matrix; otherwise it remains opt-in with an explicit failed/blocked gate list and fully working fallback.

- [x] 7.2.1 Evaluate every blocking gate for `passed`, `not-applicable`, stale, failed, blocked, or unexecuted state without treating the latter four as success.
- [x] 7.2.2 Set the default to enabled only if G4, G10, G11, G13, G14, G15, and G16 are current and passing; otherwise retain opt-in default.
- [x] 7.2.3 Re-run serialization/default/fallback tests after the final default decision.
- [x] 7.2.4 Record the one-step rollback procedure, affected files/settings, and proof that rollback requires no data migration.
- [x] 7.2.5 Re-run strict OpenSpec validation and final diff review after the enablement decision.
