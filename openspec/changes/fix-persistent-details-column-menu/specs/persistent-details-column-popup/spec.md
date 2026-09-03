## ADDED Requirements

### Requirement: Persistent column visibility activation
The Details column popup SHALL keep the same visible popup session open when an enabled column visibility row is activated and SHALL allow that row and other enabled visibility rows to be activated repeatedly.

#### Scenario: Repeated check and uncheck
- **WHEN** a user activates an unchecked enabled column row and then activates the same row again
- **THEN** the popup remains visible through both activations and the row becomes checked and then unchecked

#### Scenario: Stable popup context
- **WHEN** one or more visibility rows are activated in an open popup
- **THEN** the popup HWND, screen position, and scroll offset remain unchanged

### Requirement: Immediate native and Details feedback
Each persistent activation SHALL repaint the resulting native check state before the next interaction and SHALL reconcile the corresponding Details column to that requested visibility state on the foreground UI during the same popup session.

#### Scenario: Enable a column
- **WHEN** the user activates an unchecked enabled column row
- **THEN** that row displays a check mark and the corresponding Details column becomes visible without closing or reopening the popup

#### Scenario: Disable a column
- **WHEN** the user activates a checked enabled column row
- **THEN** that row removes its check mark and the corresponding Details column becomes hidden without closing or reopening the popup

#### Scenario: Rapid ordered changes
- **WHEN** multiple persistent activations occur before one foreground repaint
- **THEN** the requested resulting states are applied in activation order without inversion or duplicate-toggle drift

### Requirement: Fixed and terminal commands
The popup SHALL reject visibility changes for the required `Name` column and SHALL close after a terminal command is successfully activated.

#### Scenario: Required Name column
- **WHEN** the popup displays the `Name` row
- **THEN** the row is checked and disabled and activation does not publish a visibility event

#### Scenario: Auto-size command
- **WHEN** the user activates auto-size-this-column or auto-size-all-columns
- **THEN** the command is applied once and the popup session closes

#### Scenario: Target-specific terminal command
- **WHEN** the user activates an available target-specific display command
- **THEN** the command is applied once and the popup session closes

### Requirement: Explicit dismissal
The persistent popup SHALL close without changing column visibility when dismissed by Escape, outside click, application deactivation, or a validated replacement gesture.

#### Scenario: Escape dismissal
- **WHEN** the user presses Escape after zero or more completed persistent activations
- **THEN** the popup closes and no additional visibility event is produced

#### Scenario: Outside dismissal
- **WHEN** the user clicks outside the popup after zero or more completed persistent activations
- **THEN** the popup closes and no additional visibility event is produced

### Requirement: Thread isolation and consistency failure
The popup worker MUST NOT directly borrow or update GPUI, and the popup SHALL terminate when it cannot publish or apply a persistent state without risking divergence.

#### Scenario: Foreground delivery
- **WHEN** a persistent state event is published successfully
- **THEN** `ExplorerRoot` applies its requested state on the foreground context and the popup worker performs no direct GPUI access

#### Scenario: Event bridge unavailable
- **WHEN** the persistent event bridge is full, disconnected, stale, or mapped to an invalid command index
- **THEN** the popup session terminates, releases its native resources, and does not continue accepting divergent visibility changes

#### Scenario: Stale session event
- **WHEN** an event arrives after its popup session or owning window has ended
- **THEN** the event is ignored and does not mutate the current Details layout

### Requirement: Popup lifecycle integrity
Persistent activation SHALL preserve the existing top-level positioning, work-area clamping, theme, fallback, and bounded resource-lifecycle behavior of the immersive popup.

#### Scenario: Small main window
- **WHEN** the Details popup opens from a deliberately small main window on a screen with sufficient work area
- **THEN** all column rows are materialized in the independent popup and are not clipped by the main window

#### Scenario: Repeated session cleanup
- **WHEN** persistent popup sessions are opened, used, and dismissed repeatedly
- **THEN** each session releases its HWND, HMENU, shadow, font, capture, and event-bridge resources within the existing lifecycle bounds
