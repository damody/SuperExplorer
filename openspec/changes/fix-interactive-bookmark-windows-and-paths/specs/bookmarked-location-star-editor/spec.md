# Bookmarked-location Star Editor

## ADDED Requirements

### Requirement: The current-location star distinguishes saved state

The bookmark toolbar SHALL show an outline star for a bookmarkable current location without an exact saved target and SHALL show a solid star using the active theme focus blue when the exact target is saved.

#### Scenario: Existing target is visible

- **WHEN** the current Local, ADB, or SFTP folder resolves to a bookmark target whose ID exists
- **THEN** the star is solid and uses the theme focus color

#### Scenario: Target is not saved

- **WHEN** the current location is bookmarkable but has no exact saved target
- **THEN** the star remains an enabled outline star

### Requirement: A saved star opens the dedicated editor

Clicking the solid star SHALL start an update draft for the matching bookmark and present the normal dedicated bookmark editor window.

#### Scenario: Edit an existing current-folder bookmark

- **WHEN** the user clicks the solid star
- **THEN** the editor contains that bookmark's name, exact editable path, and destination
- **AND** Save persists edits through the existing durable mutation path
- **AND** Remove deletes only after the user explicitly selects it

#### Scenario: Invalid path remains editable

- **WHEN** the existing bookmark contains a non-empty unavailable or syntactically unusual path
- **THEN** the editor displays and permits saving the exact text without existence validation

### Requirement: The editor uses compact independent-window presentation

The editor SHALL be a centered, resizable normal window rather than an in-surface overlay. Its initial width SHALL be 80% of the primary display with a 640px minimum. It SHALL have no native titlebar and SHALL expose Name, Path, Destination, Remove Bookmark, Cancel, and Save controls.

#### Scenario: Existing bookmark editor opens

- **WHEN** the editor is presented for an existing bookmark
- **THEN** it remains independently interactive and opens at 80% of the primary display width, subject to the 640px minimum
- **AND** no native minimize, maximize, or close buttons are displayed

#### Scenario: Remove an unsaved add draft

- **WHEN** the editor represents a bookmark that has not yet been persisted and the user selects Remove Bookmark
- **THEN** the draft is cancelled and the editor closes without a persistence deletion
