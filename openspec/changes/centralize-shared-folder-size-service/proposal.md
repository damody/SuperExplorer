## Why

Folder Size and Size Map currently own separate recursive measurement paths, caches, cancellation, and lifecycle behavior even though they need the same directory-size facts. This duplicates expensive I/O, can produce inconsistent values, and prevents the application from centrally selecting accelerated MFT or Everything backends. Installed-build evidence on 2026-08-06 also showed that a missing `SuperExplorerMft` service and an incomplete Everything result were silently accepted, publishing false exact `0 B` folder sizes.

## What Changes

- Add a host-owned Folder Size Snapshot Service that coalesces aggregate and tree consumers over one generation-safe internal snapshot.
- Add a lazy local-NTFS MFT path that elevates only the helper through UAC, with Everything and bounded recursive fallbacks.
- Enforce one Explorer-safe semantic policy across backends: canonical-root containment and no recursion through directory reparse points.
- Add shared memory/disk caching, watcher/USN invalidation, cancellation, partial states, diagnostics, and backend profiling.
- **BREAKING**: remove filesystem measurement from the visual-column extension responsibility; Folder Size becomes a renderer over host aggregate data.
- Modify Size Map to consume the shared host tree snapshot rather than owning an independent recursive scan.
- Preserve independent feature toggles: disabling Folder Size cannot disable Size Map, and vice versa.
- Make MFT service create/configure/start/RUNNING verification a blocking installer contract.
- Require an explicit completeness proof before any accelerated result, especially exact zero, may be published; otherwise fall back to recursive traversal.
- Move persistent data-column cache ownership into the Host; plugins compute cache misses but cannot choose persistence, identity, expiry, or invalidation policy.

## Capabilities

### New Capabilities

- `shared-folder-size-snapshot-service`: Host-owned aggregate/tree snapshots, backend selection, caching, invalidation, elevation, fallback, coalescing, and diagnostics.

### Modified Capabilities

- `extension-jobs-values-and-dynamic-columns`: Visual columns consume host aggregate values and no longer implement folder measurement callbacks.
- `extension-view-modes-and-directory-tree-scan`: Size Map consumes the shared folder tree snapshot while preserving generation, partial-state, scale, selection, and accessibility contracts.
- `extension-package-and-feature-lifecycle`: Feature data requirements bind to authorized host snapshot capabilities without creating plugin-to-plugin dependencies.

## Impact

- Application: new core service plus integration with existing MFT helper, Everything SDK, watcher generations, Folder Size runtime, and Size Map runtime.
- Public ABI: contribution data-requirement descriptors and render-only visual-column boundary; compatibility handling and ABI fingerprint updates are required.
- Extensions: Folder Size loses recursive measurement/cache code; Size Map remains a layout/renderer consumer.
- Packaging: the LocalSystem MFT service is installed and verified RUNNING; the elevated helper remains a fallback and the adjacent Everything DLL remains available only behind its equivalence gate.
- Testing: backend-equivalence profiling, UAC-decline fallback, reparse-point policy, shared-work counters, extension lifecycle matrices, and headful screenshots.
