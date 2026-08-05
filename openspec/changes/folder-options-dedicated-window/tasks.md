## 1. Baseline and contracts

### 1.1 Capture current behavior and ownership seams

**目的：** Establish an implementation baseline that identifies the overlay, reducers, native-window composition, scroll helpers, and registered UITEST entry points without changing behavior.
**輸入：** Approved design document, proposal, delta spec, current dirty-worktree status, and existing Folder Options source/tests.
**產出：** Baseline report, focused source map, initial failing behavior assertions, and evidence index entries.
**依賴：** None.
**Owner／Wave：** Primary integrator, wave 0; owns this change directory and focused inspection only; must not revert concurrent worktree edits.
**Gate／Evidence：** G1 baseline completeness; `openspec/changes/folder-options-dedicated-window/evidence/evidence-index.jsonl`, task IDs `1.1.*`.
**完成門檻：** Every affected state/window/render/test seam has a named path and existing behavior is captured by executable assertions or a documented headful observation.

- [ ] 1.1.1 Record the exact dirty-worktree baseline, relevant file hashes, GPUI revision, and current Folder Options source/test locations.
- [ ] 1.1.2 Add a failing Rust assertion proving Open Folder Options still enters the Explorer overlay instead of requesting a dedicated window.
- [x] 1.1.3 Add a failing UITEST registration/fixture skeleton that identifies the Explorer HWND and expects one distinct Folder Options HWND.
- [ ] 1.1.4 Create append-only evidence records for the baseline commands, expected failures, actual results, hashes, and timestamps.

### 1.2 Freeze typed window and settings contracts

**目的：** Define the smallest application/UI interfaces for single-instance window commands, draft revisioning, apply results, and close reasons.
**輸入：** 1.1 source map, existing `ExplorerAction` reducers, `FolderOptionsDraft`, persistence path, and GPUI window APIs.
**產出：** Typed controller/view contracts and contract-level unit tests with no duplicate settings implementation.
**依賴：** 1.1.
**Owner／Wave：** Primary integrator, wave 0; owns focused `explorer-app`/`explorer-ui` contract edits; no plugin ABI or persisted-schema changes.
**Gate／Evidence：** G2 contract review; evidence records `1.2.*` plus focused test output.
**完成門檻：** Contracts express open/activate, stale close, Apply/OK/Cancel/Escape/title-close, validation failure, revision broadcast, and exactly-once terminal behavior without a public ABI change.

- [x] 1.2.1 Define typed Folder Options window open/activate/close intents and application controller state.
- [ ] 1.2.2 Define draft snapshot, applied baseline revision, dirty state, and typed apply success/failure results.
- [ ] 1.2.3 Add contract tests for a single live instance, stale-handle replacement, retry after creation failure, and idempotent close.
- [ ] 1.2.4 Add reducer tests for Apply baseline replacement, OK-after-success, and Cancel/Escape/title-close discard equivalence.

## 2. Dedicated window implementation

### 2.1 Build the application-owned window controller

**目的：** Create and own exactly one modeless native Folder Options window and synchronize its settings with all live Explorer windows.
**輸入：** 1.2 contracts, existing application lifecycle/open-window composition, applied settings store, and Explorer-root notification path.
**產出：** Production controller, open/activate/recovery flow, apply broadcast, diagnostics, and shutdown cleanup.
**依賴：** 1.2.
**Owner／Wave：** Primary integrator, wave 1; owns focused `crates/explorer-app/src/**` changes; must preserve unrelated extension composition edits.
**Gate／Evidence：** G3 controller lifecycle; evidence records `2.1.*`, named unit/integration tests, and diagnostic capture.
**完成門檻：** Repeated open activates one HWND/entity, creation failure is retryable, stale handles recover, Apply broadcasts atomically, and shutdown leaves no live options controller.

- [x] 2.1.1 Implement fallible create-or-activate logic that publishes controller state only after native window creation succeeds.
- [x] 2.1.2 Implement stale-handle detection and exactly-once clearing for title close, entity close, and application shutdown.
- [ ] 2.1.3 Route successful Apply through existing persistence and broadcast paths with a monotonic applied-settings revision.
- [ ] 2.1.4 Keep dirty drafts stable across external setting broadcasts while allowing clean drafts to adopt the latest snapshot.
- [ ] 2.1.5 Emit structured, non-fatal diagnostics for create, activate, validation, and persistence failures.
- [ ] 2.1.6 Run and record focused controller/state tests, including two live Explorer windows receiving one Apply snapshot.

