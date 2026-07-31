## ADDED Requirements

### Requirement: Shifted character selection follows the caret
While a Super Explorer editable text control has focus, the system SHALL use `Shift+Left` and `Shift+Right` to move the active selection endpoint by one grapheme while retaining the original selection anchor.

#### Scenario: Extend selection to the right
- **WHEN** the caret is positioned before a character and the user presses `Shift+Right`
- **THEN** that complete grapheme is selected and the anchor remains at the original caret position

#### Scenario: Extend selection to the left
- **WHEN** the caret is positioned after a character and the user presses `Shift+Left`
- **THEN** that complete grapheme is selected and the anchor remains at the original caret position

#### Scenario: Reverse selection direction
- **WHEN** a user reverses shifted arrow direction after extending a selection
- **THEN** the selection contracts toward its anchor and grows on the other side after crossing the anchor

### Requirement: Shifted line-boundary selection matches Windows Explorer
While a Super Explorer editable text control has focus, the system SHALL use `Shift+Home` and `Shift+End` to move the active selection endpoint to the current line boundary while retaining the original selection anchor.

#### Scenario: Select the full single-line value from the left boundary
- **WHEN** the caret is at the beginning of a single-line editor and the user presses `Shift+End`
- **THEN** the complete editor value is selected

#### Scenario: Select the full single-line value from the right boundary
- **WHEN** the caret is at the end of a single-line editor and the user presses `Shift+Home`
- **THEN** the complete editor value is selected

#### Scenario: Preserve selection at a line boundary
- **WHEN** a shifted line-boundary command resolves to the active endpoint's current position
- **THEN** the existing selection and anchor remain unchanged

### Requirement: Selection shortcuts are consistent across editors
The system SHALL expose the four shifted selection shortcuts through the shared editable-text implementation used by the address, search, and inline rename editors.

#### Scenario: Use shifted selection in each editor type
- **WHEN** the address editor, search editor, or inline rename editor has focus
- **THEN** `Shift+Home`, `Shift+End`, `Shift+Left`, and `Shift+Right` perform the same selection operations without triggering window navigation

### Requirement: Keyboard selection has automated regression coverage
The project SHALL include unit, binding-contract, and UTIT coverage for shifted keyboard selection.

#### Scenario: Run the UTIT selection item
- **WHEN** the dedicated editable keyboard selection UTIT item runs on an interactive Windows desktop
- **THEN** it validates genuine-keyboard selection boundaries through exact replacement results in representative editable controls
