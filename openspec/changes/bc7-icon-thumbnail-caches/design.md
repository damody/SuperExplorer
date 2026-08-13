## Context

SuperExplorer's Windows image path crosses four ownership boundaries: Shell extraction in `explorer-shell-win`, cache policy in the Host/UI integration, GPUI image abstractions, and the vendored GPUI D3D11 renderer. The current project disk cache serializes RGBA-derived images as WebP. A cache hit therefore decodes into RGBA, occupies CPU memory, and uploads uncompressed texels through GPUI's `DXGI_FORMAT_R8G8B8A8_UNORM` atlas.

The approved source design is `docs/superpowers/specs/2026-08-06-bc7-thumbnail-gpu-path-design.md`. It requires both Shell icons and thumbnails to use a private BC7 representation across disk, memory, and GPU caches while preserving a provider-backed RGBA fallback. Windows remains on D3D11; Vulkan `VkFormat` constants are not applicable. BC7 support, small-icon quality, dependency suitability, and Release performance are blocking gates.

The worktree also contains an in-progress independent-cache/telemetry change. This change consumes the resulting independent icon/thumbnail ownership and settings contracts but does not absorb extension data-column or MFT cache work.

## Goals / Non-Goals

**Goals:**

- Persist validated BC7 blocks in a private versioned container for icons and thumbnails.
- Upload a valid cache hit directly to a D3D11 BC7 UNORM resource without WebP decode, RGBA materialization, or recompression.
- Bound icon and thumbnail memory, disk, conversion, staging, texture, and GPU resource use independently.
- Preserve correct rendering through provider-backed RGBA fallback, stale-work suppression, cancellation, and device-loss recovery.
- Provide measurable telemetry and evidence for memory, disk, GPU bytes, compression cost, hit latency, upload bytes, frame time, and visual quality.

**Non-Goals:**

- Replacing D3D11, changing extension ABI values, or exposing the cache container as a user format.
- Compressing GPUI glyph/UI atlases, arbitrary SVG output, or animated frames.
- Bulk-converting legacy WebP entries or guaranteeing cache survival across schema changes.
- Allowing the optimization to bypass independently approved quality or performance gates.

## Decisions

### 1. Use a private fixed-endian BC7 container

The container begins with fixed magic, schema version, header length, content kind, format identifier, logical dimensions, padded block dimensions, row pitch, payload length, invalidation identity, and checksum. Integer fields use explicitly documented little-endian encoding. The reader validates the entire header with checked arithmetic before allocating or slicing the payload. The extension is `.bc7cache` and icon/thumbnail namespaces remain separate.

DDS was rejected because its generalized surface metadata adds parser surface without benefiting this internal single-mip 2D contract. KTX2 was rejected because cross-API packaging is unnecessary for the D3D11-only path. WebP was rejected for the new schema because it cannot be uploaded as BC7 without decode and recompression.

### 2. Encode on bounded background workers

Provider RGBA output remains the miss-path source of truth. A keyed in-flight registry deduplicates conversion by source identity, requested presentation size, content kind, and source generation. Jobs carry cancellation and generation tokens. Queue length, concurrency, per-entry dimensions, aggregate staging bytes, and output bytes are hard-bounded. Atomic write-then-rename publishes only fully validated entries.

The dependency choice is blocked until a spike proves deterministic Windows Release builds, BC7 alpha/display-color output, bounded APIs, acceptable licenses and redistribution, and required CPU support behavior. Dependency selection is a B-level correction if the chosen library changes without changing the contract; changing format, platform, or gates is C-level.

### 3. Add a dedicated immutable D3D11 compressed-image path

GPUI gains an internal compressed-raster handle that carries logical size and an opaque renderer-owned resource identity. The Windows renderer capability-checks `DXGI_FORMAT_BC7_UNORM`, creates immutable/default 2D textures with shader-resource binding, validates complete block-row pitch, and samples through an UNORM shader-resource view. This deliberately matches GPUI's existing polychrome atlas; an sRGB SRV would double-linearize and darken assets. Logical dimensions and UV bounds exclude edge padding.

BC7 assets do not enter the arbitrary-rectangle RGBA atlas. Glyphs, UI chrome, unsupported adapters, and failed requests retain the existing atlas. Renderer/public API changes must remain source-compatible for non-Windows backends through default/fallback behavior.

### 4. Keep icon and thumbnail caches independent

The shared container/codec implementation accepts an explicit content kind, but each kind has separate roots, memory LRU, disk LRU/quota, GPU LRU, telemetry counters, and settings. Defaults remain 32 MB icon memory and 128 MB thumbnail memory. Disk and GPU limits are separately represented and never inferred by splitting a combined budget. Actual BC7 payload plus bounded per-entry metadata is the accounting basis.

### 5. Never decode BC7 for fallback

If the adapter lacks BC7 support, a read/validation/upload fails, or work becomes stale, the request obtains RGBA from the Shell/thumbnail provider and uses the existing path. This avoids adding a BC7 decoder solely for rollback. A failed compressed path cannot fail navigation or suppress an otherwise valid provider image.

