## ADDED Requirements

### Requirement: Live MFT state is memory-first
The MFT Service SHALL apply relevant USN changes to a coherent in-memory volume generation without requiring disk persistence, and folder-size queries SHALL observe the current proven in-memory generation through the existing service boundary.

#### Scenario: Query before persistence deadline
- **WHEN** a relevant file change is ingested before the ten-minute persistence deadline
- **THEN** a subsequent folder-size query observes the updated proven aggregate without a SQLite transaction or sidecar file

#### Scenario: Unfocused live operation
- **WHEN** Super Explorer remains unfocused while valid USN events arrive
- **THEN** the service continues coalescing and serving proven in-memory changes without periodic MFT cache writes

#### Scenario: Completeness becomes unproven
- **WHEN** overflow, ambiguous topology, journal discontinuity, or corrupt state prevents proof of the current result
- **THEN** the service returns a typed partial, unavailable, or rebuild-required outcome and MUST NOT present stale durable data as current exact data

### Requirement: Foreground focus is represented by authenticated expiring leases
Super Explorer SHALL report its GPUI window foreground-focus state through a dedicated versioned concurrent MFT Service lease channel using bounded frames, bounded connections/resources, cancelable overlapped I/O, real deadlines, and stop cancellation. A lease SHALL be bound to its authorized client process handle, connection, creation identity, token SID, active session, and protected installed-image identity; it SHALL expire without renewal and SHALL be removed on explicit release, disconnect, session loss, or identity mismatch. The LocalSystem service MUST NOT depend on cross-session foreground-HWND inspection. Persistence eligibility SHALL exist while at least one verified client reports focus through a valid lease.

#### Scenario: Focused authorized window
- **WHEN** an authorized connected Super Explorer window acquires or renews a focus lease
- **THEN** that lease contributes to the aggregate foreground gate until release, disconnect, or expiry

#### Scenario: Multiple windows
- **WHEN** two authorized windows hold valid leases and one loses focus
- **THEN** the foreground gate remains open until the final valid focused lease is released, disconnected, or expires

#### Scenario: Crashed client
- **WHEN** a focused client terminates or disconnects without releasing its lease
- **THEN** the service removes or expires that lease and it cannot authorize later persistence

#### Scenario: Unauthorized request
- **WHEN** a same-user spoofed process, copied executable, wrong-session client, or PID-reuse attempt requests or renews a focus lease
- **THEN** the service rejects the request and the aggregate foreground gate remains unchanged

#### Scenario: Stalled lease client
- **WHEN** a lease client sends a partial frame or stops responding
- **THEN** the service enforces its deadline/resource cap, ordinary queries and other windows remain available, and service stop cancels the stalled I/O

### Requirement: Durable commits require both time and focus gates
For each volume, the service SHALL begin a normal persistence transaction only when durable work is pending, at least ten minutes have elapsed since the last successful commit or process-start reference time, at least ten minutes have elapsed since the last disk-write attempt or no attempt exists, and a valid foreground-focus lease exists. The service SHALL record the attempt immediately before `BEGIN`; failure SHALL NOT advance the successful-commit clock, but the independent attempt throttle SHALL prevent another write for ten minutes. Focus renewals reset neither clock.

#### Scenario: Ten minutes elapsed without focus
- **WHEN** pending work is at least ten minutes old but no valid foreground-focus lease exists
- **THEN** the service performs no persistence transaction, migration, rebuild, or WAL checkpoint

#### Scenario: Focus exists before deadline
- **WHEN** a valid foreground-focus lease exists but fewer than ten minutes have elapsed since the reference time
- **THEN** the service continues serving memory state without beginning a normal persistence transaction

#### Scenario: Focus arrives after deadline
- **WHEN** pending work is due and the first valid foreground-focus lease becomes active
- **THEN** the service schedules exactly one volume transaction promptly and later lease renewals do not create another transaction inside the new ten-minute interval

#### Scenario: Transaction fails
- **WHEN** a due focused transaction fails
- **THEN** the durable cursor remains unchanged, the write-attempt time advances, and no retry that can write occurs for another ten minutes or without focus

