# BC7 Thumbnail GPU Path Design

## Context

SuperExplorer uses GPUI's native Windows Direct3D 11 renderer. Raster images are decoded into CPU RGBA frames, inserted into an `DXGI_FORMAT_R8G8B8A8_UNORM` atlas, and uploaded with `UpdateSubresource`. Vulkan `VkFormat` values cannot be used by this backend. ASTC and ETC2 are not the appropriate broadly supported D3D11 desktop formats; BC7 is the suitable high-quality sRGB block-compression format.

The existing GPUI atlas supports arbitrary rectangles and dynamic updates. BC7 operates on 4x4 blocks, so converting the shared atlas would complicate glyphs, SVG output, icons, allocation, and partial uploads. The optimized path therefore applies only to qualifying static thumbnails through a separate texture pool.

## Goal

Reduce thumbnail VRAM, GPU upload bytes, memory bandwidth, and texture-cache pressure by storing qualifying GPU-resident thumbnails as BC7 sRGB textures while preserving a correct bounded RGBA fallback.

## Non-Goals

- Replacing GPUI's D3D11 renderer with Vulkan or wgpu.
- Compressing text glyphs, SVG rasterizations, UI chrome, or small Shell icons.
- Replacing WebP disk persistence; WebP remains the portable disk-cache representation.
- Promising BC7 on adapters that do not report the required format support.

## Architecture

GPUI Windows gains a second raster-image representation for immutable compressed textures. Qualifying SuperExplorer thumbnails are decoded from the provider or WebP cache, converted to BC7 off the UI thread, and uploaded to a dedicated D3D11 shader-resource texture pool. The ordinary RGBA atlas remains unchanged.

Eligibility requires:

- a static single-frame thumbnail;
- decoded dimensions of at least 128x128;
- dimensions and block storage validated with checked arithmetic;
- adapter support for `DXGI_FORMAT_BC7_UNORM` or `DXGI_FORMAT_BC7_UNORM_SRGB` as a two-dimensional shader-sampled texture;
- successful bounded background conversion before the request becomes stale or cancelled.

The renderer samples BC7 through an sRGB shader-resource view. Alpha is retained. Unsupported adapters, conversion failures, animated images, small images, and stale/cancelled work use the existing RGBA atlas path.

## Data Flow

1. Shell/provider or WebP cache returns bounded RGBA thumbnail pixels.
2. The Host determines BC7 eligibility and submits one deduplicated background conversion job.
3. The converter pads dimensions to complete 4x4 blocks where necessary while retaining the original logical dimensions for layout and UV bounds.
4. GPUI creates an immutable `D3D11_USAGE_DEFAULT` BC7 texture with `D3D11_BIND_SHADER_RESOURCE` and uploads complete block rows.
5. Rendering references the dedicated texture and original logical bounds.
6. After upload acknowledgement, the BC7 path releases its temporary RGBA and BC7 staging buffers; normal Host cache policy decides whether a separate decoded thumbnail remains cached.
7. Device loss invalidates GPU handles and rebuilds visible thumbnails from WebP or the provider.

## Resource Ownership and Limits

The BC7 texture pool uses byte-based LRU accounting from actual block storage, not logical RGBA size. GPU texture handles never enter persistent cache or extension ABI values. Conversion concurrency, queued jobs, staging bytes, texture count, and total BC7 bytes are bounded. Shrinking the thumbnail GPU-cache limit evicts least-recently-used BC7 textures without changing the CPU thumbnail cache limit.

BC7 jobs retain cancellation and generation identity through decode, conversion, upload, and publication. Late work is dropped before it can replace a newer thumbnail.

## Compatibility and Fallback

The feature is runtime capability-gated and deny-by-default on unknown support. The current RGBA atlas remains a complete fallback and rollback path. No user data migration is required. WebP disk entries remain renderer-independent.

## Performance Gate

The optimized path is enabled by default only if Release profiling demonstrates:

- BC7 GPU storage and upload payload are no more than 25% of equivalent RGBA bytes, excluding bounded metadata;
- repeated thumbnail navigation shows no sustained GPU-resource growth beyond the configured LRU limit;
- no material regression in interaction frame time;
- first-thumbnail latency and background CPU cost are recorded and accepted rather than hidden;
- visual comparison confirms acceptable thumbnail quality and alpha behavior.

If the latency or frame-time gate fails, the BC7 path remains disabled by default while the verified RGBA fallback ships. Gate thresholds cannot be weakened without user approval.

## Error Handling

- Capability-query failure selects RGBA.
- Overflow, invalid dimensions, unsupported block layout, conversion failure, or upload failure selects RGBA and records bounded diagnostics.
- Device loss drops GPU resources and schedules normal visible-item reconstruction.
- Errors do not fail navigation, WebP cache reads, or the owning thumbnail request when RGBA remains valid.

## Verification

- Unit tests for block sizing, padding, UV bounds, byte accounting, cancellation, stale generation, and LRU eviction.
- Renderer contract tests for capability gating, BC7 texture descriptors, shader-resource views, upload pitch, and RGBA fallback.
- Device-loss reconstruction and unsupported-adapter tests.
- Visual fixtures covering alpha, gradients, photographs, odd dimensions, and small-image exclusion.
- Release A/B profiling of VRAM, upload bytes, CPU working set, conversion time, first display latency, cache hits, and scrolling frame time.
- UITest evidence that fallback and BC7 produce the same logical sizing and selection behavior.

## Acceptance Criteria

- The native Windows backend uses D3D11 BC7, not Vulkan `VkFormat` constants.
- Only eligible thumbnails use the compressed path; the shared RGBA atlas remains unchanged.
- Unsupported or failed compressed rendering falls back automatically.
- Temporary uncompressed buffers are released after successful upload acknowledgement.
- GPU cache accounting and LRU enforcement use actual BC7 storage bytes.
- The performance gate passes before BC7 becomes enabled by default.
