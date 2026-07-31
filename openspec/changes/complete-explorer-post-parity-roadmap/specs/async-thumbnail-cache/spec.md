## ADDED Requirements

### Requirement: Explorer-like thumbnail selection
The application SHALL request Windows Shell thumbnails for eligible items and thumbnail-capable views, while respecting icon-only settings and using authentic Shell icons when no usable thumbnail exists.

#### Scenario: Mixed image document folder
- **WHEN** an icon view contains thumbnail-capable images, supported documents, folders, and unsupported files
- **THEN** eligible files SHALL progressively show correctly oriented thumbnails and all other items SHALL retain their authentic Shell icon and overlay

### Requirement: Viewport-prioritized scheduling
Thumbnail requests SHALL be limited to visible and bounded near-visible items, prioritized by active viewport distance, deduplicated across consumers, and cancelled when no consumer remains.

#### Scenario: Fast scroll through a large folder
- **WHEN** the viewport moves rapidly through 100,000 items
- **THEN** off-screen requests SHALL not grow without bound, abandoned work SHALL be cancelled or suppressed, and visible rows SHALL receive priority

### Requirement: Generation and identity isolation
Every thumbnail result SHALL be correlated to item identity, source generation, requested physical size, scale, and display mode before it can update UI state.

#### Scenario: Navigate while extraction is running
- **WHEN** a slow result arrives after its tab navigates or the file identity/version changes
- **THEN** the stale result SHALL be discarded and SHALL NOT replace a thumbnail in the new snapshot

### Requirement: Bounded memory and disk caches
The thumbnail subsystem SHALL provide a decoded-byte-cost memory LRU and an optional versioned, checksummed, bounded disk cache with centralized budgets and recoverable corruption handling.

#### Scenario: Cache budget exceeded
- **WHEN** adding a decoded image would exceed the memory or disk budget
- **THEN** lower-priority least-recently-used entries SHALL be evicted without leaking GDI, COM, file, or texture resources

#### Scenario: Corrupt disk entry
- **WHEN** a disk cache entry fails schema, checksum, dimension, or decode validation
- **THEN** it SHALL be discarded and the item SHALL fall back to Shell retrieval or its icon without failing folder rendering

### Requirement: Source invalidation
The application SHALL invalidate thumbnails when file identity metadata, association, overlay generation, DPI, requested size, relevant theme state, cache schema, or Windows build compatibility changes.

#### Scenario: File is replaced in place
- **WHEN** watcher evidence shows that a visible file has been replaced or modified
- **THEN** the old thumbnail SHALL stop being authoritative and a new generation-scoped request SHALL be scheduled

### Requirement: No unintended cloud hydration
Automatic thumbnail visibility SHALL NOT force offline cloud placeholders to download; unavailable cached thumbnails SHALL fall back to provider/Shell icons until explicit user activity makes content local.

#### Scenario: Offline placeholder enters viewport
- **WHEN** a non-hydrated cloud placeholder becomes visible
- **THEN** the application SHALL avoid content hydration and display a truthful provider icon, overlay, or cached thumbnail

### Requirement: Safe extraction and fallback
Thumbnail extraction SHALL have deadlines, cancellation, size/dimension validation, owned pixel boundaries, and a safe icon fallback; after broker rollout, untrusted codec/provider activation SHALL occur only in disposable broker workers.

#### Scenario: Codec hangs or crashes
- **WHEN** a thumbnail provider exceeds its deadline or crashes its worker
- **THEN** the UI SHALL remain responsive, show an icon fallback, record correlated diagnostics, and keep later thumbnail requests runnable

### Requirement: Thumbnail interaction parity
Changing view size, Ctrl+wheel zoom, DPI, theme, sort, or selection SHALL preserve stable selection and scroll semantics while requesting the correct physical thumbnail size.

#### Scenario: Zoom icon view
- **WHEN** the user holds Ctrl and scrolls through supported icon sizes
- **THEN** layout and request sizes SHALL update incrementally without losing item identity, selection, or the nearest visible anchor

### Requirement: Thumbnail telemetry and evidence
The subsystem SHALL report queue depth, outstanding work, hit/miss/eviction counts, decoded bytes, disk bytes, latency, cancellations, failures, and owned resource counts for deterministic and real-Shell validation.

#### Scenario: Thumbnail soak
- **WHEN** repeated scrolling, resizing, navigation, file replacement, and view switching run against a large mixed fixture
- **THEN** caches and native resources SHALL remain within configured bounds and terminal requests SHALL balance
