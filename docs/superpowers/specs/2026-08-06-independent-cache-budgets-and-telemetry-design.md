# Independent Cache Budgets, Telemetry, and WebP Disk Cache Design

## Goal

Give icon and thumbnail memory caches independent user-configurable budgets while showing current usage for SuperExplorer memory caches, disk caches, and the MFT Service in Folder Options. Store icon and thumbnail disk entries as WebP to reduce disk usage without degrading icon alpha edges.

## Scope

This change covers:

- independent icon and thumbnail memory-cache settings;
- live cache-usage telemetry owned and aggregated by the Host;
- memory, disk, and MFT Service cache reporting in Folder Options;
- WebP persistence for Shell icons and thumbnails;
- migration from the current session and raw-RGBA disk-cache formats;
- unit, protocol, and UI integration tests.

It does not attempt to report Windows-managed caches outside SuperExplorer, GPU driver allocations, the filesystem cache, or arbitrary plugin-private storage that was not registered with the Host.

## Settings and Compatibility

`ViewSettings` gains a separate thumbnail memory-cache budget. The existing `icon_cache_memory_mb` remains the icon budget and defaults to 32 MiB. The new thumbnail budget defaults to 128 MiB. Both values use bounded presets and are persisted by the existing versioned session contract.

Sessions that predate the thumbnail field deserialize it with the 128 MiB default. Existing icon settings retain their meaning and value. Lowering either setting immediately updates only that cache and evicts its least-recently-used entries until it is within its new byte budget. Raising one setting does not change the other.

## Host Cache Telemetry

The Host owns a `CacheTelemetrySnapshotV1` assembled from registered cache reporters. A reporter exposes a stable cache identity, category, current bytes, optional byte limit, entry count, and availability state. Counters use saturating integer arithmetic, and unknown or temporarily unavailable values remain explicit rather than being reported as zero.

The initial reporters are:

- visible Shell icon texture memory;
- shared/base icon memory;
- decoded thumbnail memory;
- extension data-column Host memory and persistent cache;
- icon WebP disk cache;
- thumbnail WebP disk cache;
- MFT Service aggregate LRU and persisted index storage.

Plugins do not publish arbitrary numbers directly into the UI. Host-managed extension cache storage reports through the Host registry, preserving the existing boundary that persistence policy belongs to the Host.

Folder Options requests a snapshot once per second while the window is open. Memory counters are captured directly. Disk sizes are refreshed by a background sampler and returned from its latest completed snapshot; the UI thread never recursively scans cache directories. Refresh requests are single-flight so a slow sample cannot accumulate work.

## Folder Options UI

The View page contains independent controls:

- `Icon memory cache limit`, default 32 MB;
- `Thumbnail memory cache limit`, default 128 MB.

A `Cache usage` section is divided into:

1. **Memory** — icon, shared/base icon, thumbnail, extension data-column Host cache, and subtotal.
2. **Disk** — icon WebP, thumbnail WebP, extension data-column persistent cache, and subtotal.
3. **MFT Service** — aggregate LRU usage and limit, persisted index size, hits, and misses.

Bounded caches display `used / limit`. Host-managed caches without a configured hard limit display their current usage and `Managed by Host`. A disconnected or uninstalled service displays `Unavailable`; it never blocks or closes Folder Options. Totals only include available byte values and visually identify when one or more components are unavailable.

## WebP Disk Format

The current raw-RGBA disk-cache schema is replaced by a new versioned WebP schema:

- icons use lossless WebP with alpha preservation;
- thumbnails use lossy WebP at quality 80;
- entries retain a bounded binary envelope containing magic, schema, cache kind, key digest, decoded dimensions, encoded length, and checksum;
- files use a `.webp` extension and are atomically published through a temporary file;
- load validates the envelope, encoded length, checksum, dimensions, decoded stride, and maximum decoded bytes before constructing an owned payload.

Icon and thumbnail cache roots remain separate so their policy, accounting, and clearing operations stay independent. Existing `.rgba` entries are not converted in bulk. A new-schema miss regenerates the entry through the normal Shell path. Existing cache cleanup removes obsolete entries under normal quota enforcement, avoiding a startup migration scan.

## MFT Service Diagnostics

The MFT Service named-pipe protocol gains a separate fixed-size diagnostics request and response. It returns only aggregate telemetry: cache bytes, configured limit, entry or volume count, persisted index bytes, hits, misses, and generation. It never returns paths, file names, individual file sizes, or the MFT index.

The diagnostics endpoint uses the same local-only pipe security boundary as folder-size queries. Arithmetic is bounded, malformed requests are rejected, and an unavailable service maps to an explicit UI state.

## Failure and Performance Behavior

- Cache accounting failures do not fail navigation or thumbnail rendering.
- Corrupt WebP entries are rejected and removed; the original provider is used as fallback.
- WebP encoding remains off the UI thread and respects existing job cancellation and resource limits.
- Disk usage sampling is asynchronous and single-flight.
- Closing Folder Options cancels its refresh timer and releases its subscription.
- Telemetry contains cache identities and numeric counters only; it does not expose user paths.

## Verification

Unit and integration coverage must include:

- independent defaults, normalization, persistence, and prior-session migration;
- changing one budget without changing the other;
- immediate LRU eviction when either budget is lowered;
- lossless icon alpha round-trip;
- thumbnail WebP quality-80 encode/decode within decoded resource bounds;
- corrupt, truncated, mismatched-key, oversized, and decompression-bomb rejection;
- atomic cache publication and independent icon/thumbnail quota enforcement;
- telemetry aggregation, subtotals, unavailable values, and saturating totals;
- non-reentrant one-second refresh lifecycle;
- fixed-size, local-only MFT diagnostics IPC;
- UITest coverage for both budget controls, all three usage sections, live refresh, and unavailable-service presentation;
- Release-build memory profiling after repeated folder navigation to confirm cache usage remains within each configured budget.

## Acceptance Criteria

- Icon and thumbnail budgets can be changed independently in Folder Options.
- Defaults are Icon 32 MB and Thumbnail 128 MB.
- Folder Options updates memory, disk, and MFT cache usage every second without UI stalls.
- Icons are persisted as lossless WebP and thumbnails as quality-80 lossy WebP.
- Each cache remains inside its configured limit after LRU eviction settles.
- Existing sessions restore safely, and obsolete raw-RGBA entries cannot be mistaken for WebP entries.
