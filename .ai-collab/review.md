# Codex Review — 2026-09-27

## Grok delegation

- Broad repair job `run-mujvxyz9-zwhty6`, Grok thread `285c381c-4ac6-4f04-ac95-442f17048d5b`: cancelled after 7m50s with only reading/intent messages and no implementation edits.
- Focused P0-2 job `run-mujw8r4f-7wklag`, Grok thread `792c0f64-41e1-4728-8f8d-016375a2bdef`, model `grok-4.7-build-fast`: cancelled after 5m13s with only an initial intent message and no implementation edits.
- Bridge `check --json` reported ready and authenticated. Both headless processes started with `--always-approve`; no tool action, terminal result, or file edits appeared before cancellation. Cause is unconfirmed. Do not treat these jobs as successful implementation or testing.

## Primary-agent repair and validation

- `crates/explorer-ui/src/state.rs`: added `open_column_item` to produce `ExplorerCommand::OpenItem` with the clicked file's identity, location, tab context and default-application disposition. Added `column_focused_open_target` for keyboard Enter and a focused command test.
- `crates/explorer-ui/src/lib.rs`: double-click now uses `open_column_item`; Enter targets the selected column item and does not fall through to ordinary row opening when the column has no selection.
- `cargo test -p explorer-model -p explorer-ui --lib column_view -- --test-threads=8`: PASS (16 model, 7 UI).
- `cargo check --workspace`: PASS.
- `rustfmt --edition 2024 --check crates/explorer-ui/src/state.rs`: PASS.
- `git diff --check`: PASS (line-ending warnings only).
- `cargo fmt --all -- --check`: FAIL; existing formatting differences remain in `chrome.rs`, `column_view.rs`, and `lib.rs`. The newly added state code is formatted.

P0-2 has a passing command-level regression test, but a headful double-click/Enter assertion is still required for final acceptance. P0-1, P0-3, P1-4 through P1-10, P2-11 and P2-12 remain unresolved. Do not mark their OpenSpec tasks complete.

The OpenSpec checklist was corrected from 17/22 to 6/22 complete by reopening items whose stated acceptance conditions have not been met. The UITEST commands in `tasks.md` and `GROK_IMPLEMENTATION_PROMPT.md` now specify the required `--bin explorer-uitest`.

`.ai-collab/task.md` now contains the next bounded Grok assignment: real integrated image preview (P0-1/P1-10). Start a fresh write job only after confirming the prior jobs are terminal and the Grok service is responding; do not resume the cancelled P0-2 task, which Codex already repaired.

## Grok retry and independent acceptance (later on 2026-09-27)

- Job `run-mujyas7i-4pbqj8`, thread `2d6424f8-5158-4335-b30d-c714e5854ec7`, model `grok-4.7-build-fast`: **completed** after 17m50s. This establishes that the Grok bridge, repository tools, edit tools and test execution now work. It connected `preview_texture` to `ColumnStrip`, draws an aspect-fit image, adds Preview Handler lifecycle states and focused tests, and added `scripts/test_local_column_view_jpeg_preview.ps1`.
- Independent `cargo check --workspace`: PASS. `cargo test -p explorer-model -p explorer-ui --lib column_view -- --test-threads=8`: PASS (16 model, 9 UI). Focused `column_preview_routes_image_folder_offline_and_multiple_without_false_loading`: PASS. `cargo fmt --all -- --check`: PASS. `openspec validate add-local-column-view --strict`: PASS.
- Grok's first headful JPEG run failed because the deep fixture's active column was outside the viewport. Codex added `reveal_active_column` on the `SetViewMode(Columns)` route and a deep-path regression test, then rebuilt `SuperExplorer.exe`.
- Independent headful run `target/column-view-jpeg-preview-pixels-20260927/report.json`: PASS. UIA found the image inside the integrated preview, and a captured screenshot sampled the runner JPEG's center pixel as R=220 G=20 B=59. Selecting `notes.txt` removed the stale image. Screenshot: `target/column-view-jpeg-preview-pixels-20260927/jpeg-preview-selected.png`.
- This validates the local JPEG subcase of P0-1. Preview Handler HWND bounds/focus, horizontal user scrolling, other file commands and the remaining OpenSpec tasks still need implementation and headful evidence. The 3.1, 3.2, 4.1 and 4.2 checkboxes remain open because their full acceptance conditions are broader than this JPEG test.

