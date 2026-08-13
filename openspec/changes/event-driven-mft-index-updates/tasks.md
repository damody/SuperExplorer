## 1. Persisted Protocol and Journal Primitives

### 1.1 Versioned checkpoint and delta records

**目的：** Define crash-consistent persisted contracts shared by Service and Host.  
**輸入：** Approved design, existing `MftIndexV1` format, `%ProgramData%\SuperExplorer\MftIndex` layout.  
**產出：** Versioned checkpoint/delta/status types, codecs, validation, atomic publication helpers.  
**依賴：** None.  
**Owner／Wave：** Primary agent / Wave 1.  
**Gate／Evidence：** G-PROTOCOL; `target/openspec-evidence/event-driven-mft-index-updates/1.1.*`.  
**完成門檻：** Round-trip, corruption, ordering, replay, and temporary-file tests pass with unique evidence records.

- [x] 1.1.1 Define volume identity, checkpoint, delta generation, normalized change, and status record types with explicit schema versions.
- [x] 1.1.2 Implement bounded binary codecs and checksum/commit validation for checkpoint, delta, and status records.
- [x] 1.1.3 Implement temporary-write, flush, atomic-rename delta publication followed by checkpoint publication.
- [x] 1.1.4 Add unit tests for round-trip, unsupported schema, checksum failure, truncated file, commit ordering, and idempotent replay boundaries.
- [ ] 1.1.5 Write the G-PROTOCOL evidence records for tasks 1.1.1–1.1.4.

### 1.2 USN Journal access and normalization

**目的：** Add safe Windows primitives for journal identity/query/read and normalized folder-size-relevant events.  
**輸入：** Windows volume handle code and persisted protocol types.  
**產出：** Journal metadata/cursor validation, bounded reads, reason mapping, cancellation seam.  
**依賴：** 1.1.  
**Owner／Wave：** Primary agent / Wave 1.  
**Gate／Evidence：** G-JOURNAL; `target/openspec-evidence/event-driven-mft-index-updates/1.2.*`.  
**完成門檻：** Synthetic and opt-in NTFS tests prove cursor/reason/cancellation behavior without unbounded allocation.

- [x] 1.2.1 Add bounded `FSCTL_QUERY_USN_JOURNAL` and `FSCTL_READ_USN_JOURNAL` wrappers with RAII handles and cancellation behavior.
- [x] 1.2.2 Normalize create, delete, rename, data, hard-link, and conservative invalidation reasons into delta changes.
- [x] 1.2.3 Add cursor compatibility checks for volume identity, journal ID, retained range, and next USN.
- [x] 1.2.4 Add unit and opt-in NTFS tests for parsing, rename halves, unknown reasons, cursor truncation, and blocked-read cancellation.
- [ ] 1.2.5 Write the G-JOURNAL evidence records for tasks 1.2.1–1.2.4.

## 2. Event-Driven Service

### 2.1 Initialization, reuse, and recovery

**目的：** Replace unconditional startup reconstruction with compatible snapshot/checkpoint reuse and explicit recovery.  
**輸入：** Protocol and journal primitives, existing complete snapshot builder.  
**產出：** Per-volume initialization state machine and serialized recovery path.  
**依賴：** 1.1, 1.2.  
**Owner／Wave：** Primary agent / Wave 2.  
**Gate／Evidence：** G-INIT; `target/openspec-evidence/event-driven-mft-index-updates/2.1.*`.  
**完成門檻：** First use rebuilds once; compatible restart performs no full scan; every specified incompatibility enters one recovery rebuild while preserving prior valid data.

- [x] 2.1.1 Implement eligible NTFS volume discovery and stable per-volume identity without drive-letter-only identity assumptions.
- [x] 2.1.2 Implement compatible base/checkpoint reuse and initial checkpoint establishment at a safe journal boundary.
- [x] 2.1.3 Implement serialized recovery for missing/corrupt state, journal mismatch/truncation, generation gaps, and overflow reasons.
- [ ] 2.1.4 Add state-machine tests for first initialization, compatible restart, failed replacement, and exactly-once recovery scheduling.
- [ ] 2.1.5 Write the G-INIT evidence records for tasks 2.1.1–2.1.4.

### 2.2 Blocking readers and bounded coalescing

**目的：** Make normal service operation event driven with 5-second debounce and 10-second maximum delay.  
**輸入：** Initialized checkpoints and normalized journal events.  
**產出：** Per-volume workers, bounded coordinator, rename pairing, durable batch publication.  
**依賴：** 2.1.  
**Owner／Wave：** Primary agent / Wave 2.  
**Gate／Evidence：** G-COALESCE; `target/openspec-evidence/event-driven-mft-index-updates/2.2.*`.  
**完成門檻：** No fixed full-scan timer remains; deterministic time tests prove debounce/cap; overflow and ambiguous transitions become explicit recovery/invalidation.