### 2.2 Extract and render the dedicated Folder Options entity

**目的：** Move the existing General, View, and Extensions controls into a reusable dedicated-window entity while preserving typed reducers and setting inventory.
**輸入：** 1.2 UI contracts, 2.1 controller bridge, existing page builders, theme tokens, focus helpers, and layout constants.
**產出：** Dedicated entity, normal native-window shell, extracted page composition, fixed header/footer, focus behavior, and removed overlay path.
**依賴：** 1.2 and 2.1 create/open interface.
**Owner／Wave：** Primary integrator, wave 1; owns focused `crates/explorer-ui/src/**` Folder Options edits; forbidden from redesigning unrelated chrome.
**Gate／Evidence：** G4 render/focus parity; evidence records `2.2.*`, render snapshots/source assertions, and focused UI tests.
**完成門檻：** All existing controls remain reachable, top tabs and bottom actions remain fixed, the overlay/backdrop no longer renders, and keyboard focus never escapes into an Explorer entity.

- [x] 2.2.1 Extract the three page builders from the overlay shell without changing their setting actions or labels.
- [x] 2.2.2 Implement the resizable Folder Options entity with normal title bar, initial bounds, and minimum logical size.
- [x] 2.2.3 Implement fixed page tabs and fixed OK/Cancel/Apply footer around a `min_h_0` page viewport.
- [ ] 2.2.4 Route Tab/Shift+Tab, page focus entry, Enter, Escape, and native title-close through the typed entity/controller transitions.
- [x] 2.2.5 Remove the `folder-options-overlay` render/backdrop path and route Open Folder Options only to the application controller.
- [ ] 2.2.6 Add render and focus tests proving the overlay is absent and all three pages/actions remain accessible at minimum size.

## 3. Scrolling and interaction isolation

### 3.1 Implement page-local visible scrollbar behavior

**目的：** Give every Folder Options page an independent clamped vertical scroll position and a permanently reserved right-side scrollbar track.
**輸入：** 2.2 entity/viewport, existing Explorer scrollbar geometry and pointer capture, GPUI `ScrollHandle`, semantic tokens, and DPI conversion helper.
**產出：** Three page scroll handles, shared scrollbar renderer/interaction, disabled-fit state, and DPI-safe drag behavior.
**依賴：** 2.2.
**Owner／Wave：** Primary integrator, wave 2; owns Folder Options scroll code and reusable helper changes only; file/navigation scrollbar behavior must remain unchanged.
**Gate／Evidence：** G5 scroll geometry and DPI; evidence records `3.1.*`, unit/render outputs, and offset traces.
**完成門檻：** Wheel/touchpad, track, thumb, Page Up/Down, Home/End work within bounds; page offsets restore independently; fit content shows a light full-height thumb; DPI conversion occurs exactly once.

- [x] 3.1.1 Add one persistent `ScrollHandle` per General, View, and Extensions page and restore the matching handle on page switch.
- [x] 3.1.2 Reserve scrollbar width in the page viewport so no setting content paints beneath the track.
- [ ] 3.1.3 Reuse or extract Explorer scrollbar geometry for thumb ratio, track paging, pointer capture, grab offset, and clamping.
- [x] 3.1.4 Render the visible disabled/light full-height thumb when page content fits the viewport.
- [x] 3.1.5 Implement wheel/touchpad, Page Up/Down, Home/End, track click, and thumb drag for the active page only.
- [ ] 3.1.6 Add unit/render tests for independent offsets, resize clamping, fit/overflow states, and 100/125/150/200 percent DPI coordinate conversion.

### 3.2 Prevent input leakage to Explorer windows

**目的：** Ensure Folder Options pointer, wheel, drag, and keyboard events affect only its own native window and terminate safely on focus/capture changes.
**輸入：** 2.2 focus routing, 3.1 scrollbar sessions, Explorer file/navigation scroll handles, and GPUI event propagation APIs.
**產出：** Window-local event capture/consumption and regression tests comparing foreground/background scroll offsets.
**依賴：** 3.1.
**Owner／Wave：** Primary integrator, wave 2; owns options-window event routing; must not globally suppress Explorer input.
**Gate／Evidence：** G6 input isolation; evidence records `3.2.*` with before/after offset and terminal-state traces.
**完成門檻：** Options-window gestures never alter Explorer file/navigation offsets, scrollbar capture terminates exactly once, and Explorer remains independently operable while the modeless window exists.