### 6. Migrate lazily and roll back by feature gate

Legacy `.webp` entries are misses. Bounded quota cleanup may remove them; startup performs no tree-wide migration or deletion. Icon and thumbnail BC7 enablement have independent runtime gates. Rollback disables the affected gate, keeps provider-backed RGBA rendering, and leaves derived `.bc7cache` files eligible for later cleanup.

### 7. Treat quality and performance as blocking release gates

BC7 storage for a valid surface MUST be at most 25% of equivalent RGBA bytes, excluding bounded metadata. Release A/B runs record CPU working set, GPU allocation, upload payload, cache-hit latency, compression latency, first display, disk I/O, and scrolling frame time. Repeated navigation must show no sustained resource growth beyond configured limits and no material interaction-frame-time regression.

Icon and thumbnail visual gates are independent. Icon fixtures include 16x16, 20x20, 24x24, and 32x32 transparent/high-contrast/overlay cases. A failed gate leaves that content kind disabled by default. Thresholds and required evidence cannot be reduced without user approval.

## Component and Data Boundaries

- **BC7 codec/container module:** block geometry, edge padding, encoding adapter, serialization, validation, checksums, atomic persistence; no UI or D3D handles.
- **Shell image caches:** source keys/invalidation, independent LRU/quota, legacy lazy misses, in-flight conversion, telemetry; depend on the codec/container.
- **GPUI compressed-raster contract:** logical dimensions and renderer-neutral lifetime; no filesystem knowledge.
- **D3D11 compressed resource pool:** capability, texture/SRV creation, upload, GPU byte LRU, device-loss invalidation; no Shell keys.
- **Host/UI settings and telemetry:** applies independent budgets and displays usage/capability/fallback state; no codec implementation.
- **UITest/profiling harness:** deterministic fixtures, feature forcing, metrics collection, screenshot/evidence indexing.

## Failure Handling and Observability

All rejected container reads produce bounded reason counters (schema, checksum, dimensions, pitch, length, stale identity, I/O) and behave as misses. Compression and upload failures have bounded counters and do not log unbounded paths or payload data. Telemetry exposes icon and thumbnail memory/disk/GPU used and limits, queue/in-flight/staging bytes, hit/miss/direct-upload/fallback counts, adapter capability, and last bounded error category.

Disk I/O, provider extraction, compression, cleanup, and usage scans stay off the UI thread. Refresh loops must stop with their owning window/entity, permit at most one sample in flight, and discard stale samples.

## Security and Robustness

Cache files are untrusted derived input. Readers reject unknown magic/schema/format/kind, zero or excessive dimensions, inconsistent padded sizes, invalid pitch, arithmetic overflow, payload length mismatch, trailing-data violations, and checksum failure before GPU upload. Paths are derived from hashed keys under registered cache roots; symlinks and traversal are not followed by cleanup. Writes use private application cache roots and atomic replacement.

## Testing Strategy

Unit tests cover block math, odd dimensions, edge replication, deterministic headers, checksum/corruption, bounds, independent LRU, cancellation, stale generations, and legacy misses. Renderer contract tests cover capability gating, texture/SRV descriptors, pitch, logical UVs, resource eviction, unsupported adapters, upload failure, and device loss. Integration tests prove cold miss publication and direct-upload warm hits. UITest captures icon and thumbnail BC7/fallback behavior and Folder Options telemetry. Release profiling produces machine-readable A/B reports plus indexed screenshots.

## Migration and Rollback

1. Land codec/container and dependency evidence behind disabled content-kind gates.
2. Land renderer support and RGBA fallback behind the same gates.
3. Integrate caches, independent budgets, telemetry, and lazy legacy cleanup.
4. Pass unit/integration/UITest and unsupported-device recovery tests.
5. Run Release A/B performance and independent visual gates.
6. Enable only the content kinds whose blocking gates pass; retain immediate configuration rollback.

No installer migration is needed because cache files are derived. Removing the feature disables reads/writes of the new schema and provider-backed RGBA continues to work.

## Evidence and Change Control

Evidence records live under `openspec/changes/bc7-icon-thumbnail-caches/evidence/`. Each completed atomic task maps to a unique `task_id` record containing command or procedure, expected and actual result, exit status/reviewer, relevant hashes, gate identifiers, adjustment identifier when applicable, and timestamp.

- **A - task refinement:** leaf split/order/command/evidence-path corrections that do not change scope, requirements, gates, or public contracts.
- **B - design/spec correction:** corrections inside the approved scope; pause affected work, update design/spec/tasks, revalidate, mark dependent evidence stale, and preserve lineage.
- **C - material change:** scope, public commitment, platform/format, permissions, external/destructive action, blocking status, threshold, or required-evidence change; requires user approval.

Failed, blocked, stale, or unexecuted evidence never closes a task. Conditional work closes only with passed evidence or an evidence-backed `not-applicable` disposition.

## Open Questions

There are no scope-level open questions. The BC7 encoder implementation and exact GPU allocation API remain gated implementation decisions whose acceptable outcomes are fixed above.