#### Scenario: Failure after WAL frames are written
- **WHEN** repeated injected transactions fail after causing WAL file events
- **THEN** write attempts and resulting file-event bursts remain limited to one per volume per ten-minute interval

#### Scenario: Focus is lost during transaction
- **WHEN** the last lease expires or releases after an atomic transaction has begun
- **THEN** the service may finish that transaction but SHALL NOT begin subsequent persistence or maintenance without a new valid lease

### Requirement: SQLite transaction atomically binds entries and cursor
Each eligible NTFS volume SHALL use one versioned bundled-SQLite database in WAL mode under the fixed MFT cache root. A committed batch SHALL apply its coalesced entry mutations and durable journal cursor in one transaction, and readers SHALL admit only a schema-compatible, integrity-valid committed state with matching volume and journal identity.

#### Scenario: Successful transaction
- **WHEN** a pending batch commits successfully
- **THEN** all captured mutations and the resulting durable cursor become visible together

#### Scenario: Failure before commit
- **WHEN** an error or crash occurs before SQLite commits the batch
- **THEN** restart observes the previous cursor and none of the uncommitted mutations as durable

#### Scenario: Later events arrive during commit
- **WHEN** new USN events arrive after the transaction snapshot was captured
- **THEN** they remain in the next memory batch and are not skipped by the committed cursor

#### Scenario: Incompatible identity or schema
- **WHEN** the database schema, volume identity, journal identity, integrity, or cursor bounds are invalid
- **THEN** the service rejects the existing canonical set as exact and enters typed quarantine/rebuild-required state without legacy migration

### Requirement: Steady-state persistence uses a fixed file set
Normal steady-state MFT persistence SHALL NOT create per-generation checkpoint, delta, or status files. Each active volume SHALL use only its SQLite main database and SQLite-managed WAL/shared-memory companions, excluding bounded temporary files used during explicitly gated migration or recovery.

#### Scenario: Unfocused active volume
- **WHEN** a volume receives changes for at least twenty minutes while Super Explorer is unfocused
- **THEN** no `.semftcp`, `.semftdelta`, or periodically rewritten `.semftstatus` file is created and SQLite cache timestamps do not show periodic commits

#### Scenario: Two focused commit intervals
- **WHEN** a focused volume commits changes across two eligible ten-minute intervals
- **THEN** the active cache file identities remain the fixed SQLite file set rather than adding generation-named files

#### Scenario: Service diagnostics change
- **WHEN** pending counts, focus state, or observed cursor changes between queries
- **THEN** diagnostics are returned through IPC without requiring a status-file rewrite

### Requirement: WAL maintenance is bounded and foreground gated
The service SHALL disable SQLite automatic and last-close WAL checkpointing, including asserted `SQLITE_DBCONFIG_NO_CKPT_ON_CLOSE` on every applicable connection. It SHALL attempt `wal_checkpoint(TRUNCATE)` only when the WAL exceeds 256 MiB, a valid focus lease exists, and no persistence transaction or query-critical refresh is active. It SHALL prohibit another WAL-appending transaction when current WAL bytes plus conservatively measured worst-case growth for one maximum admitted batch would exceed the hard bound of 256 MiB plus that one batch and its measured SQLite frame/page overhead. Checkpoint failure SHALL preserve recoverable state and use bounded retry without permitting unbounded WAL growth.

#### Scenario: WAL below threshold
- **WHEN** the WAL is at or below 256 MiB
- **THEN** the service does not initiate a maintenance checkpoint regardless of focus

#### Scenario: WAL above threshold without focus
- **WHEN** the WAL exceeds 256 MiB and no valid focus lease exists
- **THEN** the service reports the over-threshold condition but does not checkpoint or truncate the WAL

#### Scenario: WAL above threshold with focus
- **WHEN** the WAL exceeds 256 MiB, focus is valid, and conflicting work is idle
- **THEN** the service may run one truncate checkpoint and records its outcome

#### Scenario: Checkpoint failure
- **WHEN** a truncate checkpoint fails or is busy
- **THEN** committed data remains usable, the failure counter advances, retry cannot occur in the background or without bounded backoff, and WAL-appending commits stop before the computed hard bound can be exceeded

