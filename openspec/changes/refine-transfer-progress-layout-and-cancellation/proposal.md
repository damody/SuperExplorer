## Why

Long-running Local, ADB, and SFTP transfers currently present an oversized cancel control, irregular progress refresh, and cancellation that can appear unresponsive while a provider is busy. The operation surface must remain responsive and space-efficient while preserving truthful progress and source safety.

## What Changes

- Split an active operation row into a fixed 250px cancel region and a progress region that fills the remaining width.
- Publish ordinary byte progress at no more than one visible update per 200ms while keeping lifecycle and terminal transitions immediate.
- Show an immediate cancelling state and promptly interrupt ADB/SFTP work, cross-remote staging, and local streaming where the provider permits interruption.
- Preserve the last real byte/item counts on cancellation and never delete a move source unless the destination completed successfully.
- Add focused automated and headful verification, then package/install through `build_test_install.bat`.

## Capabilities

### New Capabilities

- `transfer-progress-and-cancellation`: Operation-surface layout, progress publication cadence, and request-scoped cancellation behavior across Local, ADB, SFTP, and staged cross-provider transfers.

### Modified Capabilities

None.

## Impact

Affected areas include the GPUI operation center, application operation state, remote transfer progress reporter, ADB subprocess lifecycle, SFTP streaming loops, Windows shell progress publication, transfer-engine stage boundaries, focused tests, and the existing Windows packaging/install workflow. No public extension ABI or persistent credential format changes are required.
