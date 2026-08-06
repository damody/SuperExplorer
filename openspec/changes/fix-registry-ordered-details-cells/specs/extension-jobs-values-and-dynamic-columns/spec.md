## MODIFIED Requirements

### Requirement: Dynamic column registry
The system SHALL represent built-in and extension columns with stable IDs and descriptors
containing value type, width, alignment, applicability, sort semantics and cost. Single, batch and
aggregate providers and feature-gated GPUI renderers SHALL be registrable. In Details view, the
header and every row SHALL use the same visible `ColumnRegistry` descriptor projection in stable
`ColumnId` order. Column width, visibility, sorting identity, accessibility identity, and rendered
cell data SHALL be resolved from the same exact descriptor ID and SHALL NOT be inferred from a
renderer family's position.

#### Scenario: Plugin column is enabled
- **WHEN** a valid extension registers a column and provider
- **THEN** the column appears in the column chooser, can be displayed, resized, reordered and
  sorted like a built-in column

#### Scenario: Renderer families interleave in registry order
- **WHEN** Folder size, Lua Code lines, Rust Main code lines, and Lock owners descriptors are all
  visible and their stable IDs interleave renderer families
- **THEN** each header and row cell occupies the same descriptor position and each cell displays
  only the value produced for that exact descriptor ID

#### Scenario: Extension is disabled independently
- **WHEN** one visible extension contribution is disabled or removed while other extension columns
  remain enabled
- **THEN** its header and cell are both removed from the next current projection and all remaining
  headers stay aligned with their own cells

#### Scenario: Stale runtime remains after descriptor removal
- **WHEN** a runtime or result remains available for an extension descriptor that is no longer in
  the current registry
- **THEN** the stale runtime or result renders no cell and cannot populate another descriptor's
  column

#### Scenario: Current descriptor has no ready runtime
- **WHEN** a visible registered extension descriptor has no matching ready runtime in the current
  render snapshot
- **THEN** its own column geometry remains associated with that descriptor and the UI shows only
  its loading or unavailable state without borrowing adjacent column data

#### Scenario: Extension is re-enabled
- **WHEN** an extension with the same stable ID is re-enabled after removal
- **THEN** its header and cell return at the registry-defined position with its retained visibility
  and width preferences and without changing neighboring column identity
