# System-status stale generation recovery design

## Goal

Keep SuperDesktop volume, mute, input, and network controls responsive when `system-status-host` restarts between the last displayed snapshot and a user command. A recoverable generation race must resynchronize and complete the original action instead of surfacing `StaleGeneration` and dropping the operation.

## Existing defect

The UI builds `SystemStatusCommandRequest.expected_host_generation` from `StatusReconciler::snapshot`. `SystemStatusClient` starts or restarts the host as needed, but a newly started host has a different generation. The host correctly rejects the old request before executing it. `apply_system_status_action` then fetches a fresh snapshot but never retries, so volume and mute controls appear broken.

## Decision

Add one bounded recovery cycle at the system-status command boundary:

1. Send the command with the reconciler's current host generation.
2. If the terminal is not `StaleGeneration`, preserve the existing terminal and snapshot reconciliation behavior.
3. On `StaleGeneration`, do not apply or report that obsolete terminal. Fetch an authoritative snapshot from the same client, apply it, and rebuild the request with the new host generation, a new correlation ID, and a fresh deadline.
4. Retry the unchanged command exactly once.
5. Fetch and apply a final snapshot after the terminal so UI state reflects the authoritative Windows observation.

The initial stale request is safe to retry because the host compares generation before dispatching any platform command. A second stale generation, timeout, provider failure, invalid response, or snapshot failure remains terminal and is printed to the console through the existing error reporter. Successful recovery emits trace markers but no error.

## Windows behavior alignment

Volume changes continue through `IAudioEndpointVolume::SetMasterVolumeLevelScalar` with normalized values and are read back through Core Audio. Mute uses the corresponding endpoint control. SuperDesktop does not optimistically invent a volume result: the host returns an observed terminal only after the platform adapter confirms the requested state, and the UI refreshes from an authoritative snapshot. This matches the Windows endpoint-volume notification model while keeping host-generation fencing internal and invisible to the user.

## Boundaries and safety

- Retry applies to the common system-status command path, including volume, mute, input, and Wi-Fi commands.
- Only `StaleGeneration` is automatically retried.
- Retry count is fixed at one; no loop can keep replaying across a restart storm.
- Each attempt has a unique correlation ID and its own deadline.
- No `RefCell` borrow is held across an IPC request or GPUI update.
- The obsolete stale terminal is not inserted into `StatusReconciler`, so it cannot poison terminal deduplication for the retry.

## Verification

- Pure/unit tests cover ordinary success, stale then snapshot then success, two consecutive stale terminals, resync failure, and unique correlation/deadline construction.
- Host tests prove generation mismatch occurs before the Core Audio adapter is invoked.
- Client/fixture integration forcibly restarts the status host between snapshot and volume/mute commands and verifies one retry plus final authoritative state.
- Headful UTIT opens the volume flyout, changes volume through pointer and keyboard paths, restarts the host during the sequence, and verifies the displayed and observed endpoint values converge without a `status:command` error.
- Workspace tests, warnings-denied Clippy, release build, installer packaging, and strict OpenSpec validation are blocking gates.

## Non-goals

This change does not redesign the volume flyout, change volume step size, implement per-application mixing, suppress genuine provider errors, or weaken protocol validation.
