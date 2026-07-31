## ADDED Requirements

### Requirement: Navigation history focus follows the active input target
The system SHALL render Back and Forward history menu pointer hover and keyboard focus with the same neutral Explorer-style gray indication and SHALL activate the currently targeted entry.

#### Scenario: Pointer moves between history entries
- **WHEN** a history popup is open and the pointer moves from one entry to another
- **THEN** the gray focus indication SHALL follow the pointer and the previously targeted entry SHALL return to the normal menu background

#### Scenario: Keyboard activates a history entry
- **WHEN** the user moves history focus with the keyboard and presses Enter
- **THEN** the focused history entry SHALL be activated

### Requirement: Permanent-delete confirmation owns accessible button focus
The system SHALL expose a visible neutral-gray focus state for the active Shift+Delete confirmation button and SHALL support bounded pointer and keyboard activation.

#### Scenario: Keyboard traverses permanent-delete actions
- **WHEN** the permanent-delete dialog is open and the user presses Tab or Shift+Tab
- **THEN** focus SHALL cycle only between Cancel and Delete and the focused button SHALL display a gray indication

#### Scenario: Focused permanent-delete action is invoked
- **WHEN** the user presses Enter or Space while a confirmation button is focused
- **THEN** the system SHALL invoke that focused action exactly once

#### Scenario: Pointer targets a permanent-delete action
- **WHEN** the pointer hovers and clicks Cancel or Delete
- **THEN** the gray indication SHALL follow the pointer and the clicked action SHALL run

### Requirement: Editable address selection preserves balanced contrast
The system SHALL render address selection with balanced vertical space inside the focused field and SHALL retain the normal dark foreground for every unselected glyph.

#### Scenario: Partial address text is selected
- **WHEN** the user selects only part of the editable address with the mouse or keyboard
- **THEN** only selected glyphs SHALL use selected-text foreground, unselected glyphs SHALL use normal foreground, and the selection background SHALL not be clipped asymmetrically by the focused border