- [x] 2.2.1 Replace the 30-second rebuild loop with per-volume blocking journal workers and a bounded event channel.
- [x] 2.2.2 Implement file-reference coalescing, cross-boundary rename pairing, five-second debounce, and ten-second maximum publication deadline.
- [x] 2.2.3 Resolve live file sizes for committed changes and publish ordered durable deltas without retaining a full Service-side index.
- [x] 2.2.4 Implement count/byte high-water limits whose overflow records diagnostics and schedules recovery without claiming freshness.
- [x] 2.2.5 Add deterministic coordinator tests for quiet batches, sustained activity, redundant changes, ambiguous rename/link events, and overflow.
- [ ] 2.2.6 Write the G-COALESCE evidence records for tasks 2.2.1–2.2.5.

### 2.3 Lifecycle and diagnostics

**目的：** Make blocked readers stoppable and expose auditable per-volume state.  
**輸入：** Service workers/coordinator and status protocol.  
**產出：** SCM cancellation/join path and atomic diagnostics.  
**依賴：** 2.2.  
**Owner／Wave：** Primary agent / Wave 2.  
**Gate／Evidence：** G-LIFECYCLE; `target/openspec-evidence/event-driven-mft-index-updates/2.3.*`.  
**完成門檻：** SCM stop completes within the test timeout with no worker/handle leak; status transitions and high-water values are validated.

- [x] 2.3.1 Implement stop-event propagation, journal-read wake/cancellation, worker join, and correct SCM pending/stopped reporting.
- [x] 2.3.2 Publish atomic per-volume initializing/journal/recovering/error diagnostics with generations, USN, queues, timestamps, and reasons.
- [x] 2.3.3 Add lifecycle and status tests covering stop during blocked read, stop during debounce, recovery transitions, and error persistence.
- [ ] 2.3.4 Write the G-LIFECYCLE evidence records for tasks 2.3.1–2.3.3.

## 3. Host Delta Application and Cache Correctness

### 3.1 Contiguous immutable index generations

**目的：** Apply Service deltas without exposing partial or invalid Host state.  
**輸入：** Existing Host MFT loading, base/checkpoint/delta protocol.  
**產出：** Delta discovery, validation, topology mutation, immutable generation swap.  
**依賴：** 1.1, 2.2.  
**Owner／Wave：** Primary agent / Wave 3.  
**Gate／Evidence：** G-HOST-INDEX; `target/openspec-evidence/event-driven-mft-index-updates/3.1.*`.  
**完成門檻：** Valid chains apply once and atomically; gaps/mismatches/corruption preserve the last valid generation and expose recovery need.

- [x] 3.1.1 Implement ordered delta discovery and compatibility/contiguity validation from the last committed Host generation.
- [x] 3.1.2 Implement create, update, delete, and rename topology mutations on a private materialized index generation.
- [x] 3.1.3 Implement idempotent replay and atomic publication while retaining the previous generation on any batch failure.
- [ ] 3.1.4 Add Host tests for contiguous chains, gaps, journal/volume mismatch, corruption, duplicate replay, and all-or-nothing visibility.
- [ ] 3.1.5 Write the G-HOST-INDEX evidence records for tasks 3.1.1–3.1.4.

### 3.2 Aggregate and persistent cache invalidation

**目的：** Refresh affected folder sizes while retaining unrelated Host cache hits.  
**輸入：** Old/new Host topology, aggregate index, Host-owned persistent data-column cache.  
**產出：** Old/new ancestor invalidation and consumer-visible updated values.  
**依賴：** 3.1.  
**Owner／Wave：** Primary agent / Wave 3.  
**Gate／Evidence：** G-CACHE; `target/openspec-evidence/event-driven-mft-index-updates/3.2.*`.  
**完成門檻：** Mutation matrix updates built-in Size and Folder size within the shared Host path; unrelated cache keys remain hits.

- [x] 3.2.1 Derive old ancestry before mutation and new ancestry after mutation, including conservative invalidation for ambiguous topology.
- [x] 3.2.2 Invalidate/recompute affected aggregates and Host persistent data-column cache keys without clearing unrelated subtrees.
- [x] 3.2.3 Wire delta generation refresh into existing folder-size requests for built-in Size and enabled/disabled extension configurations.
- [ ] 3.2.4 Add tests for grow, truncate, create, delete, same-parent rename, cross-parent move, ambiguous link, and unrelated cache retention.
- [ ] 3.2.5 Write the G-CACHE evidence records for tasks 3.2.1–3.2.4.

## 4. Migration, Packaging, and Verification

## 3.3 Bounded active-folder cache window

