## Why

Search already has Everything, MFT, and filesystem enumeration, but the app auto-selects and silently falls back. Users cannot choose an engine, and unsupported backends look available.

## What Changes

- Add a single-select Search engine group on Folder Options → General: Everything, MFT, and file enumeration.
- Persist the preference on view settings with a serde default of Everything so old sessions still decode.
- Probe availability when Folder Options opens and again before search. Disabled radios cannot be clicked and show a hint.
- Run only the selected engine. Do not fall back to another backend.
- Add MFT filename search by reconstructing paths from the volume SQLite index.

## Capabilities

### New Capabilities

- `search-engine-preference`: Folder Options single-select search engine, availability, persistence, and exclusive execution.

### Modified Capabilities

- `search-backend-selection`: Remove silent Everything → LocalIndex/filesystem fallback when the user has chosen an engine.

## Impact

Affects `explorer-model` view/session settings, `explorer-ui` Folder Options, `explorer-i18n` catalogs, `explorer-shell-win` search pipeline, `explorer-mft` path reconstruction, and README. No schema version bump; new field uses serde default. SuperDesktop is unchanged.
