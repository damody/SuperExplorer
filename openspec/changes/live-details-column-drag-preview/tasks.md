## 1. Midpoint and preview state contract

- [x] 1.1 Add a pure insertion-slot resolver that removes the dragged stable column before applying
  `pointer_x < midpoint` and `pointer_x >= midpoint`, including terminal-slot and invalid-coordinate
  handling.
- [x] 1.2 Add focused resolver tests for adjacent right/left movement, exact midpoint, terminal slot,
  non-finite/bad bounds, source-position skew, and repeated-slot no-op behavior.
- [x] 1.3 Replace the single dragged-column marker with a bounded preview session that retains original
  order, current prospective order/slot, and stable source identity while preserving `Name` first.
- [x] 1.4 Add state tests for preview updates, one-time commit, Escape/pointer cancellation restoration,
  unavailable dynamic identities, and `Name` rejection.

## 2. Unified live projection and GPUI lifecycle

- [x] 2.1 Expose one effective visible Details order that selects the active prospective order or the
  persisted `OrderedColumnLayout` without mutating persisted settings during preview.
- [x] 2.2 Route headers, row cells, filter controls, hit testing, and accessibility traversal through
  the effective order, retaining stable `ColumnId` value pairing.
- [x] 2.3 Add projection tests proving headers and representative row values move together during
  preview and return together on cancellation without provider recomputation.
- [x] 2.4 Wire header-body `on_drag_move` to midpoint preview updates and valid `on_drop` to one atomic
  commit; dispatch cancellation for Escape and terminal drag loss.
- [x] 2.5 Verify click-to-sort, resize-grip isolation, filtering, dynamic extension columns, and
  committed session restore with focused explorer-ui/model tests.

## 3. Blocking UTIT regression

- [x] 3.1 Extend the Details-column headful automation helper to hold the primary button while moving
  only past the immediately adjacent right-hand midpoint and expose a pre-mouse-up observation point.
- [x] 3.2 Assert before mouse-up that both the dragged header and a representative data cell changed
  bounds, then release and verify the committed order remains.
- [x] 3.3 Perform the inverse adjacent leftward drag, verify the same midpoint threshold, and record DPI-
  aware physical pointer/header bounds plus failure screenshots and report fields.
- [x] 3.4 Register/update the blocking UTIT manifest artifacts and run the focused headful case to a
  passing report.

## 4. Validation and completion

- [x] 4.1 Run formatting, explorer-ui/model focused tests, and any affected workspace compile checks;
  resolve warnings or failures introduced by this change.
- [x] 4.2 Run `openspec validate live-details-column-drag-preview --strict`, review requirement-to-test
  traceability and the final diff, and mark every completed task with its reproducible evidence.
