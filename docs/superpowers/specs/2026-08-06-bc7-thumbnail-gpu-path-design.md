# BC7 Icon and Thumbnail Cache Design

## Context

SuperExplorer uses GPUI's native Windows Direct3D 11 renderer. Raster images currently become CPU RGBA frames, enter an `DXGI_FORMAT_R8G8B8A8_UNORM` atlas, and are uploaded with `UpdateSubresource`. The existing disk cache uses WebP, which saves disk space but requires decode to RGBA and another conversion before a GPU-compressed representation can be used.

Vulkan `VkFormat` values cannot be passed to the D3D11 backend. The equivalent broadly supported Windows desktop format is `DXGI_FORMAT_BC7_UNORM_SRGB`. BC7 operates on 4x4 blocks and retains alpha.

## Goal

Store both Shell icons and thumbnails as BC7 in the memory cache, disk cache, and GPU cache. A valid disk-cache hit should be uploadable to D3D11 without WebP decoding or BC7 recompression, reducing CPU RGBA retention, upload bytes, VRAM, memory bandwidth, and cache pressure.

## Non-Goals

- Replacing GPUI's D3D11 renderer with Vulkan or wgpu.
- Compressing glyphs, UI chrome, arbitrary SVG output, or animated image frames.
- Defining a user-facing image format or preserving cache files as user data.
- Requiring BC7 on adapters that do not report the necessary D3D11 format support.

## Chosen Architecture

SuperExplorer uses a private, versioned BC7 cache container rather than DDS, KTX2, or WebP. Icon and thumbnail caches share the container implementation but retain independent namespaces, memory limits, disk limits, LRU state, and telemetry.

Each container records:

- magic and schema version;
- content-kind discriminator for icon or thumbnail;
- original logical width and height;
- padded block width and height;
- `DXGI_FORMAT_BC7_UNORM_SRGB` color-space identifier;
- row pitch and payload length;
- source identity and invalidation metadata already required by the Host cache;
- payload checksum.

The payload contains complete BC7 block rows. Dimensions not divisible by four are edge-padded during compression, while layout and UV sampling retain the original logical dimensions. All sizes and offsets use checked arithmetic and are validated against per-entry limits before allocation.

GPUI Windows gains an immutable compressed-raster representation backed by dedicated D3D11 shader-resource textures. The existing RGBA atlas remains unchanged for glyphs, UI rendering, unsupported hardware, and error fallback. BC7 textures are not packed into the arbitrary-rectangle RGBA atlas because block alignment and partial updates have incompatible constraints.

## Data Flow

### Cache miss

1. The Shell provider returns bounded RGBA pixels for an icon or thumbnail.
2. A deduplicated background job compresses the pixels to BC7.
3. The Host validates the result and atomically persists the private BC7 container.
4. The renderer uploads complete BC7 block rows to an immutable D3D11 texture.
5. After upload acknowledgement, temporary RGBA and BC7 staging buffers are released unless another bounded cache owner explicitly retains them.

### Cache hit

1. The Host reads and validates the BC7 container off the UI thread.
2. Valid BC7 blocks are placed in the appropriate byte-bounded memory cache.
3. The renderer uploads the blocks directly without decoding to RGBA or recompressing.
4. Corrupt, stale, oversized, or incompatible entries are treated as misses and removed lazily.

### Unsupported adapter or failure

When BC7 sampling is unsupported, SuperExplorer does not attempt to decode the private BC7 payload back to RGBA. It obtains RGBA from the normal Shell or thumbnail provider and uses the existing atlas for that request. BC7 compression failure, upload failure, cancellation, and stale generations follow the same RGBA fallback. Navigation and item display must remain functional.

## Icon Quality

Icons of every requested size, including 16x16, 20x20, 24x24, and 32x32, are eligible for BC7. Because small high-contrast icons are more sensitive to block artifacts than photographs, representative transparent-edge, overlay, text-like, and high-contrast fixtures must pass visual comparison before the BC7 icon path is enabled by default. A failed quality gate keeps icon BC7 disabled by default without weakening the thumbnail path or removing RGBA fallback.

