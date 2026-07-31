## ADDED Requirements

### Requirement: Active tab forms a continuous content surface
The active tab SHALL use the same semantic surface color as the content below it and MUST NOT show
a divider along its shared bottom edge. Every inactive tab SHALL use the same semantic gray surface
as the surrounding tab strip. Keyboard focus indication SHALL remain distinguishable from active
tab selection.

#### Scenario: Two tabs with one active tab
- **WHEN** two tabs are visible and the second tab is active
- **THEN** the second tab and content surface have matching fill with no bottom divider, while the
  first tab and tab-strip background have matching gray fill

#### Scenario: Keyboard focus enters the tab strip
- **WHEN** keyboard focus moves to the active tab
- **THEN** a visible focus indication is present without changing inactive-tab identity or restoring
  the blue active-selection fill

### Requirement: Breadcrumb menu highlight follows the pointer
An open breadcrumb child menu SHALL maintain one current row. Moving the physical pointer across
actionable rows MUST move the current-row identity and gray hover fill to the row under the pointer,
and the previous row MUST return to the menu fill. Pointer tracking MUST NOT change row activation,
stable location identity, or asynchronous menu generation.

#### Scenario: Pointer moves between two child folders
- **WHEN** the pointer moves from the first actionable child row to a second actionable child row
- **THEN** the gray highlight moves to the second row and the first row returns to the menu fill

#### Scenario: Pointer and keyboard share current-row identity
- **WHEN** keyboard navigation selects a row and the pointer subsequently moves over another row
- **THEN** Enter or click activation targets the most recently focused row through the existing
  stable breadcrumb location identity

#### Scenario: Invalid pointer focus update
- **WHEN** a stale or out-of-range pointer focus action arrives after the menu closes or changes
- **THEN** the action is ignored without changing navigation, focus restoration, or menu generation
