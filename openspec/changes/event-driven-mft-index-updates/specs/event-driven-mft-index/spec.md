## ADDED Requirements

### Requirement: Initial snapshot establishes a journal checkpoint
For each eligible fixed NTFS volume, the service SHALL establish a valid base snapshot and a checkpoint containing the volume identity, journal ID, and next USN before publishing journal mode. It SHALL release memory proportional to the completed base after publication.

#### Scenario: First initialization
- **WHEN** an eligible volume has no compatible base and checkpoint
- **THEN** the service builds and atomically publishes one base snapshot and checkpoint before entering journal mode

#### Scenario: Compatible restart
- **WHEN** the service restarts with a compatible base and a journal cursor still present in the retained journal range
- **THEN** it resumes from that cursor without performing a complete MFT scan

### Requirement: Idle operation is event driven
After initialization, the service SHALL wait on NTFS USN Journal changes and SHALL NOT use a fixed interval to initiate complete MFT scans or base rewrites.

#### Scenario: Two idle minutes
- **WHEN** an initialized volume receives no relevant changes for at least two minutes
- **THEN** its base snapshot timestamp and generation remain unchanged and service telemetry contains no repeated full-scan signature

#### Scenario: Service waits without polling rebuilds
- **WHEN** the volume remains idle after entering journal mode
- **THEN** the journal worker remains blocked or in a bounded wait using fixed-size resources rather than allocating work proportional to the volume record count

### Requirement: Relevant changes update proven in-memory query state
The service SHALL normalize and coalesce relevant USN reasons into current proven in-memory state without requiring durable publication. The durability interval and storage mechanism SHALL be governed by `mft-sqlite-foreground-persistence`.

#### Scenario: File grows
- **WHEN** a file's logical or allocated size changes on an initialized volume
- **THEN** its current size and parent identity become available to service queries from proven in-memory state without waiting for SQLite persistence

#### Scenario: Sustained activity
- **WHEN** relevant events continue while disk persistence is not eligible
- **THEN** query freshness continues from bounded in-memory coalescing rather than being postponed until a durable commit

#### Scenario: Irrelevant or redundant events
- **WHEN** multiple records for the same file reference do not require distinct externally visible transitions
- **THEN** the batch coalesces them without losing the final state or journal boundary

### Requirement: Legacy delta and checkpoint state remains safe for migration
Existing delta/checkpoint generations SHALL remain schema-, identity-, cursor-, checksum-, and commit-validated when read as legacy migration input. New durable state SHALL use the atomic SQLite contract in `mft-sqlite-foreground-persistence`.

#### Scenario: Successful commit
- **WHEN** a pending batch is published successfully
- **THEN** the committed delta is atomically visible before a checkpoint referencing its next USN becomes visible

#### Scenario: Crash before checkpoint advance
- **WHEN** the service stops after the delta commits but before the checkpoint advances
- **THEN** restart replays or recognizes the committed delta idempotently without skipping its changes

#### Scenario: Partial temporary file
- **WHEN** a delta write is interrupted before atomic publication
- **THEN** Host and service readers ignore the temporary or checksum-invalid file

### Requirement: Host applies contiguous deltas atomically
The Host SHALL accept only compatible contiguous generation and USN sequences, apply each accepted batch to a private index generation, and publish the resulting generation atomically.

#### Scenario: Contiguous delta
- **WHEN** a valid delta directly follows the Host's current base/delta generation
- **THEN** all of its entry and relationship mutations become visible together

#### Scenario: Gap or identity mismatch
- **WHEN** a delta has a generation gap, USN gap, journal mismatch, volume mismatch, unsupported schema, or invalid checksum
- **THEN** the Host rejects that chain, retains the last valid generation, and requests or awaits recovery

#### Scenario: Replayed delta
- **WHEN** the Host encounters a delta generation it has already committed
- **THEN** it does not apply the changes twice

### Requirement: Folder-size caches invalidate affected ancestry only
For an accepted change, the Host SHALL invalidate folder aggregates and persistent data-column cache entries for the changed item plus its old and new ancestor chains, while preserving unrelated valid entries.

#### Scenario: File size changes in place
- **WHEN** a file's size changes without moving
- **THEN** the file's ancestor folders are invalidated and an unrelated sibling subtree remains cached

#### Scenario: Item moves or is renamed across parents
- **WHEN** an item moves from one parent directory to another
- **THEN** both old and new ancestor chains are invalidated before the new generation is published

#### Scenario: Item is deleted
- **WHEN** a deletion delta is accepted
- **THEN** the Host derives invalidation from the old topology before removing the entry

