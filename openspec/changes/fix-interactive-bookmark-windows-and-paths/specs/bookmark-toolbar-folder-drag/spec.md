## ADDED Requirements

### Requirement: Toolbar bookmarks can be organized by left-button drag

The system SHALL allow a toolbar bookmark to be dragged with the left mouse button into a logical bookmark folder or onto the toolbar root.

#### Scenario: Drop into folder
- **WHEN** a bookmark is dropped on a logical folder
- **THEN** its parent becomes that folder and it is appended to the destination order

#### Scenario: Drop on root
- **WHEN** a nested bookmark is dropped on the toolbar background
- **THEN** its parent becomes the root

#### Scenario: Persistence failure
- **WHEN** durable persistence fails after a move
- **THEN** the original parent and order are restored

#### Scenario: Invalid or unchanged target
- **WHEN** the destination is missing or already the bookmark's parent
- **THEN** no mutation is produced
