# Suppress Idle File-Drag INFO Log Design

## Problem

Each rendered file row forwards every pointer move as `UpdateFileDrag`, including ordinary hover when no drag candidate exists. The action dispatcher correctly returns `Disabled` in that state, but it currently logs every dispatch at `INFO`. Moving the pointer across the file view therefore floods the normal application log with disabled `UpdateFileDrag` records.

## Decision

Keep the row-level pointer-move callback and action dispatch unchanged because the same path advances active left-drag and right-button gesture candidates. Classify `UpdateFileDrag` as high-frequency pointer telemetry in the dispatcher:

- emit its dispatch trace at `TRACE`, regardless of whether the outcome is `Handled` or `Disabled`;
- continue emitting all other action dispatches at `INFO`;
- keep `BeginFileDrag`, `CancelFileDrag`, external drop, and completed transfer actions at `INFO` so meaningful drag lifecycle events remain visible.

This changes observability only. Action availability, drag state transitions, focus handling, and returned `ActionTrace` values remain unchanged.

## Alternatives Considered

- Remove the row `on_mouse_move` handler while idle. Rejected because rendering code does not own the authoritative drag-session state, and gating the callback there risks missing candidate transitions or right-button release recovery.
- Suppress only `Disabled` updates. Rejected because handled pointer updates are also emitted at pointer frequency and would still flood normal logs during a real drag.
- Remove logging entirely. Rejected because TRACE-level records remain useful for targeted drag diagnostics without appearing under the normal INFO filter.

## Verification

- Add a unit-testable classification helper proving `UpdateFileDrag` is excluded from INFO while representative lifecycle and ordinary actions remain included.
- Run the focused `explorer-ui` tests for the classifier and existing pointer-action behavior.
- Run formatting and `cargo check -p explorer-ui`.

## Compatibility and Rollback

No public API or persisted data changes. Rollback is limited to restoring the unconditional INFO emission in `actions.rs`.
