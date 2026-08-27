## 1. Contract and deterministic foundations

### 1.1 Reconcile durability contracts

**目的：** Establish one non-contradictory durability contract before product edits.
**輸入：** Approved source design, this proposal/design/spec, active `event-driven-mft-index-updates` artifacts.
**產出：** Reconciled OpenSpec text and a requirement-to-gate traceability table.
**依賴：** None.
**Owner／Wave：** Primary integrator / Wave 1.
**Gate／Evidence：** `G-REGRESSION`; `target/openspec-evidence/mft-sqlite-foreground-persistence/1.1.*/result.json`.
**完成門檻：** No active artifact requires five-to-ten-second durable sidecars; in-memory freshness and ten-minute focused durability remain distinct and strict validation passes.

- [x] 1.1.1 Inventory every active requirement, test, script, and status field that assumes `.semftcp`, `.semftdelta`, `.semftstatus`, or ten-second durable publication; record paths and dispositions.
- [x] 1.1.2 Update the active `event-driven-mft-index-updates` artifacts so this change supersedes only durable sidecar timing while preserving USN ingestion, coalescing, query freshness, and bounded correctness behavior.
- [x] 1.1.3 Create a traceability table mapping every approved source-design clause and requirement scenario in this change to its blocking gate, implementation work package, and evidence task ID.
- [x] 1.1.4 Run strict OpenSpec validation for both changes and save distinct raw results proving no contradictory durability contract remains.

### 1.2 Add deterministic scheduler and lease models

**目的：** Make timing, focus, retry, and shutdown decisions testable without wall-clock sleeps or UI automation.
**輸入：** Reconciled contracts and existing MFT query/journal types.
**產出：** Pure clock-driven persistence policy, focus-lease registry, typed volume durability/recovery states, and unit fixtures.
**依賴：** 1.1.
**Owner／Wave：** MFT contract owner / Wave 1.
**Gate／Evidence：** `G-FOCUS-AUTH`, `G-TEN-MINUTE`, `G-NO-SHUTDOWN-WRITE`; `target/openspec-evidence/mft-sqlite-foreground-persistence/1.2.*/result.json`.
**完成門檻：** Deterministic tests cover every timing/focus boundary and no policy test requires a real ten-minute delay.

- [x] 1.2.1 Define typed observed cursor, durable cursor, pending batch, commit reference time, retry backoff, migration, and rebuild-required states with explicit invariants.
- [x] 1.2.2 Implement an injectable monotonic-clock persistence eligibility policy whose fixed ten-minute write-attempt interval advances immediately before `BEGIN` for both successful and failed attempts.
- [x] 1.2.3 Implement a connection-bound focus-lease registry supporting acquire/renew, explicit release, disconnect removal, expiry, and multi-window aggregation.
- [x] 1.2.4 Add deterministic boundary tests for 9:59.999, 10:00, no focus, late focus, repeated renewal, failed attempts after WAL writes, lost focus during commit, and shutdown inhibition.

## 2. SQLite durability core

### 2.1 Implement the versioned per-volume SQLite store

**目的：** Replace generation sidecars with one crash-consistent database per volume.
**輸入：** State invariants, current `MftIndexV1`/change codecs, workspace bundled `rusqlite`.
**產出：** Focus-independent SQLite store module and schema tests.
**依賴：** 1.2.
**Owner／Wave：** SQLite store owner / Wave 2.
**Gate／Evidence：** `G-SQLITE-ATOMIC`, `G-FIXED-FILES`; `target/openspec-evidence/mft-sqlite-foreground-persistence/2.1.*/result.json`.
**完成門檻：** Store opens only valid identities/schemas, atomically binds entries to cursor, and creates no generation-named files.

- [x] 2.1.1 Add `rusqlite` to the owning `explorer-app` target and create a focused MFT SQLite module without duplicating the existing workspace dependency version.
- [x] 2.1.2 Define and initialize the versioned metadata/entries schema with explicit volume identity, journal identity, cursor, generation, completeness, and rebuild metadata.
- [x] 2.1.3 Configure and assert bundled SQLite WAL mode, `synchronous=NORMAL`, bounded busy timeout, `wal_autocheckpoint=0`, and `SQLITE_DBCONFIG_NO_CKPT_ON_CLOSE` for every applicable sole-writer connection.
- [x] 2.1.4 Implement strict open/admission validation for schema, integrity, volume/journal identity, cursor bounds, and fixed-root path policy.
- [x] 2.1.5 Add schema round-trip, incompatible-schema, corrupt-store, identity-mismatch, fixed active-file-set, and last-close-with-outstanding-WAL unit tests that inspect main/WAL/SHM bytes and directory events.

