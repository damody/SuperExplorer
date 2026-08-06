## ADDED Requirements

### Requirement: Symmetric midpoint insertion
The system SHALL resolve a dragged Details column against insertion slots formed by target-header
midpoints after excluding the dragged column from the candidate order.

#### Scenario: Adjacent rightward drag crosses the midpoint
- **WHEN** a movable column is dragged from immediately left of a target to a finite pointer X at or
  beyond that target header's midpoint
- **THEN** the prospective order places the dragged column immediately after that target without
  requiring the pointer to enter another column

#### Scenario: Adjacent leftward drag crosses the midpoint
- **WHEN** a movable column is dragged from immediately right of a target to a finite pointer X before
  that target header's midpoint
- **THEN** the prospective order places the dragged column immediately before that target

#### Scenario: Pointer remains on the original insertion side
- **WHEN** drag movement resolves to the same insertion slot as the current prospective order
- **THEN** the system leaves the preview unchanged and does not emit another order mutation

#### Scenario: Invalid pointer coordinate
- **WHEN** a drag-move event supplies a non-finite pointer coordinate or invalid target bounds
- **THEN** the system preserves the last valid prospective order and does not commit invalid input

### Requirement: Live unified Details projection
While a valid column drag is active, the system SHALL project the prospective order through the
Details headers, visible row cells, filter affordances, hit testing, and accessibility traversal
before pointer release.

#### Scenario: Preview changes before mouse-up
- **WHEN** the pointer crosses into a different insertion slot while the left button remains down
- **THEN** the target header and each visible row's corresponding data cell move into the prospective
  order before mouse-up

#### Scenario: Preview does not reload row data
- **WHEN** the prospective column order changes during a drag
- **THEN** existing row values are reprojected by stable column identity without starting a directory
  reload or extension-provider recomputation

#### Scenario: Dynamic target becomes unavailable
- **WHEN** the dragged column or its required projection target disappears during an active gesture
- **THEN** the system cancels the preview and returns all Details consumers to persisted order

### Requirement: Atomic commit and reversible cancellation
The system SHALL keep the prospective order transient until a valid drop, commit it exactly once on
drop through the existing ordered-layout persistence path, and discard it on cancellation.

#### Scenario: Valid drop commits once
- **WHEN** the user releases the primary pointer after selecting a valid prospective insertion slot
- **THEN** the system commits that order once and the order remains after the drag session ends and
  after session restore

#### Scenario: Escape cancels
- **WHEN** the user presses Escape during a column drag
- **THEN** the system restores the original persisted order without persisting any preview position

#### Scenario: Pointer drag is cancelled
- **WHEN** GPUI terminates the drag without a valid drop
- **THEN** the system restores the original persisted order and clears all preview state

### Requirement: Existing column interaction invariants
Live reordering SHALL preserve the fixed-first `Name` column and SHALL remain isolated from sorting,
resizing, filtering, and dynamic-column identity.

#### Scenario: Name remains first
- **WHEN** a user attempts to drag `Name` or insert another column before it
- **THEN** `Name` remains index zero and no preview violates that invariant

#### Scenario: Click still sorts
- **WHEN** the user presses and releases a movable header without exceeding the drag threshold
- **THEN** the system performs the existing sort action and does not start or persist a reorder

#### Scenario: Resize grip remains isolated
- **WHEN** the user drags a column resize grip
- **THEN** the system changes width through the resize path and does not start a reorder preview

#### Scenario: Extension column identity remains stable
- **WHEN** a built-in or extension column participates in a preview
- **THEN** its header and row values remain paired by stable `ColumnId` throughout preview and commit

### Requirement: Physical-pointer regression evidence
The change SHALL include blocking UTIT coverage that proves live adjacent movement and left-right
symmetry using DPI-aware physical pointer input.

#### Scenario: Rightward live preview is observed
- **WHEN** UTIT moves a header only beyond the midpoint of its immediately adjacent right-hand header
  while keeping the primary pointer down
- **THEN** automation bounds prove both the header and a representative data cell moved before
  mouse-up, and failure output records pointer/header bounds plus a screenshot

#### Scenario: Leftward symmetry and committed order are observed
- **WHEN** UTIT releases the rightward drag and then performs the inverse adjacent leftward drag
- **THEN** the committed order survives release and the leftward preview crosses the corresponding
  midpoint with the same one-column threshold
