## ADDED Requirements

### Requirement: ADB device discovery and path navigation
The system SHALL discover authorized ADB devices and browse their accessible
filesystem through direct device-root and nested `adb://` locations.

#### Scenario: Phone storage path
- **WHEN** the user opens `adb://<authorized-serial>/sdcard/Download`
- **THEN** SuperExplorer SHALL list that accessible Android directory in the current tab

### Requirement: ADB file mutations and transfers
The system SHALL support create folder, rename, permanent delete, and streamed
copy for accessible ADB paths, including Local↔ADB and ADB↔SFTP transfers.

#### Scenario: Unauthorized device
- **WHEN** an attached device is unauthorized or offline
- **THEN** the UI SHALL not expose its filesystem and SHALL display its state without executing mutations