### 2.2 Make batch and cursor commit atomic

**目的：** Prove no crash or failure can durable-skip an observed USN change.
**輸入：** SQLite store, coalesced MFT changes, cursor invariants.
**產出：** Transaction API, failure injection seams, restart tests.
**依賴：** 2.1.
**Owner／Wave：** SQLite store owner / Wave 2.
**Gate／Evidence：** `G-SQLITE-ATOMIC`, `G-RESTART`; `target/openspec-evidence/mft-sqlite-foreground-persistence/2.2.*/result.json`.
**完成門檻：** All success/failure/crash-boundary tests show entries and cursor move together, with later events retained for the next batch.

- [x] 2.2.1 Implement a transaction that applies captured upserts/deletes and advances durable metadata as one commit.
- [x] 2.2.2 Separate the captured commit batch from events arriving concurrently so the committed cursor cannot cover uncaptured changes.
- [x] 2.2.3 Add failure injection before mutation, before cursor update, and before commit; verify reopen admits only the previous complete state.
- [x] 2.2.4 Add idempotent restart/replay tests proving a retained journal gap reconstructs the exact current memory index without immediate persistence.

### 2.3 Implement bounded WAL maintenance and telemetry

**目的：** Prevent hidden automatic DB rewrites while keeping WAL growth observable and maintainable.
**輸入：** SQLite connection policy, scheduler focus state, cache telemetry contracts.
**產出：** WAL threshold controller and independent telemetry fields.
**依賴：** 2.1, 1.2.
**Owner／Wave：** SQLite store owner / Wave 2.
**Gate／Evidence：** `G-WAL-BOUND`; `target/openspec-evidence/mft-sqlite-foreground-persistence/2.3.*/result.json`.
**完成門檻：** No checkpoint occurs at or below 256 MiB or without focus; eligible truncate and failure paths are independently evidenced.

- [x] 2.3.1 Add exact main DB/WAL byte accounting, transaction/checkpoint counters, last outcomes, and last durable/observed cursor telemetry.
- [x] 2.3.2 Implement eligibility for `wal_checkpoint(TRUNCATE)` only above 256 MiB, while focused, and without conflicting transaction/query-critical work.
- [x] 2.3.3 Implement conservative pre-`BEGIN` WAL-growth admission and block WAL-appending commits at the computed 256 MiB-plus-one-bounded-batch-and-frame-overhead hard limit until checkpoint succeeds.
- [x] 2.3.4 Add tests proving automatic/close checkpointing is disabled and WAL below/equal/above-threshold boundaries obey focus and concurrency gates.
- [x] 2.3.5 Inject sustained busy/failure checkpoint outcomes and verify committed reads remain valid, retries wait for later focused opportunities, and actual WAL bytes remain within the computed hard bound.

## 3. Service and interactive-process integration

### 3.1 Convert volume workers to memory-first state

**目的：** Serve current USN-derived results without routine persistence.
**輸入：** Current journal workers/query cache, scheduler model, SQLite transaction API.
**產出：** Integrated volume runtime with observed/durable separation and bounded overload behavior.
**依賴：** 1.2, 2.2.
**Owner／Wave：** MFT service integrator / Wave 3.
**Gate／Evidence：** `G-TEN-MINUTE`, `G-RESTART`, `G-FIXED-FILES`, `G-REGRESSION`; `target/openspec-evidence/mft-sqlite-foreground-persistence/3.1.*/result.json`.
**完成門檻：** Queries see live memory state, commits obey both gates, and overflow/recovery cannot write or claim exactness while unfocused.

- [x] 3.1.1 Refactor `watch_volume` so journal reads update a coherent in-memory index and pending map without publishing sidecars or status files.
- [x] 3.1.2 Connect the ten-minute/focus scheduler to one serialized per-volume transaction and retain events arriving during commit in the next batch.
- [x] 3.1.3 Route aggregate/query generations and diagnostics to current proven memory state while preserving cache-budget enforcement and typed partial outcomes.
- [x] 3.1.4 Change overflow, ambiguity, journal replacement, cursor loss, and corruption to `rebuild-required` without background persistence or rebuild.
- [x] 3.1.5 Serialize foreground-gated rebuilds across volumes and verify failed rebuilds never promote an unproven exact state.

