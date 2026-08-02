## Context

SuperExplorer already has an OLE `IDataObject` drag source, a GPUI Windows OLE drop target, typed transfer commands, and asynchronous Shell file operations. The broken behavior is at the seams: Shift-selected rows do not start left drags, the source always publishes Move as the unmodified preferred effect, and the application target does not derive the default effect from real source/destination volumes.

## Goals / Non-Goals

**Goals:**

- Preserve one native OLE pipeline for internal and Explorer-interoperable drags.
- Match Ctrl/Shift and unmodified same-volume/cross-volume behavior.
- Route writable folder-row and current-folder drops to typed, asynchronous operations.
- Reject unsafe destinations and clear transient state on every terminal path.
- Add deterministic contract and UTIT regression coverage.

**Non-Goals:**

- Implement Alt-drag shortcut creation; the backend does not currently support link creation.
- Replace OLE with a GPUI-only drag protocol.
- Add new overwrite semantics; conflicts continue through the existing prompt flow.

## Decisions

### Keep native OLE as the single drag transport

The row gesture crosses the Windows drag threshold and submits `BeginDrag` exactly once. `DoDragDrop` retains live modifier and cursor behavior, while the existing GPUI Windows target exposes paths and OLE metadata to the UI. A custom internal protocol was rejected because it would diverge from dragging to and from Windows Explorer.

### Publish an explicit source preference only for an explicit modifier

Ctrl publishes Copy and Shift publishes Move. An unmodified drag omits `CFSTR_PREFERREDDROPEFFECT`, allowing Windows Explorer targets to select their native default instead of inheriting a false always-Move preference.

For SuperExplorer targets, when the source supplies no preference and no modifier is active, a pure path-volume resolver chooses Move only when every filesystem source and the destination share the same Windows volume prefix; otherwise it chooses Copy. Explicit live modifiers always win.

### Validate destinations before queuing mutations

The UI resolves a folder row or current-folder background to one immutable destination. It rejects empty sources, non-filesystem destinations, a folder dropped onto itself or below itself, and a no-op Move back to the same parent. Copying within the same parent remains allowed so the Shell can apply Explorer's duplicate-name behavior.

### Preserve asynchronous file operations and conflict prompts

Accepted drops become the existing typed `DropExternal` request with `ConflictDecision::Prompt`. The Shell worker resolves identities and performs Copy or Move outside the UI thread. Watcher/operation terminals refresh the affected folders through the existing path.

## Risks / Trade-offs

- **Mounted folders can share a volume without sharing a textual drive/UNC prefix** → Keep the resolver isolated and testable so it can later use a cached native volume identity without changing UI semantics; ordinary drive and UNC behavior is correct now.
- **Selection re-render can replace the pressed row** → Begin the drag candidate immediately after the selection reducer and rely on the central drag session rather than element-local identity after threshold crossing.
- **OLE nested-loop lifecycle can leak transient state** → Reuse the existing cancellation token and assert cleanup for drop, cancellation, navigation, tab switch, and shutdown.

## Migration Plan

No persisted data changes are required. Deploy as a behavioral replacement in the existing drag path. Rollback consists of reverting the UI gesture, default-effect resolver, and UTIT case.

## Open Questions

None.

