# virtual-folder-stream-and-mutation Specification

## Purpose
TBD - created by archiving change build-extensible-plugin-platform. Update Purpose after archive.
## Requirements
### Requirement: Structured virtual locations
The platform SHALL represent virtual containers with provider ID, container file identity, container generation, stable entry ID and normalized components. Virtual locations SHALL integrate with tabs, address/breadcrumb display, back/forward, parent resolution and session restore.

#### Scenario: User opens a 7z file
- **WHEN** an enabled provider recognizes and opens a `.7z` container
- **THEN** its root becomes a normal navigation location with history and breadcrumb behavior rather than a private side window

### Requirement: Safe entry normalization
Providers and host adapters SHALL reject absolute paths, drive prefixes, NUL, invalid components and parent traversal. Entries that normalize to the same name SHALL produce a conflict instead of silent overwrite.

#### Scenario: Archive contains traversal entry
- **WHEN** an archive entry is named `../../outside.txt`
- **THEN** enumeration/extraction marks it unsafe and never resolves it outside the container/destination root

### Requirement: Rich virtual entry metadata
Enumeration SHALL expose stable entry ID, name, kind, virtual path, uncompressed/compressed sizes, CRC when available, modified time, encryption state and allowed operations without materializing all content.

#### Scenario: Detail view sorts archive entries
- **WHEN** a virtual folder is shown in details mode
- **THEN** supported virtual metadata can participate in normal typed columns and sorting before full extraction

### Requirement: Bounded virtual file streams
`VirtualFileStreamProviderV1` SHALL supply authorized bounded read streams with optional seek, length/CRC, cancellation and generation. Physical materialization SHALL occur only for consumers that require a path and SHALL use quota-managed host temporary storage cleaned after the session.

#### Scenario: Preview supports streams
- **WHEN** a preview provider can read a stream
- **THEN** the archive entry is previewed without extracting the entire archive or leaving a permanent temporary file

### Requirement: Safe extraction and drag-out plans
Copying or dragging virtual entries to the filesystem SHALL use a typed extract plan that checks path escape, conflicts, declared output, space, quotas and cancellation. No entry SHALL write outside the authorized destination.

#### Scenario: Extraction exceeds resource policy
- **WHEN** declared or observed output exceeds the allowed total/ratio
- **THEN** extraction stops with a resource-limited diagnostic and does not continue allocating disk space

### Requirement: Transactional virtual mutations
Create folder, add, delete, rename and move SHALL first produce an archive mutation preview. For 7z, execution SHALL rebuild into same-volume host staging, flush, reopen/verify header and entries/CRC, recheck original identity and then atomically replace the original.

#### Scenario: Verification fails
- **WHEN** the staging archive fails reopen, header or CRC verification
- **THEN** the original archive remains bit-for-bit unchanged and staging is cleaned

#### Scenario: Original changes concurrently
- **WHEN** original file identity/size/mtime differs before commit
- **THEN** atomic replacement is refused and the user receives a conflict rather than overwriting external changes

### Requirement: Container generation invalidation
Every successful mutation SHALL advance container generation and invalidate old virtual locations, streams, previews and cache entries.

#### Scenario: Old stream survives a mutation attempt
- **WHEN** a mutation commits while an older-generation entry handle is retained
- **THEN** subsequent use of that handle is rejected as stale

### Requirement: Archive undo policy
When quota permits, mutation SHALL preserve a recoverable original archive and undo SHALL restore the complete container. If backup exceeds quota, the preview SHALL state that the operation is not app-undoable and require additional confirmation.

#### Scenario: User undoes archive rename
- **WHEN** an archive mutation has a valid original backup
- **THEN** undo restores the original container atomically rather than replaying inverse entry operations

### Requirement: Secret and encryption handling
Encrypted archives SHALL obtain passwords through host secure UI and pass only short-lived non-serializable/non-debug secret handles. Passwords SHALL NOT enter manifest, normal settings, diagnostics or logs; mutation SHALL preserve encryption policy unless explicitly changed.

#### Scenario: Wrong password is entered
- **WHEN** archive open fails due to an incorrect secret
- **THEN** the error omits secret contents and the secret handle is destroyed after the attempt

### Requirement: Archive resource policy
The host SHALL enforce limits for entry count, path depth, per-entry and total output, compression ratio, CPU, memory and temporary disk for read/extract/mutation.

#### Scenario: Compression bomb fixture is opened
- **WHEN** a fixture declares extreme output or ratio
- **THEN** processing returns a diagnosable resource-limited state without exhausting host memory/disk
