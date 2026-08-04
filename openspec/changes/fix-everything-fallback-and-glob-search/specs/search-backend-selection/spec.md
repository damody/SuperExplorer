## ADDED Requirements

### Requirement: Successful Everything queries are authoritative
The application SHALL complete a search through Everything whenever the SDK query and result extraction succeed, regardless of the number of results.

#### Scenario: Everything returns matching entries
- **WHEN** Everything completes successfully with one or more scoped entries
- **THEN** the application publishes those entries and one finished terminal without starting a fallback backend

#### Scenario: Everything returns zero entries
- **WHEN** Everything completes successfully with zero scoped entries
- **THEN** the application publishes an Everything complete status and one finished terminal without starting a fallback backend

### Requirement: Fallback has an explicit failure boundary
The application SHALL start LocalIndex or filesystem fallback only when the Everything DLL, ABI, current-session IPC/database, timeout, or query operation is unavailable or fails.

#### Scenario: Current-session IPC is unavailable
- **WHEN** the SDK DLL loads but the Everything IPC database does not respond
- **THEN** the application reports Everything unavailable and starts the configured local fallback for the same request

#### Scenario: Everything fails after publishing a batch
- **WHEN** Everything publishes zero or more batches and then reports an IPC or query failure
- **THEN** the application starts fallback, suppresses duplicate identities or paths, and emits exactly one eventual terminal event

### Requirement: Cancellation does not trigger fallback
The application MUST terminate a cancelled Everything request without starting another provider for that request.

#### Scenario: User cancels an active Everything query
- **WHEN** the correlated cancellation token is set before successful completion
- **THEN** no fallback starts, no later result batch is emitted, and exactly one cancelled terminal is produced
