## ADDED Requirements

### Requirement: Active filesystem identity is closed and fail-closed
The system SHALL classify ordinary path-backed filesystem locations as `local`, ADB virtual locations as `adb`, and SFTP virtual locations as `sftp`. Unknown virtual providers SHALL match none of these identities and SHALL NOT inherit a declared scope.

#### Scenario: Known remote provider is classified
- **WHEN** the active location belongs to the ADB or SFTP provider
- **THEN** the effective filesystem identity is respectively `adb` or `sftp`

#### Scenario: Unknown virtual provider is opened
- **WHEN** the active location belongs to a virtual provider other than ADB or SFTP
- **THEN** no supported filesystem identity is assigned and scoped extension columns remain inapplicable

### Requirement: Details uses one non-destructive applicability projection
The system SHALL derive header, row, chooser, filter-menu, auto-size, drag-target, and sorting availability from one intersection of persisted column layout and active-filesystem applicability. Projection SHALL NOT mutate persisted visibility, order, width, or sort state.

#### Scenario: Local-only columns disappear remotely
- **WHEN** a persisted layout containing local-only extension columns is displayed at an ADB or SFTP location
- **THEN** those columns are absent from every Details presentation and interaction surface while saved layout state remains unchanged

#### Scenario: Compatible location returns
- **WHEN** the user returns from a remote location to Local
- **THEN** previously configured local columns restore their saved visibility, order, and width without manual reconfiguration

#### Scenario: Header and rows use the same projection
- **WHEN** a Details view contains a mix of applicable and inapplicable descriptors
- **THEN** every rendered row has exactly one aligned cell for each rendered header in the same descriptor order

### Requirement: Inapplicable sorting falls back without persistence loss
When the persisted sort column is inapplicable to the active filesystem, the system SHALL use Name ascending as the effective sort without overwriting the persisted sort descriptor.

#### Scenario: Remote navigation hides saved sort column
- **WHEN** a Local-only column is the persisted sort and the user opens ADB or SFTP
- **THEN** the remote listing is effectively sorted by Name ascending and the saved Local sort remains intact

#### Scenario: Saved sort becomes applicable again
- **WHEN** the user returns to a filesystem on which the persisted sort column applies
- **THEN** the original sort descriptor becomes effective again

### Requirement: Built-in applicability matches available filesystem semantics
The Host SHALL keep Name, Date modified, Type, and Size applicable to ADB and SFTP when provider metadata supports them. It SHALL exclude Windows Shell, MFT, content-analysis, and local-process columns from ADB and SFTP. Permissions SHALL apply to ADB and SFTP and SHALL be excluded from Local Windows locations.

#### Scenario: Remote Details chooser opens
- **WHEN** the user opens the column chooser on ADB or SFTP
- **THEN** unsupported Local built-ins and Local-only extensions are absent while supported ordinary columns and Permissions are available

### Requirement: Inapplicable extension work is suppressed before input preparation
The Host SHALL reject a data-column contribution as inapplicable before scheduling work, collecting entry payloads, reading files, preparing streams, or invoking Rust/Lua callbacks.

#### Scenario: Local-only content column is visible in saved layout
- **WHEN** ADB or SFTP becomes active while a Local-only content column is saved as visible
- **THEN** no job, local read, remote payload preparation, or extension callback starts for that column

#### Scenario: Stale applicable result arrives after navigation
- **WHEN** an applicable extension job started on one filesystem and its result arrives after navigation changes the generation or applicability
- **THEN** existing generation checks discard it and it does not populate the current view
