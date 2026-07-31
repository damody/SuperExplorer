## ADDED Requirements

### Requirement: Details header context menu
Right-clicking a visible Details header SHALL open an app-owned, focusable, occluding context menu without selecting, hovering, or activating a file row.

#### Scenario: Header is right-clicked
- **WHEN** the pointer releases the right button over a Details column header
- **THEN** the column menu opens at the pointer, receives menu focus, and the underlying file view remains unchanged

#### Scenario: Menu is navigated
- **WHEN** the user uses hover, Up, Down, Home, End, Enter, Space, Escape, or clicks outside
- **THEN** enabled rows highlight and activate exactly once, dismissal restores focus, and pointer events do not pass through

### Requirement: Column sizing commands
The column menu SHALL provide commands to auto size the clicked column and all visible columns using already-owned header and row presentation values.

#### Scenario: Current column is auto sized
- **WHEN** the user activates the current-column sizing command
- **THEN** that column width is clamped to the existing minimum and maximum and other widths are unchanged

#### Scenario: All columns are auto sized
- **WHEN** the user activates the all-column sizing command
- **THEN** every visible column is independently sized and hidden columns remain unchanged

### Requirement: Immediate complete column visibility menu
The header context menu SHALL always display Name, Date Modified, Type, Size, Date Created, Authors, Tags, and Title as one checkable list. It SHALL NOT require an Other Columns expansion, confirmation, or cancellation step.

#### Scenario: Optional column is toggled
- **WHEN** the user toggles any optional column in the menu
- **THEN** its header and cells appear or disappear immediately while Name remains visible

#### Scenario: Complete list is opened
- **WHEN** the user opens the Details header context menu
- **THEN** all supported column choices are immediately visible and no Other, OK, or Cancel command is shown

#### Scenario: User attempts to hide Name
- **WHEN** the Name row is activated
- **THEN** the action is disabled or ignored truthfully and at least one column remains visible

#### Scenario: Optional metadata is absent
- **WHEN** an entry has no owned value for Date Created, Authors, Tags, or Title
- **THEN** its cell is blank and rendering does not load a property handler or read the file

### Requirement: Column settings persist safely
Column order, visibility, and width SHALL be owned per tab, persisted in the existing session snapshot, bounded during validation, and migrated from the previous four-column schema.

#### Scenario: Existing session is restored
- **WHEN** a snapshot from the four-column schema is loaded
- **THEN** the original four widths/order are retained and newly supported optional columns receive deterministic defaults

#### Scenario: Tabs use different columns
- **WHEN** two tabs configure different visibility or widths
- **THEN** switching tabs restores each tab's settings without cross-contamination

### Requirement: Adaptive file-size formatting
Every Explorer surface that displays a file size SHALL use one 1024-based formatter with KB, MB, GB, and TB units, while folders and unknown values display blank.

#### Scenario: Size crosses a unit boundary
- **WHEN** a size reaches the configured KB, MB, GB, or TB threshold
- **THEN** the value promotes to the larger unit with bounded precision instead of displaying 1024 of the smaller unit

#### Scenario: Nonzero file is smaller than one KB
- **WHEN** a regular file has a size from 1 through 1023 bytes
- **THEN** it displays as `1 KB`

#### Scenario: Folder or unknown size is rendered
- **WHEN** an item is a container or has no owned size
- **THEN** the size cell and metadata value are blank

#### Scenario: Column auto size measures a file size
- **WHEN** the Size column is auto sized
- **THEN** measurement uses the exact same formatted text rendered in its cells

### Requirement: Selected-item Shell menu dismissal
The native Shell context menu opened for selected files or folders SHALL remain isolated from the app and SHALL terminate as cancelled when the user presses Escape or activates a point outside the popup.

#### Scenario: Selected-item menu is dismissed
- **WHEN** a selected file or folder context menu is showing and the user presses Escape or clicks elsewhere
- **THEN** no verb is invoked, the worker reports a cancelled terminal exactly once, and focus returns to the Explorer window
