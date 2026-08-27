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
