## Context

The UI stores the latest `SystemStatusSnapshot` in `StatusReconciler` and places its host generation in every command. The periodic observer and UI command path own separate `SystemStatusClient` children, so the displayed snapshot can belong to observer host B while the command is delivered to host A. A replacement host after transport loss produces the same mismatch. The receiving host rejects the obsolete request before platform dispatch, but `apply_system_status_action` currently reports the stale terminal, fetches a snapshot, and stops.

Windows volume writes already use `IAudioEndpointVolume::SetMasterVolumeLevelScalar` and read the endpoint back before reporting observation. The missing behavior is IPC-generation recovery, not the Core Audio adapter.

## Goals / Non-Goals

**Goals:**

- Complete a user action across one status-host generation change.
- Retain generation validation and exactly-once platform dispatch.
- Keep successful recovery invisible except for diagnostic trace markers.
- Reconcile final UI state from an authoritative snapshot.
- Bound retry, deadlines, correlations, and failure reporting.

**Non-Goals:**

- Infinite retry during a restart storm.
- Optimistic volume state, per-application volume, or new IPC fields.
- Suppression of provider, timeout, protocol, or second-generation failures.

## Decisions

### Use a small command-attempt state machine

Extract request construction and terminal classification from `apply_system_status_action`. One logical action owns an immutable `SystemStatusCommand` and makes at most two attempts. Every attempt copies the current reconciler generation, allocates a unique correlation ID, and calculates a fresh deadline.

### Resynchronize only for `StaleGeneration`

If attempt one returns `StaleGeneration`, do not insert or print that obsolete terminal. Request `Snapshot` from the same command client and retain its generation as command-session state, then issue attempt two. Offer the snapshot to the UI reconciler, but do not require acceptance: an independent observer host can have a numerically newer lineage even though only the command client's generation is valid for its stdio transport. The host checks generation before matching and executing the command, so the rejected attempt has no external effect.

All other attempt-one terminals keep existing behavior. Any attempt-two failure is final. A resync response that is missing, invalid, rejected, or not current is final and reported to the console.

### Always perform final observation refresh

After a non-stale terminal, request one final snapshot and offer it to the reconciler. Observed audio commands therefore expose the Core Audio value confirmed by the command host; if the observer lineage supersedes it, the periodic observer supplies the same Windows endpoint state on its next refresh. Accepted asynchronous Wi-Fi commands continue awaiting their normal provider refresh without invented state.

### Preserve focus lifecycle

Input-method actions restore Start focus only after the full logical action finishes. No `RefCell` borrow spans an IPC request or GPUI update.

## Risks / Trade-offs

- [Risk] Retrying a command could duplicate an external action. → Only stale-before-dispatch is retried; the host's generation check remains before platform command matching.
- [Risk] A second host restart can cause a retry loop. → The state machine has a compile-time maximum of two attempts.
- [Risk] A stale terminal can pollute deduplication. → It is classified before applying it to `StatusReconciler`, and retry uses a new correlation ID.
- [Risk] Observer and command hosts have incomparable command authority. → Retry generation comes only from the snapshot returned over the command client's own transport, never from numeric ordering in the UI reconciler.
- [Risk] Snapshot traffic adds latency. → The extra resync occurs only after a race; one final snapshot already exists in the current path.

## Migration Plan

No schema or persisted-data migration is required. Deploy through the normal SuperDesktop installer. Rollback is a code rollback because protocol messages remain compatible.

## Observability and Testing

Trace markers distinguish stale detection, resync, retry, and recovery. Unit tests use a scripted requester to cover response sequences without changing real system volume. Status-host tests prove mismatch precedes dispatch. A headful UTIT changes real endpoint volume by a bounded amount, restores the original value in `finally`, forces a host restart, and verifies the final display and endpoint observation with no `status:command` error.

## Open Questions

None.
