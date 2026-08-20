## ADDED Requirements

### Requirement: Start follows taskbar alignment
SuperDesktop SHALL position the Start popup according to the current persisted taskbar alignment, using Left as the default when alignment is missing or invalid.

#### Scenario: Default alignment opens Start on the left
- **WHEN** a user with default settings opens Start
- **THEN** the popup's left edge is placed at the selected monitor work-area left edge plus the standard Start margin

#### Scenario: Center alignment opens Start in the center
- **WHEN** taskbar alignment is Center and the user opens Start
- **THEN** the popup is horizontally centered within the selected monitor work area

#### Scenario: Saved alignment applies without restart
- **WHEN** the user changes taskbar alignment and then reopens Start without restarting SuperDesktop
- **THEN** the new popup uses the newly saved alignment

### Requirement: Start alignment uses monitor-local DPI-aware bounds
SuperDesktop SHALL calculate Start placement in the selected monitor's logical coordinate space and SHALL keep the popup within that monitor's usable horizontal bounds.

#### Scenario: Left alignment on an offset high-DPI monitor
- **WHEN** Start opens left aligned on a monitor with non-zero or negative origin and DPI other than 96
- **THEN** its logical origin is derived from that monitor's work area and DPI rather than from the primary monitor

#### Scenario: Narrow work area
- **WHEN** the selected work area is narrower than the preferred Start width plus both margins
- **THEN** the popup width and horizontal origin are clamped so the popup remains inside the work area

### Requirement: All Start activation paths share alignment behavior
SuperDesktop SHALL use the same current-alignment geometry path for taskbar pointer activation and shell keyboard activation.

#### Scenario: Pointer activation
- **WHEN** the user selects the Start taskbar button
- **THEN** Start opens using the current taskbar alignment

#### Scenario: Keyboard activation
- **WHEN** the user invokes Start or Search through a registered Windows shell hotkey
- **THEN** Start opens using the same selected monitor and current taskbar alignment as pointer activation

### Requirement: Alignment setting describes its complete effect
The Taskbar settings UI SHALL expose Left and Center through the existing alignment control and SHALL state that the control positions taskbar buttons and the Start menu.

#### Scenario: Traditional Chinese settings text
- **WHEN** Taskbar settings render in Traditional Chinese
- **THEN** the alignment row identifies both taskbar buttons and the Start menu as affected surfaces

#### Scenario: Alignment value toggles
- **WHEN** the user activates the alignment row
- **THEN** the candidate setting toggles between Left and Center and follows the existing save flow
