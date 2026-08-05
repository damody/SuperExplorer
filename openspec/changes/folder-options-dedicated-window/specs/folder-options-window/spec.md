## ADDED Requirements

### Requirement: Dedicated modeless Folder Options window
SuperExplorer SHALL host Folder Options in a native GPUI window distinct from every
Explorer window, and the Folder Options window SHALL NOT prevent normal interaction
with an Explorer window.

#### Scenario: Open Folder Options
- **WHEN** the user invokes Folder Options from an Explorer window
- **THEN** the application opens and activates a distinct native Folder Options window
- **AND** the invoking Explorer window remains available for navigation and selection

#### Scenario: Window creation fails
- **WHEN** GPUI cannot create the Folder Options window
- **THEN** the invoking Explorer remains responsive and a diagnostic is recorded
- **AND** a later open request can retry window creation

### Requirement: Application-wide single instance
SuperExplorer SHALL maintain at most one live Folder Options window and one editable
Folder Options draft per application process.

#### Scenario: Open while already visible
- **WHEN** Folder Options is invoked while its window is live
- **THEN** the application activates that same native window without creating a second window or draft

#### Scenario: Recover stale handle
- **WHEN** the controller contains a handle whose native window has already closed
- **THEN** the application clears the stale identity and creates one replacement window

#### Scenario: Application shutdown
- **WHEN** the application shuts down while Folder Options is open
- **THEN** it closes the options window and clears controller state exactly once

### Requirement: Explorer-style draft and commit behavior
Folder Options SHALL edit an isolated draft and SHALL apply settings application-wide
only through the existing typed settings path.

#### Scenario: Apply succeeds
- **WHEN** the user changes valid settings and invokes Apply
- **THEN** the application persists and broadcasts the settings without closing Folder Options
- **AND** the applied state becomes the new baseline for later cancellation

#### Scenario: OK succeeds
- **WHEN** the user invokes OK with a valid draft
- **THEN** the application performs the same commit as Apply and closes Folder Options after success

#### Scenario: Cancel after uncommitted changes
- **WHEN** the user changes the draft after opening or after the latest successful Apply and invokes Cancel
- **THEN** Folder Options closes without applying those later changes

#### Scenario: Escape or title close
- **WHEN** the user presses Escape or closes the native window through its title bar
- **THEN** the application performs the same discard-and-close transition as Cancel exactly once

#### Scenario: Apply fails
- **WHEN** validation or persistence prevents Apply from completing
- **THEN** Folder Options remains open with its draft intact and displays an actionable error
- **AND** no Explorer window receives a partially applied snapshot

#### Scenario: Applied state changes while draft is dirty
- **WHEN** another application path changes applied settings while the Folder Options draft has uncommitted edits
- **THEN** the dirty draft remains unchanged until the user applies or cancels it

### Requirement: Fixed shell and dedicated vertical scrolling
The Folder Options title/page tabs and OK/Cancel/Apply footer SHALL remain fixed while
only the selected page viewport scrolls, and the viewport SHALL reserve a visible
right-side vertical scrollbar track.

#### Scenario: Long page content
- **WHEN** the selected page content is taller than its viewport
- **THEN** the right scrollbar thumb represents the viewport-to-content ratio
- **AND** wheel, touchpad, track click, thumb drag, Page Up, Page Down, Home, and End move the page within clamped bounds

#### Scenario: Content fits viewport
- **WHEN** the selected page content fits inside its viewport
- **THEN** the scrollbar remains visible in a disabled/light state with a full-height thumb
- **AND** page content does not render underneath the reserved track

#### Scenario: Switch pages after scrolling
- **WHEN** the user scrolls one page, switches to another page, and then returns
- **THEN** each page restores its own previous clamped scroll offset

#### Scenario: Resize reduces scroll extent
- **WHEN** resizing the Folder Options window makes a saved page offset exceed the new maximum
- **THEN** the offset is clamped without moving the fixed tabs or footer out of view

### Requirement: Window-local input and focus
Folder Options SHALL contain its pointer, wheel, keyboard, and focus interactions
within its own native window.

#### Scenario: Scroll over Folder Options
- **WHEN** the pointer is over Folder Options and the user scrolls or drags its scrollbar
- **THEN** only the active Folder Options page offset changes
- **AND** Explorer file-view and navigation-pane offsets remain unchanged

#### Scenario: Keyboard traversal
- **WHEN** the user presses Tab or Shift+Tab in Folder Options
- **THEN** focus traverses Folder Options controls without moving to an Explorer window

#### Scenario: Change active page
- **WHEN** the user selects General, View, or Extensions
- **THEN** focus moves to the first interactive control on that page without resetting other page offsets

### Requirement: DPI-safe resizable layout
Folder Options SHALL use logical-pixel layout and convert native physical pointer
coordinates by the active window scale factor exactly once.

#### Scenario: Representative Windows DPI scales
- **WHEN** Folder Options is rendered and its scrollbar is dragged at 100%, 125%, 150%, or 200% scale
- **THEN** the pointer, thumb, and resulting logical offset remain aligned within the UITEST tolerance

#### Scenario: Resize to minimum supported size
- **WHEN** the user resizes Folder Options to its minimum supported dimensions
- **THEN** page tabs and OK/Cancel/Apply remain reachable and the page viewport scrolls any overflow

### Requirement: Automated evidence
The change SHALL include Rust tests and a registered headful UITEST that exercise the
dedicated window through real application behavior.

#### Scenario: Focused Rust validation
- **WHEN** the focused Rust test set runs
- **THEN** controller lifecycle, draft transitions, scrolling geometry, DPI conversion, and stale recovery pass without relying solely on source-text assertions

#### Scenario: Headful UITEST validation
- **WHEN** the registered Folder Options UITEST runs on an interactive Windows desktop
- **THEN** it records native window identity/count, scroll offsets, screenshots, and action results for single-instance, modeless, input-isolation, scrolling, resize, and DPI behavior
- **AND** failures retain diagnostic evidence under the test-owned output root
