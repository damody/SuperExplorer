## ADDED Requirements

### Requirement: Remote provider dispatch
The host SHALL dispatch supported `adb` and `sftp` virtual locations to their
remote providers while preserving virtual-location validation, generation
invalidation, bounded streaming, and cancellation semantics.

#### Scenario: Remote location reaches Shell boundary
- **WHEN** a remote virtual location is navigated or mutated
- **THEN** it SHALL be handled by the remote runtime and SHALL not be passed to a Windows Shell path API
