# remote-item-properties Specification

## ADDED Requirements

### Requirement: Remote Properties command opens useful content
For exactly one selected ADB or SFTP item with Unix mode metadata, the system SHALL open an application-owned Properties dialog showing name, type, remote location, size when available, timestamps when available, and the current four-digit permission mode.

#### Scenario: Single remote item
- **WHEN** the user invokes Properties for one eligible remote item
- **THEN** the dialog displays the captured item's metadata and permission controls

#### Scenario: Ineligible selection
- **WHEN** the selection contains multiple items or lacks Unix mode metadata
- **THEN** no empty or non-functional Properties dialog is opened

### Requirement: Permission edits use provider-native mutation
The system SHALL allow editing owner, group, and other read/write/execute bits and SHALL submit a permission-only mode through the resolved remote provider.

#### Scenario: ADB apply
- **WHEN** permission changes are applied to an ADB item
- **THEN** ADB invokes chmod with a validated four-digit octal mode and safely quoted path argument

#### Scenario: SFTP apply
- **WHEN** permission changes are applied to an SFTP item
- **THEN** SFTP sends SETSTAT with only the permissions field populated

#### Scenario: Invalid mode
- **WHEN** a mode contains bits outside `07777`
- **THEN** the provider rejects the mutation without issuing a remote write

#### Scenario: Provider failure
- **WHEN** the remote server rejects the change
- **THEN** the existing operation error surface reports failure and the application does not report success

### Requirement: Local Properties remains native
The system SHALL continue routing local file Properties to the Windows Shell.

#### Scenario: Local item
- **WHEN** Properties is invoked for a local file
- **THEN** the existing native Shell Properties path is used