## Status

Second Grok task accepted for the bounded horizontal navigation and preview resize scope. The larger OpenSpec change is still in progress.

## Findings

- Job `run-mujz9v6x-z0to6h`, thread `d9f28e9b-7bfb-4cb2-b9a8-48f707cbbd76`, model `grok-4.7-build-fast`: completed after 16m21s. It added measured strip layout, horizontal wheel/trackpad and Shift+wheel routing, a horizontal track and thumb drag, preview divider drag, and state/input tests.
- The original Grok headful test checked only a track click and divider presence. Codex extended `scripts/test_local_column_view_horizontal_scroll.ps1` to assert thumb dragging and a real divider move, and made its output path absolute. A first added test failed because the decorative thumb has no UIA node; the runner now computes its physical location from the exposed range and track. A leftward divider drag in the narrow fixture hit the minimum strip span, so the runner tests a rightward drag that can shrink the preview.
- The accepted behavior is scoped to the local narrow fixture. Wheel/trackpad routing and resize persistence have focused unit coverage, but a real-device wheel/DPI matrix and 100,000-row performance target remain open.

## Validation

- Independent `cargo check --workspace`: PASS.
- Independent `cargo test -p explorer-model -p explorer-ui --lib column_view -- --test-threads=8`: PASS (18 model, 13 UI).
- Independent `cargo fmt --all -- --check`: PASS. `git diff --check`: PASS (line-ending warnings only).
- Independent headful `target/column-view-horizontal-scroll-drag3-codex-20260927/report.json`: PASS. Track click changed offset 1920 → 1786.29; thumb drag 1786.29 → 1330.94; divider X 845 → 905.
- Independent JPEG regression `target/column-view-jpeg-after-scroll-20260927/report.json`: PASS. Center pixel R=220 G=20 B=59; selecting notes removed the stale preview.

## Decision

Grok Build bridge and two real edit/test jobs succeeded. Accept the second bounded repair. Do not mark the full local Columns feature or OpenSpec tasks 3.1/3.2 complete until performance and DPI acceptance are verified.

## Vertical-scroll repair — 2026-09-28

- An initial Grok job `run-muk06aih-6fk7r0` (thread `b1fc30d9-bb8d-419e-8a4d-d8c366eb5c45`) used the default `grok-4.7` model and was cancelled after 18m23s with no file edits. The bridge reported it as cancelled. This did not satisfy the task.
- A narrower Grok job `run-muk0uo1t-d74wo2` (thread `d46a7946-8cef-47e4-aa0d-7c25a5e2d3a1`, model `grok-4.7-build-fast`) completed after 19m12s. It added measured column-list viewport publication, vertical clamping and focus-row reveal, plus model/UI/state tests for empty, short and extreme viewport cases.
- Codex removed an O(n) full-list clone that Grok's new scroll path would otherwise perform on every wheel event. `column_displayed_entries` now borrows the branch/current/cache snapshot; `DirectorySnapshotCache::peek_ref` supports that read-only path.
- Independent `cargo check --workspace`: PASS. `cargo check -p explorer-ui`: PASS. `cargo fmt --all -- --check`: PASS. `cargo test -p explorer-model -p explorer-ui --lib column_view -- --test-threads=8`: PASS (19 model, 16 UI). `git diff --check`: PASS (line-ending warnings only). Branch remains `master`, HEAD `7e33dcc0da2b5e86c37231eac58de669466588b2`.
- Accept bounded P1-5 repair at unit/integration level. A headful wheel/keyboard assertion and the complete 100,000-row performance requirement remain open; no OpenSpec 3.1/3.4 checkbox is closed.

## Selection bridge — 2026-09-28