### 3.2 Add versioned authenticated focus IPC

**目的：** Let interactive windows accurately and safely open the service persistence gate.
**輸入：** Existing MFT framed named-pipe protocol, focus-lease registry, application window lifecycle.
**產出：** Backward-compatible lease requests/responses and UI focus reporting.
**依賴：** 1.2.
**Owner／Wave：** IPC/application integrator / Wave 3.
**Gate／Evidence：** `G-FOCUS-AUTH`, `G-REGRESSION`; `target/openspec-evidence/mft-sqlite-foreground-persistence/3.2.*/result.json`.
**完成門檻：** Authorized focus is aggregated across windows; unauthorized/expired/disconnected clients cannot authorize writes; older clients retain supported queries.

- [x] 3.2.1 Define and implement a dedicated multi-instance lease pipe with bounded frames/connections, overlapped-I/O deadlines, stop cancellation, and isolation from ordinary one-request query IPC.
- [x] 3.2.2 Obtain and hold the named-pipe client process handle and verify PID creation identity, token SID, active session, and protected installed-image/file identity before accepting client-reported GPUI focus.
- [x] 3.2.3 Bind lease ownership to the verified process/connection/session and remove leases on release, disconnect, expiry, session switch, foreground mismatch, or process identity change.
- [x] 3.2.4 Integrate Super Explorer window activation/deactivation with lease renewal/release without blocking the GPUI thread.
- [x] 3.2.5 Add compatibility/security tests for old/new endpoints, same-user spoof, copied image, PID reuse, wrong session, session switch, false/expired focus reports, malformed/partial/stalled frames, caps, expiry, disconnect, and multiple simultaneous windows/queries.
- [x] 3.2.6 Add application/service lifecycle tests proving focus churn renews one logical lease, process exit cannot leave the gate open, stalled I/O is canceled, and service stop remains bounded.

### 3.3 Enforce no-write shutdown lifecycle

**目的：** Ensure stop and Windows shutdown never bypass user-approved persistence conditions.
**輸入：** Service control handler, worker cancellation, SQLite lifecycle, migration/rebuild schedulers.
**產出：** Auditable no-final-write stop path and restart behavior.
**依賴：** 3.1, 3.2.
**Owner／Wave：** MFT service integrator / Wave 3.
**Gate／Evidence：** `G-NO-SHUTDOWN-WRITE`, `G-RESTART`; `target/openspec-evidence/mft-sqlite-foreground-persistence/3.3.*/result.json`.
**完成門檻：** Stop completes cleanly with pending work; evidence permits only an already-linearized commit and proves no new transaction, close checkpoint/backfill/truncate/unlink, migration, rebuild, cleanup, or final status write occurred.

- [x] 3.3.1 Register and handle `SERVICE_ACCEPT_SHUTDOWN` and `SERVICE_CONTROL_SHUTDOWN` in addition to stop, closing a lifecycle barrier before worker cancellation.
- [x] 3.3.2 Recheck the lifecycle barrier before `BEGIN` and commit invocation, rolling back when stop wins before commit linearization and permitting only an already-invoked commit to finish.
- [x] 3.3.3 Close SQLite with no-checkpoint-on-close and discard pending batches without implicit backfill, truncate, or unlink, including overdue-unfocused and focused-pending cases.
- [x] 3.3.4 Inject stop/shutdown at snapshot, pre-`BEGIN`, mutation, pre-commit invocation, commit invocation, and post-commit boundaries; verify the linearization rule and no later durability work.
- [x] 3.3.5 Add restart tests for exact journal catch-up and typed foreground-gated rebuild when the retained range no longer covers the gap.

## 4. Migration, packaging, and cleanup safety

### 4.1 Implement verified legacy-to-SQLite migration

**目的：** Upgrade existing installations without losing the last valid MFT state or deleting unrelated data.
**輸入：** Legacy readers, SQLite store, scheduler gates, fixed cache-root policy.
**產出：** Read-only legacy admission, gated temporary build, atomic promotion, reopen verification, and scoped cleanup inventory.
**依賴：** 2.2, 3.1, 3.2.
**Owner／Wave：** Migration owner / Wave 4.
**Gate／Evidence：** `G-MIGRATION`, `G-FIXED-FILES`; `target/openspec-evidence/mft-sqlite-foreground-persistence/4.1.*/result.json`.
**完成門檻：** Every failure before verified reopen preserves all legacy files; successful cleanup targets only inventoried recognized files inside the resolved fixed root.

