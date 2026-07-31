## ADDED Requirements

### Requirement: Revisioned shared presentation
The system SHALL expose directory entries to the file view through shared immutable storage and a revisioned presentation index, and SHALL NOT deep-clone the complete directory snapshot on the steady-state scroll path.

#### Scenario: Scroll reuses directory storage
- **WHEN** the user scrolls a fully loaded directory without changing entries, sort settings, or hidden-item settings
- **THEN** the file view reuses the existing shared snapshot and presentation revision without cloning all entries

#### Scenario: Relevant mutation advances presentation
- **WHEN** an insertion, removal, rename, active-sort metadata change, sort change, or hidden-item setting change is accepted
- **THEN** the system advances or invalidates the relevant presentation revision and publishes a new ordered entry-index projection

### Requirement: Cached allocation-free ordering
The system SHALL cache filtered and sorted entry-index projections by directory revision, sort column, sort direction, and hidden-item setting, and sort comparison SHALL NOT allocate case-folded strings.

#### Scenario: Repeated render reuses ordering
- **WHEN** the file view renders repeatedly with the same presentation cache identity
- **THEN** it reuses the same ordered indices without cloning or sorting the underlying entries

#### Scenario: Case-insensitive names share normalized keys
- **WHEN** entries differing in case are compared for a case-insensitive text sort
- **THEN** the comparator uses normalized keys created outside the comparison loop

### Requirement: Bounded one-dimensional realization
The system SHALL virtualize Details, List, and Content modes by realizing only the viewport plus bounded overscan while representing the complete collection in scroll geometry.

#### Scenario: Large Details directory
- **WHEN** a standard test viewport displays a 100,000-entry directory in Details mode
- **THEN** no more than 250 file rows are realized and the scrollbar represents all 100,000 entries

#### Scenario: Fixed Details header
- **WHEN** the user scrolls vertically or horizontally in Details mode
- **THEN** the header remains fixed vertically, follows the required horizontal offset, and row realization remains bounded

### Requirement: Bounded two-dimensional realization
The system SHALL virtualize wrapped icon and tile modes by deriving columns from viewport width and realizing only visible grid rows plus bounded overscan.

#### Scenario: Large icon grid
- **WHEN** a standard test viewport displays 100,000 entries in an icon or tile mode
- **THEN** no more than 250 cells are realized and the vertical extent represents every computed grid row

#### Scenario: Viewport resize
- **WHEN** the viewport width or zoom changes the grid column count
- **THEN** the system recomputes grid geometry without cloning or re-sorting the directory entries

### Requirement: Stable identity interactions
The system MUST preserve `ShellItemId` as the durable identity for selection, focus, rename, activation, context menus, drag/drop, and watcher updates across virtualization, sorting, and filtering.

#### Scenario: Selection survives sorting
- **WHEN** a selected item moves to a different visible ordinal after sorting
- **THEN** the same stable item remains selected and actions resolve to that item

#### Scenario: Keyboard target is offscreen
- **WHEN** keyboard navigation targets an item outside the realized range
- **THEN** the system scrolls the target into range, realizes it, and then transfers focus

### Requirement: Virtualized accessibility semantics
The system SHALL expose the logical collection size and accurate one-based positions for realized items, and SHALL realize an offscreen accessibility target before focusing or invoking it.

#### Scenario: Accessibility reads a virtual list
- **WHEN** assistive technology inspects a realized item in a partially realized 100,000-entry directory
- **THEN** the item reports its correct logical position and the collection reports the complete set size

### Requirement: Bounded scroll notification
Normal wheel and scrollbar input SHALL update the file-view virtual range without unconditionally refreshing every application window, and file-view notifications SHALL be coalesced to at most one per frame.

#### Scenario: Wheel stays within realized range
- **WHEN** a wheel delta changes offset without changing the overscanned realized range
- **THEN** the system does not rebuild the presentation or request an application-wide refresh

#### Scenario: Thumb drag crosses virtual boundary
- **WHEN** scrollbar dragging moves into a different virtual range
- **THEN** the file view realizes the new bounded range through one coalesced frame notification
