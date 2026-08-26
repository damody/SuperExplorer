## MODIFIED Requirements

### Requirement: Dynamic column registry
The system SHALL represent built-in and extension columns with stable IDs and descriptors containing value type, width, alignment, row applicability, immutable filesystem applicability, sort semantics and cost. Single, batch and aggregate providers and feature-gated GPUI renderers SHALL be registrable. The registry SHALL expose an extension column only when the active filesystem identity is present in its validated manifest scope, and the scheduler SHALL enforce the same scope before preparing input or dispatching work.

#### Scenario: Plugin column is enabled on a declared filesystem
- **WHEN** a valid extension registers a column/provider whose manifest scope contains the active filesystem
- **THEN** the column appears in the column chooser and can be displayed, resized, reordered, sorted, and dispatched like an applicable built-in column

#### Scenario: Plugin column is enabled outside its declared scope
- **WHEN** saved visibility enables a valid extension column but the active filesystem is absent from its manifest scope
- **THEN** the registry projection omits it and the scheduler starts no work for it

#### Scenario: Plugin column has an empty scope
- **WHEN** a registered extension column has a missing or empty normalized filesystem scope
- **THEN** it appears on no filesystem and receives no dispatch