## Resource Ownership and Limits

Icon and thumbnail BC7 caches are separate:

- icon memory default: 32 MB;
- thumbnail memory default: 128 MB;
- independently configurable memory and disk limits;
- byte-based LRU accounting from actual BC7 payload and bounded metadata;
- separate current-usage telemetry for memory, disk, and GPU resources.

GPU handles never enter persistent cache, extension ABI values, or Host data-column cache. Conversion concurrency, queued jobs, staging bytes, per-entry size, texture count, and aggregate GPU bytes are bounded. Limit reductions evict least-recently-used entries from the affected cache only.

The cache is derived data. Schema changes invalidate old entries lazily; no bulk migration from WebP is performed. Existing WebP cache files are ignored by the new schema and removed by bounded cleanup.

## Lifecycle and Correctness

Compression, disk I/O, and provider access stay off the UI thread. Jobs carry request generation and cancellation identity through provider read, compression, persistence, upload, and publication. Late work cannot replace a newer icon or thumbnail.

Device loss invalidates GPU handles. Visible assets are reconstructed from validated BC7 disk entries when supported, otherwise from the provider through RGBA fallback. Source invalidation continues to be owned by the Host cache contract.

## Performance Gates

The optimized path is enabled by default only when Release profiling demonstrates:

- BC7 payload and GPU storage are no more than 25% of equivalent RGBA bytes, excluding bounded metadata;
- cache hits perform no WebP decode and no BC7 recompression;
- repeated navigation shows no sustained CPU or GPU resource growth beyond configured limits;
- no material interaction-frame-time regression;
- compression latency, first-display latency, disk I/O, and background CPU cost are measured and accepted;
- icon and thumbnail visual-quality gates pass independently.

If a quality or performance gate fails, the affected icon or thumbnail BC7 path remains disabled by default while the RGBA fallback ships. Gate thresholds cannot be weakened without user approval.

## Error Handling

- Capability-query failure selects provider-backed RGBA.
- Invalid dimensions, arithmetic overflow, unsupported layout, compression failure, disk failure, or upload failure selects RGBA and records bounded diagnostics.
- Container checksum, format, pitch, length, and logical/padded dimensions are validated before allocation or upload.
- Device loss drops GPU resources and schedules visible-item reconstruction.
- Errors do not fail navigation or prevent the provider-backed image from being displayed.

## Verification

- Unit tests for block sizing, padding, edge replication, pitch, checksums, schema rejection, byte accounting, cancellation, stale generations, and independent LRU eviction.
- Disk-cache tests for atomic writes, corrupt and truncated entries, oversized payload rejection, lazy WebP cleanup, and direct-upload cache hits.
- Renderer contract tests for capability gating, BC7 descriptors, sRGB shader-resource views, upload pitch, UV bounds, and RGBA fallback.
- Device-loss reconstruction and unsupported-adapter tests.
- Visual fixtures for small Shell icons, alpha edges, overlays, high contrast, gradients, photographs, and odd dimensions.
- Release A/B profiling of CPU working set, VRAM, upload bytes, compression time, cache-hit latency, first display latency, and scrolling frame time.
- UITest evidence that BC7 and fallback preserve logical sizing, selection behavior, and visual identity.

## Acceptance Criteria

- Icon and thumbnail memory, disk, and GPU caches use the private BC7 representation when their independently gated BC7 paths are enabled.
- Valid BC7 disk hits upload without WebP decoding, RGBA materialization, or BC7 recompression.
- Icon and thumbnail limits, LRU state, and telemetry remain independent.
- Unsupported or failed compressed rendering automatically uses provider-backed RGBA.
- Temporary uncompressed buffers are released after successful upload acknowledgement.
- Cache accounting and LRU enforcement use actual BC7 bytes plus bounded metadata.
- Small-icon visual quality is explicitly verified rather than inferred from thumbnail results.
- The relevant quality and performance gates pass before each BC7 path becomes enabled by default.
