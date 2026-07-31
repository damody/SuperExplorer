## 1. Baseline and Typed Contracts

- [x] 1.1 Capture current direct and brokered context-menu counts/labels for background, file, folder, and multi-selection fixtures with ordinary and Shift profiles.
- [x] 1.2 Add bounded context-menu invocation profile and extended-verb state to `explorer-model`, preserving ordinary invocation as the default.
- [x] 1.3 Add structured lock-failure classification, lock-owner identity/eligibility, discovery/shutdown commands, progress, and exactly-one terminal types.
- [x] 1.4 Add centralized non-zero limits for registered resources, owner count/text, retries, discovery/shutdown deadlines, and IPC bytes with validation tests.
- [x] 1.5 Extend architecture and privacy gates so UI callbacks cannot perform Shell/Restart Manager/process I/O and diagnostics cannot contain process paths or command lines.

## 2. Complete Native Shell Context Menu

- [x] 2.1 Map file-view pointer and keyboard modifiers into the typed invocation profile; ordinary and Shift sessions must not leak state into each other.
- [x] 2.2 Version and round-trip the broker context-menu payload with target profile and extended-verb state, rejecting malformed or oversized values.
- [x] 2.3 Implement target-specific `QueryContextMenu` flags for item versus background menus, including Explorer/item/rename/synchronous-cascade semantics and Shift extended verbs.
- [x] 2.4 Preserve the complete bounded command range, separators, disabled/default state, icons, owner-drawn content, and nested lazy submenus returned by `IContextMenu3`.
- [x] 2.5 Verify owner-window message forwarding for init/draw/measure/menu-char/submenu messages and native pointer/keyboard invocation.
- [x] 2.6 Preserve Escape/outside-click cancellation, originating-window focus restoration, timeout, quarantine, crash cleanup, and exactly-one terminal behavior.
- [x] 2.7 Add controlled handler tests for ordinary/extended flags, owner-draw, lazy submenu, disabled/default entries, malformed IDs, crash, hang, and reentrancy.
- [x] 2.8 Add real direct-versus-brokered differential tests for all target/profile combinations and available 7-Zip, WinRAR, TortoiseGit, cloud, editor, and Send To providers.

## 3. Locked Delete Windows Adapter

- [x] 3.1 Preserve raw Windows error/HRESULT identity from `IFileOperation` and classify only sharing/lock violations as recoverable locks.
- [x] 3.2 Implement RAII Restart Manager session start/register/list/end with bounded resources, owner growth retry, cancellation, and typed error mapping.
- [x] 3.3 Convert `RM_PROCESS_INFO` into owned privacy-safe records with application/service name, PID, creation time, type, restart capability, and initial eligibility.
- [x] 3.4 Implement process creation-time, current-token integrity, protected/critical/system, self/app-helper, PID 0/4, and access eligibility checks without retaining process handles.
- [x] 3.5 Implement graceful Restart Manager shutdown for explicitly confirmed eligible owners with process identity revalidation and a fixed deadline.
- [x] 3.6 Report per-owner closed/already-exited/stale/denied/protected/refused/timeout outcomes and never call `TerminateProcess` or request elevation.
- [x] 3.7 Add fake adapter and real owned-helper tests for one/multiple owners, buffer growth, no owner, PID reuse, access denied, cancellation, refused close, timeout, and cleanup.

## 4. Service, Reducer, and Retry Lifecycle

- [x] 4.1 Add service commands/events for discovery and graceful shutdown on bounded background execution lanes with request/tab/generation correlation.
- [x] 4.2 Detect a failed recycle/permanent delete lock terminal and start one discovery while retaining the exact original operation and item identities.
- [x] 4.3 Add reducer states for discovering, ready, closing, partial, unavailable, cancelled, and retrying with stale/duplicate/late event suppression.
- [x] 4.4 Implement Retry to resubmit the original delete once without shutdown and preserve recycle versus confirmed permanent-delete semantics.
- [x] 4.5 Implement Close programs and retry to require explicit confirmation, revalidate the discovery generation, and retry deletion exactly once only after a safe close terminal.
- [x] 4.6 Cancel and clear recovery on Cancel/Escape, navigation, tab switch/close, operation supersession, window close, and app shutdown without touching files or processes.
- [x] 4.7 Preserve partial failure and ordinary non-lock error notices and refresh navigation/file view only after an actual successful delete.
- [x] 4.8 Add deterministic reducer tests for every terminal race, duplicate action, stale generation, partial close, retry limit, and destructive-operation identity.

## 5. Explorer-like Locked File Dialog

- [x] 5.1 Render an Explorer-like modal showing the locked item summary and bounded locking application list without exposing paths or command lines.
- [x] 5.2 Add Close programs and retry, Retry, and Cancel buttons with accurate enabled state, default/cancel behavior, progress, and per-process results.
- [x] 5.3 Trap Tab/Shift+Tab inside the modal, support Enter/Space/Escape, restore originating item focus, and block background file-view actions while open.
- [x] 5.4 Add UIA dialog/list/status/action names, owner eligibility/state, live announcements, high-contrast tokens, reduced motion, compact-window scrolling, and DPI-safe hit targets.
- [x] 5.5 Add visual/model and headful keyboard/mouse/focus/UIA tests for discovery, eligible/ineligible owners, close, retry, cancel, partial, and unavailable states.

