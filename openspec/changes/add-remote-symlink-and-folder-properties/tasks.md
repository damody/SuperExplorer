# Detailed Implementation Plan

Every completed leaf writes `evidence/tasks/<task-id>.json` containing the procedure or command,
expected and actual result, exit status/reviewer, artifact hashes, gate IDs, adjustment ID when
applicable, and timestamp. Failed, blocked, stale, or unexecuted evidence is not completion.

## 1. Contract and backend implementation

### 1.1 Provider-neutral operations

**目的：** Define one safe contract for symbolic-link creation and single-location metadata.
**輸入：** Approved design; current `RemoteProvider`, `VirtualLocationDescriptor`, and metadata models.
**產出：** Typed provider APIs, result model/projection, fake-provider updates, and contract tests.
**依賴：** None.
**Owner／Wave：** Primary agent / Wave 1.
**Gate／Evidence：** G1; `evidence/tasks/1.1.*.json`.
**完成門檻：** Every provider compiles; target remains opaque; cancellation and metadata semantics are tested.

- [ ] 1.1.1 Add provider-neutral symbolic-link creation and single-location metadata methods with cancellation to `RemoteProvider`.
- [ ] 1.1.2 Define or reuse a metadata result that can project the current directory into the existing remote Properties snapshot without a synthetic listing row.
- [ ] 1.1.3 Update every fake/test provider with deterministic implementations for the two new contract methods.
- [ ] 1.1.4 Add contract tests for opaque relative/absolute/dangling targets, cancellation, and partial metadata.

### 1.2 ADB backend

**目的：** Implement injection-safe ADB link creation and authoritative current-location metadata.
**輸入：** 1.1 provider contract; existing ADB runner/stat parser.
**產出：** ADB implementation and focused tests.
**依賴：** 1.1.
**Owner／Wave：** Primary agent / Wave 2.
**Gate／Evidence：** G1; `evidence/tasks/1.2.*.json`.
**完成門檻：** Exact target/name bytes stay separate from fixed shell source; success/failure/cancellation tests pass.

- [ ] 1.2.1 Implement ADB symbolic-link creation using a fixed script with positional arguments and `ln -s --`.
- [ ] 1.2.2 Implement ADB metadata for the descriptor itself using bounded cancellable stat parsing.
- [ ] 1.2.3 Test escaping-sensitive link names/targets and prove untrusted text is not interpolated into shell source.
- [ ] 1.2.4 Test ADB dangling-link success, duplicate/permission failure propagation, and cancellation.

### 1.3 SFTP backend

**目的：** Implement SFTP protocol link creation and current-location metadata.
**輸入：** 1.1 provider contract; current SFTP session/profile lookup.
**產出：** SFTP implementation and focused tests.
**依賴：** 1.1.
**Owner／Wave：** Primary agent / Wave 2.
**Gate／Evidence：** G1; `evidence/tasks/1.3.*.json`.
**完成門檻：** Protocol routing preserves target exactly and all deterministic success/failure tests pass.

- [ ] 1.3.1 Implement symbolic-link creation through the SFTP protocol operation.
- [ ] 1.3.2 Implement SFTP metadata for the descriptor itself without following or scanning descendants unnecessarily.
- [ ] 1.3.3 Test relative/absolute/dangling targets and exact protocol argument ordering.
- [ ] 1.3.4 Test duplicate/protocol failure propagation and cancellation.

## 2. Dedicated shortcut editor and async state

### 2.1 Owned editor window

**目的：** Provide a separately interactive two-field editor that retains recoverable input.
**輸入：** Approved UX; existing bookmark editor and remote Properties window patterns.
**產出：** `remote_symlink_window` module, public snapshot/request boundary, render tests.
**依賴：** 1.1 contract shape.
**Owner／Wave：** Primary agent / Wave 3.
**Gate／Evidence：** G2; `evidence/tasks/2.1.*.json`.
**完成門檻：** Window is owned/reused, both fields are editable, validation/focus/error/progress behavior is deterministic.

