## 1. Baseline and Failing Evidence

- [x] 1.1 Record current debug behavior for genuine-pointer Properties, dismissal, second non-first-row right-click, and safe command invocation with exact process IDs.
- [x] 1.2 Harden native popup and property-sheet discovery to bind evidence to the launched app/broker/worker process tree and reject generic unavailable dialogs.
- [x] 1.3 Add a failing post-Properties genuine-mouse sequence and ten-cycle resource contract to the focused context-menu UTIT.
- [x] 1.4 Add failing selection pixel/state coverage for selected idle, selected hover, popup focus transfer, inactive window, high contrast, Details, and icon view.

## 2. Selection Outline

- [x] 2.1 Add one file-row visual-state helper for fill, hover eligibility, border, and text semantics.
- [x] 2.2 Apply outline-only selection across view modes while preserving hit testing, exact selection identity, drag, rename, and context routing.
- [x] 2.3 Add Rust truth-table tests and pass the focused selection headful cases.

## 3. Persistent Host Shell STA

- [x] 3.1 Replace per-request host-owned Shell threads with one bounded application-owned STA queue and message pump.
- [x] 3.2 Route existing Properties, Share, and Pin to Start host commands through the executor without changing visible-popup broker IPC.
- [x] 3.3 Implement deterministic executor shutdown, queue terminal handling, resource snapshots, and unit tests for repeated/failing requests.

## 4. Native Properties Invocation

- [x] 4.1 Resolve the immutable popup target into one host-side `IContextMenu` and locate Properties on that queried instance.
- [x] 4.2 Invoke with `CMINVOKECOMMANDINFOEX`, the validated SuperExplorer owner HWND, Unicode metadata, and the offset from the same menu instance.
- [x] 4.3 Preserve bounded fallback for handlers without a canonical Properties verb without using `SHObjectProperties` or synthesized `IDataObject`.
- [x] 4.4 Add target, owner, failure-isolation, and post-invocation lifecycle unit/integration tests.

## 5. Manual-Mouse UTIT and Closure

- [x] 5.1 Pass genuine-pointer Properties result coverage for file, folder, executable, script, and compatible multi-selection.
- [x] 5.2 Pass ten Properties-dismiss-second-right-click-command cycles with bounded process/thread/window/menu/handle counts.
- [x] 5.3 Pass context-menu replacement, persistent broker, provider differential, worker quota, focus, Escape, outside-dismissal, and selection visual suites.
- [x] 5.4 Run complete UTIT once and fix every reproducible product failure without weakening result oracles.
- [x] 5.5 Build debug/release/installer, run the same ten-cycle installed-path smoke, and validate OpenSpec strictly.
- [x] 5.6 Commit selection and Properties lifecycle as independently revertible changes and leave unrelated/untracked workspace content untouched.
