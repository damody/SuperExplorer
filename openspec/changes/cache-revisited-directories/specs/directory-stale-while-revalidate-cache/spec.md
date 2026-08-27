## ADDED Requirements

### Requirement: Revisited directories display cached rows immediately
The system SHALL cache successfully completed Local, ADB and SFTP directory snapshots and SHALL seed a subsequent navigation to the same canonical location with the cached rows before provider enumeration completes.

#### Scenario: Backspace returns to cached SFTP parent
- **WHEN** the user has completed enumeration of `sftp://45.32.49.125/home/linuxuser`, opens its `test` child and presses Backspace
- **THEN** the parent rows are present in the first loading-state presentation
- **AND** the system submits a background Navigate for the parent

#### Scenario: Cache miss
- **WHEN** the user navigates to a supported location with no completed cache entry
- **THEN** navigation starts with an empty loading snapshot
- **AND** provider enumeration proceeds normally

#### Scenario: Navigation entry points share behavior
- **WHEN** a cached location is reached through Back, Forward, multi-step history, Backspace Up, address submission, bookmark activation or opening a folder
- **THEN** each entry point seeds from the same location cache and revalidates in the background

### Requirement: Background enumeration converges cached content
The system SHALL treat cached rows as stale presentation data and SHALL converge them using only accepted events from the active navigation request.

#### Scenario: Fresh listing changes
- **WHEN** an accepted background listing returns additions, changes or removals relative to the cached snapshot
- **THEN** batches update the visible snapshot
- **AND** successful completion removes cached rows not observed by the new request
- **AND** the final snapshot replaces the cache entry

#### Scenario: Revalidation fails
- **WHEN** background enumeration fails after a cache hit
- **THEN** the directory enters the existing recoverable error state with cached rows retained
- **AND** the failed or partial listing does not replace the prior successful cache entry

#### Scenario: Stale event arrives
- **WHEN** a batch or terminal event has an old generation, wrong request ID or cancelled request
- **THEN** it changes neither the active presentation nor the cache

### Requirement: Cache identity isolates locations correctly
The cache MUST use stable canonical identity for Local, ADB and SFTP locations and MUST exclude transient remote enumeration identity.

#### Scenario: Equivalent virtual descriptors
- **WHEN** two ADB or SFTP descriptors have the same provider, public authority and canonical components but different entry IDs or container generations
- **THEN** they resolve to the same cache key

#### Scenario: Distinct remote locations
- **WHEN** descriptors differ by provider, authority or path component
- **THEN** they do not share a cache entry

#### Scenario: Equivalent local paths
- **WHEN** Windows local paths differ only by case or trailing directory separator
- **THEN** they resolve to the same cache key

### Requirement: Directory cache memory is bounded
The in-memory cache MUST retain no more than 64 directories and 100,000 total rows and MUST evict least-recently-used entries deterministically.

#### Scenario: Directory count exceeds limit
- **WHEN** inserting a completed snapshot would exceed 64 cached directories
- **THEN** the least recently used directory is evicted until the limit is satisfied

#### Scenario: Aggregate row count exceeds limit
- **WHEN** inserting a completed snapshot would exceed 100,000 total cached rows
- **THEN** least recently used entries are evicted until the row limit is satisfied

#### Scenario: Single snapshot exceeds row limit
- **WHEN** one completed snapshot contains more than 100,000 rows
- **THEN** that snapshot is not cached
- **AND** existing cache entries remain intact

#### Scenario: Process restarts
- **WHEN** SuperExplorer closes and starts again
- **THEN** no directory snapshot cache is restored from session data
