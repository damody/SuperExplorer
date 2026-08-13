## Why

`SuperExplorerMft` currently performs a complete MFT scan of every available volume after each fixed 30-second delay. On the current C: and D: volumes this repeatedly raises the service working set to roughly 400–500 MB and rewrites hundreds of megabytes of index data even when nothing changed, so the service needs a change-driven freshness contract rather than periodic reconstruction.

## What Changes

- Build or validate one complete MFT base snapshot when a fixed NTFS volume is initialized.
- Replace the fixed 30-second rebuild loop with blocking USN Journal readers and bounded in-memory change coalescing, with current proven query state updated promptly from the journal.
- The later `mft-sqlite-foreground-persistence` change supersedes generation sidecars and the 10-second durability deadline with foreground-gated SQLite durability; this change retains only the USN ingestion, normalization, memory freshness, and correctness-loss contracts.
- Apply accepted deltas for a query batch, then release complete volume topology from the Host and retain only data-column terminal results within three levels of the active folder.
- Fall back to a single full rebuild only when the journal cursor, volume identity, persisted generation chain, or bounded event stream cannot preserve correctness.
- Add per-volume diagnostics and installed-service resource/freshness verification.
- Preserve the existing folder-size consumer APIs and calculation-method presentation; this is not a plugin-specific event implementation.

## Capabilities

### New Capabilities

- `event-driven-mft-index`: Defines initial MFT snapshot construction, USN Journal event ingestion, durable delta/checkpoint ordering, Host application and cache invalidation, recovery, lifecycle, diagnostics, and resource/freshness gates.

### Modified Capabilities

None. Existing extension column contracts continue consuming Host-owned folder-size results without a public ABI change.

## Impact

- Windows service and MFT primitives in `crates/explorer-app/src/bin/mft_service.rs` and `crates/explorer-app/src/mft_size_map.rs`.
- Host folder-size snapshot/cache logic in `crates/explorer-app/src/folder_size_service.rs` and its application integration.
- Persisted MFT cache files under `%ProgramData%\SuperExplorer\MftIndex`, including a versioned migration/recovery path.
- Windows installer/service upgrade and shutdown behavior.
- Unit, NTFS integration, UTIT manifest, installed-service performance evidence, and diagnostic reporting.
- No new network service, external dependency, plugin ABI, or non-NTFS guarantee.
