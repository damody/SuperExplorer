## ADDED Requirements

### Requirement: Transparent startup splash
SuperExplorer SHALL display the supplied upper wordmark in a centered, borderless splash window whose background is transparent and whose logo remains visually opaque and unmodified.

#### Scenario: Production startup displays branding
- **WHEN** SuperExplorer starts outside an automated visual-fixture mode
- **THEN** the transparent splash window appears centered above the main Explorer window

#### Scenario: Splash asset preserves the logo
- **WHEN** the packaged splash PNG is decoded
- **THEN** it has transparent background pixels and visible opaque logo pixels in the original yellow, white, and dark palette

### Requirement: Concurrent main-window loading
SuperExplorer SHALL create and initialize its normal main window while the splash remains visible above it.

#### Scenario: Main window loads behind splash
- **WHEN** the production startup sequence creates the splash
- **THEN** the main Explorer window has already been created and continues normal initialization without waiting for splash dismissal

### Requirement: Bounded splash lifetime
SuperExplorer SHALL hold the rendered splash for 1 second, fade it from fully visible to fully transparent over 180 milliseconds, close it, and return focus to the main window.

#### Scenario: Splash dismisses after the configured interval
- **WHEN** the splash has completed its first rendered frame
- **THEN** it remains fully visible for 1 second and is removed after the following 180 millisecond fade

#### Scenario: Splash window is already closed
- **WHEN** a scheduled fade or removal update finds that the splash no longer exists
- **THEN** startup continues without terminating or failing the main Explorer window

### Requirement: Consistent Windows application icon
The SuperExplorer executable SHALL embed the supplied lower square artwork as a multi-resolution Windows icon with 16, 24, 32, 48, 64, 128, and 256 pixel frames.

#### Scenario: Windows selects an application icon size
- **WHEN** Windows requests an icon for the executable, taskbar, Alt+Tab surface, shortcut, or native window chrome
- **THEN** the executable resource provides the matching or nearest embedded SuperExplorer icon frame

### Requirement: Automation remains deterministic
SuperExplorer SHALL omit the splash whenever a visual fixture is active or `EXPLORER_AUTO_CLOSE_MS` is set.

#### Scenario: Visual fixture startup
- **WHEN** startup is configured with a visual fixture
- **THEN** only the expected test window is created and no splash timer affects the capture

#### Scenario: Auto-close startup
- **WHEN** `EXPLORER_AUTO_CLOSE_MS` is set
- **THEN** no splash window is created and the existing automated close timing remains authoritative

### Requirement: Splash failure is non-fatal
SuperExplorer SHALL continue running the successfully created main Explorer window if splash creation fails.

#### Scenario: Splash window creation fails
- **WHEN** GPUI cannot create the splash window after the main window exists
- **THEN** the application records a diagnostic warning and leaves the main Explorer window running
