## ADDED Requirements

### Requirement: Selected file rows use an outline without a full-row fill
The application SHALL render a selected file or folder with a visible semantic focus outline while retaining the normal surface interior and legible surface text.

#### Scenario: Selected row at rest
- **WHEN** a file row is selected in Details, List, or icon view
- **THEN** its selection SHALL be represented by a bounded outline and SHALL NOT paint an opaque selected band across the item interior

#### Scenario: Pointer hovers a selected row
- **WHEN** the pointer moves over an already selected row
- **THEN** the unselected-row hover fill SHALL NOT replace the selected row's surface interior or outline

#### Scenario: Native popup owns foreground focus
- **WHEN** the selected row's native context menu is open
- **THEN** the row SHALL retain an obvious active outline without changing to a full-row fill

#### Scenario: High contrast selection
- **WHEN** Windows high contrast is active and a row is selected
- **THEN** the interior SHALL use system Window/WindowText roles and the outline SHALL use a visible system Highlight role

### Requirement: Selection visuals remain interaction-neutral
Selection styling SHALL NOT change hit testing, focused item identity, multi-selection membership, drag initiation, inline rename, or context-menu routing.

#### Scenario: Right-click a non-first selected target
- **WHEN** the user physically right-clicks a visible non-first row after the selection visual changes
- **THEN** that row SHALL remain the exact focused target and its complete native menu SHALL open
