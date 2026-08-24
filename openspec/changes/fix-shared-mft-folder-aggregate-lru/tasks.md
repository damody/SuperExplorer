## 1. Baseline and Contract Freeze

### 1.1 Focused baseline and evidence contract

**目的：** Freeze the current failing path, focused command set, and auditable evidence format before behavior changes.
**輸入：** Approved source design, proposal, design, delta spec, current dirty worktree, `D:\trace`, installed service state.
**產出：** `evidence/index.jsonl`, baseline command logs, cache/service inventory, and an overlap-safe file ownership note.
**依賴：** None.
**Owner／Wave：** Primary integrator / Wave 1.
**Gate／Evidence：** G1 baseline; `evidence/1.1/` and unique `task_id` records in `evidence/index.jsonl`.
**完成門檻：** Every leaf has a record containing task ID, procedure or command, expected/actual result, exit status, artifact hashes, gate, timestamp, and disposition; unrelated worktree changes are identified and preserved.

- [ ] 1.1.1 Record the current revisions, dirty paths overlapping `application.rs`, `folder_size_service.rs`, `mft_query.rs`, and `mft_service.rs`, installed app/service versions, and `D:\trace` folder inventory without modifying source content.
- [ ] 1.1.2 Run and save the smallest existing MFT result-LRU, folder-size pending/cancellation, and IPC frame tests as the pre-change focused baseline.
- [ ] 1.1.3 Create the append-only `evidence/index.jsonl` schema and index the immutable baseline artifacts with hashes and unique task IDs.

### 1.2 Contract and call-path inventory

**目的：** Identify every Details aggregate consumer and the exact IPC/cache seams that must change without pulling Size Map into the service-result cache.
**輸入：** G1 baseline, current application/service code, existing MFT query frames and diagnostics.
**產出：** `evidence/1.2/call-path-inventory.md` mapping consumers, caches, fallbacks, generation checks, and test seams.
**依賴：** 1.1.
**Owner／Wave：** Primary integrator / Wave 1.
**Gate／Evidence：** G2 ownership boundary; `evidence/1.2/`.
**完成門檻：** Folder size, File Count, Folder Count, Code Lines admission, and Size Map paths each have one explicit post-change owner and no unresolved shared-cache ambiguity.

- [ ] 1.2.1 Trace built-in and extension Details Folder size requests from UI submission through Host cache/worker to MFT IPC and list every fallback branch.
- [ ] 1.2.2 Trace File Count and Folder Count dependency acquisition and confirm their terminal results can be supplied by the same direct aggregate response.
- [ ] 1.2.3 Trace Size Map projection ownership and record the exact APIs/namespaces that remain outside the Details result LRU.
- [ ] 1.2.4 Freeze any IPC diagnostic field additions and backward-version behavior required by the delta spec before editing producers or consumers.

## 2. Service-Owned Shared Result Database

### 2.1 Bounded true-LRU result store

**目的：** Replace ad hoc result-map accounting with a service-global cache that implements the approved promotion, cost, byte-limit, and entry-limit contract.
**輸入：** G2 ownership/contract inventory and existing `ServiceFolderAggregateCacheV1` tests.
**產出：** Result entry/key model, accounting helpers, insert/get/replace/trim operations, and focused unit tests in the MFT service target.
**依賴：** 1.2.
**Owner／Wave：** Primary integrator / Wave 2.
**Gate／Evidence：** G3 LRU correctness; `evidence/2.1/`.
**完成門檻：** Focused tests prove promotion, replacement, least-recent eviction, 192-byte minimum accounting, formula-derived count limit, immediate lower-limit trim, oversized non-retention, and isolation from other MFT stores.

- [ ] 2.1.1 Introduce the retained result entry with aggregate value, valid-through generation, accounted cost, and monotonic last-access sequence.
- [ ] 2.1.2 Implement successful-hit and replacement promotion without changing results for an invalid generation proof.
- [ ] 2.1.3 Implement conservative per-entry accounting with a 192-byte minimum and checked total-byte updates.
- [ ] 2.1.4 Implement the `max(1, min(effective_lru_bytes / 192, 262144))` entry ceiling alongside the existing byte limit.
- [ ] 2.1.5 Implement insertion and limit-change trimming of the true least-recently-used entries while returning but not retaining an oversized result.
- [ ] 2.1.6 Add focused LRU tests covering order, replacement, both limits, lowering, oversized result, and proof that volume/SQLite/aggregate stores are unchanged by result eviction.