- Grok job `run-muk1nkdq-1dpwix`, thread `e8a27282-946e-431d-867b-792987f40995`, model `grok-4.7-build-fast`: completed after 15m58s. It added full-order Shift ranges and cross-column Ctrl reset in `ColumnBranch`; UI state now derives selected command items, preview entry and status count from the active column. It also guards stale image results, neutralizes multiple/empty preview, and adds focused model/state/app tests.
- Independent `cargo check --workspace`: PASS. `cargo test -p explorer-model -p explorer-ui --lib column_view -- --test-threads=8`: PASS (22 model, 19 UI). `cargo fmt --all -- --check`: PASS. `git diff --check`: PASS (line-ending warnings only). Branch/HEAD unchanged.
- Independent headful JPEG regression `target/column-view-jpeg-after-selection-20260928/report.json`: PASS. JPEG center pixel R=220 G=20 B=59; selecting notes cleared stale preview.
- Accept the bounded selection bridge. The current column rendering still uses raw snapshot entries rather than the filtered/sorted `DirectoryPresentation` order, so P1-7 remains open and Shift order is only consistent with that raw rendering. Generic commands that read `tab.selection` directly still need the P0-3 command integration audit. Do not close OpenSpec 3.3/4.4 yet.

## Per-column projection — 2026-09-28

- Grok job `run-muk2di1v-2nxdn0`, thread `2a7bd404-56c5-4fc0-b6cb-dc69472257af`, model `grok-4.7-build-fast`: completed after 15m57s. It routes every column through `DirectoryPresentation::build_filtered`, reuses shared snapshot entries and cached ordered indices, and aligns render, Shift range, keyboard, preview and vertical-scroll count with the visible projection. Focused mixed hidden/system/folder/file tests cover ancestor/current columns, sort changes, Shift selection and cache reuse on repeated scroll.
- Independent `cargo check --workspace`: PASS. `cargo test -p explorer-model -p explorer-ui --lib column_view -- --test-threads=8`: PASS (22 model, 20 UI). `cargo fmt --all -- --check`: PASS. `git diff --check`: PASS (line-ending warnings only). Branch/HEAD unchanged.
- Independent headful `target/column-view-horizontal-after-projection-20260928/report.json`: PASS (track/thumb/divider). Independent `target/column-view-jpeg-after-projection-20260928/report.json`: PASS (JPEG pixel and stale-image clearing).
- Accept bounded P1-7 projection repair. This is not P1-6 bulk virtualization: `column_strip_model` still builds every row model. No 100,000-entry performance result or headful hidden/sort UIA assertion exists yet.

## Column command routing — 2026-09-28

- Grok job `run-muk33j7o-m9xg9x`, thread `71d8fece-b216-4071-ae1f-ec7196b47130`, model `grok-4.7-build-fast`: completed after 20m10s. It wired row/context/background actions, F2 and Delete keyboard routing, inline column rename editor, clipboard/paste parent, external/internal drop destinations, and ancestor-aware command capabilities/refresh. The new tests inspect actual ancestor item/parent command payloads and intended action paths.
- Independent `cargo check --workspace`: PASS. `cargo test -p explorer-model -p explorer-ui --lib column_view -- --test-threads=8`: PASS (22 model, 25 UI). `cargo fmt --all -- --check`: PASS. `git diff --check`: PASS (line-ending warnings only). Branch/HEAD unchanged.
- Accept command routing at code and focused-test level. A real Windows headful check of right-click, F2 rename, clipboard/paste, Delete and cross-column drag/drop is still required before closing OpenSpec 3.3. `column_view_rows_wire_context_menu_drag_drop_and_inline_rename` is a source-wiring test and does not prove pointer behavior by itself.

## Auxiliary listing backpressure — 2026-09-28

- Grok job `run-muk3vkie-f4sqrx`, thread `570fd7c0-cb7f-4962-980c-a834295c300b`, model `grok-4.7-build-fast`: completed after 16m27s. It keeps auxiliary request slots occupied until a terminal, prioritizes columns in the measured visible range, and publishes batches and terminals through one ordered bounded lane. A full lane reports resource failure rather than success or user cancellation.
- Independent inspection covered `ColumnLoadCoordinator::plan/complete`, `plan_column_loads`, `apply_column_event`, `ReliableTerminalPublisher::try_publish_batch_reserving_terminal`, and `process_column_enumeration`. The lane reserves a retained slot for a terminal and does not release coordinator capacity on a batch.
- Independent `cargo check --workspace`: PASS. `cargo test -p explorer-model -p explorer-ui --lib column_view -- --test-threads=8`: PASS (23 model, 26 UI). `cargo test -p explorer-shell-win --lib column_enumeration -- --test-threads=1`: PASS (4 Shell). `cargo fmt --all -- --check`: PASS. `git diff --check`: PASS with line-ending warnings only.
- Accept the bounded P1-8 repair. This verifies 145 real fixture entries and synthetic slow/full consumers; high-volume 100,000-entry rendering and an end-to-end rapid sibling-switch run still need evidence before closing OpenSpec 2.2/2.3/3.1.

