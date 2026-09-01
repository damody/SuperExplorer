# bookmark-star-popup Specification

## ADDED Requirements

### Requirement: Star opens a compact bookmark editor
The system SHALL present a Firefox-inspired compact bookmark editor after the current-location star is activated.

#### Scenario: New bookmark
- **WHEN** the user activates an unfilled star for a bookmarkable location
- **THEN** the editor displays `新增書籤`, selects the proposed name, focuses the name input, and shows destination, Save, and Cancel controls

#### Scenario: Existing bookmark
- **WHEN** the user activates a filled star
- **THEN** the editor displays `編輯書籤` with the existing name and destination

### Requirement: Keyboard completion is immediate
The editor SHALL save on Enter and cancel on Escape.

#### Scenario: Save with Enter
- **WHEN** the editor is open and the user presses Enter
- **THEN** the current name and destination are committed through the existing persistence workflow and the editor closes on success

#### Scenario: Cancel with Escape
- **WHEN** the editor is open and the user presses Escape
- **THEN** the draft is discarded and the editor closes without a bookmark mutation
