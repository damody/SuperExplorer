## Why

ADB clipboard items must remain pasteable when the user navigates to any writable filesystem supported by SuperExplorer. The transfer engine already has generic Local and Virtual routing, but the newly exposed context-menu Paste path needs provider-independent availability and regression coverage so it cannot silently work only for SFTP-to-Local.

## What Changes

- Make internal file Paste availability depend on clipboard validity and destination write capability, not the ADB/SFTP provider name.
- Ensure ADB clipboard items route through the shared transfer engine to Local, SFTP, ADB, and other registered writable virtual providers.
- Add regression tests for ADB-to-Local and ADB-to-remote transfers, including provider routing, current-folder destination, and failure behavior.
- Preserve existing conflict, cancellation, staging, Cut cleanup, and native Windows clipboard semantics.

## Capabilities

### New Capabilities

- `provider-independent-file-paste`: Provider-independent internal clipboard Paste across Local and registered writable virtual filesystems.

### Modified Capabilities

None.

## Impact

The change affects `explorer-ui` command availability/state tests, `explorer-app` clipboard dispatch tests, and `explorer-remote` transfer routing tests. It does not change provider APIs, credentials, persisted formats, dependencies, or external clipboard formats.