## Headful F2 rename and coverage gate — 2026-09-28

- Rebuilt `target/debug/SuperExplorer.exe` from the accepted command/backpressure source. Independent `scripts/test_local_column_view_rename_headful.ps1` PASS at `target/column-view-rename-headful-v5-20260928/report.json`: F2 created a visible inline editor in the selected column row, typing a new name and Enter renamed the runner-owned file. Screenshot: `target/column-view-rename-headful-v5-20260928/after-f2.png`.
- The first UIA-only assertion failed because GPUI's inline text input is rendered but is not exposed as an `Edit` automation node; only the Search box was reported. The runner now verifies the visible editor by screenshot and the operation by filesystem outcome. Accessible edit semantics remain an independent P1-10/3.4 issue to investigate; do not interpret the first failure as a rename behavior failure.
- `cargo run -p explorer-uitest --bin explorer-uitest -- --validate-only` fails on 548 repository-wide uncovered OpenSpec requirements. The `local-column-view-contract` entry currently uses a broad wildcard and does not establish per-requirement proof. This is a baseline coverage gate failure, not evidence that the local Columns headful cases passed.

## Deterministic Columns UITEST cases — 2026-09-28

- Repaired `scripts/test_local_column_view_headful.ps1`: use an absolute runner-owned fixture path, disable repeated-launch redirection for the isolated process, assert the initial fixture is visible, assert opening folder `b` reveals `child.txt` in the next column, and assert Alt+P removes the integrated preview. Standalone report `target/column-view-headful-v4-20260928/report.json`: PASS.
- Replaced `add-local-column-view/*` with six explicit requirement selectors in `uitest/manifest.json`; registered separate JPEG, horizontal drag and F2 rename cases. `cargo run -p explorer-uitest --bin explorer-uitest -- --list` accepts the manifest. The real Preview Handler lifecycle requirement deliberately remains uncovered.
- Independent `explorer-uitest` case runs: `local-column-view-headful` PASS (`target/column-view-uitest-headful-20260928`), `local-column-view-horizontal-headful` PASS (`target/column-view-uitest-horizontal-20260928`), `local-column-view-rename-headful` PASS (`target/column-view-uitest-rename-20260928`), `local-column-view-jpeg-headful` PASS (`target/column-view-uitest-jpeg-20260928`). All used the current `target/debug/SuperExplorer.exe` before the pending virtualization job.
- Updated `docs/UITEST.md` to state what the registered cases actually assert and the repository-wide coverage-gate limitation. Do not close OpenSpec 5.2/5.3 until the remaining scenario matrix and latest build are verified.

## Bounded row materialization — 2026-09-28

- Grok job `run-muk4ptqy-8asab6`, thread `2ec2477c-8c5e-49ff-a370-6e4619ec6653`, model `grok-4.7-build-fast`: completed after 17m19s. It moved visible row slicing ahead of `ColumnRowModel` allocation, keeps a full filtered row count for scrolling, uses sparse logical gaps for unbuilt rows, retains an active rename target when off-screen, and collects icon candidates directly from visible projections rather than building a full strip.
- Independent inspection found `materialize_column_window` uses the shared projection's ordinal accessor, `column_strip_model_for_viewport` builds only the measured visible columns, and `visible_column_entries` limits icon candidates to 96 visible/overscan rows. No full-directory row clone remains on the steady render path. Branch/HEAD remain `master`/`7e33dcc0da2b5e86c37231eac58de669466588b2`.
- Independent `cargo check --workspace`: PASS. `cargo test -p explorer-model -p explorer-ui --lib column_view -- --test-threads=8`: PASS (23 model, 28 UI). `cargo fmt --all -- --check`: PASS. `git diff --check`: PASS with line-ending warnings only.
- Focused 100,000-entry/five-level test PASS: initial projection/model/icon work 53.9951 ms, 30 materialized rows and 30 icon candidates; vertical scroll 50.9 µs with 50 rows, horizontal scroll 24.8 µs with 3 rows, selection render 619.2 µs with 50 rows, rename render 62.7 µs with 30 rows plus one pinned rename target, steady maximum 56.7 µs. These are debug-profile timings on this host, not a release benchmark or end-to-end frame measurement.
- Rebuilt `target/debug/SuperExplorer.exe`; all four latest-source `explorer-uitest` headful cases PASS in `target/column-view-after-virtual-headful-20260928`, `target/column-view-after-virtual-horizontal-20260928`, `target/column-view-after-virtual-rename-20260928`, and `target/column-view-after-virtual-jpeg-20260928`.

