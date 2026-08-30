## Why

ADB and SFTP background context menus currently stop at folder creation, so users cannot create
native Linux symbolic links or inspect the directory they are viewing. These omissions make
routine remote filesystem work fall back to external shells even though SuperExplorer already
owns the required menu, worker, and Properties-window infrastructure.

## What Changes

- Add `新增捷徑` to ADB/SFTP background menus and open a dedicated editor for link name and target.
- Create native Linux symbolic links through provider-specific ADB and SFTP implementations while
  permitting relative, absolute, and dangling targets.
- Add background `內容` that opens the existing remote Properties experience for the current
  directory using provider metadata.
- Preserve remote-menu visuals, dismissal, accessibility, non-blocking I/O, stale-result rejection,
  and current item-menu command behavior.
- Add provider, state, window, command-membership, and headful ADB/SFTP verification.

## Capabilities

### New Capabilities

- `remote-symbolic-link-creation`: Dedicated-window creation of native ADB/SFTP symbolic links with
  safe child-name validation, asynchronous execution, refresh, and recoverable error handling.
- `remote-current-directory-properties`: Background Properties for the current ADB/SFTP directory
  using authoritative provider metadata and the existing owned Properties window.

### Modified Capabilities

None. No matching baseline remote-menu capability currently exists under `openspec/specs`.

## Impact

- Extends the provider-neutral contract in `explorer-remote` and its ADB/SFTP implementations.
- Adds a GPUI remote-shortcut editor window and observer wiring across `explorer-ui` and
  `explorer-app`.
- Extends remote background command state, async completion, and metadata projection.
- Uses existing ADB executable and SFTP library dependencies; no new dependency or persistence
  migration is required.
- Does not alter Local filesystem context menus or create Windows `.lnk`/Linux `.desktop` files.
