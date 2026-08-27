## Why

Copying an item in an SFTP location creates valid application-owned clipboard state, but a local item context menu does not expose Paste. Users therefore cannot discover or invoke the already-supported remote-to-local transfer from the context menu they opened.

## What Changes

- Add Paste to the host-owned native context-menu command contract.
- Project Paste into writable local context menus for background, file, and folder hits whenever the application clipboard is usable.
- Keep the paste destination fixed to the current tab's folder rather than the item under the pointer.
- Expose the same Paste action in remote item and background context menus.
- Verify SFTP-copy-to-local-paste routing, typed failure behavior, and destination semantics.

## Capabilities

### New Capabilities

- `current-folder-context-paste`: Defines context-menu Paste availability, current-folder destination semantics, and remote-to-local clipboard routing.

### Modified Capabilities

None.

## Impact

The change affects the internal context-menu request/host-command model, Windows native menu composition, GPUI context-menu action mapping, and remote-service tests. It changes no extension ABI, persisted format, SFTP credential handling, provider identity, or conflict policy.