## Real PDF Preview Handler — 2026-09-28

- Grok job `run-muk5hud2-fvph4p`, thread `ca83dfd6-38ef-4de3-9de9-5ec8e60fb633`, was stopped by Codex after 35m29s because its second headful UIA path remained blocked. The bridge reports `cancelled` with the generic label `Stopped by user`; the user did not cancel it. Preserve and review its edits independently.
- Source review: `preview_host_client_rect` now treats GPUI layout bounds as window-client coordinates and clamps them after DPI conversion, fixing an origin subtraction that had shifted the child HWND over the columns. `UpdatePreviewHostBoundary` bypasses ordinary click dispatch so measuring layout cannot close the View menu. Closing the integrated pane clears the stale boundary; focus falls back to FileView when preview closes. Model/UI tests cover bounds, resize/DPI updates, accelerators, unload on selection/tab/mode, and timeout recovery.
- Independent `cargo check --workspace`: PASS. Column tests PASS (23 model, 29 UI). Focused `cargo test -p explorer-ui --lib column_preview_handler -- --test-threads=1`: PASS (2 UI). `cargo fmt --all -- --check` and `git diff --check`: PASS.
- Grok's partial headful reports showed a registered PDF handler and initial HWND within preview; one run proved host width 446 → 526 after divider drag. Its full script hung during synchronous Win32 resize/UIA and must not be treated as a full PASS. Codex added `-Quick` to bypass those unsafe steps and independently ran `target/column-view-handler-quick-codex-20260928/report.json`: PASS for registered handler, initial containment, divider drag, Alt+P unload/reopen and selection unload. Window resize, actual DPI change, Tab focus UIA readback and crash injection are explicit SKIP; focused tests cover routing but are not headful proof.
- Registered `local-column-view-handler-headful` with `-Quick` and an exact OpenSpec requirement selector. Independent `explorer-uitest --case local-column-view-handler-headful` PASS at `target/column-view-handler-uitest-20260928`. `--validate-only` still fails for 548 repository-wide uncovered requirements, with none under `add-local-column-view`.
- The F2 headful screenshot was cropped to the app window. An intermittent View-menu lookup failed twice during the Grok run; after the run, two independent `local-column-view-rename-headful` retries passed and the cropped image shows the inline editor. Keep the menu timing flake in mind for future full-suite runs.

## Empty and failed column states — 2026-09-28

- Grok job `run-muk6y5as-0rzlp2`, thread `80a00222-9f93-4d31-840c-bbef5861f54c`, model `grok-4.7-build-fast`: completed. It derives current status from `DirectoryState`, keeps ancestor request-open partial results in Loading, distinguishes empty from error, and adds accessible status and Retry controls. Focused tests cover loading/empty/error including retained prior rows and refresh behavior.
- Codex made the headful script's file/folder row lookup use UIA ListItem names so the assertion targets an actual column row. Independent `cargo check --workspace`, `cargo test -p explorer-model -p explorer-ui --lib column_view -- --test-threads=8`, `cargo test -p explorer-ui --lib column_preview_handler -- --test-threads=1`, `cargo fmt --all -- --check`, and `git diff --check`: PASS. Rebuilt `target/debug/SuperExplorer.exe`.
- Independent `target/column-view-empty-headful-codex-20260928/report.json`: PASS. The empty runner-owned folder showed `此資料夾是空的`, retained the parent `keep.txt` row, and exposed `重試`. Registered `local-column-view-empty-headful`; `explorer-uitest --case local-column-view-empty-headful` PASS at `target/column-view-empty-uitest-20260928`. Inaccessible-folder headful remains SKIP; no ACLs were changed. This accepts the bounded P2-11 repair but does not complete all folder/operation acceptance.

