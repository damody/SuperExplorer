## Why

Details headers use the dynamic column registry order while row cells are emitted in hard-coded
renderer-family groups, allowing visible titles to become detached from their data. This already
swaps Code lines and Folder size, and it will recur under production extension enable/disable and
reload combinations unless descriptor identity becomes authoritative for both header and row.

## What Changes

- Make the registry's stable `ColumnId` order the single ordering source for details headers and
  row cells.
- Dispatch each row cell by exact descriptor ID instead of renderer category or vector position.
- Fail closed for stale or mismatched extension runtimes so one column can never display another
  column's data.
- Rebuild the visible projection across extension lifecycle changes without stale cell slots.
- Add combination, lifecycle, mismatch, and headful screenshot verification for Folder size, Lua
  Code lines, Rust Main code lines, and Lock owners.

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `extension-jobs-values-and-dynamic-columns`: Strengthen dynamic-column layout requirements so
  headers, widths, cells, sorting identities, and accessibility identities share one ordered
  descriptor projection and remain aligned through extension lifecycle changes.

## Impact

- Primary implementation: `crates/explorer-ui/src/chrome.rs` and focused UI/model tests.
- Extension lifecycle integration: existing runtime/visual projections passed by
  `crates/explorer-ui/src/lib.rs`; no public ABI or manifest-format change is intended.
- Verification: Rust unit tests plus `scripts/smoke_tokei_plugin_headful.ps1` and captured evidence.
- Compatibility: existing stable `ColumnId` order and saved visibility/width preferences remain
  unchanged; this corrects rendering alignment without adding user-controlled reordering.
