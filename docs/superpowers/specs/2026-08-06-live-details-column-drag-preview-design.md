# Live Details column drag preview

## Goal

Make Details-column reordering symmetric and immediately understandable. Dragging a non-`Name`
column across an adjacent column midpoint must update both the header and every visible data cell
while the pointer remains down. Releasing commits the previewed order; cancellation restores the
original order.

## Current defect

Each complete header is currently a `move_before(target)` drop zone. Moving left into the previous
header therefore works immediately, while moving right into the next header requests the original
order and appears to do nothing. The pointer must enter one more header before the column moves.
The existing headful coverage exercises only a leftward drag and cannot detect this asymmetry.

## Interaction contract

- `Name` remains fixed at index zero and cannot start a reorder drag.
- The resize grip remains isolated from header dragging.
- A pointer movement past the existing drag threshold starts a reorder preview; an ordinary click
  continues to sort.
- Every target header is split logically at its horizontal midpoint. The left half resolves to an
  insertion before that target; the right half resolves to an insertion after that target, expressed
  as before the following visible column or at the end. Formally, `pointer_x < midpoint` selects the
  left slot and `pointer_x >= midpoint` selects the right slot.
- Crossing a midpoint immediately reprojects the header and all row cells in the preview order.
  Filter controls and accessibility traversal use the same projection.
- Dropping commits the current preview exactly once through the existing ordered-layout and session
  persistence path.
- Escape, pointer cancellation, or termination without a valid drop discards the preview and restores
  the original order without persistence.
- Repeated drag-move events that resolve to the same insertion slot are no-ops.

## Architecture

The Explorer UI state owns a bounded drag-preview session containing the dragged column, the
original visible order, and the current preview insertion slot. A pure midpoint resolver converts
the pointer X and target bounds into a before/after slot. It must reject non-finite coordinates and
preserve the `Name` invariant.

Rendering projects one effective order: the preview order while a session is active, otherwise the
persisted `OrderedColumnLayout`. Headers, rows, filter affordances, hit testing, and accessibility
must all consume this same effective order so the screen cannot show mismatched columns.

GPUI `on_drag_move` updates only the preview session. `on_drop` commits the final effective order via
one model mutation. A cancel action clears the session without touching persisted settings. Column
resize and sort handlers remain independent.

## Testing

- Pure unit tests for left and right halves of an adjacent target, exact-midpoint behavior, final-slot
  insertion, non-finite input rejection, and repeated-slot no-op behavior.
- State/model tests that preview left and right moves, commit once, cancel to the original order, and
  keep `Name` first.
- UI projection tests proving header labels and row values use the same preview order.
- A blocking UTIT headful scenario performs genuine pointer input. It drags a column right only past
  the adjacent header midpoint, verifies both header and data-cell bounds change before mouse-up,
  releases and verifies persistence, then repeats leftward to prove symmetry. Failure evidence must
  include a screenshot and the physical pointer/header bounds used by the assertion.

## Non-goals

- Reordering multiple columns as a group.
- Moving `Name` away from the first position.
- Changing column resize, sorting, filtering, or data-provider semantics.
- Persisting intermediate preview positions.
