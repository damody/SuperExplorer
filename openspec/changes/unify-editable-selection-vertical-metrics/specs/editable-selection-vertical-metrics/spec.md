## ADDED Requirements

### Requirement: Near-full-height selected text geometry
The application SHALL render the selected character range in every supported single-line editor with a selection background that fully covers the glyph band and occupies most of the editor's inner height without painting unselected characters.

#### Scenario: Address text is selected
- **WHEN** the address bar enters editing mode and any address characters are selected
- **THEN** the selection background covers the selected glyphs and remains limited horizontally to the selected range

#### Scenario: Search text is selected
- **WHEN** the search box is editing and any query characters are selected
- **THEN** the search selection uses the same near-full-height geometry as the address editor

#### Scenario: Rename text is selected
- **WHEN** inline rename is active and any filename characters are selected
- **THEN** the rename selection uses the same metric rule with the rename control's own height and border

### Requirement: Symmetric vertical selection margins
The application SHALL center the selected-text line box inside the editor focus border so the visible top and bottom margins are equal after DPI scaling within one physical pixel.

#### Scenario: Normal DPI rendering
- **WHEN** an editable field selection is rendered at 100 percent scaling
- **THEN** the top and bottom selection margins differ by no more than one physical pixel

#### Scenario: Scaled rendering
- **WHEN** an editable field selection is rendered at a supported non-integer DPI scale
- **THEN** rounding preserves top and bottom selection margins within one physical pixel

### Requirement: Editing colors and interactions remain intact
The application SHALL use the address editing mode's normal foreground, opaque selected background, selected foreground, and caret colors for every supported editing mode while retaining pointer selection, keyboard selection, and commit or cancel behavior.

#### Scenario: Partial selection preserves unselected text
- **WHEN** only part of an address, search query, or filename is selected
- **THEN** every editor uses the same opaque address selection background and selected-text foreground while unselected text retains the same primary foreground

#### Scenario: Pointer caret placement after metric change
- **WHEN** the user clicks or drags within an editable field
- **THEN** the caret and selection correspond to the pointer's text position without coordinate offset

### Requirement: Automated coverage for every editor
UTIT SHALL exercise and capture the address, search, and inline rename selections and SHALL fail when glyph coverage or symmetric-margin requirements are violated.

#### Scenario: Headful editable selection verification
- **WHEN** the editable pointer-input UTIT case runs
- **THEN** it produces address, search, and rename evidence and validates selection height, glyph coverage, and top-bottom margin parity for each surface
