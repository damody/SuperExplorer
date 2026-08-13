## ADDED Requirements

### Requirement: Private BC7 cache container
The system SHALL persist icon and thumbnail compressed cache entries in a private, versioned, little-endian BC7 container whose validated payload is complete `DXGI_FORMAT_BC7_UNORM` block rows.

#### Scenario: Odd dimensions are preserved logically
- **WHEN** an eligible image has dimensions not divisible by four
- **THEN** the system edge-pads storage to complete 4x4 blocks and records the original logical dimensions for layout and UV bounds

#### Scenario: Corrupt entry is rejected before allocation or upload
- **WHEN** magic, schema, kind, format, dimensions, padded dimensions, pitch, payload length, invalidation identity, or checksum is inconsistent
- **THEN** the system treats the entry as a cache miss without uploading its payload and schedules only bounded lazy cleanup

#### Scenario: Oversized arithmetic is rejected
- **WHEN** header fields overflow checked block, pitch, payload, or allocation calculations or exceed configured entry bounds
- **THEN** the reader rejects the entry before allocating payload-sized memory

### Requirement: Direct BC7 cache-hit upload
On a valid supported cache hit, the system SHALL upload the persisted BC7 blocks to D3D11 without WebP decoding, RGBA materialization, or BC7 recompression.

#### Scenario: Warm supported hit
- **WHEN** a valid icon or thumbnail BC7 entry is requested on an adapter supporting BC7 sampling
- **THEN** the renderer creates or reuses the compressed resource from the stored block rows and increments direct-upload telemetry without invoking the provider or encoder

#### Scenario: Stale source identity
- **WHEN** the persisted entry no longer matches the Host source invalidation identity
- **THEN** the entry is a miss and cannot be published or uploaded as the current image

### Requirement: Bounded background creation
The system SHALL perform provider extraction, BC7 compression, disk I/O, and persistence away from the UI thread with bounded concurrency, queue length, staging bytes, entry size, and total output bytes.

#### Scenario: Duplicate cold requests
- **WHEN** concurrent requests have the same content kind, source identity, presentation size, and generation
- **THEN** exactly one conversion job owns compression and persistence while valid waiters share its result

#### Scenario: Queue or staging bound reached
- **WHEN** accepting another conversion would exceed a configured hard bound
- **THEN** the compressed job is deferred or rejected and the request remains displayable through provider-backed RGBA

#### Scenario: Cancelled or superseded work completes late
- **WHEN** compression or I/O completes after cancellation or a newer generation exists
- **THEN** the late result cannot replace or publish over the newer image

### Requirement: Native D3D11 BC7 rendering
The Windows GPUI renderer SHALL capability-gate immutable BC7 UNORM shader resources and SHALL preserve the existing RGBA atlas for all non-BC7 content and fallback.

#### Scenario: Supported upload
- **WHEN** validated block rows and logical dimensions reach a BC7-capable D3D11 renderer
- **THEN** the renderer uses a BC7 UNORM texture/SRV with validated block-row pitch and samples only the logical image bounds without the double-linearization that darkens GPUI polychrome assets

#### Scenario: Unsupported adapter
- **WHEN** required D3D11 BC7 two-dimensional shader-sampling support is absent or cannot be determined
- **THEN** the system does not upload the BC7 payload and obtains provider RGBA for the existing atlas path

#### Scenario: Upload failure
- **WHEN** texture creation, shader-resource creation, or upload fails
- **THEN** the request falls back to provider RGBA and navigation remains usable

#### Scenario: Device loss
- **WHEN** the D3D11 device is lost
- **THEN** compressed GPU handles are invalidated and visible images reconstruct from validated BC7 cache entries on supported recovery or from provider RGBA otherwise

### Requirement: Independent icon and thumbnail resource policy
The system SHALL maintain separate icon and thumbnail namespaces, memory LRU, disk LRU/quota, GPU LRU, limits, and telemetry, using actual BC7 bytes plus bounded metadata for accounting.

