## 1. Built-in mode and session compatibility

- [x] 1.1 Add `Columns` to built-in and persisted view-mode enums, conversion, icon policy, and all exhaustive matches; verify model/session tests and a workspace `cargo check` pass.
- [x] 1.2 Add per-tab column width, preview width, and integrated-preview visibility with defaults and normalization for old sessions; verify old-session load and new-session round-trip tests pass.
- [x] 1.3 Implement local-drive eligibility and effective Details fallback without overwriting the chosen Columns preference; verify local, mapped/UNC, WSL, virtual, and archive location cases with focused tests.
- [ ] 1.4 Add localized View-menu entry, checked/disabled behavior, menu focus indices, and keyboard activation; verify menu mouse/keyboard tests and missing-translation checks pass.

## 2. Column branch and directory data

- [x] 2.1 Model per-tab branch levels, selected child identities, active column, scroll positions, loading/error state, and branch revision; verify branch truncation and tab-isolation unit tests pass.
- [x] 2.2 Add typed auxiliary column enumeration command and batch/terminal events through the existing local service; verify request routing, cancellation, exactly-one terminal behavior, and rejection of unsupported locations in contract tests.
- [x] 2.3 Build a bounded column-loading coordinator that reuses valid cached snapshots, prioritizes visible levels, and rejects stale tab/branch/request results; verify fast sibling-switch and out-of-order completion tests pass.
- [x] 2.4 Connect folder single-click, ancestor file selection, Back/Forward/Up, direct address, breadcrumbs and restored locations to formal navigation; verify the address, history and visible branch remain aligned in reducer tests.
- [ ] 2.5 Reconcile F5, file-operation completion, watcher changes, deleted folders, inaccessible directories, reparse cycles and empty directories; verify error/refresh cases preserve a navigable ancestor and clear invalid descendants.

## 3. Column surface and interaction

- [x] 3.1 Render the horizontally scrollable column strip with independent virtualized row lists and selected ancestor indicators; verify a deep path and a 100,000-entry folder keep visible-row work bounded.
- [x] 3.2 Add drag-resizable column dividers and preview width with logical-pixel clamping, viewport reveal and per-tab persistence; verify resize, narrow-window and DPI-focused layout tests pass.
- [x] 3.3 Bridge active-column item identities to selection, open, context menu, rename, clipboard, drag/drop, delete and status commands; verify an operation from an ancestor column targets its actual item and parent folder in focused tests.
- [x] 3.4 Implement Up/Down/Left/Right, Enter, Ctrl/Shift selection within one column, focus restoration and accessible names/roles/status; verify keyboard and UIA interaction cases pass.

## 4. Integrated preview

- [ ] 4.1 Render one integrated preview slot and suppress the ordinary side pane in effective Columns mode; verify single/multiple/folder/empty selections show the specified content or fallback with no duplicate pane.
- [ ] 4.2 Connect existing image preview and brokered Preview Handler to column selection and host bounds; verify supported image aspect ratio, handler fallback and preview resize/focus cases pass.
- [x] 4.3 Route Alt+P and the Preview Pane command to the per-tab integrated-preview preference while Columns is effective; verify command checked state and restoration of the ordinary pane preference after mode switch.
- [ ] 4.4 Cancel or ignore stale preview work on selection, branch, tab, mode and window changes; verify out-of-order image results, handler timeout/crash and offline-placeholder no-hydration cases pass.

## 5. Integration evidence and documentation

- [ ] 5.1 Add deterministic fixtures and tests for local depth, siblings, empty/inaccessible folders, junction cycles, large directories and rapid navigation; verify the focused model/UI test set passes.
- [ ] 5.2 Add `explorer-uitest` manifest coverage for each new OpenSpec requirement and headful local-column cases using runner-owned fixtures; verify `cargo run -p explorer-uitest --bin explorer-uitest -- --validate-only` reports complete mapping.
- [ ] 5.3 Run formatting, `cargo check --workspace`, focused tests, UITEST quick and relevant headful/full/visual cases; record actual pass/fail/skip and evidence paths, without claiming unrun scenarios passed.
- [x] 5.4 Update user-facing help/manual tests for Columns, Alt+P, local-only fallback and single-click history behavior; verify documented steps reproduce the observed Windows interaction.
- [x] 5.5 Review the final diff against the preexisting dirty working tree, confirm unrelated changes remain intact, and summarize changed files, validation evidence and remaining limitations in the handoff.
