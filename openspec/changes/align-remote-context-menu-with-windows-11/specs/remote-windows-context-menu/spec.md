## ADDED Requirements

### Requirement: Remote item menus use the Windows 11 command hierarchy
The system SHALL render ADB and SFTP item context menus with a Windows 11-style icon command strip, grouped text-command section, separators, rounded menu surface, semantic border, and shadow.

#### Scenario: Open a writable remote item menu
- **WHEN** the user right-clicks an item in a writable ADB or SFTP directory
- **THEN** the menu displays Cut, Copy, Rename, and permanent Delete in the icon command strip and Open in the text-command section

#### Scenario: Paste is available for an item context
- **WHEN** the user opens a remote item menu while compatible clipboard content can be pasted into the current directory
- **THEN** the menu displays Paste as an applicable text command without changing the item command strip

### Requirement: Remote background menus share the Windows 11 visual contract
The system SHALL render ADB and SFTP directory-background menus with the same surface, text row, icon slot, separator, and interaction-state contract as remote item menus while omitting item-only commands.

#### Scenario: Open a writable remote background menu
- **WHEN** the user right-clicks the empty background of a writable ADB or SFTP directory
- **THEN** the menu displays New folder and does not display Open, Cut, Copy, Rename, or Delete

#### Scenario: Paste is available on a remote background
- **WHEN** compatible clipboard content is available and the current remote directory accepts writes
- **THEN** the background menu displays Paste and dispatches the existing remote Paste action when activated

### Requirement: Remote menus expose complete interaction states
The system MUST render commands using the active semantic theme and expose hover, pressed, disabled, keyboard-focus, danger, and accessible-label states in light, dark, and high-contrast modes.

#### Scenario: Pointer moves across commands
- **WHEN** the pointer enters and presses an enabled remote menu command
- **THEN** the command displays the semantic hover and pressed surfaces and the menu remains open until activation or explicit dismissal

#### Scenario: Destructive command is presented
- **WHEN** permanent Delete is present in an item menu
- **THEN** it uses the semantic danger treatment and retains the existing confirmation and permanent-delete behavior

#### Scenario: High-contrast mode is active
- **WHEN** a remote context menu opens while Windows high-contrast mode is active
- **THEN** its surface, text, border, focus, and command states derive from the application's high-contrast semantic mappings

### Requirement: Remote menu commands preserve provider-aware behavior
The system SHALL dispatch existing `ExplorerAction` values and SHALL hide or disable commands that the active ADB or SFTP presentation cannot execute.

#### Scenario: Activate a supported command
- **WHEN** the user activates Open, Cut, Copy, Paste, Rename, New folder, or permanent Delete from a remote menu
- **THEN** the existing provider-aware action pipeline receives the action and existing detailed operation success or failure reporting remains authoritative

#### Scenario: Command is not supported
- **WHEN** the active remote location cannot execute a candidate command
- **THEN** the menu does not offer that command as an enabled action

### Requirement: Remote menus remain visible and dismiss predictably
The system SHALL position the menu within client bounds, keep it open while interacting inside it, and dismiss it on outside click, Escape, or a completed dismissing command.

#### Scenario: Menu opens near a window edge
- **WHEN** the requested context-menu anchor would place part of the revised menu outside the client area
- **THEN** the menu position is clamped so the full contracted menu surface remains inside the available client bounds

#### Scenario: Pointer interacts inside the menu
- **WHEN** the user moves or presses the pointer inside the remote menu surface
- **THEN** the overlay does not interpret that interaction as an outside dismissal

#### Scenario: User dismisses the menu
- **WHEN** the user clicks outside the menu or presses Escape
- **THEN** the remote menu closes without dispatching a file operation

### Requirement: Local Shell menus remain unchanged
The system MUST continue to use the existing native Windows Shell context-menu path for local filesystem items.

#### Scenario: Open a local item menu
- **WHEN** the user right-clicks a local filesystem item
- **THEN** the native Windows Shell context menu opens and the remote GPUI menu renderer is not used
