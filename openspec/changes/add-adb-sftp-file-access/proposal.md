## Why

SuperExplorer currently delegates every navigable location and file mutation to
the Windows Shell, so Android devices and SFTP servers cannot be browsed or
used as transfer endpoints.  The repository's reference implementation proves
the product needs both workflows; adding them now uses the existing typed
virtual-location and request-correlation contracts instead of treating remote
paths as local Windows paths.

## What Changes

- Add a host-owned remote-filesystem provider layer for ADB and SFTP paths.
- Add an ADB provider that discovers authorized devices and supports direct
  `adb://<serial>/<path>` navigation, including phone storage paths.
- Add an SFTP provider that connects through SSH with password authentication,
  verifies a host key, and supports direct `sftp://<profile>/<path>` navigation.
- Add remote directory listing, folder creation, rename, delete, refresh, and
  copy/move operations between local, ADB, and SFTP endpoints.
- Extend file clipboard and OLE drag/drop so Local, ADB, and SFTP items can be
  copied or moved in either direction without consuming text/image clipboard data.
- Add navigation-pane connection entry points and an SFTP connection dialog;
  credentials are stored only in Windows Credential Manager and never in tabs,
  bookmarks, logs, diagnostics, or the repository.
- Add deterministic provider tests plus opt-in integration tests for the
  supplied SFTP endpoint and a connected authorized Android device.

## Capabilities

### New Capabilities

- `remote-provider-runtime`: Provider contracts, virtual URI resolution, request
  lifecycle, and cross-provider mutation dispatch.
- `adb-file-access`: Android device discovery and Android filesystem browsing
  and transfer through the installed Android Debug Bridge.
- `sftp-file-access`: Secure SSH/SFTP connection profiles, remote file access,
  transfer, host-key verification, and credential handling.
- `remote-access-ui`: Explorer navigation, address input, status/error surfaces,
  and remote destructive-operation confirmation.

### Modified Capabilities

- `virtual-folder-stream-and-mutation`: Route virtual locations through remote
  providers and preserve its stream/cancellation/mutation invariants.

## Impact

The change affects `explorer-model` location and operation contracts, the
application composition root, Windows credential access, the Shell STA routing
boundary, and GPUI navigation/operation UI. It adds Rust SSH/SFTP dependencies
and invokes the user-installed `adb.exe`; no external service configuration or
bundled credentials are introduced.
