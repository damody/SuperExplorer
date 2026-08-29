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

## 8. Firefox-style folder content menu

### 8.1 Pointer-button behavior separation

**目的：** Keep left-click folder menus browse-only and right-click menus management-only.
**輸入：** Existing folder panel, nested bookmark tree, and toolbar context menu.
**產出：** Ordered immediate content rows, child-folder navigation, and regression tests.
**依賴：** 3.1, 7.1.
**Owner／Wave：** Primary integrator／wave 10.
**Gate／Evidence：** G14; focused UI tests and application check.
**完成門檻：** Left-click contains no mutation commands; root and nested right-click retain management commands.

- [x] 8.1.1 Replace recursive flattened entries with ordered immediate folder/bookmark menu items.
- [x] 8.1.2 Add child-folder disclosure navigation and nested right-click context routing.
- [x] 8.1.3 Remove rename, create-child, and delete controls from the left-click panel.
- [x] 8.1.4 Add pointer-button separation tests and run focused UI, app, formatting, and OpenSpec validation.

## 9. Provider-aware bookmark icons

### 9.1 Shared icon projection

**目的：** Distinguish Local, ADB, SFTP, and Lua bookmarks consistently.
**輸入：** Structured and raw bookmark target variants.
**產出：** Central icon classifier and updated bookmark projections.
**依賴：** 1.1, 8.1.
**Owner／Wave：** Primary integrator／wave 11.
**Gate／Evidence：** G15; classifier and focused UI tests plus app check.
**完成門檻：** Every bookmark surface uses the same classification without validating raw paths.

- [x] 9.1.1 Add structured/raw Local, ADB, SFTP, and Lua icon classification.
- [x] 9.1.2 Apply the shared icon to toolbar, overflow, folder content, manager, and navigation projections.
- [x] 9.1.3 Embed and attribute Lua.org's unchanged official logo with an offline asset integrity test.
- [x] 9.1.4 Add classification regression tests and run focused UI, app, formatting, and OpenSpec validation.

## 10. Bookmark browse-menu dismissal

### 10.1 Close before activation

**目的：** Prevent folder and overflow menus from remaining visible after bookmark selection.
**輸入：** Existing activation reducer and browse-menu state.
**產出：** Synchronous dismissal helper, reducer ordering, and regression test.
**依賴：** 8.1.
**Owner／Wave：** Primary integrator／wave 12.
**Gate／Evidence：** G16; focused reducer/UI tests and application check.
**完成門檻：** Every bookmark activation attempt closes browse menus before target lookup; child-folder drill-in remains open.

- [x] 10.1.1 Add one state helper that clears folder and overflow browse menus.
- [x] 10.1.2 Invoke dismissal before bookmark lookup and provider-specific activation.
- [x] 10.1.3 Add ordering regression coverage and run focused UI, app, formatting, and OpenSpec validation.

## 11. Inline bookmark context menu

### 11.1 Folder-style right-click commands

**目的：** Replace the large bookmark action window route with the logical-folder context-menu style.
**輸入：** Bookmark right-click actions, folder context styling, and delete confirmation window.
**產出：** Validated popup state, compact renderer, reducer routing, and regression tests.
**依賴：** 3.1, 10.1.
**Owner／Wave：** Primary integrator／wave 13.
**Gate／Evidence：** G17; focused state/render tests and application check.
**完成門檻：** Right-click opens only the compact menu; commands dismiss correctly; delete remains confirmed.

- [x] 11.1.1 Add validated bookmark context state with popup exclusivity and close behavior.
- [x] 11.1.2 Render applicable commands using the folder context menu visual contract.
- [x] 11.1.3 Route normal commands after dismissal and Delete through the existing confirmation window.
- [x] 11.1.4 Remove native action-window presentation from right-click and add regression coverage.
- [x] 11.1.5 Run focused UI tests, app check, formatting, strict OpenSpec validation, and scoped diff review.

## 12. Bookmarked-location star editor

### 12.1 Stateful star and compact editor

**目的：** Make the current-location bookmark state visible and provide Firefox-inspired editing in a dedicated window.
**輸入：** Current typed-target lookup, existing toggle action, and shared bookmark editor content.
**產出：** Focus-blue solid state, compact fixed window, and regression coverage.
**依賴：** 1.1, 3.1.
**Owner／Wave：** Primary integrator／wave 14.
**Gate／Evidence：** G18; focused UI tests, application check, and strict OpenSpec validation.
**完成門檻：** An exact existing bookmark shows a blue solid star and clicking it opens the compact editor with edit/save/remove controls.

