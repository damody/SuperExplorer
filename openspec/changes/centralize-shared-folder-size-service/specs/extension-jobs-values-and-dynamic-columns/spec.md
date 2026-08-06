## MODIFIED Requirements

### Requirement: Dynamic column registry
The system SHALL represent built-in and extension columns with stable IDs and descriptors containing value type, width, alignment, applicability, sort semantics, cost, and declarative host-data requirements. Single, batch and aggregate providers and feature-gated GPUI renderers SHALL be registrable; a visual renderer that requests `folder.aggregate` SHALL consume host-owned values and SHALL NOT own recursive filesystem measurement.

#### Scenario: Plugin column is enabled
- **WHEN** a valid extension registers a render-only folder aggregate column
- **THEN** the column appears in the chooser, can be displayed, resized, reordered and sorted, and receives values from the shared host snapshot

### Requirement: Worker-safe column render-plan context
Column render-plan callbacks SHALL receive only public value/aggregate/loading/error state, selection/hover state, DPI, theme facade, settings, host-attested item identity and full-snapshot revision. Every synchronous ABI invocation SHALL run on a bounded host worker under its own durable call marker and SHALL NOT enumerate files, measure folders, perform parsing, receive an invalidation handle, or access GPUI types. GPUI SHALL only paint a returned data-only plan while its full-snapshot revision remains current.

#### Scenario: Folder size visual renders
- **WHEN** the shared host snapshot provides a folder byte value and largest-sibling value
- **THEN** the renderer returns a proportional-bar data plan and the host draws the custom GPUI cell without invoking plugin filesystem measurement

### Requirement: Host-owned persistent data-column cache
The Host SHALL exclusively own persistent cache identity, storage, bounds, schema, lookup, admission, and invalidation for extension data columns. Plugins SHALL receive cache misses and SHALL NOT read or write their own persistent data-column cache. Host cache identity SHALL include canonical filesystem identity and modification time so unchanged data is reused and changed data is recomputed.

#### Scenario: Application restarts with unchanged input
- **WHEN** a data-column value was cached by the Host and the canonical path and modification time remain unchanged after restart
- **THEN** the Host reuses the bounded persistent value without dispatching the plugin

#### Scenario: Input modification time changes
- **WHEN** the input modification time differs from the persisted Host record
- **THEN** the Host rejects the record and dispatches the plugin to recompute the value
