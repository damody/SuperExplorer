## ADDED Requirements

### Requirement: Usable Lua bookmark editor

The system SHALL render the Lua bookmark name and source in a dedicated native window as visible, focusable, editable controls with token-derived foreground, background, border, selection, and caret colours. Opening the `+` action MUST create or activate that window and focus the name control without covering or blocking the Explorer file view.

#### Scenario: Create Lua bookmark
- **WHEN** the user activates the bookmark toolbar `+`
- **THEN** the system MUST show visible name and Lua-source fields, accept input, allow cancel, and save a Lua bookmark only after durable mutation succeeds

#### Scenario: Editor window loses activation
- **WHEN** the dedicated bookmark editor has become active and focus moves to any other window
- **THEN** the system MUST cancel the unsaved draft and close the dedicated editor window

#### Scenario: Cancel or persistence failure
- **WHEN** the user cancels, presses Escape, or persistence fails
- **THEN** the system MUST respectively discard the draft or retain it with an error, and MUST leave no input-blocking overlay after cancellation

### Requirement: Destination-selecting bookmark editor

The system SHALL open the same dedicated bookmark editor window rather than immediately toggle state when the user activates the current-folder star or adds a selected filesystem item. The editor SHALL prefill a name and target, expose a root-or-folder destination picker, and persist the chosen parent.

#### Scenario: Save current folder into a chosen folder
- **WHEN** a user activates the star at a physical folder and selects a bookmark folder before saving
- **THEN** the system MUST create the current-folder bookmark under that logical folder and preserve the chosen destination after restart

#### Scenario: Edit or remove starred bookmark
- **WHEN** the current physical folder is already bookmarked and the user activates the star
- **THEN** the system MUST open that bookmark's editor and offer a remove action that deletes only after confirmation where required by folder policy

#### Scenario: Non-filesystem location
- **WHEN** the current location is not a physical filesystem folder
- **THEN** the system MUST disable the star and MUST NOT create a bookmark draft