#### Scenario: Default memory budgets
- **WHEN** no user override exists
- **THEN** the icon memory limit is 32 MB and the thumbnail memory limit is 128 MB without splitting a shared budget

#### Scenario: One limit shrinks
- **WHEN** an icon or thumbnail limit is reduced below current use
- **THEN** only that content kind evicts least-recently-used entries until it is within its own limit

#### Scenario: Usage is displayed
- **WHEN** Folder Options cache telemetry is visible
- **THEN** icon and thumbnail memory, disk, and GPU used/limit values and compressed-path state are reported independently from bounded Host-owned snapshots

### Requirement: Provider-backed fallback
The system SHALL keep icons and thumbnails displayable through the existing provider-backed RGBA path when the compressed path is unavailable, invalid, stale, cancelled, or failed.

#### Scenario: Compression failure
- **WHEN** the BC7 encoder rejects or fails an otherwise valid provider image
- **THEN** the provider RGBA image is displayed and a bounded failure category is recorded

#### Scenario: BC7 disk data cannot be used
- **WHEN** the adapter is unsupported or a disk entry fails validation
- **THEN** the system reacquires provider RGBA rather than requiring a BC7 decoder for fallback

### Requirement: Lazy legacy cache transition
The system SHALL treat legacy WebP cache files as derived-data misses and SHALL NOT perform an unbounded startup migration or deletion scan.

#### Scenario: Legacy file encountered
- **WHEN** bounded lookup or quota cleanup encounters a legacy `.webp` entry
- **THEN** the entry is not decoded for the BC7 path and may be removed within the normal bounded cleanup budget

#### Scenario: Rollback disables BC7
- **WHEN** either content-kind feature gate is disabled
- **THEN** that content kind uses provider-backed RGBA while existing BC7 files remain harmless derived data eligible for later cleanup

### Requirement: Independent blocking quality gates
The system SHALL enable icon BC7 and thumbnail BC7 by default only after their applicable visual-quality evidence passes.

#### Scenario: Small-icon gate
- **WHEN** 16x16, 20x20, 24x24, and 32x32 transparent-edge, overlay, text-like, and high-contrast fixtures are compared
- **THEN** icon BC7 is enabled by default only if the recorded review accepts visual identity and alpha behavior for every required fixture class

#### Scenario: One content kind fails quality
- **WHEN** icon or thumbnail visual evidence fails independently
- **THEN** only the failing content kind remains disabled by default and its RGBA fallback remains enabled

### Requirement: Blocking performance and memory gates
The system SHALL require Release evidence before default enablement and SHALL NOT weaken the approved thresholds or evidence set without user approval.

#### Scenario: Storage ratio
- **WHEN** BC7 payload and GPU allocation are compared with the equivalent RGBA surfaces
- **THEN** each valid surface uses no more than 25% of RGBA bytes excluding bounded metadata

#### Scenario: Repeated navigation
- **WHEN** the Release A/B navigation workload repeatedly visits representative icon and thumbnail folders
- **THEN** CPU/GPU resources remain within configured limits without sustained growth and interaction frame time has no material regression

#### Scenario: Required metrics are recorded
- **WHEN** the Release gate is evaluated
- **THEN** machine-readable evidence includes CPU working set, GPU allocation, upload bytes, compression time, cache-hit latency, first-display latency, disk I/O, scrolling frame time, cache state, build identity, and adapter identity

#### Scenario: Gate fails
- **WHEN** required evidence is missing or a blocking performance threshold fails
- **THEN** the affected BC7 path remains disabled by default and the failure cannot be marked complete

### Requirement: Cache input robustness
The system SHALL treat cache files as untrusted input and constrain their paths, parsing, diagnostics, and cleanup.

#### Scenario: Path escape or symlink is encountered
- **WHEN** a cache candidate escapes a registered root or requires following a symlink during cleanup
- **THEN** the system skips the candidate without deleting or reading outside the registered root

#### Scenario: Repeated malformed inputs
- **WHEN** many corrupt entries are encountered
- **THEN** diagnostics remain bounded and do not retain unbounded paths or payload contents
