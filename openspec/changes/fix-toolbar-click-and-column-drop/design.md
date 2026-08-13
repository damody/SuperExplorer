## Context

The explorer root currently dispatches `CancelDetailsColumnDrag` on every left-button release. Details headers and their nested sort controls also cancel synchronously from `on_mouse_up_out`. These handlers were introduced to clean up a drag released outside a header, but they broadened terminal ownership beyond the active drag. Ordinary command clicks can be rerendered before their click callback completes, while a valid target's `on_drop` can lose a race to the source header's cancellation.

GPUI provides `window.defer`, which schedules state work after the current event dispatch. Existing drag state transitions already make `CommitDetailsColumnDrag` consume the pending drag and make a later cancellation harmless.

## Goals / Non-Goals

**Goals:**

- Deliver ordinary command-bar clicks without unrelated drag actions.
- Commit and persist a movable column released on a valid movable header.
- Restore the original order when release is not accepted by a valid header.
- Keep `Name` fixed leftmost.
- Verify the behavior with unit/structural tests and installed-app genuine-pointer UTIT evidence.

**Non-Goals:**

- Changing command semantics, sorting, resizing, or address editing.
- Changing extension ABI or column-order serialization.
- Redesigning the command bar or details headers.

## Decisions

### Remove root-wide mouse-up cancellation

The explorer root will not dispatch a drag cancellation for every left release. A generic ancestor cannot distinguish a button click from an unclaimed drag release at the correct event phase. Keeping it would require exceptions for every built-in and extension control.

Alternative rejected: stop propagation from individual command buttons. This is incomplete and would regress as commands are added.

### Give valid drop synchronous priority and defer fallback cancellation

Both the outer details header and nested sort control will keep synchronous `on_drop` commit callbacks. Their `on_mouse_up_out` callbacks will use `window.defer` to dispatch cancellation after the current pointer/drop event finishes. On a valid target, commit consumes the drag before deferred cancellation. Outside all valid targets, no commit occurs and the deferred cancellation restores the original order.

Alternative rejected: add generation tokens to model state. Tokens can enforce precedence, but add state and action complexity where GPUI event-phase scheduling already expresses the intended ownership.

### Preserve a single reducer contract

No new drag terminal action or persisted format will be introduced. Existing preview, commit, and cancel actions remain the only model transitions. This keeps the correction local to event delivery and lets existing persistence behavior remain authoritative.

### Verify through genuine pointer input

Structural tests will guard callback placement and deferral. UTIT will drive the installed application with real mouse gestures because synthetic action dispatch cannot reproduce GPUI event ordering. Evidence must show menu/direct command activation, a committed reorder that survives refresh or reopening, an invalid drop that restores the order, and `Name` leftmost.

## Risks / Trade-offs

- **Risk: deferred cancellation fires after a successful commit** → Existing cancel transition must be idempotent when no drag is active; retain/add a unit assertion for this terminal sequence.
- **Risk: nested sort child and outer header both schedule cancellation** → Both callbacks use the same idempotent state transition; structural coverage ensures both defer rather than cancel synchronously.
- **Risk: a release outside every header leaves a preview active for one render turn** → This is bounded to the current event dispatch and is preferable to losing valid drops.
- **Risk: UTIT coordinates may vary with DPI or localization** → Locate controls from UI Automation bounds and assert observable popup/header state instead of hard-coded absolute screen positions where possible.

## Migration Plan

No data migration is needed. Build and install the corrected executable, run targeted Rust tests and the UTIT manifest case, and capture screenshot/report evidence. Rollback is the source revert; persisted column order remains compatible.

## Open Questions

None. The approved behavior and GPUI scheduling mechanism are established.
