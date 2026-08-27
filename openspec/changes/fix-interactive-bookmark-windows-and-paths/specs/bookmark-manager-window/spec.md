## ADDED Requirements

### Requirement: Dedicated interactive bookmark manager window
The system SHALL present bookmark management in a dedicated focusable native window and MUST NOT render the manager as a file-view overlay. Repeated requests SHALL activate the existing manager window instead of creating divergent managers.

#### Scenario: Open the manager
- **WHEN** the user invokes Bookmark Manager from the toolbar or a bookmark context menu
- **THEN** a dedicated window SHALL display the authoritative logical folder and bookmark tree with interactive controls

#### Scenario: Reopen an existing manager
- **WHEN** the manager window is already open and the user invokes it again
- **THEN** the existing window MUST activate and retain a single authoritative editing session

### Requirement: Durable management operations
The manager SHALL allow creation, rename/edit, reorder or move, and deletion appropriate to logical folders and bookmarks. A successful mutation MUST refresh the manager from authoritative state; a persistence failure MUST roll back the mutation and keep an actionable editing surface.

#### Scenario: Persist a folder rename
- **WHEN** the user enters a non-empty folder name and saves it
- **THEN** the system SHALL durably rename only that logical folder and refresh every bookmark projection

#### Scenario: Persistence fails
- **WHEN** a manager mutation cannot be durably saved
- **THEN** the system MUST restore the pre-mutation tree, retain or restore the relevant draft, and display an error notice

#### Scenario: Delete a non-empty logical folder
- **WHEN** the user requests deletion of a logical folder with descendants
- **THEN** the system MUST show the descendant count and MUST NOT delete any node until the user confirms
