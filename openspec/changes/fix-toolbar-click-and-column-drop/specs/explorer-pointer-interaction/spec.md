## ADDED Requirements

### Requirement: Command-bar clicks are independent of column drag cleanup
The explorer SHALL deliver pointer clicks to enabled built-in and extension command-bar controls without dispatching details-column drag cancellation for an unrelated gesture.

#### Scenario: Menu command receives an ordinary click
- **WHEN** the user clicks an enabled command-bar menu control while no details-column drag is active
- **THEN** the corresponding menu opens and no details-column drag terminal action suppresses the click

#### Scenario: Direct command receives an ordinary click
- **WHEN** the user clicks an enabled direct command while no details-column drag is active
- **THEN** the command executes according to its existing semantics

### Requirement: Valid details-column drops commit before fallback cancellation
The explorer SHALL commit the previewed order when a movable details column is released on a valid movable details header, and any fallback cancellation scheduled by the source header MUST NOT revert that committed order.

#### Scenario: Release on another movable header
- **WHEN** the user drags a movable details column across another movable header and releases over that valid target
- **THEN** the previewed order becomes the active order after the release completes

#### Scenario: Deferred cancel follows successful commit
- **WHEN** a valid drop commits and a deferred outside-release cancellation subsequently runs for the same completed gesture
- **THEN** the committed order remains unchanged and no active drag remains

### Requirement: Unclaimed details-column drops restore the original order
The explorer SHALL cancel a details-column drag that ends without a valid header accepting the drop and SHALL restore the order that existed before the drag began.

#### Scenario: Release outside all valid headers
- **WHEN** the user releases a movable details column outside every valid details header target
- **THEN** the drag preview clears and the original column order is restored

### Requirement: Committed column order persists
The explorer SHALL persist a successfully committed movable-column order using the existing settings format.

#### Scenario: View is refreshed after valid drop
- **WHEN** the user commits a valid movable-column drop and refreshes or reopens the view
- **THEN** the committed movable-column order is restored

### Requirement: Name remains fixed leftmost
The explorer MUST keep the `Name` column at the leftmost position and MUST NOT allow it to participate as a draggable source.

#### Scenario: User attempts to reorder Name
- **WHEN** the user attempts a drag gesture beginning on the `Name` header
- **THEN** no column drag starts and `Name` remains leftmost