## 6. UTIT, Interop, and Final Closure

- [x] 6.1 Add OpenSpec requirement mappings and manifest cases for complete context menus and locked-delete recovery across quick/full/interop/visual/soak suites.
- [x] 6.2 Add contained destructive fixtures and an owned lock-holder helper that proves graceful close plus recycle and permanent-delete outcomes without affecting unrelated files.
- [x] 6.3 Run real Shell ordinary/Shift/provider differential evidence and truthfully skip unavailable extensions while requiring installed providers to match direct native results.
- [x] 6.4 Run locked-delete headful evidence for pointer/keyboard/UIA/focus plus Close-and-retry, Retry, Cancel, multiple owners, denied owner, and stale PID.
- [x] 6.5 Run repeated menu/lock discovery/shutdown/delete cycling and verify bounded app/broker/worker/helper processes, threads, handles, sessions, queues, modal state, and terminal balance.
- [x] 6.6 Run format, locked all-target check, Clippy warnings denied, workspace tests/doc tests, architecture/privacy/security gates, manifest coverage, and OpenSpec strict validation; fix all product failures.
- [x] 6.7 Run one complete UTIT pass, fix every reproducible product failure with Explorer-like behavior, rerun affected cases, and repeat the complete pass until accepted.
- [x] 6.8 Rebuild and validate release app/broker/worker and installed-path installer smoke, then update status, parity, UITEST, safety, evidence, limitations, and rollback documentation.

## 7. Host-owned Built-in Context Commands

- [x] 7.1 Add a typed, locale-independent broker terminal for Cut, Copy, Create shortcut, Delete, Rename, and Properties canonical verbs.
- [x] 7.2 Delegate those six verbs to the long-lived UI/service operation pipelines while leaving third-party invocation isolated in the worker.
- [x] 7.3 Implement collision-safe Shell `.lnk` creation and route Properties through the long-lived Shell STA.
- [x] 7.4 Add model, broker, Shell adapter, UI routing, and real `.lnk` UTIT coverage for the regression.
- [x] 7.5 Run the focused UTIT, complete workspace validation, release build, and installed-path verification.

## 8. Exact Pointer Target and Complete Command Audit

- [x] 8.1 Add a DPI-correct, genuine-pointer UTIT that right-clicks non-first fixture rows and proves the hit row becomes the exact focused selection and command target.
- [x] 8.2 Inventory every actionable top-level and nested Shell command for background, file, folder, multi-select, ordinary, and Shift profiles, recording its canonical verb or provider-owned identity.
- [x] 8.3 Delegate application-owned Open, Copy path, Share, and Quick access pin commands to long-lived Explorer actions while retaining provider-owned invocation in the isolated worker.
- [x] 8.4 Add contained result-oracle tests for every safe application-owned command and provider coverage/availability contracts for commands that launch external UI or require user credentials.
- [x] 8.5 Run focused and complete UTIT, strict validation, release build, and installed-path verification.

## 9. Properties, Inline Rename, and Pin to Start Regressions

- [x] 9.1 Record real canonical verbs and add typed `PinToStartScreen` delegation through the long-lived Shell STA.
- [x] 9.2 Replace synthesized-data-object Properties with native `IContextMenu` Properties invocation and verify a real property sheet.
- [x] 9.3 Stop pointer events inside the inline rename EditTextBox from reaching file-row selection, drag, or blur-commit handling.
- [x] 9.4 Add model/routing and headful UTIT for Properties, rename caret clicks, and Pin to Start availability/lifecycle.
- [x] 9.5 Run focused and complete validation, rebuild release/installer, verify installed binaries, and commit the regression fix.

## 10. Navigation History Menus and New Tab Parity

- [x] 10.1 Add bounded multi-step Back/Forward destinations and transactional commit/reject behavior to the navigation model.
- [x] 10.2 Add reducer-owned secondary-click history popup state, focus/hover/Escape/outside-click behavior, and exact destination activation.
- [x] 10.3 Make `+` and Ctrl+T share the root-scoped typed action and clone current, Back, and Forward history into an independent new tab.
- [x] 10.4 Add model, action-routing, and genuine pointer/keyboard UTIT for history menus, multi-step jumps, inherited history, and tab independence.
- [x] 10.5 Run complete validation, rebuild the release installer, verify the installed binaries, and commit the change.

## 11. Properties Result Matrix and Editable Text Pointer Parity

- [x] 11.1 Extend genuine-pointer Properties UTIT from a single file to file, filesystem folder, and compatible multi-selection with real property-sheet result oracles.
- [x] 11.2 Fix any canonical verb, immutable popup target, or long-lived Shell STA routing failure exposed by the expanded matrix.
- [x] 11.3 Correct vendored editable-text hit-testing to use padded inner bounds and scroll offset, and preserve the active address input entity during pointer editing.
- [x] 11.4 Paint focused selections with Explorer-like semantic Highlight and HighlightText colors in light, dark, and Windows high-contrast themes.
- [x] 11.5 Add unit and headful UTIT for caret placement, drag selection, coordinate alignment, contrast, and Properties result parity.
- [x] 11.6 End an open native popup and replay a right click on another unobscured row through the real Win32 input path, with exact-target and one-popup UTIT.
- [x] 11.7 Run complete validation, rebuild the release installer, verify installed binaries, and commit the regression fix.
