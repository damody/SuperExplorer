## ADDED Requirements

### Requirement: MFT Service is the single Details aggregate owner
The system SHALL route Details Folder size, File Count, and Folder Count queries directly to the installed MFT Service, and the Host SHALL NOT answer those queries from a long-lived Host aggregate cache or recursive filesystem fallback.

#### Scenario: Two application processes query one folder
- **WHEN** two Super Explorer processes request Details aggregate facts for the same NTFS folder
- **THEN** both requests SHALL use the same service-owned indexes and result cache
- **AND** neither process SHALL populate a private long-lived aggregate result cache

#### Scenario: Host cache contains an obsolete matching snapshot
- **WHEN** an obsolete Host snapshot exists for a requested Details folder
- **THEN** the Host SHALL ignore that snapshot and query the MFT Service

#### Scenario: MFT Service is unavailable
- **WHEN** a Details aggregate request cannot reach the MFT Service
- **THEN** the Host SHALL publish an explicit unavailable terminal result
- **AND** SHALL NOT recursively scan the requested folder as a fallback

#### Scenario: Size Map requests a tree projection
- **WHEN** Size Map requests bounded hierarchy data
- **THEN** its short-lived projection cache SHALL remain permitted for Size Map tree projection
- **AND** that projection cache SHALL NOT intercept or answer Details aggregate requests

### Requirement: Shared result cache implements bounded true LRU
The MFT Service SHALL retain folder aggregate results in one process-global true LRU that promotes every successful hit and replacement and enforces independent byte and entry-count limits.

#### Scenario: Older entry is read before pressure
- **WHEN** entry A is older than entry B, entry A is successfully read, and inserting entry C requires one eviction
- **THEN** entry B SHALL be evicted
- **AND** entry A SHALL remain cached

#### Scenario: Configured byte limit is lowered
- **WHEN** the effective MFT Service LRU byte limit is reduced below current accounted usage
- **THEN** the service SHALL immediately evict least-recently-used entries until accounted usage is within the new byte limit
- **AND** SHALL NOT trim SQLite, volume index, file data, or aggregate index stores as part of result eviction

#### Scenario: Entry ceiling is reached first
- **WHEN** retained entry count reaches `max(1, min(effective_lru_bytes / 192, 262144))` before the byte budget is exhausted
- **THEN** the next retained result SHALL evict the least-recently-used entry until count is within the ceiling

#### Scenario: One result cannot fit
- **WHEN** one computed result exceeds the effective retention budget
- **THEN** the service SHALL return it to current waiters without retaining it in the result LRU

#### Scenario: Result cost is accounted
- **WHEN** the service admits a result
- **THEN** its accounted cost SHALL include key, value, recency metadata, and bounded container overhead
- **AND** SHALL be at least 192 bytes

### Requirement: Journal invalidation preserves only proven results
The MFT Service SHALL invalidate changed folders and their known ancestors before advancing the volume result-cache generation, preserve provably unaffected results, and reject every result not proven valid through the current generation.

#### Scenario: File change has a complete ancestor closure
- **WHEN** a journal change identifies a folder and its complete ancestor chain
- **THEN** cached results for those references SHALL be removed before generation advance
- **AND** unaffected cached folders SHALL remain eligible for a current-generation hit

#### Scenario: Ancestor closure cannot be proven
- **WHEN** journal processing cannot prove the complete affected reference set
- **THEN** all result entries for that volume SHALL be cleared before generation advance

#### Scenario: Old computation finishes after generation advance
- **WHEN** a generation-bound computation completes after its volume cache generation changed
- **THEN** its result SHALL NOT enter or replace an entry in the current result LRU
- **AND** SHALL NOT be presented as a current exact value

### Requirement: Concurrent misses are service-global single flight
The MFT Service SHALL coalesce concurrent requests for the same volume, folder reference, and observed generation into one aggregate computation without blocking unrelated keys.

#### Scenario: Distinct clients miss the same key
- **WHEN** two connected clients concurrently miss the same folder and generation
- **THEN** exactly one requester SHALL lead aggregate computation
- **AND** both requesters SHALL receive the same terminal result

#### Scenario: Clients miss different folders
- **WHEN** clients concurrently miss different folder keys
- **THEN** one key's single-flight state SHALL NOT require the other key to wait for its computation to finish

