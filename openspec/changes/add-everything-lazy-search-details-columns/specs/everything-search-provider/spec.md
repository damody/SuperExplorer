## ADDED Requirements

### Requirement: Everything capability detection
The application SHALL detect a usable current-session Everything IPC endpoint through the bundled official SDK and SHALL NOT treat service registration alone as availability.

#### Scenario: Everything IPC is usable
- **WHEN** the required SDK exports load from an owned path and the Everything IPC database responds
- **THEN** the application selects the Everything backend for the search

#### Scenario: Service exists without client IPC
- **WHEN** an Everything Windows service is installed but no current-session IPC endpoint responds
- **THEN** the application selects the local SQLite backend without starting another process

#### Scenario: SDK is absent or incompatible
- **WHEN** the SDK DLL is missing, has the wrong architecture, or lacks a required export
- **THEN** application startup remains successful and the backend is reported unavailable

### Requirement: Scoped bounded Everything queries
Every Everything query MUST be constrained to the canonical active folder scope, escape user and path syntax, request only owned result fields, and deliver bounded batches.

#### Scenario: Folder-scoped query
- **WHEN** a query runs for `C:\foo`
- **THEN** no result outside `C:\foo` and its descendants is emitted

#### Scenario: Query contains operators or quotes
- **WHEN** user text or the folder path contains Everything syntax characters
- **THEN** the adapter treats those values as literals except for operators generated from the parsed expression

#### Scenario: Large result set
- **WHEN** Everything reports more results than one configured page
- **THEN** the adapter pages and emits bounded batches without allocating the entire result set at once

### Requirement: Everything cancellation and failover
The Everything adapter SHALL observe the correlated cancellation token and SHALL fail over recoverably to the local index when IPC becomes unavailable.

#### Scenario: Search is cancelled
- **WHEN** the search is cleared, replaced, navigated away, its tab closes, or shutdown begins
- **THEN** no further Everything result batch is emitted for that request and one cancelled terminal outcome is produced

#### Scenario: IPC fails during query
- **WHEN** Everything stops responding after zero or more batches
- **THEN** the application retains accepted results, publishes a truthful source status, and continues the same scope through the local index without duplicate identities

### Requirement: Everything provider privacy and packaging
The application MUST load the SDK only from canonical application-owned locations and MUST NOT log query text or private full result paths.

#### Scenario: Malicious DLL exists in working directory
- **WHEN** an unrelated `Everything64.dll` is present in the current working directory
- **THEN** the adapter does not load that DLL

#### Scenario: Diagnostic event is recorded
- **WHEN** capability detection or a query succeeds or fails
- **THEN** diagnostics include backend category, counts, and safe error codes but exclude query text and full private paths

### Requirement: Empty search text restores directory state
The search editor SHALL treat an empty or whitespace-only value as cancellation of the active search and SHALL restore the owned directory snapshot without requiring Escape or the clear button.

#### Scenario: User deletes the complete query
- **WHEN** an active search is visible and the user removes every character from the search editor
- **THEN** the correlated search generation is cancelled, late results are rejected, and the original current-folder items become visible immediately