- [x] 3.2.1 Stop wheel and pointer propagation at the dedicated window boundary without using an Explorer overlay backdrop.
- [ ] 3.2.2 Terminate options scrollbar capture idempotently on mouse-up-outside, Escape, deactivation, capture loss, and native close.
- [ ] 3.2.3 Add interaction tests that compare options, file-view, and navigation offsets before and after every supported scroll gesture.
- [x] 3.2.4 Add a modeless interaction test proving Explorer navigation succeeds while Folder Options remains open and retains its draft.

## 4. UITEST and release gates

### 4.1 Register and exercise the headful Folder Options UITEST

**目的：** Prove the user-visible native-window, scrolling, lifecycle, action, and DPI contracts through real mouse/keyboard automation on an interactive Windows desktop.
**輸入：** 2.x/3.x production implementation, UITEST runner/manifest conventions, owned fixture root, and evidence directory.
**產出：** Registered case, automation script, screenshots, JSON report, raw window/scroll/action measurements, and retained failure diagnostics.
**依賴：** 2.1, 2.2, 3.1, and 3.2.
**Owner／Wave：** Primary integrator, wave 3; owns the new UITEST case/manifest entry; interactive-desktop cases run serially.
**Gate／Evidence：** G7 headful parity; evidence records `4.1.*` and test-owned screenshots/report files.
**完成門檻：** The registered case passes distinct-HWND, singleton, modeless, page-scroll, input-isolation, Apply/OK/Cancel/Escape/title-close, minimum-size, and representative-DPI assertions with retained artifacts.

- [x] 4.1.1 Register a dedicated Folder Options case in `uitest/manifest.json` with owned fixture/evidence paths and deterministic cleanup.
- [x] 4.1.2 Automate distinct native-window discovery, repeated-open activation, and Explorer navigation while the window remains open.
- [ ] 4.1.3 Automate wheel, track, thumb, Page Up/Down, Home/End, and per-page offset restoration while recording background Explorer offsets.
- [ ] 4.1.4 Automate Apply, OK, Cancel, Escape, title-close, creation retry/stale replacement where injectable, and two-window setting broadcast.
- [ ] 4.1.5 Run minimum-size and 100/125/150/200 percent DPI assertions or record evidence-backed `not-applicable` only when the runner cannot set a scale on the available display.
- [ ] 4.1.6 Save screenshots, HWND/count, logical/physical bounds, offsets, action results, environment metadata, hashes, and terminal status in the evidence index.

### 4.2 Final validation, traceability, and handoff

**目的：** Validate the complete change, map every normative scenario to executable evidence, and ensure no unrelated dirty-worktree content is included.
**輸入：** Completed 1.x–4.1 work, proposal/design/spec/tasks, focused Rust results, UITEST artifacts, and final diff.
**產出：** Strict OpenSpec validation, focused test logs, traceability table, diff review, unresolved-risk disposition, and implementation handoff.
**依賴：** 4.1 and all prior implementation packages.
**Owner／Wave：** Primary integrator, wave 4; owns final integration/evidence; no staging or committing of unrelated user/agent changes.
**Gate／Evidence：** G8 final acceptance; evidence records `4.2.*`, strict validation output, and requirement-to-task matrix.
**完成門檻：** Every requirement/scenario maps to passing evidence, OpenSpec strict validation and focused tests pass, the final diff contains only scoped changes, and no P0/P1 issue remains unresolved.

- [x] 4.2.1 Run formatting and focused Rust unit/integration tests for `explorer-ui` and `explorer-app`, recording exact commands and exit statuses.
- [x] 4.2.2 Run the registered Folder Options UITEST serially and index every retained artifact with a content hash.
- [ ] 4.2.3 Build a requirement/scenario-to-task/gate/evidence traceability table and mark any failed, stale, or unexecuted leaf incomplete.
- [x] 4.2.4 Run `openspec validate folder-options-dedicated-window --strict` and the detailed-task validator, then scan artifacts for incomplete tokens and contradictions.
- [x] 4.2.5 Review the final working diff for overlap with concurrent edits, accessibility, DPI, lifecycle, and test completeness; resolve all P0/P1 findings.
- [x] 4.2.6 Update task checkboxes/evidence lineage truthfully and deliver the implementation summary with remaining non-blocking risks.