#### Scenario: Last connection closes with WAL frames
- **WHEN** the service closes its last applicable SQLite connection with outstanding WAL frames
- **THEN** no implicit checkpoint, main-database backfill, WAL truncation or unlink, SHM unlink, or cache-directory mutation occurs

### Requirement: Stop and shutdown perform no final persistence
The service SHALL accept and handle both SCM stop and Windows shutdown controls. The control path SHALL close a lifecycle barrier before cancellation so no new SQLite transaction, WAL checkpoint, migration, rebuild, or legacy cleanup can start. A transaction SHALL recheck the barrier immediately before `BEGIN` and immediately before invoking SQLite `COMMIT`; it SHALL roll back if stop wins before commit invocation, while an already-invoked commit may finish atomically. Connection close SHALL perform no implicit checkpoint or cache-directory mutation.

#### Scenario: Stop with pending changes
- **WHEN** SCM requests stop while a volume has uncommitted pending changes and no transaction has crossed the commit-invocation linearization point
- **THEN** the service reaches stopped state without advancing the durable cursor or changing the cache through a final persistence operation

#### Scenario: Shutdown while unfocused and overdue
- **WHEN** Windows shutdown begins after the ten-minute deadline with no valid focus lease
- **THEN** shutdown does not bypass the foreground gate and performs no MFT durability work

#### Scenario: Restart after dropped pending memory
- **WHEN** the service restarts and the durable-to-current gap remains in the USN journal
- **THEN** it replays that gap into memory without requiring a full MFT rebuild or immediate persistence

#### Scenario: Stop races with commit
- **WHEN** stop or shutdown occurs before, during, or after transaction commit invocation
- **THEN** evidence shows rollback before the defined commit linearization point, atomic completion after it, and no subsequent durability work

### Requirement: Recovery and overload remain bounded behind focus
Pending USN changes SHALL remain subject to explicit count and byte bounds. When incremental correctness is lost through overflow, ambiguous topology, journal retention loss, identity replacement, corruption, or a sequence gap, the service SHALL mark the volume rebuild-required and SHALL begin a full rebuild only while a valid focus lease exists. Rebuilds SHALL remain serialized across volumes.

#### Scenario: Pending memory exceeds a bound
- **WHEN** the coalesced pending map exceeds its configured count or byte limit while unfocused
- **THEN** the service releases detailed pending entries, marks completeness lost, and performs no background write or rebuild

#### Scenario: Journal no longer covers durable cursor
- **WHEN** restart finds that the durable cursor is older than the retained journal range
- **THEN** the service reports rebuild-required and waits for valid foreground focus before starting the serialized rebuild

#### Scenario: Recovery fails
- **WHEN** a foreground-gated rebuild cannot complete or validate
- **THEN** the database is not promoted as current exact state and diagnostics retain a stable machine-readable failure reason

### Requirement: Legacy migration is gated, verified, and scoped
Only when the canonical SQLite main/WAL/SHM set is entirely absent and valid legacy MFT base/sidecar state exists SHALL the service load legacy state read-only and defer migration until both gates are satisfied. It SHALL build and pre-validate one self-contained temporary main database within the resolved fixed cache root, close/fsync it, atomically install the canonical main file, confirm that no stale canonical WAL/SHM companion exists, reopen it with the approved WAL/no-close-checkpoint configuration, and validate it again before deleting any recognized legacy file. It MUST NOT promote a live WAL/SHM set piecemeal. An existing invalid canonical set SHALL instead be quarantined as one foreground-gated unit and enter rebuild-required.

#### Scenario: Legacy state while unfocused
- **WHEN** startup finds a valid legacy chain, the canonical SQLite main/WAL/SHM set is entirely absent, and Super Explorer is unfocused
- **THEN** the service performs no migration or legacy deletion and keeps the legacy state retryable

#### Scenario: Verified promotion
- **WHEN** a gated migration atomically promotes SQLite and reopen verification confirms schema, integrity, identities, cursor, and bounded entry state
- **THEN** only recognized volume-specific `.semftidx`, `.semftcp`, `.semftdelta`, and `.semftstatus` files inside the fixed cache root become eligible for inventoried removal

