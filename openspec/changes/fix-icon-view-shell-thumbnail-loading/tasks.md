## 1. Establish visual-policy regression coverage

### 1.1 Model policy contract

**目的：** Lock the requested view-to-visual-source mapping before changing production policy.
**輸入：** Approved capability spec and existing thumbnail-policy tests.
**產出：** Focused Rust regression tests in `explorer-model`.
**依賴：** None.
**Owner／Wave：** Primary agent / Wave 1.
**Gate／Evidence：** G1; focused `cargo test` output recorded in `openspec/changes/fix-icon-view-shell-thumbnail-loading/evidence/unit-tests.txt`.
**完成門檻：** Tests distinguish thumbnail-capable modes from Shell-only modes and initially expose the tile-policy mismatch.

- [x] 1.1.1 Add assertions for 256/96/64 thumbnail targets in extra-large, large, and medium modes.
- [x] 1.1.2 Add assertions that small-icon and tile modes are Shell-icon-only.
- [x] 1.1.3 Run the focused model test and record its pre-fix result.

### 1.2 UI scheduling and admission contract

**目的：** Reproduce starvation and stale-admission failures independently of headful rendering.
**輸入：** Existing UI test helpers, request caches, and completion-event path.
**產出：** Focused `explorer-ui` regression tests.
**依賴：** 1.1.
**Owner／Wave：** Primary agent / Wave 1.
**Gate／Evidence：** G2; named test output in `evidence/unit-tests.txt`.
**完成門檻：** Tests fail on shared-budget starvation or stale current-view admission and cover fallback preservation/cache reuse.

- [x] 1.2.1 Add a regression test proving obsolete-size pending Shell work does not consume current thumbnail capacity.
- [x] 1.2.2 Add a regression test proving thumbnail saturation does not consume current Shell-icon capacity.
- [x] 1.2.3 Add a regression test rejecting a late result whose visual-demand signature is obsolete.
- [x] 1.2.4 Add regression assertions for Shell fallback preservation and compatible completed-cache reuse.

## 2. Implement current-demand icon and thumbnail loading

### 2.1 Correct view-mode policy

**目的：** Align model policy with Windows Explorer behavior requested by the user.
**輸入：** G1 tests and approved mode table.
**產出：** Updated `crates/explorer-model/src/thumbnail.rs`.
**依賴：** 1.1.
**Owner／Wave：** Primary agent / Wave 2.
**Gate／Evidence：** G3; model tests pass in `evidence/unit-tests.txt`.
**完成門檻：** Medium requests 64 px thumbnails; tiles and small icons request Shell icons only.

- [x] 2.1.1 Update the view-mode thumbnail policy without changing unrelated modes or preferences.
- [x] 2.1.2 Run and record the focused model tests.

### 2.2 Isolate visible-work budgets

**目的：** Ensure stale or saturated work in one visual class cannot starve current work in the other.
**輸入：** G2 regressions and current virtualized realized-entry list.
**產出：** Updated scheduling helpers and submission logic in `explorer-ui`.
**依賴：** 1.2 and 2.1.
**Owner／Wave：** Primary agent / Wave 2.
**Gate／Evidence：** G4; scheduling tests pass in `evidence/unit-tests.txt`.
**完成門檻：** Shell and thumbnail capacity are independently bounded using only requests compatible with current demand.

- [x] 2.2.1 Centralize construction and matching of the current visual-demand signature.
- [x] 2.2.2 Exclude obsolete-size or obsolete-context pending Shell work from current Shell capacity.
- [x] 2.2.3 Compute thumbnail capacity independently from Shell pending work.
- [x] 2.2.4 Keep both request classes bounded to realized visible entries.

### 2.3 Reject stale presentation replacement

**目的：** Prevent late work from overwriting the current-size Shell icon or thumbnail.
**輸入：** Visual-demand signature and existing completion-event flow.
**產出：** Updated completion admission and presentation-cache logic.
**依賴：** 2.2.
**Owner／Wave：** Primary agent / Wave 2.
**Gate／Evidence：** G5; stale-result/fallback/cache tests pass in `evidence/unit-tests.txt`.
**完成門檻：** Only matching completions alter presentation; failed/stale thumbnails preserve Shell fallback; compatible completed caches remain reusable.

