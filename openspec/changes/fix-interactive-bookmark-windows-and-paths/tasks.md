## 1. Editable path contract

### 1.1 Model and persistence

**目的：** Preserve exact arbitrary Folder/File target text without breaking existing sessions.
**輸入：** Approved design and current bookmark model/serde fixtures.
**產出：** Backward-compatible model representation and focused model tests.
**依賴：** None.
**Owner／Wave：** Primary integrator／wave 0.
**Gate／Evidence：** G1; `evidence/index.json` task records for `1.1.*`.
**完成門檻：** Legacy and raw targets independently round-trip with stable tree metadata.

- [x] 1.1.1 Add a serde-compatible exact raw Folder/File target representation and conversion helpers.
- [x] 1.1.2 Add legacy structured-target restoration and arbitrary raw-target round-trip tests.
- [x] 1.1.3 Verify malformed, offline, remote, and virtual-looking text is not existence-validated or normalized during persistence.

### 1.2 Editor draft semantics

**目的：** Make typed target text editable while retaining target kind and logical destination.
**輸入：** 1.1 model contract and existing bookmark editor draft.
**產出：** Draft update/commit behavior and state tests.
**依賴：** 1.1.
**Owner／Wave：** Primary integrator／wave 1.
**Gate／Evidence：** G2; `evidence/index.json` task records for `1.2.*`.
**完成門檻：** Non-empty exact text saves; empty text fails without mutation; Lua behavior is unchanged.

- [x] 1.2.1 Expose exact Folder/File target text in the bookmark draft and accept payload updates for those kinds.
- [x] 1.2.2 Commit non-empty raw target text without parse/existence validation and retain the draft on empty input.
- [x] 1.2.3 Add UI-state tests for edit, create, empty rejection, logical parent retention, and persistence rollback restoration.

## 2. Dedicated bookmark windows

### 2.1 Interactive bookmark editor

**目的：** Render path targets as focusable editable controls in the existing dedicated window.
**輸入：** 1.2 draft semantics and existing `BookmarkEditorWindow`.
**產出：** Updated editor rendering/subscriptions and regression tests.
**依賴：** 1.2.
**Owner／Wave：** Primary integrator／wave 2.
**Gate／Evidence：** G3; `evidence/index.json` task records for `2.1.*`.
**完成門檻：** Name and target can both receive input; exact saved text reopens unchanged.

- [x] 2.1.1 Replace the read-only Folder/File target summary with a target `EditableTextState` and root dispatch subscription.
- [x] 2.1.2 Preserve Lua source editing, focus order, cancel, remove, and save window lifecycle behavior.
- [x] 2.1.3 Add focused render/source-contract tests for editable typed targets and removal of the read-only label.

### 2.2 Native bookmark manager

**目的：** Replace the non-interactive main-window overlay with one application-owned native manager window.
**輸入：** Existing manager tree projection, root reducer, and child-window observer patterns.
**產出：** Manager window module, observer wiring, single-window lifecycle, and tests.
**依賴：** 2.1.
**Owner／Wave：** Primary integrator／wave 2.
**Gate／Evidence：** G4; `evidence/index.json` task records for `2.2.*`.
**完成門檻：** Manager opens independently, repeated open activates it, mutations refresh it, and no overlay is rendered.

- [x] 2.2.1 Implement `BookmarkManagerWindow` snapshot, focusable render tree, and action dispatch boundary.
- [x] 2.2.2 Wire an application-owned manager window handle/observer with create, activate-existing, close, and stale-handle recovery.
- [x] 2.2.3 Route manager create/edit/rename/reorder/delete commands through the authoritative root reducer and refresh snapshots after mutation or rollback.
- [x] 2.2.4 Remove `bookmark_manager_open` overlay rendering/state while retaining the user-facing manager action.
- [x] 2.2.5 Add focused native-window lifecycle and no-overlay regression tests.

## 3. Toolbar context management

### 3.1 Context entry points

**目的：** Provide full toolbar background, logical-folder, and path-bookmark right-click CRUD.
**輸入：** 2.2 manager action and existing bookmark context menu routing.
**產出：** Context state/actions/rendering with accessible commands.
**依賴：** 2.2.
**Owner／Wave：** Primary integrator／wave 3.
**Gate／Evidence：** G5; `evidence/index.json` task records for `3.1.*`.
**完成門檻：** Every required context target opens the correct dedicated draft or performs the scoped durable delete.

