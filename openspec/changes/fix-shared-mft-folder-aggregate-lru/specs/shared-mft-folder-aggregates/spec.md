## ADDED Requirements

### Requirement: Folder aggregate batches stream in completion order
The Host SHALL submit bounded groups of visible folder aggregate requests through one batch IPC exchange, and the MFT Service SHALL return each exact result or typed failure as soon as that item terminates without waiting for all other batch items.

#### Scenario: Fast folder completes before slow sibling
- **WHEN** two distinct folders are submitted in one batch and the second folder computation completes first
- **THEN** the service SHALL emit the second folder's response first with its original request ID
- **AND** the Host SHALL publish that row without waiting for the first folder

#### Scenario: Duplicate folders across clients
- **WHEN** two batches request the same volume identity, folder reference, and observed generation concurrently
- **THEN** the service SHALL execute one generation-bound computation
- **AND** each request ID SHALL receive its own terminal response

#### Scenario: One item fails
- **WHEN** one batch item has a terminal service error while another item succeeds
- **THEN** the failed item SHALL return its bounded detailed reason and render `Unavailable`
- **AND** the successful sibling SHALL still return and publish its exact aggregate

### Requirement: Batch work is bounded and cancellable
The protocol SHALL accept no more than 256 items per batch, the service SHALL run no more than four independent aggregate computations per volume, and the Host SHALL associate one active batch with one current view generation.

#### Scenario: Navigation cancels an unfinished batch
- **WHEN** navigation or refresh changes the view generation while batch items are unfinished
- **THEN** the Host SHALL reject all later responses from the obsolete generation
- **AND** the service SHALL clean up connection-local work without poisoning shared recovery, single-flight, or LRU state

#### Scenario: Oversized or malformed batch
- **WHEN** a frame exceeds the item or byte bound, repeats a request ID, or contains an invalid item length
- **THEN** the service SHALL reject the batch before scheduling folder work
- **AND** it SHALL NOT reinterpret the frame as a legacy single-folder request

### Requirement: Foreground exact recovery precedes persistence
For an actively queried volume that requires rebuild, the service SHALL treat the normal `ERROR_HANDLE_EOF` enumeration terminator as successful completion and SHALL publish a fully checked exact in-memory index before rebuilding its SQLite accelerator.

#### Scenario: Exact rebuild fits the configured budgets
- **WHEN** the complete NTFS enumeration reaches `ERROR_HANDLE_EOF`, the result fits both live budgets, and journal catch-up remains valid
- **THEN** waiting batch items SHALL be released against the exact in-memory index
- **AND** SQLite persistence latency SHALL NOT delay those item responses

#### Scenario: Non-EOF enumeration error
- **WHEN** NTFS enumeration returns an error other than `ERROR_HANDLE_EOF`
- **THEN** the service SHALL return an I/O-stage failure containing the scan cursor and record count
- **AND** it SHALL NOT label the failure as a live-budget overflow

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

### Requirement: Active-volume paging recovers exactness within the bounded request
The MFT Service SHALL treat global volume-index and file-data limits as an active-volume working set, SHALL page non-active in-memory volume indexes out while retaining their durable SQLite stores, and SHALL attempt to recover the queried volume exactly before returning an unavailable result.

#### Scenario: Target was trimmed because another volume consumed the budget
- **WHEN** a folder query targets an incomplete volume whose complete index can fit the configured limits by releasing non-active in-memory indexes
- **THEN** the service SHALL make the target the active volume and perform one generation-bound exact recovery
- **AND** joined folder queries SHALL wait for that recovery rather than receive an immediate partial result

#### Scenario: Durable target is behind the current journal
- **WHEN** the active target has a complete durable SQLite snapshot whose cursor is behind the current journal
- **THEN** the service SHALL attempt bounded journal catch-up before rebuilding from NTFS metadata
- **AND** SHALL publish only after revalidating the current observed generation

#### Scenario: Active volume changes
- **WHEN** navigation changes the active volume after a previous volume was recovered
- **THEN** the service MAY release the previous volume's in-memory index
- **AND** SHALL retain its durable SQLite store for a later optimized reload

#### Scenario: Target alone exceeds its configured budget
- **WHEN** the measured complete target volume index or file data cannot fit its corresponding configured limit after non-active memory is released
- **THEN** the service SHALL return a terminal exactness failure rather than partial numeric facts
- **AND** the console diagnostic SHALL include measured and configured bytes

#### Scenario: Paging occurs under memory pressure
- **WHEN** an active-volume recovery replaces incomplete and non-active indexes
- **THEN** scratch reservations plus admitted live indexes SHALL remain within the configured hard peak limits

### Requirement: Visible requests terminate without stale publication
Every non-obsolete visible Details aggregate request SHALL leave its loading state within ten seconds with an exact or explicit unavailable result; partial numeric results SHALL NOT be displayed or used for sorting, and cancelled or superseded work SHALL NOT publish into the current view.