## Command matrix implementation and runner repair — 2026-09-28

- Grok job `run-muk7mkth-vdmcp2`, thread `4a7b6cc0-f2d3-46ca-9aaa-056729309bd3`, model `grok-4.7-build-fast`: completed after 13m53s. It repaired selected-folder paste targeting and Column drop dispatch, added focused tests and wrote `scripts/test_local_column_view_commands_headful.ps1`.
- Codex independently ran `cargo check --workspace`, focused column tests, `cargo fmt --all -- --check`, `git diff --check` and rebuilt `SuperExplorer.exe`: PASS. Full model/UI library tests: model 191 PASS; UI 578 PASS, 10 FAIL, 2 ignored. The 10 failing UI test names match the earlier 2026-09-27 review's broad dirty-tree baseline; detailed output is `target/column-view-full-lib-tests-20260928.txt`. Their attribution has not been proved with a clean baseline.
- Codex's first command script run found PowerShell 5.1 UTF-8 BOM and generic `List` conversion errors. After those harness fixes, `target/column-view-commands-codex-v9-20260928/trace.txt` reached the ancestor right-click popup and recorded `context-after-close closed=True`, then hung in a whole-window UIA row scan. The primary stopped only its owned test run. Delete/paste/drag did not execute and must not be claimed as headful PASS. A bounded Grok repair job `run-muk8igxf-rcb37p` is in progress. The production fixes are accepted at focused-test level only pending this independent interaction matrix.
- On the rebuilt production source, independent UITEST cases `local-column-view-contract`, `local-column-view-headful`, JPEG, horizontal, F2 rename, PDF handler Quick, and empty folder all PASS. Reports are under `target/column-view-contract-after-commands-20260928` and `target/column-view-after-commands-<case>-20260928`. The JPEG case took 85.8 seconds but passed. `explorer-uitest --validate-only` still reports 548 repository-wide uncovered requirements, none under `add-local-column-view`, at `target/column-view-coverage-after-commands-20260928.txt`.
- Grok repair job `run-muk8igxf-rcb37p` completed after 6m20s. It split headful command cases into independently watched processes and stopped using UIA after a native menu opened. Codex fixed a PowerShell case-insensitive `$Worker`/`$worker` collision and a `$Target`/`$target` collision, then ran each case. Ancestor Delete PASS (`target/column-view-command-delete-codex-v2-20260928`), paste after navigating to the destination folder PASS (`target/column-view-command-paste-codex-v5-20260928`), and ancestor right-click PASS (`target/column-view-command-context-codex-v2-20260928`). The menu popup is owned outside the app process tree, so the runner now accepts a foreground native popup with the correct class, bounded menu read, position, labels and no filesystem change. The native menu still makes post-popup whole-window UIA unreliable, and the case avoids that scan.
- Cross-column drag FAIL (`target/column-view-command-drag-codex-v3-20260928`). The runner's source/target coordinates hit the app window and the app log records `begin_drag`, but no `DropOnColumn` or move was observed; the source file stayed in `drag-src`. A post-release OLE wake move did not change this result. This is an open P0-3 issue, not a PASS or SKIP. The next Grok task investigates the production drop path and driver contract.

## Cross-column drop repair and command acceptance — 2026-09-28

