## ADDED Requirements

### Requirement: Address editing survives its entry gesture
The explorer SHALL keep address editing active after the pointer release that follows a click on unused address-bar space, and the editor SHALL expose, focus, and select the complete parsing path.

#### Scenario: Pointer enters address editing
- **WHEN** the user clicks unused address-bar space while the breadcrumb is browsing
- **THEN** the complete parsing path editor remains active after the click is released
- **AND** subsequent text input is received by that editor

#### Scenario: Keyboard enters address editing
- **WHEN** the user presses `Ctrl+L` or `Alt+D`
- **THEN** the complete parsing path editor receives focus and selects all text

### Requirement: Address editing retains Explorer termination semantics
The explorer SHALL cancel an address draft on `Esc`, submit it on `Enter`, and close editing for an ordinary click outside the address surface.

#### Scenario: Escape restores breadcrumb
- **WHEN** the user changes the address draft and presses `Esc`
- **THEN** the draft is discarded and the resolved breadcrumb is restored

#### Scenario: Enter submits a valid address
- **WHEN** the user submits a valid filesystem address with `Enter`
- **THEN** navigation resolves through the existing address parser and commits the resulting location

#### Scenario: Ordinary outside click closes editing
- **WHEN** address editing is active and the user clicks an unrelated focusable surface
- **THEN** address editing ends according to the existing click-outside behavior

### Requirement: Details-column drag lifecycle is focus-neutral
Details-column drag update, commit, and cancellation actions SHALL NOT independently terminate address editing or inline rename, and an inactive root-level drag cancellation SHALL be a focus-neutral no-op.

#### Scenario: Ordinary pointer release has no editor effect
- **WHEN** no details-column drag is active and the user releases the left pointer button
- **THEN** inactive drag cancellation does not close address editing, commit inline rename, or change column order

#### Scenario: Active drag released outside header is canceled
- **WHEN** a details-column drag is active and the user releases outside a valid header drop target
- **THEN** the drag preview is cleared exactly once without changing the persisted column order

#### Scenario: Valid drag terminal action preserves editor lifecycle
- **WHEN** a details-column drag update, commit, or cancellation reaches the global action dispatcher
- **THEN** that lifecycle action alone does not close address editing or commit inline rename

### Requirement: Real-input regression evidence covers both interactions
UTIT SHALL exercise address editing and details-column drag cleanup with genuine Windows input and SHALL produce machine-readable results plus screenshot evidence.

#### Scenario: UTIT passes address and drag regression flow
- **WHEN** the address-bar edit lifecycle UTIT case runs
- **THEN** it verifies pointer entry survives release, keyboard entry selects the complete path, `Esc` restores browsing, `Enter` submits a valid path, and an outside drag release clears an active column drag
- **AND** the run records a passing report and screenshot