#### Scenario: Service returns complete facts
- **WHEN** a current request receives a complete response with a matching generation
- **THEN** the UI SHALL display exact Folder size and dependent directory facts
- **AND** SHALL remove `Calculating...`

#### Scenario: Service returns typed partial
- **WHEN** a current request receives a typed partial response
- **THEN** the UI SHALL discard all partial numeric fields and display `Unavailable`
- **AND** SHALL remove `Calculating...`

#### Scenario: Current service encounters an incomplete source
- **WHEN** the current service can still attempt active-volume exactness recovery within the request deadline
- **THEN** it SHALL keep the request in progress rather than publish a typed partial response

#### Scenario: Response deadline elapses
- **WHEN** a current aggregate request reaches the configured response deadline
- **THEN** the UI SHALL display `Unavailable` no later than ten seconds after submission
- **AND** SHALL allow a later refresh or service recovery to retry

#### Scenario: Old tab generation completes
- **WHEN** a cancelled or superseded tab generation receives a late response
- **THEN** the Host SHALL discard the response
- **AND** SHALL NOT overwrite the current view

#### Scenario: Earlier visible folder is slow
- **WHEN** one visible folder query remains in flight while later visible folders can complete
- **THEN** bounded Host scheduling SHALL allow the later queries to proceed without waiting for the earlier query to terminate

#### Scenario: Required installed locations are visited
- **WHEN** Details view visits `D:\`, `D:\SuperExplorer`, and `D:\UE_5.7` and remains at each location for ten seconds
- **THEN** at least one visible child folder at each location SHALL display a complete folder size
- **AND** no partial folder size SHALL be displayed

### Requirement: Folder Options shows current cache usage
The system SHALL forward cache telemetry through service decorators and SHALL render the latest measured current usage for every available cache budget row.

#### Scenario: Decorated local service supplies telemetry
- **WHEN** the active Explorer service is wrapped by a remote-location decorator
- **THEN** the decorator SHALL forward `cache_telemetry_snapshot` for local cache usage

#### Scenario: View page opens or budgets are applied
- **WHEN** Folder Options opens, the View page is selected, or cache budgets are applied
- **THEN** the window SHALL refresh its cache-usage snapshot
- **AND** available rows SHALL display measured bytes rather than an em dash

#### Scenario: Telemetry query is confirmed unavailable
- **WHEN** a cache telemetry source returns a confirmed failure
- **THEN** its row SHALL display `Unavailable / <limit>`

#### Scenario: MFT resource ownership is shown
- **WHEN** the Folder Options View page renders cache budgets
- **THEN** the five MFT Service-owned resource rows SHALL appear inside one labeled bordered group
- **AND** the group SHALL state that all SuperExplorer processes share the resources, distinguish restart-persistent disk index data from restart-rebuilt memory caches, and exclude Folder size cache TTL

### Requirement: Unavailable folder sizes have detailed local diagnostics
The system SHALL display only `Unavailable` in a failed Folder size cell, the MFT Service SHALL return its complete bounded failure reason to the requesting client, and the client SHALL emit a detailed local console and persistent-log reason sufficient to identify the failed stage and service state.

#### Scenario: Partial, timeout, or service error terminates a request
- **WHEN** a current folder aggregate ends with a partial response, deadline, IPC failure, stale generation, or service computation error
- **THEN** the console SHALL record the path, request identity, elapsed time, failure stage, service/index generation context when available, and complete error chain
- **AND** shared path-free cache telemetry SHALL remain free of paths and file names

#### Scenario: Service computation returns a detailed failure
- **WHEN** the MFT Service cannot produce an exact aggregate
- **THEN** its response SHALL carry a bounded UTF-8 failure payload to the requesting client
- **AND** the client SHALL print and persist that payload while the cell displays only `Unavailable`

#### Scenario: Client connects to an older service
- **WHEN** a current client receives a legacy generic folder-query error status
- **THEN** it SHALL retain the legacy generic diagnostic instead of misparsing aggregate fields or blocking for a payload

### Requirement: Obsolete Host snapshot cleanup is bounded and safe
The system SHALL stop reading and writing the obsolete Host Details snapshot namespace and SHALL retire it through bounded startup maintenance confined to the validated application cache directory.

#### Scenario: Obsolete regular files exist
- **WHEN** startup finds validated obsolete regular `.json` cache files directly under `%LOCALAPPDATA%\SuperExplorer\folder-snapshot-cache\v2`
- **THEN** it SHALL remove no more than 256 files in oldest-first order during that launch

#### Scenario: Cache root contains unsafe or unexpected entries
- **WHEN** the obsolete `v2` namespace contains a symlink, reparse point, directory, unexpected record, or target outside the exact validated namespace
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