- [x] 3.1.1 Add toolbar-background context state and commands for root folder, root path bookmark, and manager creation.
- [x] 3.1.2 Add logical-folder commands for child folder, child path bookmark, rename, and confirmed delete.
- [x] 3.1.3 Ensure path-bookmark commands expose activate, edit/rename/target, delete, and folder-target new-tab behavior.
- [x] 3.1.4 Prefill new path bookmarks from the selected item or current location while allowing the user to replace all target text.
- [x] 3.1.5 Add context positioning, dismissal, accessibility label, parent-selection, and no-filesystem-delete tests.

### 3.2 Deferred activation errors

**目的：** Report invalid targets at activation without mutating stored bookmarks.
**輸入：** 1.1 raw target conversion and existing bookmark activation handlers.
**產出：** Deferred parse/open path and failure tests.
**依賴：** 1.1, 3.1.
**Owner／Wave：** Primary integrator／wave 3.
**Gate／Evidence：** G6; `evidence/index.json` task records for `3.2.*`.
**完成門檻：** Failed activation displays a notice and exact bookmark data remains unchanged.

- [x] 3.2.1 Resolve raw targets only during activation and dispatch through existing folder/file open behavior when resolvable.
- [x] 3.2.2 Add invalid/unavailable activation tests proving notice emission and zero bookmark mutation.

## 4. Integration and verification

### 4.1 Focused automated validation

**目的：** Demonstrate model, UI, app wiring, formatting, and compilation gates.
**輸入：** Completed phases 1–3.
**產出：** Command logs and `evidence/index.json` with hashes and timestamps.
**依賴：** 1–3.
**Owner／Wave：** Primary integrator／wave 4.
**Gate／Evidence：** G7; `evidence/index.json` task records for `4.1.*`.
**完成門檻：** Every command exits successfully or has an explicit evidence-backed not-applicable disposition; no P0/P1 regression remains.

- [x] 4.1.1 Run `cargo fmt --all -- --check` after scoped formatting.
- [x] 4.1.2 Run focused `explorer-model` bookmark tests.
- [x] 4.1.3 Run focused `explorer-ui` bookmark/window/context tests.
- [x] 4.1.4 Run focused `explorer-app` compile or tests covering application child-window wiring.
- [x] 4.1.5 Record each leaf result, command/manual procedure, exit status, artifact hash, gate, and timestamp in `evidence/index.json`.

### 4.2 Contract and scoped-diff review

**目的：** Confirm traceability, compatibility, and preservation of unrelated worktree edits.
**輸入：** 4.1 evidence and final scoped diff.
**產出：** Passing OpenSpec validation and review evidence.
**依賴：** 4.1.
**Owner／Wave：** Primary integrator／wave 4.
**Gate／Evidence：** G8; `evidence/index.json` task records for `4.2.*`.
**完成門檻：** Strict validation passes, every requirement traces to passing evidence, and unrelated changes are not reverted.

- [x] 4.2.1 Run strict OpenSpec validation and the detailed-task validator.
- [x] 4.2.2 Review proposal-to-spec-to-task traceability and leaf atomicity with no unresolved P0/P1 gap.
- [x] 4.2.3 Review the final scoped diff against the pre-existing dirty worktree and record preserved unrelated paths.

## 5. Confirmed bookmark action window

### 5.1 Native window and reducer boundary

**目的：** Replace bookmark-item overlay menus with one explicitly confirmed native action window.
**輸入：** Completed bookmark windows, stable bookmark IDs, and approved follow-up design.
**產出：** Action-window module, observer wiring, reducer bridge, and lifecycle tests.
**依賴：** 2.2, 3.1.
**Owner／Wave：** Primary integrator／wave 5.
**Gate／Evidence：** G9; `evidence/index.json` task records for `5.1.*`.
**完成門檻：** Right-click opens/retargets one native window; no command dispatches before confirmation.