- Grok job `run-muk97s75-nws4a1`, thread `83fb2035-f1b5-4856-a088-7498adcde4e5`, model `grok-4.7-build-fast`: completed after 13m41s. It found that the navigation pane's drag-move listener negotiated the current folder even while the pointer was in Columns, resetting a valid sibling-folder Move to `None`. It added pointer bounds checks to that listener, target-specific hover effect handling on column rows/backgrounds, focused effect/dispatch tests, and path-free drop outcome logs.
- Codex independently inspected the UI/GPUI routing, ran `cargo check --workspace`, focused model/UI column tests (23 model, 32 UI), `cargo fmt --all -- --check`, `git diff --check`, and rebuilt the app: PASS. New fixture `target/column-view-command-drag-codex-v4-20260928/report.json` PASS: exactly one file moved `drag-src\\drag-me.txt` → `drag-dst\\drag-me.txt`, `not-dragged.txt` stayed; interaction log records `column_drop result=submitted column=6 folder=true effect=Move paths=1`.
- Full four-case matrix PASS at `target/column-view-command-matrix-codex-20260928/report.json`, then registered `local-column-view-commands-headful` and independently ran it through UITEST: PASS at `target/column-view-commands-uitest-20260928`. All four cases use separate runner-owned fixtures; Codex executed the Delete/Move cases after reviewing targets. Command script includes a 120-second worker wall clock and a native-menu read timeout.
- After the drag patch, UITEST contract, basic headful, JPEG, horizontal, F2 rename, PDF handler Quick, and empty folder cases all PASS at `target/column-view-after-drag-<case>-20260928`. The JPEG case took 85.5 seconds. Latest full model/UI library run: model 191 PASS; UI 580 PASS, same 10 FAIL names as before, 2 ignored (`target/column-view-full-lib-tests-after-drag-20260928.txt`). OpenSpec strict validation, formatting, workspace check and diff check PASS. Full `explorer-uitest --validate-only` still fails for 548 unrelated uncovered requirements; no local-column-view requirements are uncovered. Branch `master` and HEAD `7e33dcc0da2b5e86c37231eac58de669466588b2` unchanged.

## Keyboard and accessibility acceptance — 2026-09-28

- Grok job `run-muka3uil-v2rjlc`, thread `47185651-c446-4a1b-9406-4a37933c024e`, model `grok-4.7-build-fast`: completed after 18m22s. It routed Shift/Ctrl arrow intent into the active Column branch, added Ctrl+Space toggle and focused tests, and wrote a five-case headful keyboard/UIA script and UITEST entry. It did not run the headful script.
- Codex independently found and fixed headful harness defects: the startup check had looked for Column rows before mode activation; UIA address names included Unicode direction isolates; PowerShell 5.1 required UTF-8 BOM for Chinese row labels; repeated whole-tree UIA scans were slow, so the script caches discovered column lists; `keybd_event` sent Shift+Down without the Shift modifier reaching GPUI, so modified keys now use Windows SendKeys. A temporary path-free keyboard trace proved the original input had `shift=false` and the corrected input had `shift=true`; the trace was removed from production source after diagnosis.
- Independent focused tests PASS: 23 explorer-model Column tests, 32 explorer-ui Column tests, and the two new keyboard tests. `cargo check --workspace`, `cargo fmt --all -- --check`, `openspec validate add-local-column-view --strict`, and `git diff --check` PASS. Rebuilt `target/debug/SuperExplorer.exe` after removing diagnostics. PowerShell parser reports 0 errors and the keyboard script retains UTF-8 BOM.
- Formal `local-column-view-keyboard-headful` PASS at `target/column-view-keyboard-uitest-final-20260928`: keyboard View activation, Down/Right/Left and address, active-column Shift/Ctrl selection, UIA names/roles/status, and focus return all PASS. View-menu checked-state is explicitly SKIP because the Button lacks UIA TogglePattern. Post-keyboard command matrix PASS at `target/column-view-commands-after-keyboard-20260928`; basic headful PASS at `target/column-view-headful-after-keyboard-20260928`.
- Codex filled `column-preview-handler-ready` in 18 missing locales; all 20 locale files now contain the key. The global locale completeness test still fails on 7 other zh-CN keys, and catalog lookup previously failed on `status-thumbnail-quota-full`. Full model/UI lib run after keyboard: model 191 PASS; UI 582 PASS, same 10 FAIL names, 2 ignored (`target/column-view-full-lib-tests-after-keyboard-20260928.txt`). Global UITEST coverage gate still fails on 548 other requirements, none under `add-local-column-view` (`target/column-view-coverage-after-keyboard-20260928.txt`). Branch/HEAD unchanged; no unrelated dirty work was reset.
