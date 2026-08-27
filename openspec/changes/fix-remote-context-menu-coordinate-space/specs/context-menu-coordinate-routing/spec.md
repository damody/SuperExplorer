## ADDED Requirements

### Requirement: Context menu anchors preserve coordinate spaces
The UI MUST preserve both the window-client anchor and Windows screen anchor from the original mouse or keyboard invocation without treating one coordinate space as the other.

#### Scenario: Mouse invocation creates two anchors
- **WHEN** the user right-clicks a file-view row or background
- **THEN** the action records the original client point and the corresponding screen point

#### Scenario: Keyboard invocation creates two anchors
- **WHEN** the user invokes Menu or Shift+F10 on the focused row
- **THEN** the action derives both anchors from that row's client position

### Requirement: Menu implementation selects its native coordinate space
The UI MUST position GPUI custom ADB/SFTP menus using the client anchor and MUST submit Local Windows Shell menus using the screen anchor.

#### Scenario: Remote custom menu
- **WHEN** the current writable provider is ADB or SFTP
- **THEN** the custom menu is anchored near the invocation's client point

#### Scenario: Local native menu
- **WHEN** the target is a Local Shell location
- **THEN** the Windows Shell request receives the invocation's screen point unchanged

#### Scenario: Unsupported virtual provider
- **WHEN** the target is a virtual provider without the remote-menu capability
- **THEN** the system fails closed without displaying the ADB/SFTP custom menu

### Requirement: Custom menu remains inside the window
The custom remote menu MUST clamp its final client position so the menu remains visible within the current window.

#### Scenario: Invocation near right or bottom edge
- **WHEN** the client anchor leaves insufficient room to the right or below
- **THEN** the menu moves left or upward only as far as required to remain visible

#### Scenario: Negative client input
- **WHEN** a malformed or stale client anchor is negative
- **THEN** the menu clamps the corresponding coordinate to zero

### Requirement: Remote custom menu has an explicit lifetime
The UI MUST keep an open ADB/SFTP custom menu visible during pointer movement and MUST close it only for an explicit overlay dismissal, Escape, replacement invocation, or accepted menu command.

#### Scenario: Pointer moves after menu opens
- **WHEN** an ADB/SFTP custom menu is open and hover or pointer-tracking actions occur
- **THEN** the menu remains open at its existing anchor

#### Scenario: Explicit dismissal
- **WHEN** the user clicks outside the menu or presses Escape
- **THEN** the remote menu closes without executing a file command

#### Scenario: Menu command executes
- **WHEN** the user activates a command in the remote menu
- **THEN** the menu closes and that command continues through its existing action route

### Requirement: Remote background menu covers the file viewport
The UI MUST treat every non-row point inside the file-view viewport below its chrome/header origin as a Background context-menu target, including empty space below a short directory listing.

#### Scenario: Empty space below the last row
- **WHEN** the user right-clicks inside the file-view viewport below the final ADB/SFTP row
- **THEN** the remote Background menu opens with create-folder and paste commands

#### Scenario: Point outside the file viewport
- **WHEN** a right-click occurs in toolbar, navigation, Details header, or outside the file-view viewport
- **THEN** the file-view Background handler does not open a remote menu

#### Scenario: File row owns the invocation
- **WHEN** the user right-clicks an ADB/SFTP row
- **THEN** the row opens the Items menu and prevents a duplicate Background invocation