#### Scenario: Leader fails
- **WHEN** the single-flight leader returns an error, disconnects, or is rejected as stale
- **THEN** every joined waiter SHALL be terminalized
- **AND** the in-flight registry entry SHALL be removed

### Requirement: Service source order is optimized and restart-safe
On a result miss, the MFT Service SHALL prefer a valid memory aggregate index, then a read-only SQLite aggregate query when durable state matches the current journal, and SHALL build missing bounded aggregate state only when required.

#### Scenario: Service restarts with current SQLite index
- **WHEN** the service result LRU is empty after restart and SQLite matches the current journal
- **THEN** the first query SHALL be answered through the SQLite optimized path without reading user file contents
- **AND** a subsequent same-folder query SHALL be eligible for a result-LRU hit

#### Scenario: Memory aggregate is available
- **WHEN** a valid memory aggregate contains the requested reference
- **THEN** the service SHALL answer from that aggregate before issuing a SQLite aggregate query or aggregate build

### Requirement: Visible requests terminate without stale publication
Every non-obsolete visible Details aggregate request SHALL leave its loading state with an exact, typed partial, or explicit unavailable result; cancelled or superseded work SHALL NOT publish into the current view.

#### Scenario: Service returns complete facts
- **WHEN** a current request receives a complete response with a matching generation
- **THEN** the UI SHALL display exact Folder size and dependent directory facts
- **AND** SHALL remove `Calculating...`

#### Scenario: Service returns typed partial
- **WHEN** a current request receives a typed partial response
- **THEN** the UI SHALL use the existing visible partial presentation
- **AND** SHALL remove `Calculating...`

#### Scenario: Response deadline elapses
- **WHEN** a current aggregate request reaches the configured response deadline
- **THEN** the UI SHALL display an explicit unavailable terminal result
- **AND** SHALL allow a later refresh or service recovery to retry

#### Scenario: Old tab generation completes
- **WHEN** a cancelled or superseded tab generation receives a late response
- **THEN** the Host SHALL discard the response
- **AND** SHALL NOT overwrite the current view

### Requirement: Obsolete Host snapshot cleanup is bounded and safe
The system SHALL stop reading and writing the obsolete Host Details snapshot namespace and SHALL retire it through bounded startup maintenance confined to the validated application cache directory.

#### Scenario: Obsolete regular files exist
- **WHEN** startup finds validated obsolete regular cache files directly under `%LOCALAPPDATA%\SuperExplorer\folder-snapshot-cache`
- **THEN** it SHALL remove no more than 256 files in oldest-first order during that launch

#### Scenario: Cache root contains unsafe or unexpected entries
- **WHEN** the obsolete cache root contains a symlink, reparse point, directory, unexpected record, or target outside the exact validated root
- **THEN** maintenance SHALL skip that entry and SHALL NOT follow or remove it

#### Scenario: Cleanup fails
- **WHEN** an obsolete cache file cannot be removed
- **THEN** startup and direct MFT aggregate queries SHALL continue

#### Scenario: Source folders are present
- **WHEN** obsolete-cache maintenance runs while `D:\trace` or any other source folder exists
- **THEN** no source folder content SHALL be modified

### Requirement: Shared behavior is observable without path disclosure
MFT diagnostics SHALL expose privacy-safe result-LRU, single-flight, invalidation, and source-selection counters through a versioned backward-safe contract.

#### Scenario: Warm query is shared
- **WHEN** a second Super Explorer process queries a result warmed by the first process
- **THEN** service-global hit counters SHALL increase
- **AND** diagnostics SHALL report shared result entry and byte usage without a path or file name

#### Scenario: Unsupported diagnostic frame version is received
- **WHEN** either endpoint receives a diagnostic frame version it does not support
- **THEN** it SHALL reject the frame without shifting or misparsing existing fields

### Requirement: Code Lines admission remains unchanged
The change SHALL NOT alter Code Lines File Count admission limits or the existing red `Limit` presentation.

#### Scenario: Folder exceeds Code Lines File Count admission
- **WHEN** a folder exceeds the existing Code Lines File Count admission limit after its MFT facts become available
- **THEN** Main code lines and Code lines SHALL retain their existing red `Limit` behavior