### 2.2 Journal-aware validity and stale rejection

**目的：** Keep unaffected cached facts warm across safe journal updates while making stale exact publication impossible.
**輸入：** 2.1 result store, journal change application, ancestor/reference APIs, volume generation state.
**產出：** Pre-advance invalidation transaction, volume fallback clearing, generation-proof checks, and focused invalidation tests.
**依賴：** 2.1.
**Owner／Wave：** Primary integrator / Wave 2.
**Gate／Evidence：** G4 generation correctness; `evidence/2.2/`.
**完成門檻：** Tests prove affected ancestors are removed before generation advance, unaffected entries remain hits, unknown closure clears only that volume, and late old-generation work cannot publish.

- [ ] 2.2.1 Collect the complete changed-reference and ancestor closure needed to invalidate aggregate results before applying a generation advance.
- [ ] 2.2.2 Add the volume-scoped fallback that clears result entries when the complete affected closure cannot be proven.
- [ ] 2.2.3 Carry unaffected entries forward as valid through the advanced volume cache generation.
- [ ] 2.2.4 Reject late computation insertion and response publication when its observed generation is no longer current.
- [ ] 2.2.5 Add focused tests for precise invalidation, rename/remove ancestry, unknown-closure fallback, cross-volume isolation, unaffected warm hits, and stale completion.

### 2.3 Global single-flight and optimized source order

**目的：** Coalesce same-key misses across clients without serializing unrelated keys and keep all expensive source selection inside the service.
**輸入：** 2.1 result store, 2.2 generation checks, current live-index/SQLite/build query paths.
**產出：** Generation-bound in-flight registry, leader/waiter lifecycle, source classification, and concurrency/restart tests.
**依賴：** 2.1 and 2.2.
**Owner／Wave：** Primary integrator / Wave 3.
**Gate／Evidence：** G5 shared computation; `evidence/2.3/`.
**完成門檻：** Concurrent same-key clients observe one computation, different keys progress independently, every leader exit releases waiters, and restart tests prove SQLite-first cold recovery followed by a warm LRU hit.

- [ ] 2.3.1 Add a service-global in-flight identity keyed by volume identity, folder reference, and observed generation.
- [ ] 2.3.2 Implement leader/joiner coordination that never waits while holding result-cache, live-volume, or SQLite mutation locks.
- [ ] 2.3.3 Guarantee registry removal and waiter terminalization for success, service error, stale generation, disconnect, and unwinding-safe failure paths.
- [ ] 2.3.4 Instrument and preserve source order: valid memory aggregate, current read-only SQLite, then bounded aggregate build.
- [ ] 2.3.5 Add focused concurrency tests for same-key coalescing, different-key independence, failed leader cleanup, and stale leader rejection.
- [ ] 2.3.6 Add a focused restart fixture proving cold SQLite recovery reads no user file contents and the next client obtains a result-LRU hit.

## 3. IPC and Host Direct Query Integration

### 3.1 Versioned diagnostics and response contract

**目的：** Expose enough backward-safe data to validate service-global cache behavior and terminal response generations.
**輸入：** 1.2 frozen contract, 2.1–2.3 counters and generation semantics, current fixed frames.
**產出：** Updated versioned encode/decode logic, compatibility checks, and focused protocol tests.
**依賴：** 2.1, 2.2, and 2.3.
**Owner／Wave：** Primary integrator / Wave 4.
**Gate／Evidence：** G6 protocol compatibility; `evidence/3.1/`.
**完成門檻：** Producer/consumer round trips expose all approved counters and generation fields, contain no paths, and reject unsupported or malformed versions without shifting legacy fields.