- [x] 4.1.1 Admit valid legacy chains for migration only when the canonical SQLite main/WAL/SHM set is entirely absent; reject an existing invalid canonical set into quarantine/rebuild-required without startup writes.
- [x] 4.1.2 Gate construction of a migration-only rollback-journal SQLite store on both approved gates; close/fsync and pre-validate its self-contained main file with no live WAL/SHM set.
- [x] 4.1.3 Atomically install the canonical main file only after confirming no canonical WAL/SHM exists, reopen it in WAL/no-checkpoint-on-close mode, and repeat admission verification before cleanup.
- [x] 4.1.4 Resolve and validate exact cleanup targets against the fixed root and approved volume-specific legacy filename patterns; emit a pre-delete inventory with hashes.
- [x] 4.1.5 Remove only verified legacy targets after successful promotion and emit a post-delete recovery/disposition record.
- [x] 4.1.6 Inventory invalid canonical sets/orphans without startup mutation; implement a foreground-gated fsynced quarantine-intent manifest and identity-checked idempotent per-member disposition that blocks admission until complete.
- [x] 4.1.7 Add fault-matrix tests at build, temp-journal commit, fsync, pre-verify, promote, reopen, post-verify, quarantine intent, each main/WAL/SHM move, resume/completion, inventory, and cleanup boundaries plus live-temp-WAL rejection, path-escape, identity-change, and unrecognized-file cases.

### 4.2 Update installer and rollback behavior

**目的：** Ensure the installed LocalSystem service carries SQLite and upgrades/reverts predictably.
**輸入：** Service binary changes, installer service lifecycle, migration policy.
**產出：** Updated package, upgrade/rollback instructions, and installed lifecycle tests.
**依賴：** 4.1, 3.3.
**Owner／Wave：** Packaging integrator / Wave 4.
**Gate／Evidence：** `G-MIGRATION`, `G-NO-SHUTDOWN-WRITE`, `G-REGRESSION`; `target/openspec-evidence/mft-sqlite-foreground-persistence/4.2.*/result.json`.
**完成門檻：** Fresh install, legacy upgrade, repair, uninstall/service stop, and rollback disposition pass without external SQLite runtime or shutdown writes.

- [x] 4.2.1 Verify the bundled SQLite symbols/runtime are contained in the packaged service and no new DLL/system installation dependency is introduced.
- [x] 4.2.2 Update installer upgrade/repair handling to preserve the MFT cache and let the service own gated migration rather than deleting cache eagerly.
- [x] 4.2.3 Document rollback behavior: older binaries ignore SQLite and rebuild legacy state without requiring stop-time SQLite deletion.
- [ ] 4.2.4 Run fresh-install, legacy-upgrade, repair, service-stop, uninstall, and rollback test procedures with exact package/service identities and retained artifacts.

## 5. Verification and performance evidence

### 5.1 Run automated correctness and quality gates

**目的：** Prove all contracts before installed behavioral measurement.
**輸入：** Integrated implementation and fixtures.
**產出：** Raw unit/integration/protocol/lifecycle results and indexed evidence.
**依賴：** 3.3, 4.2.
**Owner／Wave：** Primary integrator / Wave 5.
**Gate／Evidence：** `G-SQLITE-ATOMIC`, `G-FOCUS-AUTH`, `G-TEN-MINUTE`, `G-NO-SHUTDOWN-WRITE`, `G-RESTART`, `G-WAL-BOUND`, `G-MIGRATION`, `G-FIXED-FILES`, `G-REGRESSION`; `target/openspec-evidence/mft-sqlite-foreground-persistence/5.1.*/result.json`.
**完成門檻：** Every independently failing suite passes with unique evidence; no ignored, stale, or unevidenced result is counted complete.