- [x] 2.3.1 Validate thumbnail completion against the active visual demand before inserting presentation state.
- [x] 2.3.2 Preserve successful completed cache entries separately from active presentation admission.
- [x] 2.3.3 Keep the correct Shell icon visible on thumbnail failure, cancellation, or stale completion.
- [x] 2.3.4 Run and record the focused UI regression suite.

### 2.4 Stabilize maximum-size cache admission

- [x] 2.4.1 Align UI thumbnail concurrency with the two-worker Windows Shell domain.
- [x] 2.4.2 Raise thumbnail raster demand to the actual presentation size when zoom/DPI exceeds the mode baseline.
- [x] 2.4.3 Limit pre-layout and view-switch priming by realized range and half of the configured cache budget.
- [x] 2.4.4 Default the presentation cache to 128 MiB and add Folder Options presets through 1 GiB with persistence and normalization.
- [x] 2.4.5 Add regression tests for maximum-size admission, cache presets, and Folder Options cancel/apply behavior.

### 2.5 Recover maximum-size folder Shell icons

**Purpose:** Prevent a failed exact-size folder request from leaving every folder on the fixed yellow fallback.
**Inputs:** Approved maximum-folder fallback design, current visible/base icon caches, and Shell completion handling.
**Outputs:** Compatible-size cache lookup, bounded real-item recovery, and focused regression tests.
**Dependencies:** 2.4.
**Owner/Wave:** Primary agent / Wave 4.
**Gate/Evidence:** G8; focused tests and headful screenshot recorded in `evidence/maximum-folder-fallback.txt`.
**Completion threshold:** Exact-size preference is preserved, the largest compatible Shell texture is used when exact pixels are absent, and an exact shared-base failure does not permanently suppress recovery.

- [x] 2.5.1 Add focused tests for exact-size preference, largest compatible same-item/shared-base selection, and incompatible-context rejection.
- [x] 2.5.2 Implement compatible-size lookup without changing cache admission or memory limits.
- [x] 2.5.3 Make shared-base exact-size failure fall through to one bounded real-item request instead of permanently failing the folder class.
- [x] 2.5.4 Extend the headful UTIT maximum-zoom assertion to reject the fixed yellow fallback.
- [x] 2.5.5 Run focused compile/tests and record G8 evidence.

## 3. Verify real Windows view behavior

### 3.1 Add headful UTIT coverage

**目的：** Exercise the actual Windows Shell/GPUI path across all affected modes and virtualization boundaries.
**輸入：** A deterministic fixture containing a folder, plain file, bitmap image, and video; existing UTIT harness conventions.
**產出：** UTIT scenario, manifest registration, screenshots, and run log.
**依賴：** 2.3.
**Owner／Wave：** Primary agent / Wave 3.
**Gate／Evidence：** G6; `evidence/utit/` screenshots and log.
**完成門檻：** All five modes pass, thumbnail-capable modes replace fallback where supported, Shell-only modes show correct icons, and newly realized scrolled items load.

- [x] 3.1.1 Create or reuse a deterministic mixed-file fixture for Shell icon and thumbnail verification.
- [x] 3.1.2 Add a UTIT scenario that rapidly switches extra-large, large, medium, small-icon, and tile views.
- [x] 3.1.3 Extend the scenario to scroll previously unrealized items into view and wait for visual readiness.
- [x] 3.1.4 Register the scenario in `uitest/manifest.json` and capture screenshots/log evidence.

### 3.2 Final integration gates

**目的：** Confirm the narrow fix integrates without formatting, compile, or specification regressions.
**輸入：** G3-G6 implementation and evidence.
**產出：** Final verification logs and reviewed diff.
**依賴：** 3.1.
**Owner／Wave：** Primary agent / Wave 3.
**Gate／Evidence：** G7; `evidence/final-verification.txt`.
**完成門檻：** Formatting, focused checks/tests, headful UTIT, strict OpenSpec validation, task-plan validation, and diff review all pass.

- [x] 3.2.1 Run formatting and focused compile/test commands and record output.
- [x] 3.2.2 Run the registered UTIT scenario and record its terminal status and artifacts.
- [x] 3.2.3 Run strict OpenSpec validation and detailed-task validation.
- [x] 3.2.4 Review the final diff for unrelated workspace changes and record the files owned by this change.

### 3.3 Revalidate the maximum-folder correction

- [x] 3.3.1 Run formatting, strict OpenSpec validation, manifest parsing, and focused diff review after the folder fallback correction.
