## ADDED Requirements

### Requirement: Persistent nested bookmark folders

The system SHALL persist a hierarchy of logical bookmark folders and typed bookmark entries with stable identities, parent ownership, and sibling order. Existing valid flat bookmarks MUST restore as root-level entries without loss of name, target, Lua source, ID, or relative order.

#### Scenario: Upgrade a flat bookmark session
- **WHEN** the system loads a valid session written before folders existed
- **THEN** it MUST expose every bookmark as a root entry in the original order and preserve it after a restart

#### Scenario: Recover invalid tree references
- **WHEN** a persisted bookmark node has a missing parent or cyclic parent chain
- **THEN** the system MUST retain the valid node as a root entry, avoid an infinite traversal, and continue loading the remaining session

### Requirement: Bookmark folder context management

The system SHALL expose an accessible right-click menu for the logical favourites tree. The root collection SHALL allow folder creation; a bookmark folder SHALL allow create-subfolder, rename, and delete; a bookmark entry SHALL allow edit, move, and remove.

#### Scenario: Create and rename a folder
- **WHEN** the user right-clicks the root or a bookmark folder and saves a non-empty folder name
- **THEN** the system MUST insert or rename the logical folder at that parent and persist the result

#### Scenario: Delete non-empty logical folder
- **WHEN** the user requests deletion of a folder containing descendants
- **THEN** the system MUST show the descendant count and MUST NOT remove any bookmark node until the user confirms

#### Scenario: Persistence fails
- **WHEN** a folder mutation cannot be persisted
- **THEN** the system MUST restore the exact preceding tree and display a non-blocking error

### Requirement: Folder-aware bookmark projections

The system SHALL project root and expanded logical bookmark folders into the favourites navigation area and SHALL render root folders in the bookmark toolbar as expandable bookmark menus. Within each parent, the displayed order MUST equal durable sibling order.

#### Scenario: Open a bookmark inside a folder
- **WHEN** the user selects a folder-contained folder or file bookmark from navigation or its toolbar menu
- **THEN** the system SHALL use the same navigation or Shell-open behavior as the equivalent root bookmark

