## ADDED Requirements

### Requirement: Bookmark toolbar background context menu
The bookmark toolbar SHALL expose a right-click menu on unoccupied toolbar space with commands to create a root logical folder, create a root path bookmark, and open the bookmark manager.

#### Scenario: Create from empty toolbar space
- **WHEN** the user right-clicks unoccupied bookmark-toolbar space and selects a create command
- **THEN** the system SHALL open the appropriate dedicated name or bookmark editor with root selected as its parent

### Requirement: Logical folder context management
A logical bookmark folder context menu SHALL allow creating a child folder, creating a child path bookmark, renaming the folder, and deleting it under the existing confirmation policy.

#### Scenario: Create a bookmark inside a folder
- **WHEN** the user invokes New Path Bookmark on a logical folder
- **THEN** the dedicated bookmark editor SHALL open with that logical folder preselected as the parent

#### Scenario: Rename a logical folder
- **WHEN** the user invokes Rename on a logical folder and saves a non-empty new name
- **THEN** the system SHALL preserve the folder identity, children, and sibling order while durably changing its name

### Requirement: Path bookmark context management
A file or folder path bookmark context menu SHALL allow activation, editing including rename and target changes, and deletion. Folder targets SHALL additionally allow opening in a new tab.

#### Scenario: Edit a path bookmark
- **WHEN** the user selects Edit from a path bookmark context menu
- **THEN** the system SHALL open the dedicated bookmark editor containing the bookmark's exact current name, target text, kind, and logical parent

#### Scenario: Delete a bookmark
- **WHEN** the user selects Delete on a path bookmark
- **THEN** the system SHALL delete only the logical bookmark after durable persistence and MUST NOT delete its filesystem target
