## ADDED Requirements

### Requirement: Bounded production extension command surface
The production Extensions popup SHALL constrain every contribution label and command
panel to its popup bounds. Long labels SHALL remain single-line, display an ellipsis,
and expose the complete label to accessibility clients.

#### Scenario: Long development contribution label is rendered
- **WHEN** a loaded extension contribution name is wider than the popup
- **THEN** no text paints outside the popup and UI Automation reports the full name

### Requirement: Interactive official command panels
Invoking `Rename from EXIF` or `Bulk folder generator` SHALL replace the command list
with an anchored host-rendered choice and preview panel. The panel SHALL provide
explicit preview-labelled actions and Cancel; opening, previewing, cancelling, clicking
outside, or pressing Escape SHALL NOT mutate the filesystem.

#### Scenario: User cancels a bulk-folder draft
- **WHEN** the user opens the generation choices and presses Cancel or Escape
- **THEN** the command list returns and no folder has been created

#### Scenario: Escape is pressed twice
- **WHEN** a command panel is open and the user presses Escape twice
- **THEN** the first press returns to the Extensions command list and the second closes the popup

### Requirement: Production bulk-folder preview and execution
The bulk-folder panel SHALL offer bounded count presets, validate count 1..100000 and
every generated Windows name, and show representative first/last names. Only selecting
an explicit valid plan SHALL create directories in the active folder.

#### Scenario: Valid bulk-folder plan is confirmed
- **WHEN** the user selects the `Folder-001…010` plan
- **THEN** the active folder contains `Folder-001` through `Folder-010` after serialized host operations complete

#### Scenario: Invalid folder count reaches the planner
- **WHEN** count is zero or greater than 100000
- **THEN** planning is rejected and disk remains unchanged

### Requirement: Production EXIF rename preview and execution
The EXIF panel SHALL operate on selected supported image files, parse metadata without
external executables or network, preserve extensions, preview the selected naming rule,
and reject missing metadata, invalid names, stale selection, and
case-insensitive collisions. Only an explicitly confirmed valid plan SHALL rename.

#### Scenario: Valid selected image rename is confirmed
- **WHEN** selected images contain the required EXIF tokens and the user confirms the preview
- **THEN** the host executes the displayed rename plan and retains an undo record

#### Scenario: Required EXIF token is missing
- **WHEN** a selected image lacks metadata referenced by the chosen template
- **THEN** that failure appears in preview, Rename is disabled, and no source is renamed
