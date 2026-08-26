## Why

After restoring an ADB/SFTP tab, folder rows can show the simplified vector placeholder when item-specific Windows Shell visuals are absent, including after returning to a local directory. The render snapshot already contains an authentic generic Windows Shell folder texture, but file rows do not use it as their fallback.

## What Changes

- Prefer item-specific Shell icons or thumbnails for file rows as today.
- Use the generic Windows Shell folder texture for container rows that lack an item-specific visual.
- Retain the vector placeholder only when neither Shell texture is available.
- Prevent non-container files from receiving the folder fallback.

## Capabilities

### New Capabilities

- `file-row-folder-icon-fallback`: Defines authentic Shell folder fallback selection for local and remote-backed file rows.

### Modified Capabilities

None.

## Impact

- Affected code: `crates/explorer-ui/src/chrome.rs` and focused explorer-ui tests.
- No public API, persisted state, cache key, provider routing, or dependency changes.
- Existing cached generic Shell texture is reused; no additional per-row service requests are introduced.