- [ ] 2.1.1 Add centered owned window options and a snapshot containing parent location plus initial field values.
- [ ] 2.1.2 Implement editable link-name and target inputs with tab order, Cancel, Create, and accessibility labels.
- [ ] 2.1.3 Implement child-name and nonempty-target validation while preserving both inputs on error.
- [ ] 2.1.4 Implement busy/error rendering that prevents duplicate dispatch and retains inputs after provider failure.
- [ ] 2.1.5 Add source/render tests for field labels, buttons, focus order, invalid names, dangling targets, and repeated-window replacement.

### 2.2 ExplorerRoot request lifecycle

**目的：** Submit link creation off GPUI and apply only context-current completions.
**輸入：** 1.1 operations; 2.1 editor request; existing remote job/coordinator and refresh routes.
**產出：** UI state/actions/completions and lifecycle tests.
**依賴：** 1.1, 2.1.
**Owner／Wave：** Primary agent / Wave 4.
**Gate／Evidence：** G2; `evidence/tasks/2.2.*.json`.
**完成門檻：** Exactly one job dispatches; success refreshes/selects; failure retains editor; stale completion is inert.

- [ ] 2.2.1 Add immutable create-link request/completion state capturing tab, generation, parent, destination, and target.
- [ ] 2.2.2 Route create-link work through the existing remote worker/coordinator rather than the GPUI thread.
- [ ] 2.2.3 Apply current success by closing the editor, refreshing the captured directory, and selecting the new link when listed.
- [ ] 2.2.4 Apply failure to the matching editor without clearing inputs or reporting success.
- [ ] 2.2.5 Reject completion after navigation, generation replacement, editor replacement, or cancellation.
- [ ] 2.2.6 Add state tests for validation no-dispatch, single dispatch, success refresh/selection, retained failure, and every stale-result boundary.

## 3. Background commands and current-directory Properties

### 3.1 Menu command integration

**目的：** Add Create Shortcut and Properties to remote backgrounds without changing item menus or visual behavior.
**輸入：** 2.1 editor observer; accepted remote menu renderer and current command grammar.
**產出：** Background command rows/actions and membership/interaction tests.
**依賴：** 2.1.
**Owner／Wave：** Primary agent / Wave 4.
**Gate／Evidence：** G3; `evidence/tasks/3.1.*.json`.
**完成門檻：** Both providers show the specified order; item menus and all existing menu interaction contracts remain unchanged.

- [ ] 3.1.1 Add `新增捷徑` after `新增資料夾` and background `內容` using the shared Windows-style command renderer.
- [ ] 3.1.2 Wire `新增捷徑` to close the menu and open/replace the owned editor for the captured current location.
- [ ] 3.1.3 Add membership/order tests for ADB/SFTP background and unchanged item menus.
- [ ] 3.1.4 Run existing remote visual, dismissal, keyboard, accessibility, replacement, and edge-clamp tests.
- [x] 3.1.5 Add folder-item `新增捷徑` for ADB/SFTP, derive the first free sibling name, and submit
  the clicked folder display name as a relative target without opening the editor.
- [x] 3.1.6 Test folder-only membership, collision suffixing, direct async dispatch, refresh/selection,
  provider failure, and stale completion behavior.

### 3.2 Current-directory metadata and Properties

**目的：** Display authoritative metadata for the directory currently being viewed.
**輸入：** 1.1 metadata operation; existing remote Properties observer/window.
**產出：** Background metadata request/completion route, snapshot projection, and tests.
**依賴：** 1.1.
**Owner／Wave：** Primary agent / Wave 4.
**Gate／Evidence：** G2, G3; `evidence/tasks/3.2.*.json`.
**完成門檻：** Current context opens one truthful Properties snapshot; failure and stale completion cannot open a false window.

- [ ] 3.2.1 Add current-directory metadata request state capturing tab, generation, and location without synthesizing a selected row.
- [ ] 3.2.2 Project provider metadata into the existing Properties snapshot with explicit unavailable optional fields.
- [ ] 3.2.3 Wire background `內容` to the async metadata route and existing owned-window observer.
- [ ] 3.2.4 Reject metadata failure and stale navigation results without opening or replacing a false Properties window.
- [ ] 3.2.5 Add tests for complete/partial metadata, canonical public path, current/stale completion, failure, and unchanged item Properties.

## 4. Provider integration verification

### 4.1 ADB headful verification

