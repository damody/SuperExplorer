## ADDED Requirements

### Requirement: Left-click folder menus browse content only

The system SHALL show immediate bookmark and child-folder content when a toolbar folder is left-clicked, and SHALL NOT show rename, create-child, or delete commands in that panel.

#### Scenario: Browse folder
- **WHEN** a logical bookmark folder is left-clicked
- **THEN** its immediate children appear in stored order
- **AND** child folders display a folder affordance and disclosure arrow

#### Scenario: Browse child folder
- **WHEN** a child-folder row is left-clicked
- **THEN** the panel switches to that child's content

#### Scenario: Manage folder
- **WHEN** a toolbar or nested logical folder is right-clicked
- **THEN** the existing context menu provides create, rename, and delete commands
