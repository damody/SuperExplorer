## Why

SuperExplorer currently divides one memory setting between icons and thumbnails, concealing which cache consumes memory and preventing users from tuning their different workloads. Its raw-RGBA disk entries also consume avoidable space, while Folder Options cannot explain memory, disk, extension Host, or MFT Service cache use.

## What Changes

- Restore independent icon and thumbnail memory budgets with defaults of 32 MiB and 128 MiB, independent persistence, and immediate per-cache LRU enforcement.
- Add Host-owned cache telemetry covering registered memory caches, disk caches, extension data-column Host storage, and fixed-size MFT Service diagnostics.
- Display memory, disk, and MFT Service cache usage in Folder Options and refresh the single-flight snapshot once per second while that window is open.
- Replace raw-RGBA Shell icon disk entries with lossless WebP and thumbnail disk entries with quality-80 lossy WebP inside a bounded, checksummed, versioned envelope.
- Preserve existing session icon settings, default the new thumbnail setting for prior sessions, and regenerate obsolete raw cache entries lazily rather than blocking startup with conversion.
- Add contract, corruption, migration, UI lifecycle, UITest, and Release memory-profile evidence.

## Capabilities

### New Capabilities

- `independent-cache-budgets-and-telemetry`: Independent memory-cache budgets plus Host-aggregated memory, disk, extension, and MFT Service telemetry shown live in Folder Options.
- `webp-shell-cache-persistence`: Bounded, versioned, independently accounted WebP persistence for Shell icons and thumbnails.

### Modified Capabilities

None. Existing extension persistence requirements remain Host-owned; this change adds observable telemetry without transferring policy to plugins.

## Impact

- `explorer-model`: view settings, normalization, and session compatibility.
- `explorer-jobs` and `explorer-ui`: independent LRU controls, telemetry snapshots, one-second lifecycle, and Folder Options presentation.
- `explorer-shell-win`: icon and thumbnail WebP codecs, cache envelope, quota accounting, corruption recovery, and disk statistics.
- `explorer-extension-host` and `explorer-app`: registered Host cache reporting and asynchronous disk sampling.
- `superexplorer-mft-service`: local-only fixed-size diagnostics IPC and aggregate counters.
- UITest/OpenSpec evidence and Release memory profiling.
- Dependency impact: enable or add a Rust WebP codec whose license and decoded-resource behavior pass repository policy and offline locked builds.