**目的：** Prove the real emulator creates and reports a disposable dangling link and current-folder Properties.
**輸入：** Built app; online ADB device; stable writable fixture directory.
**產出：** Screenshots, UIA report, command transcript, readlink/stat result, cleanup proof.
**依賴：** 1–3 complete.
**Owner／Wave：** Primary agent / Wave 5.
**Gate／Evidence：** G4; `evidence/tasks/4.1.*.json` plus `evidence/headful/adb/`.
**完成門檻：** Create/readback/Properties/dismissal pass and the disposable link is removed.

- [ ] 4.1.1 Extend the headful harness to open both new background commands and interact with the owned editor/Properties window.
- [ ] 4.1.2 Create a uniquely named ADB dangling link through UI and verify its exact target with `readlink`.
- [ ] 4.1.3 Open ADB current-directory Properties and verify path/type/permission fields through UIA and screenshot evidence.
- [ ] 4.1.4 Remove the disposable ADB link and retain cleanup proof.

### 4.2 SFTP headful verification

**目的：** Prove the saved SFTP profile creates and reports a disposable dangling link and current-folder Properties.
**輸入：** Built app; reachable saved SFTP profile; stable writable fixture directory.
**產出：** Screenshots, UIA report, protocol/readback result, cleanup proof.
**依賴：** 1–3 complete.
**Owner／Wave：** Primary agent / Wave 5.
**Gate／Evidence：** G4; `evidence/tasks/4.2.*.json` plus `evidence/headful/sftp/`.
**完成門檻：** Create/readback/Properties/dismissal pass and the disposable link is removed.

- [ ] 4.2.1 Create a uniquely named SFTP dangling link through UI and verify its exact target through the provider probe.
- [ ] 4.2.2 Open SFTP current-directory Properties and verify path/type/permission fields through UIA and screenshot evidence.
- [ ] 4.2.3 Remove the disposable SFTP link and retain cleanup proof.
- [ ] 4.2.4 Compare ADB/SFTP editor and Properties interaction behavior and record any provider-specific unavailable fields without weakening requirements.

## 5. Final validation and handoff

### 5.1 Automated quality gates

**目的：** Establish a clean, reproducible final implementation result.
**輸入：** All implementation and headful evidence.
**產出：** Command outputs and unique evidence records.
**依賴：** 1–4 complete.
**Owner／Wave：** Primary agent / Wave 6.
**Gate／Evidence：** G5; `evidence/tasks/5.1.*.json`.
**完成門檻：** Every command exits zero and every affected test suite passes without ignored failures.

- [ ] 5.1.1 Run `cargo fmt --all -- --check`.
- [ ] 5.1.2 Run focused `explorer-remote` ADB/SFTP provider tests.
- [ ] 5.1.3 Run focused `explorer-ui` remote menu/window/state tests.
- [ ] 5.1.4 Run `cargo check -p explorer-app --locked`.
- [ ] 5.1.5 Run `openspec validate add-remote-symlink-and-folder-properties --strict`.
- [ ] 5.1.6 Run the detailed-task validator and `git diff --check`.

### 5.2 Final scoped review

**目的：** Confirm traceability, safety, cleanup, and unrelated-work preservation before handoff.
**輸入：** Proposal, design, specs, tasks, diff, all evidence.
**產出：** Traceability/review report and rollback record.
**依賴：** 5.1.
**Owner／Wave：** Primary agent / Wave 6.
**Gate／Evidence：** G5; `evidence/tasks/5.2.*.json` and `evidence/final/`.
**完成門檻：** No unresolved P0/P1, no leaked target/path diagnostics, no test artifacts remain remotely, and every requirement scenario traces to passing evidence.

- [ ] 5.2.1 Review ADB script separation, child-name containment, cancellation, stale results, error redaction, and GPUI-thread isolation.
- [ ] 5.2.2 Build proposal → decision → requirement/scenario → gate → task → evidence traceability and resolve every gap.
- [ ] 5.2.3 Verify remote disposable links are absent and record the one-step source rollback/no-migration procedure.
- [ ] 5.2.4 Inspect the scoped diff, preserve unrelated dirty-worktree changes, resolve all P0/P1 findings, and rerun affected gates.
