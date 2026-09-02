## ADDED Requirements

### Requirement: Cancel reaches the active remote worker
`RemoteExplorerService` MUST cancel the same token used by an active ADB/SFTP operation and MUST remove the request from its active registry on every terminal path.

#### Scenario: Active remote transfer is cancelled
- **WHEN** Cancel names an active remote transfer request
- **THEN** its token is cancelled immediately
- **AND** the provider stops at its earliest safe boundary and publishes one Cancelled terminal

#### Scenario: Cancel names a non-remote request
- **WHEN** the remote registry does not contain the request
- **THEN** Cancel is delegated to the inner Shell service

#### Scenario: Remote operation terminates
- **WHEN** a remote worker finishes, fails, panics, or is cancelled
- **THEN** its active registry entry is removed
- **AND** late progress cannot mutate the terminal record

### Requirement: Progress follows Fluent visual structure
The active operation progress region SHALL contain a rounded neutral track and rounded accent progress presentation separated from the summary text.

#### Scenario: Determinate transfer renders
- **WHEN** total bytes are known
- **THEN** a rounded accent fill represents the real ratio inside the rounded track

#### Scenario: Indeterminate transfer renders
- **WHEN** total bytes are unknown
- **THEN** a shorter rounded accent segment is shown inside the track instead of tinting the full width

#### Scenario: Transfer is cancelled
- **WHEN** the operation terminal is Cancelled
- **THEN** the visual preserves the last real ratio and does not fill to 100%
