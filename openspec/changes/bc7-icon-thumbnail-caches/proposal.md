## Why

SuperExplorer currently retains or reconstructs RGBA image data for Shell icons and thumbnails, and its WebP disk cache requires decoding plus another conversion before GPU-compressed rendering. This increases CPU working set, VRAM, upload traffic, and cache-hit latency precisely where repeated folder navigation should reuse compact derived data.

## What Changes

- Add a private, versioned BC7 cache container used by both icon and thumbnail disk caches.
- Add independently bounded BC7 memory, disk, and GPU caches for icons and thumbnails, preserving their separate settings, LRU state, and telemetry.
- Extend the native GPUI D3D11 renderer with immutable `DXGI_FORMAT_BC7_UNORM` image resources and direct BC7 block upload, matching GPUI's existing UNORM polychrome sampling contract.
- Convert provider RGBA results to BC7 on bounded background workers and release temporary uncompressed buffers after publication.
- Make validated disk-cache hits directly uploadable without WebP decoding, RGBA materialization, or BC7 recompression.
- Retain provider-backed RGBA rendering when the adapter lacks BC7 support or compression, validation, I/O, upload, cancellation, generation, or device-lifecycle checks reject the compressed path.
- Lazily ignore and remove legacy WebP cache files; no bulk migration or user-data conversion is performed.
- Gate icon and thumbnail enablement independently on Release performance and visual-quality evidence, including small Shell icons.

## Capabilities

### New Capabilities

- `bc7-raster-cache`: Defines the private BC7 icon/thumbnail cache contract, D3D11 direct-upload behavior, independent resource limits and telemetry, fallback and recovery behavior, legacy-cache handling, and blocking quality/performance gates.

### Modified Capabilities

None. Existing product specs do not define the raster-cache representation or D3D11 compressed-image contract.

## Impact

- Affected components include `explorer-shell-win` provider and disk-cache code, `explorer-ui` cache ownership/settings/telemetry, the vendored GPUI Windows D3D11 renderer, Folder Options cache reporting, UITest fixtures, and Release profiling evidence.
- The WebP dependency can be removed only after no remaining production or test consumer requires it; the BC7 encoder dependency must pass Windows build, license, and redistribution review.
- Persistent cache schema changes are intentionally incompatible but affect derived cache data only. Existing `.webp` entries become lazy misses and bounded cleanup targets.
- No extension ABI, user file format, navigation contract, or non-Windows renderer behavior changes.
- This change does not replace D3D11 with Vulkan/wgpu, compress glyph/UI atlases, or add animated-frame compression.
