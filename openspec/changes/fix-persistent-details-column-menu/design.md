## Context

The Details header chooser is now rendered through `explorer-shell-win` as an independent Windows 11-style popup, so it is positioned beside the pointer and is not clipped by the GPUI window. Its current activation contract returns one command and destroys the popup. That contract is correct for filesystem context menus and terminal commands, but wrong for File Explorer-style column visibility rows, which must update and remain interactive.

The popup runs on a background executor because running its modal message loop inside an `ExplorerRoot` callback caused a verified GPUI `RefCell already borrowed` panic. The solution must therefore preserve thread isolation, apply visibility changes on the foreground UI, and repaint the native check mark without reconstructing the popup.

## Goals / Non-Goals

**Goals:**

- Keep the same popup HWND, position, scroll offset, hover context, and message loop across repeated column visibility changes.
- Repaint each resulting check state immediately and reconcile the requested state in `ExplorerRoot` during the same session.
- Keep auto-size and target-specific commands terminal.
- Preserve ordinary immersive filesystem context-menu semantics and fallback behavior.
- Fail closed when native and UI state cannot be kept consistent.

**Non-Goals:**

- Changing column order, labels, registry discovery, persistence schema, extension ABI, or popup visual metrics.
- Making arbitrary Shell extension commands persistent.
- Adding a second in-window chooser implementation.

## Decisions

### Opt-in command activation policy

`OwnedPopupMenuItem` will carry a terminal or persistent-toggle activation policy. The general immersive renderer remains terminal by default. Only the application-owned Details popup supplies persistent items.

Alternative rejected: infer persistence from `MF_CHECKED`. Checked terminal commands and unchecked toggle commands make inference ambiguous.

### Native state changes before publication

Activating a persistent row will compute the resulting checked state, update the owning `HMENU` item and materialized `Row`, repaint the popup, and then publish a typed event containing stable command-row index plus requested checked state. The message loop will continue with `result == 0`.

Alternative rejected: close and reopen at the same point. That visibly flashes, replaces HWND/session identity, loses scroll/hover state, and races rapid clicks.

### Foreground requested-state reconciliation

The shell worker will publish persistent events through a bounded channel owned by one Details popup session. `ExplorerRoot` will apply an idempotent `SetDetailsColumnVisibility { column, visible }` action rather than a timing-sensitive toggle. The `Name` row remains disabled in both native metadata and reducer validation. Session completion invalidates the receiver; late events are ignored.

Alternative rejected: invoke GPUI directly from the popup thread. The prior headful failure proves this violates GPUI borrowing and thread ownership.

### Split persistent events from terminal completion

The popup worker produces zero or more persistent events followed by exactly one terminal result. Auto-size rows and target-specific display rows follow the existing terminal result path. Escape, outside click, deactivation, and replacement gestures produce cancellation without synthesizing visibility events.

### Failure and capacity policy

The persistent event channel is bounded above the maximum number of meaningful rapid clicks in one UI pump interval. A full or disconnected channel tells the popup renderer to terminate the session, preventing a displayed check state from continuing to diverge from application state. Invalid indices, separators, and disabled rows never publish.

### Evidence-driven corrections

- **A — task refinement:** commands, leaf splits, evidence filenames, or ordering may change without changing requirements or gates.
- **B — design/spec correction:** an implementation fact may require an in-scope contract correction; affected tasks pause, design/spec/tasks are updated, and dependent evidence is marked stale before work resumes.
- **C — material change:** weakening persistence, immediate feedback, stable HWND, foreground isolation, failure closure, or required headful evidence requires user approval.

## Data flow

1. Header right-click builds ordered popup entries and matching UI actions.
2. The app starts the top-level popup on a background executor and creates the session event bridge.
3. A persistent row click updates the native check state and publishes `{command_index, checked}` without leaving the message loop.
4. The foreground receiver maps the stable index to a column and dispatches requested-state reconciliation.
5. The Details view repaints while the popup remains visible.
6. Terminal selection or dismissal ends the worker, closes the bridge, and releases HWND/HMENU/shadow/font resources once.

## Risks / Trade-offs

- [Native check changes before UI acknowledgement] → Use an idempotent requested state, a bounded low-latency foreground pump, and close the popup on publication failure.
- [Rapid double clicks reorder state] → Preserve FIFO session ordering and carry resulting state rather than a toggle intent.
- [Popup completion races queued events] → Drain accepted persistent events in order before final session cleanup; reject events from stale session IDs.
- [Generic popup regressions] → Keep persistence opt-in and rerun the full immersive popup tests, including the 1000-cycle resource slope.
- [Headful automation sees transient accessibility lag] → Validate the native HMENU check state and stable HWND directly, then independently verify Details header accessibility state.

## Migration Plan

No data migration is required. Land the opt-in shell contract, UI requested-state action, app bridge, and tests together. Rollback consists of reverting this change; persisted column visibility remains compatible.

## Blocking gates

- **G1 Contract:** persistent activation keeps one session and publishes ordered resulting states; terminal activation still closes.
- **G2 Isolation:** no popup worker directly borrows or updates GPUI, and the verified re-entry panic does not recur.
- **G3 Consistency:** repeated on/off clicks update native checks and Details visibility in order; `Name` cannot be hidden.
- **G4 Lifecycle:** dismissal and publication failure release popup resources with no stale UI mutation.
- **G5 User workflow:** one headful session proves stable HWND, immediate repeated check/uncheck, live Details changes, and Escape/outside dismissal in a small main window.

Evidence is stored under `openspec/changes/fix-persistent-details-column-menu/evidence/` with one JSON record per atomic task or an immutable shared record plus unique subcheck keys.

## Open Questions

None. The approved interaction and failure contracts are fully specified for implementation.
