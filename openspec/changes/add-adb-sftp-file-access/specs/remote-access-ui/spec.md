## ADDED Requirements

### Requirement: Remote entry points and state
The navigation pane SHALL expose Android Devices and SFTP sections and the
address bar SHALL accept the supported remote URI forms. Connection, loading,
permission, offline, and failure states SHALL be distinguishable without
revealing credentials.

#### Scenario: Add SFTP profile
- **WHEN** the user invokes Add SFTP connection
- **THEN** the UI SHALL collect label, host, port, user, and password separately and SHALL not render the password after submission

### Requirement: Remote destructive-operation warning
The UI SHALL require explicit confirmation for remote delete and SHALL state
that remote deletion is permanent.

#### Scenario: Delete is cancelled
- **WHEN** the user dismisses the permanent-delete confirmation
- **THEN** no remote mutation request SHALL be sent
