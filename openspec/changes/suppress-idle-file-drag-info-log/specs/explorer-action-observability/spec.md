## ADDED Requirements

### Requirement: High-frequency file-drag pointer telemetry stays out of normal logs
The explorer action dispatcher SHALL emit every `UpdateFileDrag` dispatch record at TRACE rather than INFO, regardless of whether the action is handled or disabled.

#### Scenario: Idle pointer hover
- **WHEN** pointer movement over a file row dispatches `UpdateFileDrag` without an active drag candidate
- **THEN** the dispatcher returns the normal disabled action trace
- **AND** no INFO-level `UpdateFileDrag` dispatch record is emitted
- **AND** the record remains available at TRACE

#### Scenario: Active drag pointer movement
- **WHEN** pointer movement dispatches `UpdateFileDrag` during an active drag gesture
- **THEN** the dispatcher preserves the normal handled action behavior
- **AND** the per-movement dispatch record is emitted at TRACE rather than INFO

### Requirement: Meaningful explorer actions remain in normal logs
The explorer action dispatcher SHALL continue to emit non-`UpdateFileDrag` action dispatch records at INFO.

#### Scenario: Drag lifecycle boundary
- **WHEN** the dispatcher processes a drag lifecycle action such as `BeginFileDrag` or `CancelFileDrag`
- **THEN** it emits the dispatch record at INFO

#### Scenario: Ordinary explorer action
- **WHEN** the dispatcher processes an ordinary non-pointer action
- **THEN** it emits the dispatch record at INFO
