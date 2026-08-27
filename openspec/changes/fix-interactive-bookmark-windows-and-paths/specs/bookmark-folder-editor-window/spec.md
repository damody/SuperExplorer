## ADDED Requirements

### Requirement: Bookmark folder naming uses a dedicated native window

The system SHALL present bookmark-folder creation and rename input in a focusable `WindowKind::Normal` window and SHALL NOT render that editor as an overlay in the explorer or bookmark-manager window.

#### Scenario: Rename from the manager

- **WHEN** the user requests rename for a logical bookmark folder
- **THEN** one native folder editor window opens or is retargeted and activated
- **AND** its name field accepts keyboard input without blocking either window event loop

#### Scenario: Save succeeds

- **WHEN** the user enters a non-empty name and confirms
- **THEN** the existing durable bookmark mutation path stores the name
- **AND** the editor window closes

#### Scenario: Save fails

- **WHEN** validation or durable persistence rejects the change
- **THEN** the editor window and authored name remain available for correction

#### Scenario: Cancel or close

- **WHEN** the user presses Escape, selects Cancel, or closes the editor window
- **THEN** the draft is cancelled without changing the stored folder name