- [x] 5.1.1 Run targeted SQLite schema/transaction/failure/restart tests and index each subcheck against `G-SQLITE-ATOMIC` and `G-RESTART`.
- [x] 5.1.2 Run scheduler/focus/authentication/protocol compatibility tests and index each subcheck against `G-FOCUS-AUTH` and `G-TEN-MINUTE`.
- [x] 5.1.3 Run shutdown, overflow, rebuild, migration fault-matrix, path-safety, and fixed-file tests with separate gate records.
- [ ] 5.1.4 Run formatter, package checks, clippy with warnings denied, and relevant workspace tests using locked/offline commands where supported.
- [x] 5.1.5 Validate the evidence index schema, hashes, unique task IDs/subchecks, source revision, timestamps, and adjustment lineage.

### 5.2 Capture installed foreground/background and Defender evidence

**目的：** Demonstrate the user-visible reduction in cache churn without relying on Defender memory as a proxy.
**輸入：** Versioned installed package, representative NTFS mutation fixture, process/file event collection procedure.
**產出：** Background and focused traces, cache inventories, Defender CPU/I/O comparison, and analysis report.
**依賴：** 5.1.
**Owner／Wave：** Primary agent (privileged installed operations) / Wave 5.
**Gate／Evidence：** `G-TEN-MINUTE`, `G-FIXED-FILES`, `G-DEFENDER-IO`, `G-NO-SHUTDOWN-WRITE`; `target/openspec-evidence/mft-sqlite-foreground-persistence/5.2.*/result.json`.
**完成門檻：** At least twenty unfocused minutes show no prohibited writes; focused intervals obey cadence; active files remain fixed; Defender comparison reports CPU/I/O/file events and limitations.

- [x] 5.2.1 Record the pre-change cache inventory, file-create/write cadence, service/Defender CPU and I/O counters, exact binary identity, and environment metadata using a repeatable procedure.
- [x] 5.2.2 Install the candidate package and verify service path, account, binary hash/version, SQLite configuration, and initial migration state.
- [x] 5.2.3 Run at least twenty minutes of representative NTFS mutations while Super Explorer is unfocused and record zero periodic transaction/checkpoint/migration/rebuild/cleanup/generation-file events.
- [x] 5.2.4 Run focused mutations and injected post-WAL write failures across at least two persistence deadlines and prove no more than one disk-write attempt per volume per ten-minute interval.
- [ ] 5.2.5 Exercise focus loss/reacquisition, multi-window leases, same-user spoof rejection, crash/disconnect/session-switch expiry, SCM stop with pending work, and restart catch-up in installed-service traces.
- [ ] 5.2.6 Exercise Windows shutdown/reboot with pending work and outstanding WAL frames; prove lifecycle linearization and no implicit close checkpoint, backfill, truncate, or unlink.
- [x] 5.2.7 Compare pre/post Defender CPU/I/O and cache file events, state environmental limits, and explicitly exclude working-set-only conclusions.

## 6. Final integration and release disposition

### 6.1 Complete traceability and independent review

**目的：** Establish that the change is complete, non-contradictory, and safe to archive/apply as a release candidate.
**輸入：** All implementation artifacts, evidence, active related changes, and approved thresholds.
**產出：** Final report, reconciled statuses, strict validation, and review disposition.
**依賴：** 5.2.
**Owner／Wave：** Primary integrator / Wave 6.
**Gate／Evidence：** All gates; `target/openspec-evidence/mft-sqlite-foreground-persistence/6.1.*/result.json`.
**完成門檻：** Every scenario traces to passing evidence, all P0/P1 findings are fixed, strict validation passes, and no task/evidence remains stale, blocked, failed, or silently weakened.

- [x] 6.1.1 Reconcile proposal, design, specs, implementation, installer behavior, diagnostics, scripts, and both related OpenSpec task states against actual evidence.
- [x] 6.1.2 Produce the final requirement-to-scenario-to-gate-to-task-to-evidence matrix and identify any evidence made stale or superseded by adjustments.
- [ ] 6.1.3 Obtain an independent architecture/security/concurrency/migration/test-completeness review and resolve every P0/P1 finding.
  - `govern-dead-code-warnings` retains `RecoveryReasonV1` and `MigrationStateV1` as this task's typed diagnostics contract; remove its narrow expectations when this review completes the wiring.
- [x] 6.1.4 Run the detailed-task validator, placeholder/contradiction scan, `openspec validate --strict`, and final status checks; save all raw outputs.
- [x] 6.1.5 Write the final release/rollback report with exact installed identities, measured cadence, Defender evidence limitations, known residual risks, and approval disposition.