- [x] 12.1.1 Render an exact current-location bookmark as a solid theme-focus-blue star while retaining the outline and disabled states.
- [x] 12.1.2 Restyle the existing normal bookmark editor as a compact fixed-size window without changing its authoritative edit flow.
- [x] 12.1.3 Add regression coverage for star state, editor dispatch, window contract, and retained controls.
- [x] 12.1.4 Run focused UI tests, application check, formatting, strict OpenSpec validation, and scoped diff review.

## 13. Responsive bookmark editor width correction

### 13.1 Restore display-relative sizing

**目的：** Preserve the requested wide editor presentation across display sizes.
**輸入：** Primary-display bounds and existing dedicated editor options.
**產出：** 80%-wide resizable editor with a safe minimum and regression tests.
**依賴：** 12.1.
**Owner／Wave：** Primary integrator／wave 15.
**Gate／Evidence：** G19; width unit tests, focused window test, app check, and strict validation.
**完成門檻：** The editor initially uses 80% of display width, never less than 640px, and remains resizable.

- [x] 13.1.1 Restore display-relative 80% width, 640px minimum, 560px height, and resizable window options.
- [x] 13.1.2 Add width calculation and window-contract regression coverage.
- [x] 13.1.3 Run focused tests, application check, formatting, strict OpenSpec validation, and scoped diff review.

## 14. Frameless editor action correction

### 14.1 Complete action row and remove native controls

**目的：** Match the requested bookmark editor controls without Windows chrome.
**輸入：** Existing editor draft identity, remove reducer, and window options.
**產出：** Always-visible Remove action, unsaved-draft cancellation, and frameless window.
**依賴：** 13.1.
**Owner／Wave：** Primary integrator／wave 16.
**Gate／Evidence：** G20; focused UI/window tests, app check, and strict validation.
**完成門檻：** Remove is always visible and meaningful, while no native caption buttons appear.

- [x] 14.1.1 Render Remove Bookmark for both persisted and unsaved editor drafts.
- [x] 14.1.2 Cancel an unsaved add draft through Remove while retaining durable deletion for existing IDs.
- [x] 14.1.3 Suppress the native titlebar and add action/window contract regression tests.
- [x] 14.1.4 Run focused tests, application check, formatting, strict OpenSpec validation, and scoped diff review.

## 15. Classic remote context-menu style

### 15.1 Vertical square menu presentation

**目的：** Match the reference typography and classic menu geometry without copying its commands.
**輸入：** Existing remote context command model and custom GPUI renderer.
**產出：** Vertical command rows, classic spacing/font, square borders, and retained behavior.
**依賴：** Existing remote context lifecycle.
**Owner／Wave：** Primary integrator／wave 17.
**Gate／Evidence：** G21; focused command/render tests, app check, and strict validation.
**完成門檻：** No horizontal strip or rounded UI remains, while all contextual commands still dispatch unchanged.

- [x] 15.1.1 Reorder the existing item commands into classic vertical menu order without changing actions.
- [x] 15.1.2 Replace the command strip with 16px, 30px full-width rows, icon gutter, divider, and square menu geometry.
- [x] 15.1.3 Update positioning and regression coverage for command membership, typography, lifecycle, and no-rounded/no-strip contracts.
- [x] 15.1.4 Run focused tests, application check, formatting, strict OpenSpec validation, and scoped diff review.

## 16. Classic menu DPI alignment

### 16.1 Calibrate typography and row geometry

**目的：** Align the visible ADB menu baseline and density with the supplied classic-menu reference at the active DPI scale.
**輸入：** Screenshot comparison and existing vertical renderer.
**產出：** Exact logical font, row, gutter, and clamp dimensions.
**依賴：** 15.1.
**Owner／Wave：** Primary integrator／wave 18.
**Gate／Evidence：** G22; focused geometry tests, app check, and strict validation.
**完成門檻：** Rows use 12px text, 18px height, a 14px icon slot, 4px gap, and 216px menu width with consistent text alignment.

