## Context

Details-column reordering already persists through `OrderedColumnLayout`, but the GPUI header
implementation registers each complete header as a `move_before(target)` drop zone. That makes the
gesture asymmetric: entering the previous header moves left, while entering the next header asks for
the unchanged order. The implementation also has separate pointer begin/drop actions that do not
model a prospective order shared by headers and rows.

The change is confined to Details presentation and interaction. `Name` is an immovable first column;
sort, resize, filters, dynamic extension columns, and session persistence must remain compatible.

## Goals / Non-Goals

**Goals:**

- Resolve adjacent left and right reorders at target-header midpoints.
- Show the prospective header and row order before mouse-up.
- Commit once on valid drop and restore the original order on cancellation.
- Use one effective-order projection for headers, rows, filters, hit testing, and accessibility.
- Add focused tests and blocking physical-pointer UTIT evidence.

**Non-Goals:**

- Multi-column drag, moving `Name`, or changing sort/resize/filter semantics.
- A persistence-schema, extension ABI, provider, packaging, or dependency change.
- Persisting intermediate pointer positions.

## Decisions

### A transient preview session owns in-flight order

Explorer UI state will hold a bounded session with the dragged stable `ColumnId`, the original
visible order, and the current insertion slot. It will derive a prospective order without mutating
`OrderedColumnLayout`. On valid drop, the final prospective order is committed through the existing
model API once; cancellation simply clears the session.

This is preferred to mutating the persisted layout at every midpoint because it avoids session-write
churn and makes Escape restoration deterministic. A cue-only implementation is rejected because the
approved behavior requires row cells to move before mouse-up.

### Midpoints map pointer input to insertion slots

A pure resolver receives finite logical pointer X and target bounds. `pointer_x < midpoint` selects
before-target; `pointer_x >= midpoint` selects after-target, represented by the next visible stable
column or the terminal slot. Slots are computed after removing the dragged column, preventing the
source position from skewing adjacent rightward movement. Repeated events resolving to the current
slot are no-ops.

Invalid coordinates do not change the last valid preview. Targets are stable IDs rather than indexes,
so dynamic-column registration cannot silently retarget an active gesture; an unavailable source or
target cancels the preview.

### One effective-order projection drives every Details consumer

State exposes the preview order while a session is active and the persisted visible order otherwise.
The existing header and row construction paths, filter affordances, hit testing, and accessibility
traversal will consume that same projection. This prevents a header-only animation from temporarily
mislabeling data cells.

### GPUI drag lifecycle remains isolated from click and resize

The header body starts GPUI drag only after the existing movement threshold. `on_drag_move` reports
pointer position and target bounds to the preview updater, and `on_drop` commits the active preview.
Escape and drag cancellation dispatch a terminal cancel action. The resize grip remains a separate
hit target and `Name` never registers a draggable payload. An ordinary click without a drag continues
to dispatch sort exactly once.

### Blocking evidence uses real pointer coordinates

Focused unit/state tests validate the resolver and lifecycle. UTIT will press a real header, move only
past the adjacent target midpoint, and inspect automation bounds before releasing the mouse to prove
that both header and representative data cell moved live. It will then release, verify the committed
order, repeat in the opposite direction, and record physical pointer/header bounds plus screenshots.

## Risks / Trade-offs

- **[Risk] Re-rendering on every drag event could cause unnecessary work**: update state only when
  the resolved insertion slot changes; no provider work or data reload is triggered.
- **[Risk] Header and row order could diverge during preview**: expose one effective-order helper and
  test both projections from the same session.
- **[Risk] Sort or resize could fire at drag termination**: preserve GPUI threshold semantics, stop
  propagation on committed drop, and retain the dedicated resize hit target.
- **[Risk] Dynamic columns change during a gesture**: identify columns by stable ID and cancel if the
  active preview can no longer be projected.
- **[Risk] DPI scaling masks the original asymmetry in automation**: compute UTIT endpoints from UIA
  physical bounds and assert the pointer crossed only one adjacent midpoint.

## Migration Plan

No stored-session migration is required. Existing committed column order remains authoritative until
a drag starts. Rollback removes transient preview behavior while leaving persisted layouts readable.

## Adjustment Policy

- **A - task refinement:** implementation file split, command, or task order may change without
  changing requirements or gates.
- **B - in-scope correction:** midpoint/projection details may be corrected only by updating design,
  spec, tasks, and affected evidence before continuing.
- **C - material change:** changing `Name` immovability, live-before-mouse-up behavior, blocking UTIT,
  persistence/ABI scope, or cancellation semantics requires user approval.

Blocking thresholds and required evidence cannot be weakened silently.

## Open Questions

None. The approved design fixes exact-midpoint behavior as right-side insertion.
