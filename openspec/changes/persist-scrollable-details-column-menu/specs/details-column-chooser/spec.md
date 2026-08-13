## ADDED Requirements

### Requirement: Column visibility rows support persistent repeated toggles
The Details column chooser SHALL remain open while an enabled column visibility row is toggled, SHALL update its checked state immediately, and SHALL allow the same row to be toggled repeatedly without reopening the chooser.

#### Scenario: Checked row is unchecked
- **WHEN** the user clicks a checked enabled column row
- **THEN** the column becomes hidden, the row becomes unchecked, and the chooser remains open

#### Scenario: Same row is checked again
- **WHEN** the user clicks the same unchecked enabled row while the chooser remains open
- **THEN** the column becomes visible, the row becomes checked, and the chooser remains open

#### Scenario: Originating header is hidden
- **WHEN** the user unchecks the column whose header opened the chooser
- **THEN** the header is removed but the chooser remains open and usable

### Requirement: Name is a fixed chooser entry
The chooser MUST present `Name` as checked and disabled, and attempts to activate that row MUST NOT hide or move the `Name` column.

#### Scenario: Name row is activated
- **WHEN** the user clicks or invokes the disabled `Name` row
- **THEN** `Name` remains checked, visible, and leftmost

### Requirement: Overflowing chooser content is vertically scrollable
The chooser SHALL constrain its rendered height to the usable menu area and SHALL provide vertical mouse-wheel, touchpad, and scrollbar access to every registered built-in and extension column when its content overflows.

#### Scenario: Registered rows exceed available height
- **WHEN** the chooser contains more rows than fit inside its bounded height
- **THEN** content outside the initial viewport is clipped and the user can scroll to the final row

#### Scenario: Bottom row is toggled after scrolling
- **WHEN** the user scrolls to the final enabled row and toggles it
- **THEN** its checked state changes, the chooser remains open, and the scroll position does not jump back to the top

#### Scenario: User scrolls back to earlier rows
- **WHEN** the user scrolls upward after toggling a lower row
- **THEN** earlier row state changes remain applied in the same open chooser

### Requirement: Chooser uses explicit dismissal boundaries
The chooser SHALL close only when an existing explicit dismissal boundary occurs, including an outside click, outside right-click, `Esc`, navigation, tab change, or replacement by another popup.

#### Scenario: Row toggle is not dismissal
- **WHEN** a visibility or chooser display row handles a pointer click
- **THEN** that click does not reach the outside dismiss layer

#### Scenario: Inactive resize terminal accompanies a row click
- **WHEN** a separator emits `EndDetailsColumnResize` while no resize session is active and the chooser is open
- **THEN** the terminal action is disabled and the chooser remains open for the row toggle

#### Scenario: Escape dismisses the chooser
- **WHEN** the chooser is open and the user presses `Esc`
- **THEN** the chooser closes and the current visibility changes remain applied

### Requirement: Visibility changes retain existing persistence semantics
Successful chooser visibility toggles SHALL update the active tab through the existing ordered details-layout settings path without changing the serialization format.

#### Scenario: View is refreshed after toggles
- **WHEN** the user changes column visibility and refreshes or reopens the view
- **THEN** the committed visibility state is restored from the existing settings representation
