## ADDED Requirements

### Requirement: Every enabled manager control is functional
The bookmark manager SHALL expose no enabled-looking control without a typed behavior.

#### Scenario: Toolbar commands
- **WHEN** the user activates Manage, View, Import, Backup, Back, or Forward
- **THEN** the corresponding menu, mutation, view change, transfer, or history navigation occurs

### Requirement: Tree and table form one navigable selection model
The manager SHALL filter the table by the selected tree location and SHALL keep a valid selected item or clear it after mutations.

#### Scenario: Select and edit a bookmark
- **WHEN** a bookmark row is selected
- **THEN** the details pane shows that bookmark and committed edits persist or roll back visibly

### Requirement: Manager editors open centered
Editors launched from the manager SHALL open centered, while star-launched editors SHALL retain their anchor.

#### Scenario: Double-click manager row
- **WHEN** the user double-clicks a bookmark in the manager
- **THEN** its editor opens centered and focused

### Requirement: Transfers are truthful
Import and backup SHALL preserve the complete bookmark tree and reject invalid input without mutation.

#### Scenario: Invalid clipboard import
- **WHEN** clipboard data is not a valid bookmark document
- **THEN** bookmarks remain unchanged and a visible failure notice is produced
