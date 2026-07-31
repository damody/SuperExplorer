## ADDED Requirements

### Requirement: Bounded extension scheduler
The Extension Job Scheduler SHALL provide separate CPU/I/O queues, global and per-package concurrency limits, visible-row priority, cancellation, deadlines, progress, backpressure and typed terminal states. Provider callbacks SHALL be synchronous ABI functions invoked by host workers; futures/runtime handles SHALL NOT cross the ABI.

#### Scenario: Large folder opens
- **WHEN** a folder with 1,000 items is opened with several extension columns enabled
- **THEN** the basic file list becomes interactive before extension jobs finish, visible rows are prioritized and work remains within configured limits

### Requirement: Incremental result and UI batching
Jobs SHALL submit bounded incremental batches tagged with job/item generation. The host SHALL coalesce invalidations in a bounded 16–50 ms window and SHALL NOT synchronously redraw once per item.

#### Scenario: One thousand results arrive
- **WHEN** a provider completes 1,000 item values in rapid succession
- **THEN** values become incrementally available while GPUI receives coalesced invalidation batches rather than 1,000 synchronous redraws

### Requirement: Typed extension values and outcomes
The public API SHALL represent booleans, integers, floats, bytes, time, duration, text, localized display and structured/opaque data, with separate stable sort values. Outcomes SHALL distinguish unsupported, unavailable, cancelled, plugin error and incompatible.

#### Scenario: Byte value is sorted
- **WHEN** display strings include `900 MB` and `1.2 GB`
- **THEN** sorting uses exact byte values instead of lexicographic display text

#### Scenario: Unsupported file is analyzed
- **WHEN** a code-line provider receives an unsupported binary file
- **THEN** it returns Unsupported and the UI does not display a valid zero or report a plugin fault

### Requirement: Generation-safe handles and cache
Item/location handles, job results and cache entries SHALL carry generations. Cache keys SHALL include package, interface, plugin data version, file identity/metadata and option hash; recursive scans SHALL additionally account for watcher/TTL/manual invalidation. Stale generations SHALL never update current UI.

#### Scenario: Folder changes before completion
- **WHEN** a user navigates away while a background result is still running
- **THEN** the job is cancelled or allowed to finish, but its old-generation result is discarded

### Requirement: Dynamic column registry
The system SHALL represent built-in and extension columns with stable IDs and descriptors containing value type, width, alignment, applicability, sort semantics and cost. Single, batch and aggregate providers and feature-gated GPUI renderers SHALL be registrable.

#### Scenario: Plugin column is enabled
- **WHEN** a valid extension registers a column and provider
- **THEN** the column appears in the column chooser, can be displayed, resized, reordered and sorted like a built-in column

### Requirement: Dynamic layout persistence and migration
Column visibility, order and widths SHALL use an extensible map/ordered-list model rather than a fixed bitmask. Existing built-in settings SHALL migrate; settings for temporarily unknown plugin IDs SHALL be retained but hidden.

#### Scenario: Plugin is reinstalled
- **WHEN** a user removes a plugin column and later reinstalls the same stable ID
- **THEN** its prior width/order/visibility preferences can be restored

### Requirement: Column GPUI render context
Column renderers SHALL receive only public value/aggregate/loading/error state, selection/hover geometry, DPI, theme facade and invalidation handle. Renderer execution SHALL occur on the GPUI thread and SHALL NOT enumerate files or perform parsing.

#### Scenario: Folder size visual renders
- **WHEN** background aggregation provides a folder byte value and largest-sibling value
- **THEN** the renderer can draw a proportional custom GPUI cell without accessing internal Explorer state

### Requirement: Input stream for decoders
The host SHALL provide `InputStreamV1` only when `filesystem.read` is authorized, with bounded read, optional seek, length, deadline, cancellation and source generation, without exposing arbitrary paths or native handles.

#### Scenario: Source changes during decode
- **WHEN** a file generation changes while a decoder is reading
- **THEN** the result is cancelled or marked stale and cannot populate current metadata
