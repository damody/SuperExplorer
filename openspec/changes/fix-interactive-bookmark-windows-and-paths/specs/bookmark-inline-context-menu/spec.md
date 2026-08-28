## ADDED Requirements

### Requirement: Bookmark right-click matches logical-folder context styling

The system SHALL show bookmark commands in a compact inline menu with the same visual contract and dismissal behavior as the logical bookmark-folder right-click menu, and SHALL NOT open the large bookmark action window.

#### Scenario: Open bookmark context menu
- **WHEN** a bookmark entry is right-clicked
- **THEN** a menu appears at the pointer with Open, Edit, and Delete
- **AND** folder targets additionally show Open in New Tab

#### Scenario: Select command
- **WHEN** a non-delete command is selected
- **THEN** the menu closes and the command dispatches

#### Scenario: Delete bookmark
- **WHEN** Delete is selected
- **THEN** the menu closes and the dedicated delete-confirmation window opens

#### Scenario: Dismiss menu
- **WHEN** the user clicks outside, right-clicks elsewhere, or presses Escape
- **THEN** the inline menu closes without mutation
