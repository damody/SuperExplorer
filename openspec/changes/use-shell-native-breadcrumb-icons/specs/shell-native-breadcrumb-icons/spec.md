## ADDED Requirements

### Requirement: Shell-native icon for every breadcrumb location
The application SHALL request and render the Windows Shell icon for every This PC, drive, folder, archive, and namespace location displayed by the browsing address bar.

#### Scenario: Concrete location icon succeeds
- **WHEN** Windows Shell returns an icon for a breadcrumb location
- **THEN** the root, visible segment, overflow item, or child-menu item renders that exact location texture

#### Scenario: Concrete icon arrives after fallback
- **WHEN** a location-specific icon completes after the breadcrumb was rendered with a fallback
- **THEN** the next render replaces the fallback in place without changing breadcrumb layout or focus

### Requirement: Generic fallback originates from Windows Shell
The application SHALL obtain the generic folder fallback from Windows Shell during initialization and SHALL NOT draw or bundle its own breadcrumb folder fallback.

#### Scenario: Application initializes
- **WHEN** the initial navigation icon batch is submitted
- **THEN** it includes one deduplicated generic directory icon request keyed by the current DPI, theme, and association generation

#### Scenario: Concrete icon is pending or fails
- **WHEN** a breadcrumb location has no available concrete Shell texture
- **THEN** the renderer uses the generic Shell folder texture if available

#### Scenario: First Shell response is pending
- **WHEN** neither the concrete nor generic Shell texture has completed on the first-ever frame
- **THEN** the renderer reserves the normal icon slot without drawing an application-owned icon

### Requirement: Shell icon loading remains responsive and bounded
The application SHALL perform breadcrumb icon acquisition through the asynchronous Shell service and existing bounded caches without synchronous Shell calls on the UI thread.

#### Scenario: Shell handler is slow or unavailable
- **WHEN** a concrete or generic icon request times out, fails, or cannot be queued
- **THEN** breadcrumb navigation, pointer input, keyboard input, and rendering remain responsive

#### Scenario: DPI theme or association changes
- **WHEN** icon DPI, theme, or association generation changes
- **THEN** the application derives a distinct generic key and schedules the correct Shell icon without reusing incompatible pixels

### Requirement: Automated breadcrumb icon coverage
UTIT SHALL verify generic initialization, Shell-only fallback structure, concrete replacement behavior, and visible icon slots in a multi-level browsing address bar.

#### Scenario: Headful breadcrumb icon verification
- **WHEN** the breadcrumb icon UTIT case navigates a multi-level fixture
- **THEN** it captures evidence for the root and all visible segments while confirming the window remains interactive

#### Scenario: Deterministic fallback verification
- **WHEN** deterministic icon state contains a generic texture followed by a concrete texture or failure
- **THEN** tests verify concrete-over-generic precedence and generic retention after failure

### Requirement: Shell-native navigation pane icons
The application SHALL preserve Windows Shell drive icons when This PC navigation loads newer cache epochs and SHALL render ordinary navigation-tree folders with the Windows Shell generic folder icon.

#### Scenario: This PC replaces a drive cache epoch
- **WHEN** opening This PC loads a newer Shell icon key for a drive already displayed in the navigation pane
- **THEN** the drive row resolves the newest compatible texture for the same location instead of reverting to an application-drawn drive placeholder

#### Scenario: Expanded folder is rendered
- **WHEN** an ordinary filesystem folder appears in an expanded navigation node
- **THEN** its row renders the generic folder texture obtained through `SHGFI_USEFILEATTRIBUTES | FILE_ATTRIBUTE_DIRECTORY`

#### Scenario: Shell texture is not available yet
- **WHEN** neither a compatible drive texture nor the generic folder texture is available
- **THEN** the navigation row reserves the normal icon slot without drawing a gray drive outline or orange folder block
