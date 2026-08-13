## ADDED Requirements

### Requirement: Cache telemetry distinguishes delay from inability
The system SHALL represent pending or stale cache telemetry separately from a confirmed unavailable cache owner.

#### Scenario: First sample is pending
- **WHEN** Folder Options opens before a cache owner has returned its first usage sample
- **THEN** the UI SHALL display `— / configured limit` and SHALL NOT display `Unavailable`

#### Scenario: Refresh is delayed after a successful sample
- **WHEN** a cache owner previously returned usage and its next sample is pending or delayed
- **THEN** the UI SHALL retain the last successful `used / limit` presentation

#### Scenario: Owner is confirmed unavailable
- **WHEN** the owner boundary reports a missing or stopped service, incompatible or rejected protocol, or terminal connection failure
- **THEN** the UI SHALL display `Unavailable / configured limit`

### Requirement: Telemetry presentation recovers without reopening
The system SHALL update availability and usage in an already open Folder Options window as owner state changes.

#### Scenario: Pending owner returns a sample
- **WHEN** a pending cache owner returns a successful usage sample
- **THEN** the open UI SHALL replace the pending presentation with the newest `used / effective limit`

#### Scenario: Unavailable owner reconnects
- **WHEN** a confirmed unavailable owner enters retry and subsequently returns a valid sample
- **THEN** the UI SHALL leave `Unavailable`, pass through pending if necessary, and display the recovered sample without reopening Folder Options

### Requirement: Configuration remains usable while telemetry is pending
The system SHALL keep every cache budget editor enabled and SHALL update the displayed configured limit immediately even when usage telemetry is pending or stale.

#### Scenario: Limit changes while first sample is pending
- **WHEN** the user applies a new cache limit before the first usage sample arrives
- **THEN** the UI SHALL display `— / new configured limit` and the owner SHALL receive the configuration through the existing path

#### Scenario: Limit changes while a stale value is retained
- **WHEN** the user applies a new cache limit while the last successful usage value is retained
- **THEN** the UI SHALL preserve the retained usage value, update the displayed limit, and later replace usage with the next successful sample
