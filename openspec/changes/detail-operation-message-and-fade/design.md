## Context

`OperationCenter` currently renders the latest `OperationRecord` but terminal states collapse to generic English text and remain indefinitely. The model should stay clock-free; visibility timing belongs to window UI state. Local and virtual locations already use typed `LocationDescriptor` values.

## Goals / Non-Goals

**Goals:**

- Produce detailed, safe summaries for every file-operation kind.
- Keep active operations visible and remove terminal messages after an exact eight-second UI lifetime.
- Use one behavior for Local, ADB and SFTP without exposing credentials.
- Preserve partial outcome details and cancellation controls.

**Non-Goals:**

- Notification history or stacked messages.
- Provider protocol, storage or I/O changes.
- A full application regression run.

## Decisions

### Keep timing in UI state

`AppViewState` records the latest terminal request id and `Instant` when an accepted `OperationFinished` event is applied. Starting a new operation clears the old terminal notice. This avoids contaminating serializable model records with presentation clocks and prevents stale events from extending a newer notice.

### Use pure typed summary formatting

A pure formatter maps `FileOperationKind` and terminal state to operation label, source summary, destination and result. Multi-source operations show the first full location plus the remaining count. Rename combines a filesystem parent safely when possible and otherwise presents original location plus new leaf name. Location display uses canonical descriptors and never authentication secrets.

### Derive visibility and opacity from elapsed time

A pure presentation function returns visible/full opacity for active operations and terminal ages below seven seconds, linear opacity during seconds seven through eight, and hidden at eight seconds. OperationCenter requests animation frames only while fading; a one-shot delayed invalidation wakes the window at the seven- and eight-second boundaries so an otherwise idle window still transitions and releases layout height.

### Preserve one latest record

The existing latest-record contract remains. No queue is introduced. Partial outcomes retain at most five detail rows.

## Risks / Trade-offs

- [An idle window never redraws at the deadline] → Schedule bounded delayed invalidations at lifecycle boundaries and animation frames only during fade.
- [A stale completion resets visibility] → Match terminal timestamp to the latest accepted request identity.
- [Long paths exceed the row] → Keep one-line summaries and existing layout clipping; summarize additional sources by count.
- [Virtual URI leaks secrets] → Format only canonical `LocationDescriptor` data, never connection credentials or debug payloads.

## Migration Plan

Add transient state, formatter and rendering behavior with focused tests. No persisted migration is required. Rollback restores generic rendering and removes the transient timestamp without affecting operation records.

## Open Questions

None.
