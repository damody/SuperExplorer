## ADDED Requirements

### Requirement: Frame-coalesced directory presentation
The system SHALL preserve existing Shell item- and byte-bounded batch safety limits while merging accepted UI batches and rebuilding directory presentation at most once per frame.

#### Scenario: Multiple batches arrive in one frame
- **WHEN** several correlated directory batches are available during one service-pump frame
- **THEN** the model accepts all valid entries and the UI publishes at most one presentation rebuild and one file-view notification

#### Scenario: First batch arrives from a slow provider
- **WHEN** the first valid batch arrives before directory enumeration completes
- **THEN** the system displays its virtual visible entries and keeps cancellation, scrolling, and tab switching available

### Requirement: Generation-safe progressive enumeration
The system MUST reject stale or cancelled directory events and MUST NOT let stale icon or thumbnail completion restore obsolete UI state or trigger unbounded repaint work.

#### Scenario: Navigation cancels an active load
- **WHEN** the user navigates away while directory, icon, or thumbnail work is active
- **THEN** old-generation results cannot alter the new directory presentation, selection, or focused item

### Requirement: Bounded visible work queues
Icon, visible-item override, and thumbnail schedulers SHALL enforce configured queue, concurrency, and decoded-byte limits and SHALL remove consumers that leave the virtual range.

#### Scenario: Fast scrolling replaces consumers
- **WHEN** rapid scrolling traverses more items than the queue capacity
- **THEN** the queue remains within its configured bounds and prioritizes the current visible range

### Requirement: Privacy-safe performance diagnostics
Release diagnostics SHALL record directory and presentation revisions, realized count, projection rebuilds, render and scroll percentiles, snapshot clone count, cache accounting, queue depth, cancellations, and evictions without recording full private paths or filenames.

#### Scenario: Performance snapshot is emitted
- **WHEN** the diagnostic interval records a large-directory interaction
- **THEN** it emits aggregate counters and timings without including the directory path or entry names

### Requirement: Large-directory realization gate
The release performance suite SHALL render and scroll a generated 100,000-entry directory and SHALL fail if a standard viewport realizes more than 250 rows or cells or if steady-state scrolling clones the complete snapshot or rebuilds sorting.

#### Scenario: Actual 100,000-entry scroll test
- **WHEN** the performance suite scrolls each supported file-view family through a 100,000-entry fixture
- **THEN** it verifies bounded realization, zero complete-snapshot clones on the scroll path, and zero sort rebuilds without relevant mutations

### Requirement: Scroll responsiveness gate
On the reference local fixture, release-build scroll-frame time SHALL be at most 16.7 ms at p95. In the network/provider matrix it SHALL be at most 33 ms at p95, with no UI-thread stall above 100 ms attributable to file-view rendering.

#### Scenario: Local scroll benchmark
- **WHEN** the release benchmark scrolls the reference local large-directory fixture
- **THEN** measured p95 frame time is at most 16.7 ms

#### Scenario: Network provider benchmark
- **WHEN** the release benchmark scrolls while the network/provider fixture enumerates
- **THEN** measured p95 frame time is at most 33 ms and no file-view render stall exceeds 100 ms

### Requirement: Progressive input responsiveness gate
After the first directory batch is visible, pointer and keyboard input latency SHALL be at most 50 ms at p95 while enumeration continues.

#### Scenario: Input during enumeration
- **WHEN** the benchmark sends pointer and keyboard input after first-batch realization but before terminal enumeration
- **THEN** p95 input-to-handling latency is at most 50 ms and the operation remains cancellable

### Requirement: Functional regression coverage
The change MUST retain sorting, selection, range selection, rename, context menus, drag/drop, keyboard navigation, fixed Details header behavior, UI Automation semantics, overlays, custom icons, and thumbnails across large-directory tests.

#### Scenario: Headful large-directory matrix
- **WHEN** the headful suite exercises all supported view families and interaction paths on large fixtures
- **THEN** every existing functional contract passes while realized work and queues remain bounded
