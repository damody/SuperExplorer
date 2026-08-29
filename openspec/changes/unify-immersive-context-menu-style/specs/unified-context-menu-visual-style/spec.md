## ADDED Requirements

### Requirement: Accepted Local immersive visual baseline
The system SHALL index an accepted Local immersive-menu baseline for each supported theme and DPI before remote visual tokens are approved.

#### Scenario: Baseline is captured
- **WHEN** Local file, folder, and background menus are captured on a supported environment
- **THEN** evidence records OS build, theme, DPI, font, crop geometry, target type, screenshot hash, and measured visual values

#### Scenario: Baseline evidence is incomplete
- **WHEN** any required environment metadata or screenshot hash is missing
- **THEN** the corresponding remote token approval remains incomplete

### Requirement: Typed shared remote visual tokens
The system SHALL project context-menu surface, border, divider, text, danger, hover, pressed, font, row height, icon gutter, inset, width policy, and shadow through one typed theme/DPI contract consumed by ADB/SFTP menus.

#### Scenario: Remote menu renders in light or dark mode
- **WHEN** an ADB or SFTP menu opens in an approved light or dark theme/DPI combination
- **THEN** every governed visual property comes from `ContextMenuVisualTokens`

#### Scenario: Theme or DPI changes
- **WHEN** the active theme, high-contrast state, monitor, or DPI changes before a remote menu opens
- **THEN** the menu uses a fresh projection for the current environment

### Requirement: Visual parity within approved tolerances
The ADB/SFTP renderer SHALL match the accepted Local baseline within the evidence-defined tolerances for typography, row geometry, icon/text alignment, colors, dividers, interaction states, border, and shadow.

#### Scenario: ADB and SFTP item menus are compared
- **WHEN** indexed ADB and SFTP screenshots are compared with the matching Local baseline
- **THEN** every measured property passes its recorded tolerance and no unmeasured manual claim closes the gate

#### Scenario: Folder and background variants are compared
- **WHEN** remote folder and background menus expose different command membership
- **THEN** their shared rows and surfaces retain the same visual metrics while command membership remains contextual

### Requirement: Listing-color isolation
Context-menu styling MUST NOT change Local, ADB, or SFTP file/folder listing-row background, selection, hover, or text colors.

#### Scenario: Menu tokens are changed
- **WHEN** a context-menu surface or interaction token is updated
- **THEN** listing-row theme projections and render contracts remain byte-for-byte or semantically unchanged

### Requirement: Remote interaction and accessibility parity
ADB/SFTP custom menus SHALL preserve full-row pointer feedback, keyboard navigation, accessibility roles, dismissal, edge clamping, and contextual command behavior while adopting the shared style.

#### Scenario: Pointer command is selected
- **WHEN** the user clicks a remote command
- **THEN** the full row shows governed feedback, the action dispatches once, and the menu closes

#### Scenario: Keyboard navigation is used
- **WHEN** the user traverses commands, activates one, or presses Escape
- **THEN** focus order, accessible labels, activation, and dismissal remain correct

#### Scenario: Menu opens near a monitor edge
- **WHEN** the requested popup origin would place the styled menu outside the work area
- **THEN** the complete menu is clamped without changing row metrics

### Requirement: Visual fallback independence
Remote visual tokens SHALL remain defined when the native immersive capability is unsupported or disabled.

#### Scenario: Local adapter falls back
- **WHEN** Local menus use the existing unstyled native path
- **THEN** ADB/SFTP continues using the last approved immersive visual-token contract and remains functional