- [x] 5.1.1 Define typed applicable commands, selected-command state, delete-confirmation stage, and stale-target checks.
- [x] 5.1.2 Implement `BookmarkActionWindow` rendering, keyboard/cancel lifecycle, and explicit Confirm dispatch.
- [x] 5.1.3 Wire an application-owned singleton handle with create, retarget/reset, activate, and stale-handle recovery.
- [x] 5.1.4 Route edit to `BookmarkEditorWindow` and route delete through a second Confirm Delete stage and durable rollback reducer.

### 5.2 Overlay removal and regression validation

**目的：** Remove the old bookmark-item overlay without changing background/folder creation menus.
**輸入：** 5.1 native action window and current right-click projections.
**產出：** Updated chrome/root routing, focused tests, build evidence, and revalidated contracts.
**依賴：** 5.1.
**Owner／Wave：** Primary integrator／wave 6.
**Gate／Evidence：** G10; `evidence/index.json` task records for `5.2.*`.
**完成門檻：** Every bookmark projection opens the action window; focused tests and app check pass; strict OpenSpec validation passes.

- [x] 5.2.1 Replace `bookmark_context_menu` overlay state/rendering with action-window presentation while retaining all five right-click hooks.
- [x] 5.2.2 Add command applicability, default/reset selection, cancel zero-mutation, edit handoff, double-confirm delete, and stale-target tests.
- [x] 5.2.3 Run scoped formatting, bookmark UI tests, and `cargo check -p explorer-app`.
- [x] 5.2.4 Update evidence lineage, run strict OpenSpec/task validation, and review the dirty-worktree scoped diff.

## 6. Dedicated bookmark-folder editor window

### 6.1 Native editor lifecycle

**目的：** Remove the freezing rename overlay and host folder naming in an interactive native window.
**輸入：** Existing folder draft/reducer and approved dedicated-window design.
**產出：** Folder-editor window module, owner observer, and overlay-free host windows.
**依賴：** 2.2.
**Owner／Wave：** Primary integrator／wave 7.
**Gate／Evidence：** G11; focused source contracts and bookmark tests.
**完成門檻：** Add/rename opens one native window, save/cancel uses the shared reducer, and neither host renders the editor overlay.

- [x] 6.1.1 Implement the focusable normal folder-editor window with selected editable name input and Enter/Escape controls.
- [x] 6.1.2 Add root snapshot, observer, presentation, and reducer-dispatch boundaries.
- [x] 6.1.3 Wire an application-owned singleton handle with retarget, activation, and stale-handle recovery.
- [x] 6.1.4 Remove folder-editor inputs and overlay rendering from the explorer and bookmark-manager windows.

### 6.2 Regression validation

**目的：** Verify input lifecycle, compilation, and specification traceability.
**輸入：** 6.1 implementation.
**產出：** Passing focused tests, app check, and strict OpenSpec validation.
**依賴：** 6.1.
**Owner／Wave：** Primary integrator／wave 8.
**Gate／Evidence：** G12; command results and scoped diff review.
**完成門檻：** All focused commands pass and no unrelated dirty-worktree content is reverted.

- [x] 6.2.1 Add a native-window source-contract regression test.
- [x] 6.2.2 Run scoped formatting and focused bookmark UI tests.
- [x] 6.2.3 Run `cargo check -p explorer-app` and strict OpenSpec/task validation.
- [x] 6.2.4 Review the scoped diff and preserve unrelated changes.

## 7. Toolbar bookmark folder drag

### 7.1 Model and native drop routing

**目的：** Move toolbar bookmarks into logical folders with left-button drag.
**輸入：** Typed bookmark tree, GPUI `BookmarkDrag`, and durable reducer path.
**產出：** Parent-move mutation, toolbar sources/targets, and rollback integration.
**依賴：** 3.1.
**Owner／Wave：** Primary integrator／wave 9.
**Gate／Evidence：** G13; focused model/UI tests and app check.
**完成門檻：** Folder and root drops persist correctly, while invalid/same-parent drops are no-ops.

- [x] 7.1.1 Add validated bookmark parent-move mutation with destination append ordering and rollback.
- [x] 7.1.2 Add typed move-to-folder action, state bridge, persistence notice, and rollback handling.
- [x] 7.1.3 Make toolbar bookmark projections native drag sources and folder/root projections drop targets.
- [x] 7.1.4 Add model regression coverage and run focused bookmark tests, app check, formatting, and strict OpenSpec validation.
