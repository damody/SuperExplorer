## ADDED Requirements

### Requirement: Versioned MFT budget configuration
The MFT Service SHALL accept a versioned `SetCacheBudgets` IPC request for persisted index, volume index memory, file data memory, folder aggregates memory, and result LRU limits and SHALL return normalized effective values.

#### Scenario: Client applies 2048 MB LRU
- **WHEN** a connected client sends a valid request with an MFT Service LRU of 2048 MB
- **THEN** the service SHALL acknowledge 2048 MB and diagnostics SHALL report 2048 MB without requiring a folder-size query or navigation

#### Scenario: Service is unavailable
- **WHEN** configuration cannot reach the service
- **THEN** settings SHALL remain persisted, telemetry SHALL mark service limits unavailable or pending, and the Host SHALL retry the latest snapshot after reconnect

#### Scenario: Endpoint version is older
- **WHEN** either endpoint does not support the configuration request version
- **THEN** it SHALL fail safely without misparsing a folder query or reporting the new limits as applied

### Requirement: Independent MFT hard budgets
The service SHALL independently account for and trim persisted index, volume index, file data, folder aggregates, and result LRU storage according to their configured budgets.

#### Scenario: Individual structure exceeds its limit
- **WHEN** one structure exceeds its effective budget
- **THEN** the service SHALL trim only that structure using oldest/LRU policy until accounting is within budget, subject only to one indivisible record larger than the budget

#### Scenario: Four memory maxima are configured
- **WHEN** volume index, file data, folder aggregates, or result LRU limits are edited
- **THEN** each SHALL accept values from its approved minimum through 16384 MB independently

#### Scenario: Persisted store is pruned
- **WHEN** persisted MFT records exceed their disk budget
- **THEN** pruning SHALL replace the index atomically and SHALL NOT modify source filesystem content

### Requirement: Partial result correctness
Any query whose required MFT data was removed by independent trimming SHALL return a typed partial result and SHALL NOT be presented as exact.

#### Scenario: Aggregate records were trimmed
- **WHEN** a folder query depends on trimmed aggregate records
- **THEN** Details and Size Map SHALL visibly label the known value `Partial` and numeric sorting SHALL retain incomplete status

#### Scenario: Limit is raised after trimming
- **WHEN** a user raises a budget after records were removed
- **THEN** the service SHALL retain partial status until journal processing or a rebuild proves the required data complete

### Requirement: MFT budget observability
Diagnostics SHALL report current usage and effective maximum independently for all five MFT budget categories.

#### Scenario: Configuration is acknowledged
- **WHEN** the service applies a budget snapshot
- **THEN** the next telemetry sample SHALL expose each acknowledged effective maximum with its corresponding current usage
