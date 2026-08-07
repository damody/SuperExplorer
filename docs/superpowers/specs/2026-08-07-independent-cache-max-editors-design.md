# Independent Cache Maximum Editors Design

## Goal

Make every cache and MFT index component shown in Folder Options independently configurable with a numeric maximum. Applying or confirming Folder Options must update the real owning subsystem immediately, persist the values, and make telemetry report the new limits without requiring navigation or a later folder-size query.

## User-visible behavior

Each configurable telemetry row shows its current usage, effective maximum, and a whole-number MB textbox. The following rows are configurable:

| Row | Default | Minimum | Maximum | Enforcement owner |
|---|---:|---:|---:|---|
| Icon memory | 24 MB | 8 MB | 1024 MB | Explorer UI icon cache |
| Shared/base icon memory | 8 MB | 4 MB | 256 MB | Explorer UI shared icon cache |
| Thumbnail memory | 128 MB | 32 MB | 2048 MB | Explorer UI thumbnail cache |
| Extension data-column memory | 32 MB | 8 MB | 2048 MB | Host data-column cache |
| Icon GPU | 32 MB | 8 MB | 2048 MB | GPUI renderer cache |
| Thumbnail GPU | 128 MB | 32 MB | 4096 MB | GPUI renderer cache |
| Icon BC7 disk | 512 MB | 64 MB | 8192 MB | Shell icon disk cache |
| Thumbnail BC7 disk | 1024 MB | 128 MB | 16384 MB | Shell thumbnail disk cache |
| Extension data-column disk | 256 MB | 32 MB | 8192 MB | Host data-column disk cache |
| Persisted MFT index | 1024 MB | 256 MB | 16384 MB | MFT Service persisted store |
| Volume index memory | 512 MB | 128 MB | 16384 MB | MFT Service volume index |
| File data memory | 256 MB | 64 MB | 16384 MB | MFT Service file data |
| Folder aggregates memory | 512 MB | 128 MB | 16384 MB | MFT Service aggregates |
| MFT Service LRU | 512 MB | 128 MB | 16384 MB | MFT Service result LRU |

The textbox accepts decimal integer MB values only. Apply and OK parse all fields as a single transaction. Empty or non-numeric fields restore their last valid committed value. Values outside their row-specific range are clamped and the textbox is rewritten to the effective value. Cancel discards every draft value.

## Settings model and persistence

Replace scattered cache budget fields with a versioned `CacheBudgetSettingsV1` value embedded in view/session settings. Each field has a centralized default, minimum, maximum, and normalization function. Existing session files migrate by applying current defaults for absent fields and normalizing stored legacy values.

Folder Options owns one editable state per configurable row. Text changes update only the local draft. Apply and OK first normalize every visible editor, dispatch one complete cache-budget action, then commit and persist the settings. This prevents the current failure where the textbox contains `2048` but Apply commits an older `512` draft.

## Runtime propagation

Committed settings are distributed to the actual owners:

1. UI memory and Host caches update their byte budgets and evict immediately.
2. GPUI icon and thumbnail GPU cache budgets are updated through the renderer cache API.
3. Disk caches update their persisted policy and run bounded background pruning.
4. MFT Service receives a dedicated versioned `SetCacheBudgets` IPC request. Limit updates do not depend on a folder-size query. The response returns the effective normalized limits, and telemetry refresh reflects them on the next one-second sample.

If the service is unavailable, the committed settings remain persisted and the Host retries configuration when the service reconnects. The UI marks service limits unavailable instead of showing the previous limit as if the update succeeded.

## Independent hard trimming

The user selected strict per-structure trimming even when it can make folder-size results incomplete.

- Each memory structure accounts its own allocated estimate and evicts least-recently-used or oldest records until it is within its independent budget.
- Persisted MFT data prunes the oldest persisted records until its disk budget is satisfied; writes use a temporary replacement and atomic rename so interruption cannot corrupt the remaining index.
- Trimming one MFT structure records an incomplete-generation marker. Queries touching missing index, file, or aggregate data return `partial = true` rather than an apparently exact value.
- Details and Size Map render partial results with a visible `Partial` state and retain the known byte count only as an explicitly incomplete value.
- Raising a limit does not invent missing data; MFT journal/index processing repopulates it as records are observed or rebuilt.

## Telemetry UI

Telemetry rows use a common component containing label, usage, effective maximum, and number editor. Subtotals remain read-only because they are derived values. `GPU (BC7): Available`, entry/hit/miss counters, and section headers also remain read-only. Every configurable row exposes an automation ID for UITEST.

## Verification

Unit and integration coverage must verify:

- every default, minimum, maximum, and clamp;
- editing multiple fields, Cancel, Apply, and OK semantics;
- session persistence and legacy migration;
- immediate MFT `SetCacheBudgets` IPC and returned effective values;
- independent trimming of every MFT structure and partial-result propagation;
- Host, GPU, and disk cache budget updates;
- service reconnect reapplication;
- telemetry editors and effective limits through UITEST.

Final validation builds with `build_test_install.bat`, installs the test package, changes MFT Service LRU from 512 MB to 2048 MB, confirms telemetry changes to 2048 MB without navigation, changes representative limits in every cache class, restarts SuperExplorer, and captures screenshots proving persistence and runtime enforcement.
