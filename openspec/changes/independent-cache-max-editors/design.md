## Context

Cache limits are currently split across `ViewSettings`, UI-local cache objects, Shell disk caches, GPUI renderer caches, Host extension-column storage, and the MFT Windows Service. The MFT service learns its LRU budget only as a field on a folder query, so changing the Folder Options textbox can leave telemetry at the old 512 MB indefinitely. The approved source design is `docs/superpowers/specs/2026-08-07-independent-cache-max-editors-design.md`.

The work spans persistence, UI interaction, IPC compatibility, memory/disk enforcement, partial-result correctness, packaging, and installed-build validation. Existing user worktree changes are authoritative and must be integrated rather than reverted.

## Goals / Non-Goals

**Goals:**

- Centralize 14 independently normalized cache budgets with the exact approved defaults, minima, and maxima.
- Provide synchronized integer editors and 400 px logarithmic progress-sliders.
- Make Apply/OK transactional and make committed limits reach their owners immediately.
- Hard-trim each MFT structure independently and explicitly mark affected results partial.
- Preserve older session and IPC compatibility and reapply settings after service reconnect.
- Verify the installed test package, not only unit-test binaries.

**Non-Goals:**

- Making derived subtotals, availability labels, or hit/miss counters editable.
- Guaranteeing exact folder sizes after the user configures a hard limit too small to retain required MFT records.
- Replacing BC7, the current telemetry sampling interval, or the service installation model.

## Decisions

### Versioned aggregate settings contract

Introduce `CacheBudgetSettingsV1` in `explorer-model` and embed it in persisted view settings with serde defaults. One descriptor table defines stable ID, default, minimum, maximum, and display label for all 14 budgets. Normalization occurs at deserialization, UI commit, and trust boundaries. An aggregate action replaces per-field Apply dispatches so the displayed editors and committed draft cannot diverge.

Alternative: retain scattered fields and callbacks. Rejected because the existing MFT bug is caused by exactly that split commit path and adding 13 more callbacks magnifies it.

### Shared editor and logarithmic slider

Folder Options owns one editable-text entity per descriptor and a common `CacheBudgetEditor` render model. The 400 px slider maps logarithmic position across the row bounds and snaps pointer/keyboard changes to the approved shared sequence: `8, 16, 24, 32, 48, 64, 72, 84, 96, 128, 192, 256, 320, 384, 512, 640, 768, 1024, 1280, 1536, 2048, 2560, 3072, 4096, 5120, 6144, 8192, 10240, 12288, 16384`. Bounds are always inserted. Arbitrary valid textbox values use logarithmic interpolation until the next slider gesture snaps them.

Alternative: linear sliders. Rejected because a 400 px 16 GiB range cannot adjust low budgets accurately.

### Explicit runtime configuration

After persistence succeeds, the root distributes the complete settings snapshot to UI caches, Host caches, renderer caches, disk caches, and the MFT client. MFT adds a framed, versioned `SetCacheBudgets` request and response containing normalized effective limits. Diagnostics report unavailable/pending on failure; a reconnect hook retries the latest committed snapshot. Folder queries no longer mutate configuration.

Alternative: issue a synthetic folder query after Apply. Rejected because it couples configuration to navigation and cannot configure the other MFT structures.

### Independent strict enforcement and partial lineage

Each MFT store has independent accounting and oldest/LRU trimming. Persisted pruning writes a replacement file and atomically renames it. Every trim advances an incomplete-generation marker identifying the affected structure. Query aggregation carries partial lineage; Details and Size Map display `Partial` and never present the known subtotal as exact. Raising a limit permits journal/rebuild repopulation but does not clear partial state until completeness is proven.

Alternative: evict complete volume bundles. Rejected by explicit user choice in favor of tighter independent memory control.

### Planning adjustments and evidence

Task refinements that preserve scope/contracts are A-level. Corrections within the approved behavior are B-level and require design/spec/task updates plus stale evidence replacement. Any change to supported budgets, thresholds, partial semantics, IPC compatibility, platform permissions, or required installed-build evidence is C-level and requires user approval. Evidence is stored under `openspec/changes/independent-cache-max-editors/evidence/` with one indexed record per task leaf.

## Risks / Trade-offs

- **Independent trimming produces incomplete sizes** → propagate a typed partial flag end-to-end and test that exact formatting is impossible for affected results.
- **Disk pruning can lose recoverable index acceleration data** → atomic replacement, journal-based repopulation, and no deletion of source filesystem data.
- **4096/16384 MB values overflow narrow arithmetic** → settings use `u32` MB and byte calculations use checked `u64`/`usize` conversions.
- **Old service ignores new configuration** → version negotiation, unavailable telemetry, and retry after upgraded service reconnect.
- **400 px controls overflow the window** → rows wrap controls beneath labels while the active page remains vertically scrollable.
- **Immediate mass eviction stalls UI/service** → UI caches trim boundedly on their owner thread; disk pruning and MFT rebuild work run off the UI thread with cancellation.

## Migration Plan

1. Deserialize absent new fields with approved defaults and normalize legacy icon/thumbnail/MFT values into `CacheBudgetSettingsV1`.
2. Ship client and service IPC support together in the installer while retaining diagnostics/query compatibility with older endpoints.
3. On first committed settings load, configure all in-process owners and configure the service when connected.
4. Rollback uses the previous executable/service; unknown persisted fields are ignored by older serde readers where supported, while the new build retains defaults if fields are absent on return.

## Open Questions

None. Defaults, bounds, slider sequence, strict trimming, partial behavior, and installed-build validation are approved.
