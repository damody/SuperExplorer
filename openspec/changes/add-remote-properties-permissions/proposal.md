# Change: Add remote properties and permissions

## Why

ADB and SFTP context menus display Properties, but the command has no remote implementation and users cannot inspect or change POSIX permissions.

## What Changes

- Add an application-owned Properties dialog for one ADB or SFTP item.
- Display the item's general metadata and current Unix mode.
- Add a typed remote chmod operation implemented by ADB and SFTP providers.
- Refresh the directory after a completed permission change and surface failures through existing operation reporting.

## Non-goals

- Editing ownership, ACLs, extended attributes, or multiple items.
- Replacing native Windows Properties for local files.

## Capabilities

### New Capabilities
- `remote-item-properties`: inspect remote metadata and edit POSIX permissions.

### Modified Capabilities
- `virtual-folder-stream-and-mutation`: add a permission mutation for remote providers.
