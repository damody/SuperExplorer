# Restore Toolbar Clicks and Details Column Drops

## Goal

Restore Windows File Explorer-compatible pointer behavior after the details-column drag lifecycle changes: command-bar buttons must receive their normal click, a column released on another movable column must commit and persist its new position, and only a release without a valid drop target may restore the original order. `Name` remains fixed at the left edge.

## Root Cause

Two drag-cancellation paths currently compete with normal pointer delivery:

1. The explorer root handles every left-button release and dispatches `CancelDetailsColumnDrag`, including releases belonging to command-bar buttons. That action can update and rerender the explorer before the button's click callback completes, so toolbar commands appear inert.
2. A dragged header and its nested sort control handle `mouse_up_out` synchronously. When the pointer is released over a different header, the source can cancel the drag before the target's `on_drop` commits it. The preview then disappears and the original order is restored even though the user selected a valid target.

## Considered Approaches

### Event-owned terminal handling with deferred fallback — selected

Remove the unconditional root cancellation. Valid drop targets commit synchronously. A source `mouse_up_out` schedules cancellation with GPUI's `window.defer`, allowing the current release/drop dispatch to finish first. The existing reducer makes a later cancellation harmless after commit has already consumed the drag session, while a release outside every target still reaches the deferred fallback and restores the original order.

This follows the ownership model used by File Explorer: the control under the pointer owns a valid drop, while cancellation is only a fallback for an unclaimed release.

### Drag generation token

Assign every drag a generation and make cancel conditional on no commit for that generation. This makes precedence explicit but expands model state and persistence-facing action plumbing for a local event-ordering issue.

### Stop-propagation exceptions

Stop mouse-up propagation on every toolbar button and adjust individual headers. This is fragile because new commands and extensions would each need the same exception, and nested controls could regress independently.

## Design

### Command bar

The explorer root will no longer manufacture `CancelDetailsColumnDrag` for ordinary releases. Command buttons retain their existing callbacks and focus rules. No command-specific workaround is added, so built-in and extension-provided controls share the same behavior.

### Details-column drag lifecycle

- `Name` remains non-draggable and fixed first.
- Drag movement continues to update the live preview and insertion point.
- Releasing over a valid movable header invokes `CommitDetailsColumnDrag` and persists the preview order.
- Releasing outside valid headers defers `CancelDetailsColumnDrag` until the current pointer event finishes.
- If a valid drop already committed, the deferred cancel observes no active drag and is a no-op.
- If no drop committed, the deferred cancel clears the session and restores the pre-drag order.
- The nested sortable header control follows the same lifecycle as its outer header so either hit area behaves identically.

## Testing

### Unit and structural tests

- Prove ordinary command-bar pointer release has no root drag-cancel side effect.
- Prove header outside-release cancellation is deferred rather than synchronous.
- Prove both the outer header and nested sort control use commit-first/deferred-cancel handling.
- Preserve reducer tests for valid preview commit, outside cancellation, persistence, and the fixed `Name` column.

### UTIT

Add or extend a live installed-app case using genuine pointer input:

1. Click representative command buttons that cover direct commands and menus, including `New`, `Sort`, `View`, and `Extensions`; assert their expected state change or popup appears.
2. Drag a movable column onto another movable column and release over the target; assert the visual header order changes and remains changed after the pointer gesture completes.
3. Refresh or reopen the view and assert the committed order persists.
4. Drag a movable column and release outside valid headers; assert the original order is restored.
5. Assert `Name` remains the leftmost column and cannot be reordered.

The test records a report and screenshot evidence from the installed build.

## Compatibility and Scope

The change does not alter command semantics, column serialization format, sorting, resizing, address-bar behavior, or extension column registration. It only corrects pointer-event terminal ownership and extends regression coverage.