- [x] 3.3.1 Apply one three-level active-root admission boundary to persistent extension data-column caches.
- [x] 3.3.2 Release Host complete-volume MFT indexes/aggregates after Folder size and Size Map request batches.
- [x] 3.3.3 Compact retained Size Map snapshots to terminal aggregates and reject them for later full-tree reuse.
- [x] 3.3.4 Add unit coverage for `a/b/c` retention, fourth-level/out-of-root eviction, and full-tree rematerialization.
- [x] 3.3.5 Add a bounded local named-pipe folder-aggregate query and move Details folder totals to the Service.
- [x] 3.3.6 Add a volume-granular Service LRU with a numeric 128–2048 MiB setting, defaulting to 512 MiB.
- [x] 3.3.7 Persist the setting, expose a Folder Options number field, and cover protocol and normalization boundaries.

### 4.1 Cache migration and installer compatibility

**目的：** Preserve valid installed data and safely recover incompatible versions across upgrade/rollback.  
**輸入：** Current installer/service layout and new persisted protocol.  
**產出：** Migration rules, installer preservation, compatibility tests.  
**依賴：** 2.3, 3.2.  
**Owner／Wave：** Primary agent / Wave 4.  
**Gate／Evidence：** G-MIGRATION; `target/openspec-evidence/event-driven-mft-index-updates/4.1.*`.  
**完成門檻：** Upgrade preserves compatible state or performs one safe rebuild; rollback ignores sidecars; no partial generation is consumed.

- [x] 4.1.1 Implement existing-base adoption or one-time rebuild and versioned sidecar handling without destructive broad cache removal.
- [x] 4.1.2 Update installer/service packaging only as needed to preserve compatible cache and service lifecycle behavior.
- [ ] 4.1.3 Add migration tests for legacy base, compatible restart, incompatible schema, interrupted upgrade, and rollback reader behavior.
- [ ] 4.1.4 Write the G-MIGRATION evidence records for tasks 4.1.1–4.1.3.
- [x] 4.1.5 Require the installer to confirm `STOPPED` before replacing the MFT service binary, restart it, confirm `RUNNING`, and register the lifecycle contract in UTIT.

### 4.2 NTFS integration and UTIT registration

**目的：** Verify real journal semantics and register repeatable installed-app/service evidence.  
**輸入：** Completed Service and Host paths, NTFS fixture tools, UTIT runner.  
**產出：** Mutation integration script/tests, manifest case, screenshots/reports where UI-visible.  
**依賴：** 4.1.  
**Owner／Wave：** Primary agent / Wave 4.  
**Gate／Evidence：** G-NTFS; `target/openspec-evidence/event-driven-mft-index-updates/4.2.*`.  
**完成門檻：** Real NTFS mutations reach Host values within ten seconds, unrelated cache persists, discontinuity recovers once, and the manifest validates.

- [ ] 4.2.1 Add an isolated NTFS mutation fixture covering create, grow, overwrite, truncate, rename, move, hard-link, and delete.
- [ ] 4.2.2 Add journal discontinuity and blocked-service-stop integration procedures with truthful capability skips only when NTFS prerequisites are absent.
- [x] 4.2.3 Register targeted UTIT cases, required artifacts, requirement selectors, timeouts, and exclusive service resources.
- [x] 4.2.4 Run the targeted integration/UTIT cases and write G-NTFS evidence records for tasks 4.2.1–4.2.3.

### 4.3 Installed-service resource and freshness gate

**目的：** Prove the original recurring memory/I/O problem is removed in the packaged build.  
**輸入：** Release binaries, test installer, installed service, representative NTFS volumes.  
**產出：** Time-series memory/CPU/index timestamps, freshness observations, installer hashes, final evidence index.  
**依賴：** 4.2.  
**Owner／Wave：** Primary agent / Wave 5.  
**Gate／Evidence：** G-INSTALLED; `target/openspec-evidence/event-driven-mft-index-updates/4.3.*`.  
**完成門檻：** Two idle minutes produce no base rewrite/30-second scan; mutation appears within ten seconds; incremental memory is not proportional to the base; clean stop and upgrade pass.

- [x] 4.3.1 Build debug/release binaries and `build_test_install.bat --no-launch`, recording artifact hashes.
- [x] 4.3.2 Install/upgrade the produced package and verify service identity, LocalSystem account, preserved cache, and journal mode diagnostics.
- [x] 4.3.3 Record at least two idle minutes of working set, private bytes, CPU, base/delta timestamps, and generations and assert no periodic rebuild signature.
- [x] 4.3.4 Mutate a representative file and assert Host folder size updates within ten seconds while unrelated cache remains valid and memory stays bounded.
- [x] 4.3.5 Stop the service during a blocked journal read and verify timely clean shutdown, then restart and verify cursor continuation without full rebuild.
- [ ] 4.3.6 Run formatting, focused tests, strict OpenSpec validation, UTIT manifest validation, scoped diff review, and write the complete G-INSTALLED evidence index.
