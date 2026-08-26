## Context

File rows forward pointer movement through `ExplorerAction::UpdateFileDrag` so the reducer can advance left-drag candidates and recover right-button gestures. The same callback runs during ordinary hover. `dispatch_action` currently emits every returned `ActionTrace` with `tracing::info!`, so idle movement produces pointer-frequency `Disabled` records.

## Goals / Non-Goals

**Goals:**

- Remove `UpdateFileDrag` records from normal INFO logs.
- Preserve targeted TRACE diagnostics for pointer updates.
- Preserve all dispatcher and drag-session behavior.
- Keep meaningful lifecycle and ordinary actions at INFO.

**Non-Goals:**

- Changing pointer event registration or drag thresholds.
- Changing action availability or `ActionTrace` outcomes.
- Reclassifying other pointer actions in this change.

## Decisions

Add a small action-classification helper in `actions.rs` that returns whether an action belongs in the normal INFO stream. Capture that classification before `apply_action` consumes the action. After constructing the unchanged `ActionTrace`, use INFO for normal actions and TRACE for `UpdateFileDrag`.

This is preferred over gating the row callback because the dispatcher/reducer owns authoritative drag state while rendering code does not. It is preferred over checking only `ActionOutcome::Disabled` because handled motion during a real drag is equally high frequency.

The helper remains pure and receives `&ExplorerAction`, allowing a focused unit test to verify the boundary without installing a global tracing subscriber.

## Risks / Trade-offs

- [Risk] Operators using only INFO lose per-coordinate drag motion records. → Mitigation: lifecycle events remain at INFO and pointer updates remain available with TRACE enabled.
- [Risk] A broad classifier could hide unrelated actions. → Mitigation: match only `UpdateFileDrag` and test representative drag lifecycle and ordinary actions.

## Migration Plan

No migration is required. Deploy with the next application build. Rollback consists of restoring unconditional INFO logging in the dispatcher.

## Open Questions

None.