- [ ] 3.1.1 Add result insertion/replacement/eviction, single-flight leader/joiner, stale rejection, and memory/SQLite/build source counters to service diagnostics.
- [ ] 3.1.2 Extend versioned response/diagnostic framing without reinterpreting existing offsets or accepting unsupported versions.
- [ ] 3.1.3 Add focused round-trip, old-version, unsupported-version, malformed-frame, overflow, and privacy-field tests.

### 3.2 Direct Details query path and terminal UI state

**目的：** Remove Host aggregate-cache/fallback ownership while preserving current-view deduplication, cancellation, and correct terminal rendering.
**輸入：** G6 IPC contract, current `ApplicationVisualColumnRuntimeV1`, `FolderSizeServiceV1`, Folder size/Directory Facts UI ports.
**產出：** Direct service worker path, bounded current-view pending state, stale response checks, and focused Host/UI tests.
**依賴：** 3.1.
**Owner／Wave：** Primary integrator / Wave 4.
**Gate／Evidence：** G7 direct ownership and terminal state; `evidence/3.2/`.
**完成門檻：** Details facts never read/write Host aggregate cache or recurse on service failure; complete/partial/unavailable/timeout results clear loading; cancelled or old-generation results cannot publish; Code Lines `Limit` behavior is unchanged.

- [ ] 3.2.1 Replace the Details Folder size worker's Host-cache/`aggregate_or_scan` path with direct bounded `mft_query` requests.
- [ ] 3.2.2 Project Folder size, File Count, and Folder Count from one service aggregate response while retaining request deduplication and active generation identity.
- [ ] 3.2.3 Remove Details participation in `HostExtensionColumnCacheV1<FolderSizeCachedValueV1>` and persistent folder snapshot writes without removing Size Map tree APIs.
- [ ] 3.2.4 Remove recursive and Everything fallback from the Details path and map service timeout/unavailable/malformed outcomes to explicit terminal results.
- [ ] 3.2.5 Preserve cancellation and reject late responses for obsolete request/tab generations.
- [ ] 3.2.6 Add focused Host/UI tests for cache bypass, no recursive fallback, complete, partial, unavailable, timeout, cancellation, stale response, and loading-state removal.
- [ ] 3.2.7 Run the focused Code Lines admission tests and record that existing File Count threshold and red `Limit` behavior remain unchanged.

## 4. Safe Retirement of Obsolete Host Snapshots

### 4.1 Bounded startup maintenance

**目的：** Stop obsolete Details cache growth and retire only validated cache files without touching Size Map or source content.
**輸入：** Approved exact cache root, obsolete record schema/path helper, Windows reparse metadata APIs, G7 direct path.
**產出：** Versioned maintenance marker/function, path-safe bounded cleanup tests, and migration evidence.
**依賴：** 3.2.
**Owner／Wave：** Primary integrator / Wave 5; destructive action remains with the primary agent.
**Gate／Evidence：** G8 migration safety; `evidence/4.1/`.
**完成門檻：** One launch removes at most 256 oldest validated immediate regular files, unsafe/unexpected entries are untouched, failures are non-fatal, repeated launches converge, and fixture hashes prove source and Size Map content unchanged.

- [ ] 4.1.1 Implement exact-root validation that rejects symlink/reparse cache roots and never traverses subdirectories.
- [ ] 4.1.2 Validate immediate obsolete regular records and order only eligible files oldest first.
- [ ] 4.1.3 Remove at most 256 eligible files per launch and make individual metadata/removal failures non-fatal.
- [ ] 4.1.4 Keep Size Map projection namespaces outside the cleanup target and record a versioned migration completion marker only when no eligible obsolete file remains.
- [ ] 4.1.5 Add focused fixtures for limit 255/256/257, ordering, repeat convergence, corrupt records, symlinks/reparse points, subdirectories, removal failure, and exact-root escape attempts.
- [ ] 4.1.6 Hash a source fixture before/after maintenance and prove no source or Size Map projection content changed.

## 5. Focused Integration and Installed Acceptance

### 5.1 Focused automated verification

