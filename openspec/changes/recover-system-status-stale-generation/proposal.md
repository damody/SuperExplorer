## Why

When `system-status-host` restarts after the UI's last snapshot, volume and other system controls send an obsolete host generation. The host safely rejects the command, but SuperDesktop currently drops the user's action instead of resynchronizing, leaving volume controls nonfunctional and printing a recoverable race as an error.

## What Changes

- Resynchronize with the current status-host snapshot after a `StaleGeneration` terminal.
- Retry the unchanged command once with a new correlation ID, generation, and deadline.
- Preserve generation fencing and report only final, non-recovered failures to the console.
- Refresh authoritative status after the terminal so volume and mute UI converge with Windows Core Audio state.
- Add restart-race and headful volume-control coverage.

## Capabilities

### New Capabilities

- `system-status-generation-recovery`: Defines bounded resynchronization and replay for system-status commands rejected before dispatch by a host-generation change.

### Modified Capabilities

None.

## Impact

The change affects SuperDesktop's system-status command orchestration, status reconciler tests, status-host fixtures, and GUI UTIT coverage. It does not change the IPC schema, weaken host validation, add dependencies, or change Windows volume ranges and step semantics.
