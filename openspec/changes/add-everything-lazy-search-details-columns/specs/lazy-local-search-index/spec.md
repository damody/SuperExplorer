## ADDED Requirements

### Requirement: Application-owned versioned SQLite index
The fallback index SHALL use a versioned SQLite database inside the application data directory, SHALL store file metadata but no file contents, and SHALL recover from corruption without preventing search.

#### Scenario: First local search
- **WHEN** no database exists
- **THEN** the application creates the owned directory and versioned schema with WAL and bounded connection settings

#### Scenario: Database is corrupt
- **WHEN** schema or integrity validation fails
- **THEN** the application quarantines the invalid file within the owned data root and creates an empty valid database

#### Scenario: Data directory is unavailable
- **WHEN** the preferred LocalAppData location cannot be created or opened
- **THEN** the application uses its existing writable-data fallback policy or performs a recoverable non-persistent search

### Requirement: Shallow indexing of viewed folders
After a directory is successfully enumerated, the application SHALL index only the owned metadata of that directory's immediate children and SHALL NOT recursively crawl because the folder was viewed.

#### Scenario: User opens a folder
- **WHEN** enumeration of `D:\photos` completes
- **THEN** direct children of `D:\photos` are upserted and unopened descendant directories are not enumerated for indexing

#### Scenario: Folder is refreshed
- **WHEN** a previously observed folder is successfully re-enumerated
- **THEN** changed children are updated and missing direct children are removed without altering unrelated scopes

### Requirement: Cached-first active-scope search
A local search SHALL query existing rows inside the exact canonical scope first and SHALL index additional descendants only as part of the active request.

#### Scenario: Cached matches exist
- **WHEN** a local query starts in a previously viewed folder
- **THEN** matching cached rows are emitted before traversal completes

#### Scenario: Scope requires discovery
- **WHEN** the active query reaches an unobserved child directory
- **THEN** that directory is enumerated, metadata is committed in a short bounded transaction, and new matches are streamed

#### Scenario: Similar path prefix exists
- **WHEN** searching `C:\foo` while rows under `C:\foobar` exist
- **THEN** no `C:\foobar` row is selected or traversed

#### Scenario: Reparse point is encountered
- **WHEN** traversal sees a junction, symbolic link, or other reparse-like directory
- **THEN** it records the item if appropriate but does not follow it

### Requirement: Cancellation stops index growth
The local crawler MUST stop scheduling and indexing new work promptly after the correlated search is cancelled.

#### Scenario: Root-drive search is cancelled
- **WHEN** a search under `C:\` is cancelled after traversal begins
- **THEN** queued directories are discarded, the current uncommitted batch rolls back, and database row growth stops within the cancellation latency bound

#### Scenario: Search is replaced
- **WHEN** a second query replaces a running local query
- **THEN** the first crawler stops and late results or commits from its generation are rejected

#### Scenario: Application shuts down
- **WHEN** shutdown begins during indexing
- **THEN** crawler threads terminate within the bounded shutdown period and the database remains consistent

### Requirement: Bounded local indexing
The local index and traversal SHALL enforce configured limits for results, queued directories, visited entries, transaction size, path length, and database growth.

#### Scenario: A configured bound is reached
- **WHEN** an active search reaches any traversal or storage limit
- **THEN** it stops expanding, retains valid partial results, and reports a truthful partial terminal outcome