### Requirement: Correctness loss triggers bounded recovery
The service SHALL perform a serialized complete rebuild only when lossless continuation is impossible because of first initialization, incompatible/corrupt state, volume or journal identity change, retained-range loss, sequence gap, or bounded-queue overflow. It SHALL retain the prior valid generation until replacement commits.

#### Scenario: Journal cursor was truncated
- **WHEN** the checkpoint USN is older than the journal's retained range
- **THEN** the volume enters recovering state and performs exactly one replacement rebuild before resuming journal mode

#### Scenario: Queue bound is exceeded
- **WHEN** pending journal changes cannot fit within configured count or byte bounds
- **THEN** the service records an overflow recovery reason and rebuilds rather than silently dropping events

#### Scenario: Recovery fails
- **WHEN** a replacement snapshot cannot be completed
- **THEN** the last valid Host generation remains readable and diagnostics report error without publishing partial replacement state

### Requirement: Service resources and lifecycle are bounded
Journal readers SHALL use fixed-size buffers and bounded queues, normal incremental processing SHALL NOT allocate memory proportional to the total volume index, and SCM stop SHALL terminate blocked readers and reach stopped state cleanly.

#### Scenario: Small incremental batch on a large volume
- **WHEN** a bounded number of files changes on a volume whose base index is hundreds of megabytes
- **THEN** service memory growth is bounded by configured buffers and pending changes rather than base-index size

#### Scenario: Stop during blocked journal read
- **WHEN** SCM requests stop while workers are waiting for USN changes
- **THEN** the service cancels or wakes those reads, joins workers, releases handles, and reports stopped without waiting for a file-system event

### Requirement: Diagnostics expose per-volume freshness and recovery state
The service SHALL expose per-volume mode, base/delta generation, committed journal position, pending count/bytes, last publication time, queue high-water marks, and rebuild reason to local diagnostics.

#### Scenario: Healthy journal mode
- **WHEN** a volume has a valid base and committed cursor with no recovery condition
- **THEN** diagnostics report journal mode and its last committed USN and publication generation

#### Scenario: Recovery is required
- **WHEN** any recovery condition is detected
- **THEN** diagnostics identify recovering or error mode and a stable machine-readable reason

### Requirement: Existing folder-size consumers remain compatible
The built-in Size column and folder-size extension SHALL continue to receive Host-owned folder-size results without processing raw journal events or presenting event ingestion as a separate calculation method.

#### Scenario: Updated folder total reaches both consumers
- **WHEN** the Host commits a delta that changes a folder aggregate
- **THEN** both the built-in Size value and enabled Folder size column obtain the updated Host result through their existing integration path

#### Scenario: Plugin is disabled
- **WHEN** the folder-size extension is disabled but the built-in Size column queries a directory
- **THEN** the Host still uses the service-backed updated index without requiring plugin-owned event logic

### Requirement: Host retains only the active three-level result window
The MFT Service SHALL own the complete durable volume dataset. After a Host calculation batch completes, the Host SHALL release materialized complete-volume topology and aggregate indexes. It SHALL retain cacheable terminal folder data-column results only for the active folder and descendants no deeper than three path components. A Size Map full tree SHALL be request-scoped and SHALL NOT remain inside a retained terminal snapshot.

#### Scenario: Fourth-level result is discarded
- **WHEN** the active folder contains `a/b/c/d` and values through `d` were calculated
- **THEN** Host caches may retain `a`, `b`, and `c`, but discard `d` and paths outside the active folder

#### Scenario: Size Map tree does not remain resident
- **WHEN** a complete Size Map subtree has been projected and published to the renderer
- **THEN** the retained Host snapshot contains only its terminal aggregate and a later Size Map request obtains a fresh complete tree from service-owned data

### Requirement: Service owns a configurable folder-aggregate LRU
The Host SHALL query folder totals from the local MFT Service and SHALL NOT materialize a complete volume index for built-in Size or Folder size requests. The Service SHALL bound its estimated resident aggregate/index cache with volume-granular LRU eviction. The default SHALL be 512 MiB and Folder Options SHALL accept numeric values from 128 through 2048 MiB inclusive.

#### Scenario: Repeated folder query is served by Service cache
- **WHEN** an unchanged volume generation was already materialized by the Service
- **THEN** another folder query returns only its aggregate without the Host rebuilding the volume index

#### Scenario: User lowers the cache limit
- **WHEN** a request carries a lower valid cache limit and resident generations exceed it
- **THEN** the Service evicts least-recently-used volumes before admitting another generation

#### Scenario: Numeric setting is outside bounds
- **WHEN** the user enters a value below 128 or above 2048
- **THEN** the persisted and transmitted setting is clamped to the nearest bound
