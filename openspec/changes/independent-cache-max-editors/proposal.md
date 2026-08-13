## Why

Folder Options currently exposes cache telemetry without a reliable path from edited limits to the owning caches: an MFT limit can display `2048` in the textbox while the service remains at `512 MB`. Users also cannot independently bound the memory, GPU, disk, and MFT structures that dominate SuperExplorer resource use.

## What Changes

- Add independent persisted minimum/default/maximum budgets for every configurable memory, GPU, disk, extension-column, and MFT telemetry row.
- Add a number editor and synchronized 400 px logarithmic progress-slider to every configurable row.
- Commit all Folder Options budget editors transactionally on Apply/OK and discard them on Cancel.
- Propagate committed limits immediately to UI, Host, renderer, disk-cache, and MFT Service owners.
- Add a versioned MFT `SetCacheBudgets` IPC operation so service limits no longer depend on a later folder-size query.
- Enforce independent hard trimming for MFT persisted index, volume index, file data, folder aggregates, and result LRU; expose incomplete results as partial rather than exact.
- Add migration, telemetry, reconnect, UITEST, installer, and installed-build verification.
- Non-goals: changing default folder-size calculation semantics when no structure has been trimmed; exposing derived subtotals or diagnostic counters as editable budgets.

## Capabilities

### New Capabilities

- `independent-cache-budget-controls`: Persisted per-cache limits, Folder Options number/slider controls, transactional application, runtime propagation, and telemetry.
- `mft-independent-budget-enforcement`: Versioned service configuration, per-structure MFT trimming, partial-result behavior, reconnect recovery, and diagnostics.

### Modified Capabilities

None.

## Impact

Affected systems include `explorer-model` settings/session persistence, Folder Options and UITEST automation in `explorer-ui`, Host extension-column caches, GPUI icon/thumbnail renderer caches, Shell BC7 disk caches, MFT IPC/query/index/service implementations, cache telemetry, and test installer packaging. Existing sessions remain compatible through defaulted normalized migration. The MFT IPC addition is versioned and must retain safe behavior when either endpoint is older.
