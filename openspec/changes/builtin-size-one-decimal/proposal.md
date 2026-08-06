## Why

The built-in Size formatter drops fractional precision for values at or above 10 units, so the same folder appears as `251 GB` in Size and `250.5 GB` in Folder size. Both columns must present the shared byte value consistently.

## What Changes

- Format nonzero built-in Size values in KB, MB, GB, and TB with exactly one decimal place.
- Preserve `0 KB` for zero and the existing minimum-unit behavior for sub-kilobyte files.
- Update unit tests and installed-build screenshot evidence.
- Leave byte sources, sorting, Host cache, MFT Service, and Folder size plugin rendering unchanged.

## Capabilities

### New Capabilities

- `builtin-size-formatting`: Defines stable one-decimal presentation for built-in Size values.

### Modified Capabilities

None.

## Impact

- Changes `crates/explorer-ui/src/formatting.rs` and its tests.
- Changes only presentation strings; no cache, measurement, sort, extension ABI, or installer contract changes.