#### Scenario: Migration fails validation
- **WHEN** build, promotion, reopen, or validation fails
- **THEN** legacy files remain intact, unrecognized files remain untouched, and migration remains retryable

#### Scenario: Corrupt canonical store or orphan temporary set
- **WHEN** startup finds corrupt canonical SQLite or recognized orphan migration artifacts while unfocused
- **THEN** it records the entire canonical main/WAL/SHM set and orphan artifacts as quarantine-pending without filesystem mutation, defers a crash-safe staged quarantine until a valid foreground maintenance opportunity, and does not start legacy migration

#### Scenario: Quarantine is interrupted between members
- **WHEN** foreground-gated quarantine stops after its durable intent manifest but before all main/WAL/SHM members are moved
- **THEN** no residual canonical member is admitted as exact, restart verifies file identities and resumes the idempotent staged disposition, and rebuild waits until quarantine completes

#### Scenario: Cleanup target escapes root
- **WHEN** a candidate path does not resolve inside the exact fixed MFT cache root or does not match an approved volume-specific legacy pattern
- **THEN** the service refuses to delete it

### Requirement: Diagnostics distinguish memory, durability, focus, and failure
Local diagnostics SHALL report per volume the service mode, database schema, durable and observed cursors, pending count and bytes, last successful commit time, active focus-lease count/expiry state, main database and WAL bytes, transaction/checkpoint failure counters, migration state, and stable recovery reason without exposing raw user paths.

#### Scenario: Healthy unfocused lag
- **WHEN** memory state is newer than the durable cursor and no focus lease exists
- **THEN** diagnostics distinguish healthy pending lag from corruption or rebuild-required state

#### Scenario: Failed transaction and checkpoint
- **WHEN** persistence and WAL maintenance have failed
- **THEN** their counters and last outcomes are reported independently without claiming that the durable cursor advanced

#### Scenario: Diagnostic observation
- **WHEN** a client requests diagnostics repeatedly while values change
- **THEN** observation itself does not cause MFT cache persistence

### Requirement: Existing consumers and configuration remain compatible
Built-in and extension folder-size consumers SHALL continue using the existing Host/MFT query path and SHALL require no raw USN or SQLite access. Existing normalized cache-budget configuration and version compatibility behavior SHALL remain valid after the focus-lease protocol addition.

#### Scenario: Current memory result reaches consumers
- **WHEN** an in-memory USN change updates a folder aggregate before persistence
- **THEN** existing built-in and extension consumers receive the updated Host-owned result through their existing integration path

#### Scenario: Older client lacks focus operation
- **WHEN** a compatible older client connects without supporting the new focus-lease operation
- **THEN** ordinary supported queries continue to work but that client cannot open the persistence gate

#### Scenario: Cache budgets are applied
- **WHEN** the existing versioned cache-budget operation sets normalized MFT limits
- **THEN** the service continues enforcing and reporting those limits independently of focus and persistence timing

### Requirement: Installed evidence proves reduced write and Defender-trigger cadence
Release readiness SHALL include an installed-service foreground/background observation that records MFT cache file events and timestamps plus Defender CPU and I/O counters. Working-set size alone MUST NOT be used as proof of scanning activity.

#### Scenario: Background observation
- **WHEN** the installed service ingests representative changes for at least twenty minutes without a valid focus lease
- **THEN** evidence records no periodic MFT cache transaction, checkpoint, migration, rebuild, legacy cleanup, or generation-file creation

#### Scenario: Focused observation
- **WHEN** representative changes and valid focus span multiple persistence deadlines
- **THEN** evidence records no more than one disk-write attempt per volume per ten-minute interval, successful commits at least ten minutes apart, and a fixed active SQLite file set

#### Scenario: Defender comparison
- **WHEN** pre-change and post-change Defender impact is evaluated
- **THEN** the report uses comparable process CPU/I/O and cache file-event measurements, identifies environmental limitations, and does not infer reload solely from Defender memory usage