**目的：** Run only the automated checks that directly prove this change and capture reproducible evidence.
**輸入：** G3–G8 implementations and focused tests.
**產出：** Formatter output, selected Rust test logs, targeted build result, traceability matrix, and indexed hashes.
**依賴：** 2.1–4.1.
**Owner／Wave：** Primary integrator / Wave 6.
**Gate／Evidence：** G9 focused automated acceptance; `evidence/5.1/`.
**完成門檻：** Formatting, selected service/query/application/UI tests, and the smallest installable-target build all exit zero; every spec scenario maps to a passing focused test or the installed gate; the complete workspace suite is not required.

- [ ] 5.1.1 Run formatter/check on changed Rust files and save the exact command and output.
- [ ] 5.1.2 Run the selected MFT service LRU, invalidation, single-flight, source-order, and diagnostics tests.
- [ ] 5.1.3 Run the selected Host direct-query, cancellation, terminal-state, migration, and Code Lines regression tests.
- [ ] 5.1.4 Build the smallest matched Super Explorer and MFT Service installable targets needed for headful validation.
- [ ] 5.1.5 Produce a proposal→decision→requirement/scenario→task→evidence traceability matrix and index all immutable logs/hashes.

### 5.2 Installed `D:\trace` shared-cache acceptance

**目的：** Prove the user-reported folder and multi-process shared optimization in the installed application.
**輸入：** G9 matched binaries, running installed MFT Service, `D:\trace`, diagnostics capture, screenshot/automation tooling.
**產出：** Cold/warm run logs, two-process counter evidence, terminal-state screenshot, working-set samples, and source hashes.
**依賴：** 5.1.
**Owner／Wave：** Primary integrator / Wave 7; service install/restart and installed execution are performed only by the primary agent.
**Gate／Evidence：** G10 installed acceptance; `evidence/5.2/`.
**完成門檻：** Every visible `D:\trace` folder reaches exact/partial/unavailable rather than indefinite `Calculating...`; the second process increases shared hits without duplicate same-key computation; limits hold; Code Lines `Limit` remains correct; `D:\trace` hashes are unchanged.

- [ ] 5.2.1 Record pre-install hashes/versions, install the matched app/service build, and verify the running binaries match the evidence hashes.
- [ ] 5.2.2 Capture a cold `D:\trace` Details run until every visible Folder size cell reaches a terminal state and save latency/counter evidence.
- [ ] 5.2.3 Open a second Super Explorer process on `D:\trace` and prove service-global warm hits and no duplicate same-key leader computation.
- [ ] 5.2.4 Apply focused result-LRU pressure or a lower test limit and prove entries/bytes settle within both limits while folder queries still succeed.
- [ ] 5.2.5 Capture the final Details screenshot showing terminal Folder size cells and unchanged red Code Lines `Limit` cells.
- [ ] 5.2.6 Record MFT Service working-set samples and LRU/source counters for cold and warm runs without path-bearing telemetry.
- [ ] 5.2.7 Re-hash the `D:\trace` validation sample and prove installed queries and migration changed no source content.

### 5.3 Final scoped review and handoff

**目的：** Confirm the implementation stayed within the approved design, preserved unrelated work, and leaves an auditable focused result.
**輸入：** G1–G10 evidence, final diff, OpenSpec artifacts, installed acceptance output.
**產出：** `evidence/5.3/final-review.md`, final evidence index, and completed task statuses.
**依賴：** 5.2.
**Owner／Wave：** Primary integrator / Wave 8.
**Gate／Evidence：** G11 release recommendation; `evidence/5.3/`.
**完成門檻：** No unresolved P0/P1 correctness, stale-generation, data-safety, or indefinite-loading issue remains; all changed files trace to scope; every completed leaf has valid evidence; no complete-suite claim is made.

- [ ] 5.3.1 Review the final diff for scope, cache ownership, lock ordering, stale publication, unsafe deletion, and preservation of unrelated dirty-worktree edits.
- [ ] 5.3.2 Re-run `openspec validate fix-shared-mft-folder-aggregate-lru --strict` and scan artifacts/evidence for incomplete markers, contradictions, stale records, and missing scenario traceability.
- [ ] 5.3.3 Finalize the evidence index with unique task records, hashes, timestamps, actual outcomes, and any A/B adjustment lineage.
- [ ] 5.3.4 Write the final scoped verification report, including commands run, tests intentionally omitted, installed results, remaining non-blocking risks, and rollback procedure.
