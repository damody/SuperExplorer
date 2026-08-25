## Why

SuperExplorer currently stores bookmarks inside the replaceable session snapshot, so resetting or recreating session state can erase user-curated bookmarks even though installation itself does not delete LocalAppData. Bookmarks are durable user data and must survive application lifecycle operations independently from windows, tabs, and view state.

## What Changes

- Add a bounded, versioned, recoverable bookmark document under the existing per-user compatibility root.
- Load bookmarks independently from session restore and migrate the legacy session bookmark collection once when no independent document exists.
- Persist bookmark snapshots through the existing background durability worker while keeping session reset scopes unable to delete or empty bookmark storage.
- Preserve bookmarks across install, upgrade, repair, uninstall, and reinstall; only explicit bookmark deletion changes the collection.
- Add migration, corruption recovery, reset-isolation, retry, and installer preservation contract tests.
- Retain the legacy session bookmark field during the transition for downgrade and old-profile compatibility; this is not a breaking schema change.

## Capabilities

### New Capabilities

- `independent-bookmark-persistence`: Defines authoritative bookmark storage, legacy migration, failure recovery, reset isolation, and package lifecycle preservation.

### Modified Capabilities

None.

## Impact

The change affects `explorer-app` bookmark/session storage and lifecycle composition, session persistence tests, product/installer contract tests, the NSIS preservation declaration, and persistence documentation. It adds no third-party dependency, external service, credential storage, public extension ABI change, or new destructive operation.
