## ADDED Requirements

### Requirement: Remote entry points and state
The navigation pane SHALL expose Android Devices and SFTP sections and the
address bar SHALL accept the supported remote URI forms. Connection, loading,
permission, offline, and failure states SHALL be distinguishable without
revealing credentials.

#### Scenario: Add SFTP profile
- **WHEN** the user invokes Add SFTP connection
- **THEN** the UI SHALL collect label, host, port, user, and password separately and SHALL not render the password after submission

#### Scenario: Expand an Android device
- **WHEN** the user expands an authorized device in the navigation pane
- **THEN** its children SHALL be the folders under `adb://<serial>/`, not only the folders under `/sdcard`

### Requirement: Remote folder bookmarks
The bookmark toolbar SHALL allow ADB and SFTP folders to be saved and reopened
using their public device serial or profile alias without persisting credentials.

#### Scenario: Reopen an ADB bookmark
- **WHEN** the user activates a bookmark saved for `adb://emulator-5554/sdcard/Android`
- **THEN** SuperExplorer SHALL navigate to that ADB folder without exposing the internal container identity or generation

### Requirement: Remote destructive-operation warning
The UI SHALL require explicit confirmation for remote delete and SHALL state
that remote deletion is permanent.

#### Scenario: Delete is cancelled
- **WHEN** the user dismisses the permanent-delete confirmation
- **THEN** no remote mutation request SHALL be sent