- [x] 16.1.1 Set row font to 12px, row height to 18px, icon slot to 14px, gap to 4px, and menu width to 216px.
- [x] 16.1.2 Recalculate popup clamping and update exact geometry regression coverage.
- [x] 16.1.3 Run focused tests, application check, formatting, strict OpenSpec validation, and scoped diff review.

## 17. Actionable remote file commands

### 17.1 Capability-aware common commands

**目的：** Add useful ADB/SFTP item commands without exposing unsupported operations.
**輸入：** Focused row metadata and existing new-tab, clipboard-path, and bookmark actions.
**產出：** Container-aware new-tab plus URI and bookmark rows.
**依賴：** 16.1.
**Owner／Wave：** Primary integrator／wave 19.
**Gate／Evidence：** G23; membership/action tests, app check, and strict validation.
**完成門檻：** Item/folder/background menus expose only valid commands and dispatch existing authoritative actions.

- [x] 17.1.1 Capture focused row identity and container capability in the remote context snapshot.
- [x] 17.1.2 Add folder-only Open in New Tab plus item-only Copy Remote Path and Add to Bookmarks commands.
- [x] 17.1.3 Preserve classic alignment/separators and add item/folder/background action-identity tests.
- [x] 17.1.4 Run focused tests, application check, formatting, strict OpenSpec validation, and scoped diff review.

## 18. Remote download and properties commands

### 18.1 Complete current backend-backed item actions

**目的：** Add direct download and metadata entry points while preserving cross-provider semantics.
**輸入：** Selected remote descriptors, transfer engine, Downloads location, and namespace properties action.
**產出：** Download-to-Downloads request and Properties row.
**依賴：** 17.1.
**Owner／Wave：** Primary integrator／wave 20.
**Gate／Evidence：** G24; action identity, transfer request, UI tests, app check, and strict validation.
**完成門檻：** ADB/SFTP items can download to the user's Downloads folder and invoke Properties through real actions.

- [x] 18.1.1 Add a remote-only Download to Downloads action and cross-provider copy request.
- [x] 18.1.2 Add Download and Properties rows with existing classic alignment.
- [x] 18.1.3 Add membership/action regression coverage and validate background exclusion.
- [x] 18.1.4 Run focused tests, application check, formatting, strict OpenSpec validation, and scoped diff review.

## 19. Remote menu surface and density correction

### 19.1 Match the supplied soft-shadow reference

**目的：** Correct only the ADB/SFTP context-menu face, spacing, and shadow without changing file or folder row colors.
**輸入：** Latest screenshot comparison and existing classic remote-menu renderer.
**產出：** A `#F7F7F7` light menu surface, soft shadow, and corrected row geometry.
**依賴：** 18.1.
**Owner／Wave：** Primary integrator／wave 21.
**Gate／Evidence：** G25; exact render-contract tests, app check, and strict validation.
**完成門檻：** Only the remote right-click menu uses 22px rows, 18px icon slots, 10px gaps, 6px insets, 236px width, gray lines, and a soft 14px shadow.

- [x] 19.1.1 Correct row height, icon gutter, text spacing, inset, and popup width.
- [x] 19.1.2 Apply the light `#F7F7F7` color and soft shadow exclusively to the remote context-menu surface.
- [x] 19.1.3 Update popup clamping and exact style regression coverage.
- [x] 19.1.4 Run focused tests, application check, formatting, strict OpenSpec validation, and scoped diff review.

## 20. Directional remote-menu shadow

### 20.1 Remove top/left spill while preserving the lower reach

**目的：** Match the supplied directional shadow proportions without changing menu or listing colors.
**輸入：** Existing remote-menu shadow and exact requested edge behavior.
**產出：** No top/left shadow, unchanged 18px bottom reach, and shorter 10px right reach.
**依賴：** 19.1.
**Owner／Wave：** Primary integrator／wave 22.
**Gate／Evidence：** G26; exact shadow-contract test, app check, and strict validation.
**完成門檻：** The remote context menu uses offset `(5, 13)`, blur `8`, and spread `-3` exclusively.

- [x] 20.1.1 Apply the directional soft-shadow geometry only to the remote context menu.
- [x] 20.1.2 Update exact regression assertions and design/spec contracts.
- [x] 20.1.3 Run focused tests, application check, formatting, strict OpenSpec validation, and scoped diff review.
