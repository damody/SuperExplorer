## 1. Baseline and Contract Freeze

### 1.1 Focused baseline and evidence contract

**目的：** Freeze the current failing path, focused command set, and auditable evidence format before behavior changes.
**輸入：** Approved source design, proposal, design, delta spec, current dirty worktree, `D:\trace`, installed service state.
**產出：** `evidence/index.jsonl`, baseline command logs, cache/service inventory, and an overlap-safe file ownership note.
**依賴：** None.
**Owner／Wave：** Primary integrator / Wave 1.
**Gate／Evidence：** G1 baseline; `evidence/1.1/` and unique `task_id` records in `evidence/index.jsonl`.
**完成門檻：** Every leaf has a record containing task ID, procedure or command, expected/actual result, exit status, artifact hashes, gate, timestamp, and disposition; unrelated worktree changes are identified and preserved.

- [x] 1.1.1 Record the current revisions, dirty paths overlapping `application.rs`, `folder_size_service.rs`, `mft_query.rs`, and `mft_service.rs`, installed app/service versions, and `D:\trace` folder inventory without modifying source content.
- [x] 1.1.2 Run and save the smallest existing MFT result-LRU, folder-size pending/cancellation, and IPC frame tests as the pre-change focused baseline.
- [ ] 1.1.3 Create the append-only `evidence/index.jsonl` schema and index the immutable baseline artifacts with hashes and unique task IDs.

### 1.2 Contract and call-path inventory

**目的：** Identify every Details aggregate consumer and the exact IPC/cache seams that must change without pulling Size Map into the service-result cache.
**輸入：** G1 baseline, current application/service code, existing MFT query frames and diagnostics.
**產出：** `evidence/1.2/call-path-inventory.md` mapping consumers, caches, fallbacks, generation checks, and test seams.
**依賴：** 1.1.
**Owner／Wave：** Primary integrator / Wave 1.
**Gate／Evidence：** G2 ownership boundary; `evidence/1.2/`.
**完成門檻：** Folder size, File Count, Folder Count, Code Lines admission, and Size Map paths each have one explicit post-change owner and no unresolved shared-cache ambiguity.

- [x] 1.2.1 Trace built-in and extension Details Folder size requests from UI submission through Host cache/worker to MFT IPC and list every fallback branch.
- [x] 1.2.2 Trace File Count and Folder Count dependency acquisition and confirm their terminal results can be supplied by the same direct aggregate response.
- [x] 1.2.3 Trace Size Map projection ownership and record the exact APIs/namespaces that remain outside the Details result LRU.
- [x] 1.2.4 Freeze any IPC diagnostic field additions and backward-version behavior required by the delta spec before editing producers or consumers.

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
- [x] 2.1.2 Implement successful-hit and replacement promotion without changing results for an invalid generation proof.
- [x] 2.1.3 Implement conservative per-entry accounting with a 192-byte minimum and checked total-byte updates.
- [x] 2.1.4 Implement the `max(1, min(effective_lru_bytes / 192, 262144))` entry ceiling alongside the existing byte limit.
- [x] 2.1.5 Implement insertion and limit-change trimming of the true least-recently-used entries while returning but not retaining an oversized result.
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
- [x] 2.2.2 Add the volume-scoped fallback that clears result entries when the complete affected closure cannot be proven.
- [ ] 2.2.3 Carry unaffected entries forward as valid through the advanced volume cache generation.
- [x] 2.2.4 Reject late computation insertion and response publication when its observed generation is no longer current.
- [ ] 2.2.5 Add focused tests for precise invalidation, rename/remove ancestry, unknown-closure fallback, cross-volume isolation, unaffected warm hits, and stale completion.

### 2.3 Global single-flight and optimized source order

**目的：** Coalesce same-key misses across clients without serializing unrelated keys and keep all expensive source selection inside the service.
**輸入：** 2.1 result store, 2.2 generation checks, current live-index/SQLite/build query paths.
**產出：** Generation-bound in-flight registry, leader/waiter lifecycle, source classification, and concurrency/restart tests.
**依賴：** 2.1 and 2.2.
**Owner／Wave：** Primary integrator / Wave 3.
**Gate／Evidence：** G5 shared computation; `evidence/2.3/`.
**完成門檻：** Concurrent same-key clients observe one computation, different keys progress independently, every leader exit releases waiters, and restart tests prove SQLite-first cold recovery followed by a warm LRU hit.

- [x] 2.3.1 Add a service-global in-flight identity keyed by volume identity, folder reference, and observed generation.
- [x] 2.3.2 Implement leader/joiner coordination that never waits while holding result-cache, live-volume, or SQLite mutation locks.
- [ ] 2.3.3 Guarantee registry removal and waiter terminalization for success, service error, stale generation, disconnect, and unwinding-safe failure paths.
- [ ] 2.3.4 Instrument and preserve source order: valid memory aggregate, current read-only SQLite, then bounded aggregate build.
- [ ] 2.3.5 Add focused concurrency tests for same-key coalescing, different-key independence, failed leader cleanup, and stale leader rejection.
- [ ] 2.3.6 Add a focused restart fixture proving cold SQLite recovery reads no user file contents and the next client obtains a result-LRU hit.

### 2.4 Active-volume paging and exactness recovery

**目的：** Convert global live-memory pressure from an immediate partial result into a bounded exact recovery for the queried volume.
**輸入：** 2.2 generation rules, 2.3 single-flight/source order, complete per-volume SQLite stores, current live-budget coordinator.
**產出：** Active-volume recovery coordinator, peak-safe memory swap, exact query wait path, diagnostics, and focused paging tests.
**依賴：** 2.2 and 2.3.
**Owner／Wave：** Primary integrator / Wave 3B.
**Gate／Evidence：** G5b active-volume exactness; `evidence/2.4/`.
**完成門檻：** Focused tests prove C-to-D and D-to-C swaps retain SQLite, obey peak limits, coalesce one recovery, reject stale publication, and do not return immediate partial when the target alone fits.

- [x] 2.4.1 Add an active-volume recovery identity and terminal state keyed by volume identity and observed generation.
- [x] 2.4.2 Release non-active in-memory indexes and the target's incomplete snapshot before reserving the complete target allowance, without deleting SQLite stores.
- [x] 2.4.3 Load and journal-catch-up the target SQLite snapshot, falling back to bounded NTFS metadata rebuild only when catch-up cannot prove exactness.
- [x] 2.4.4 Make incomplete-source folder queries join active-volume recovery until exact success, stale generation, unrecoverable failure, cancellation, or the ten-second deadline.
- [x] 2.4.5 Revalidate the observed journal generation before installing a recovered runtime or publishing folder results.
- [x] 2.4.6 Emit paging-stage diagnostics with measured and configured volume-index/file-data bytes for genuine budget failures.
- [x] 2.4.7 Add focused tests for active-volume priority, cross-volume eviction, SQLite retention, hard peak accounting, same-volume recovery coalescing, stale recovery rejection, and target-alone oversize failure.
- [x] 2.4.8 Add a focused query regression proving a budget-trimmed target that fits alone returns an exact aggregate instead of immediate unavailable.

### 2.5 Foreground recovery latency correction

**目的：** Keep exact volume recovery inside the interactive window by recognizing normal EOF and publishing memory before durable replacement.
**輸入：** 2.4 recovery coordinator, Windows MFT enumeration behavior, persisted replacement path, and user-provided detailed error evidence.
**產出：** EOF classification, scan diagnostics, foreground source selection, deferred persistence state, and D-volume timing evidence.
**依賴：** 2.4.
**Owner／Wave：** Primary integrator / Wave 3C.
**Gate／Evidence：** G5c exact recovery latency; `evidence/2.5/`.
**完成門檻：** Normal EOF produces a complete index; non-EOF errors remain I/O failures; foreground recovery skips slow SQLite materialization, publishes exact memory before persistence, and releases at least one required-path result within ten seconds.

- [x] 2.5.1 Classify `ERROR_HANDLE_EOF` as successful MFT enumeration termination and return non-EOF errors with cursor and scanned-record context.
- [x] 2.5.2 Track scanned entries, observed file-data bytes, and the exact live-budget dimension that stopped a bounded scan.
- [x] 2.5.3 Skip whole-canonical SQLite materialization while an active foreground query-demand lease requires exact recovery.
- [x] 2.5.4 Publish a budget-checked exact runtime immediately after NTFS scan and journal catch-up, before SQLite replacement.
- [x] 2.5.5 Move replacement persistence to a retryable post-publication state without weakening persisted-budget, focus, epoch, or atomic-promotion gates.
- [x] 2.5.6 Add focused EOF/non-EOF, pre-persistence publication, retryable persistence, and budget-dimension diagnostic tests.

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
**完成門檻：** Details facts never read/write Host aggregate cache or recurse on service failure; complete/unavailable/timeout results clear loading within ten seconds; partial values become `Unavailable`; cancelled or old-generation results cannot publish; Code Lines `Limit` behavior is unchanged.

- [x] 3.2.1 Replace the Details Folder size worker's Host-cache/`aggregate_or_scan` path with direct bounded `mft_query` requests.
- [x] 3.2.2 Project Folder size, File Count, and Folder Count from one service aggregate response while retaining request deduplication and active generation identity.
- [x] 3.2.3 Remove Details participation in `HostExtensionColumnCacheV1<FolderSizeCachedValueV1>` and persistent folder snapshot writes without removing Size Map tree APIs.
- [x] 3.2.4 Remove recursive and Everything fallback from the Details path and map service timeout/unavailable/malformed outcomes to explicit terminal results.
- [x] 3.2.5 Preserve cancellation and reject late responses for obsolete request/tab generations.
- [x] 3.2.6 Add focused Host/UI tests for cache bypass, no recursive fallback, complete, partial-as-unavailable, ten-second timeout, cancellation, stale response, and loading-state removal.
- [x] 3.2.7 Run the focused Code Lines admission tests and record that existing File Count threshold and red `Limit` behavior remain unchanged.
- [x] 3.2.8 Add bounded visible-query scheduling so one slow request cannot block later visible folders.
- [x] 3.2.9 Emit detailed Host console diagnostics for partial, timeout, IPC, stale-generation, and service errors.
- [x] 3.2.10 Return bounded detailed service failures through folder-query IPC, preserve legacy generic-error decoding, and persist the client diagnostic while rendering only `Unavailable`.

### 3.3 Folder Options current-usage telemetry

**目的：** Ensure decorated services expose live cache usage and Folder Options replaces placeholder em dashes after refresh.
**輸入：** Existing `ExplorerService` telemetry contract, `RemoteExplorerService`, Folder Options usage sampler and View controls.
**產出：** Telemetry forwarding, refresh triggers, unavailable mapping, and focused tests.
**依賴：** 3.1.
**Owner／Wave：** Primary integrator / Wave 4.
**Gate／Evidence：** G7b cache usage presentation; `evidence/3.3/`.
**完成門檻：** Available rows show measured bytes, confirmed failures show `Unavailable / <limit>`, and remote service decoration cannot erase local telemetry.

- [x] 3.3.1 Forward `cache_telemetry_snapshot` through `RemoteExplorerService`.
- [x] 3.3.2 Refresh usage on Folder Options open, View-page selection, and cache-budget application while retaining prior measured bytes during pending samples.
- [x] 3.3.3 Add focused telemetry forwarding and usage-label tests.
- [x] 3.3.4 Group the five MFT Service resource rows in Folder Options and annotate shared ownership plus restart persistence without grouping Folder size cache TTL.

### 3.4 Completion-order batch folder queries

**目的：** Replace per-folder synchronous IPC with one bounded visible-first stream whose independent exact results publish immediately.
**輸入：** Approved batch-stream design, 2.3 single-flight, 2.4 exact recovery, 3.1 detailed response framing, and current UI request-generation channel.
**產出：** Batch frame codecs, client dispatcher, service scheduler/writer, compatibility path, and focused concurrency tests.
**依賴：** 2.3, 2.4, and 3.1.
**Owner／Wave：** Primary integrator / Wave 4B.
**Gate／Evidence：** G7c batch completion-order behavior; `evidence/3.4/`.
**完成門檻：** A 256-item bounded batch streams exact/per-item-error frames in completion order, uses no more than four computations per volume, shares duplicate flights and one recovery, cancels stale view generations, and preserves legacy single queries.

- [x] 3.4.1 Define bounded batch request, item response, end-frame, request-ID, and backward-compatibility constants and codecs.
- [x] 3.4.2 Reject oversized counts/payloads, duplicate request IDs, invalid item lengths, unsupported versions, and cross-volume identity mismatches before scheduling.
- [x] 3.4.3 Implement one client batch dispatcher that submits visible items first and maps completion-order frames to the existing per-item result channel.
- [x] 3.4.4 Implement service grouping by volume and one shared exact-recovery join for all items in a group.
- [x] 3.4.5 Implement a maximum-four-per-volume aggregate worker pool whose workers send typed completions to one connection writer.
- [x] 3.4.6 Integrate exact LRU hits and generation-bound same-key single-flight so duplicates receive separate request-ID responses without duplicate computation.
- [x] 3.4.7 Terminalize unfinished current-generation items on connection failure while discarding obsolete-generation completions after navigation or refresh.
- [ ] 3.4.8 Add focused protocol tests for round trip, malformed bounds, duplicate IDs, legacy compatibility, end framing, and mid-stream disconnect.
- [ ] 3.4.9 Add focused service tests proving fast-before-slow delivery, different-key parallelism, duplicate coalescing, one recovery per volume, concurrency cap, per-item failure isolation, and exact-only LRU insertion.
- [ ] 3.4.10 Add focused Host tests proving visible-first batches, per-item UI publication, stale-generation cancellation, and terminalization of unfinished items.

## 4. Safe Retirement of Obsolete Host Snapshots

### 4.1 Bounded startup maintenance

**目的：** Stop obsolete Details cache growth and retire only validated cache files without touching Size Map or source content.
**輸入：** Approved exact `%LOCALAPPDATA%\SuperExplorer\folder-snapshot-cache\v2` namespace, obsolete record schema/path helper, Windows reparse metadata APIs, G7 direct path.
**產出：** Versioned maintenance marker/function, path-safe bounded cleanup tests, and migration evidence.
**依賴：** 3.2.
**Owner／Wave：** Primary integrator / Wave 5; destructive action remains with the primary agent.
**Gate／Evidence：** G8 migration safety; `evidence/4.1/`.
**完成門檻：** One launch removes at most 256 oldest validated immediate regular files, unsafe/unexpected entries are untouched, failures are non-fatal, repeated launches converge, and fixture hashes prove source and Size Map content unchanged.

- [x] 4.1.1 Implement exact `folder-snapshot-cache\v2` namespace validation that rejects symlink/reparse roots and never traverses subdirectories.
- [x] 4.1.2 Validate immediate obsolete regular records and order only eligible files oldest first.
- [x] 4.1.3 Remove at most 256 eligible files per launch and make individual metadata/removal failures non-fatal.
- [x] 4.1.4 Keep Size Map projection namespaces outside the cleanup target and record a versioned migration completion marker only when no eligible obsolete file remains.
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

- [x] 5.1.1 Run formatter/check on changed Rust files and save the exact command and output.
- [x] 5.1.2 Run the selected MFT service LRU, invalidation, single-flight, source-order, and diagnostics tests.
- [x] 5.1.3 Run the selected Host direct-query, cancellation, terminal-state, migration, and Code Lines regression tests.
- [x] 5.1.4 Build the smallest matched Super Explorer and MFT Service installable targets needed for headful validation.
- [ ] 5.1.5 Produce a proposal→decision→requirement/scenario→task→evidence traceability matrix and index all immutable logs/hashes.
- [x] 5.1.6 Run focused detailed-error frame, named-pipe round-trip, installer console-contract, release check, and matched app/service build verification.
- [ ] 5.1.7 Run the focused batch protocol, completion-order scheduler, Host dispatcher, cancellation, and legacy-compatibility tests.
- [x] 5.1.8 Run the focused Folder Options MFT resource-group UI contract test and the smallest `explorer-ui` compile check.

### 5.2 Installed `D:\trace` shared-cache acceptance

**目的：** Prove the user-reported folder and multi-process shared optimization in the installed application.
**輸入：** G9 matched binaries, running installed MFT Service, `D:\trace`, diagnostics capture, screenshot/automation tooling.
**產出：** Cold/warm run logs, two-process counter evidence, terminal-state screenshot, working-set samples, and source hashes.
**依賴：** 5.1.
**Owner／Wave：** Primary integrator / Wave 7; service install/restart and installed execution are performed only by the primary agent.
**Gate／Evidence：** G10 installed acceptance; `evidence/5.2/`.
**完成門檻：** After ten seconds at each of `D:\`, `D:\SuperExplorer`, and `D:\UE_5.7`, at least one visible child folder has an exact size; every failed completed row shows `Unavailable`, no partial size is displayed, cache-usage rows show measured bytes or confirmed unavailable, limits hold, and Code Lines `Limit` remains correct.

- [x] 5.2.1 Record pre-install hashes/versions, install the matched app/service build, and verify the running binaries match the evidence hashes.
- [x] 5.2.2 Visit `D:\`, `D:\SuperExplorer`, and `D:\UE_5.7`, wait ten seconds at each, and capture exact-size, unavailable, latency, and console diagnostic evidence.
- [ ] 5.2.3 Open a second Super Explorer process on `D:\trace` and prove service-global warm hits and no duplicate same-key leader computation.
- [ ] 5.2.4 Apply focused result-LRU pressure or a lower test limit and prove entries/bytes settle within both limits while folder queries still succeed.
- [ ] 5.2.5 Capture final Details screenshots showing at least one exact child size per required location, no partial values, terminal unavailable cells, and unchanged red Code Lines `Limit` cells.
- [ ] 5.2.8 Capture Folder Options showing measured current usage or confirmed `Unavailable` for every cache budget row.
- [ ] 5.2.6 Record MFT Service working-set samples and LRU/source counters for cold and warm runs without path-bearing telemetry.
- [ ] 5.2.7 Re-hash the `D:\trace` validation sample and prove installed queries and migration changed no source content.
- [ ] 5.2.9 Install the matched detailed-error build, verify installed hashes, and prove the three required paths expose the service-produced reason in the client diagnostics console and log.
- [x] 5.2.10 Prove one visible-first batch at each required path returns at least one exact child within ten seconds and records fast-before-slow completion order where workloads differ.

### 5.3 Final scoped review and handoff

**目的：** Confirm the implementation stayed within the approved design, preserved unrelated work, and leaves an auditable focused result.
**輸入：** G1–G10 evidence, final diff, OpenSpec artifacts, installed acceptance output.
**產出：** `evidence/5.3/final-review.md`, final evidence index, and completed task statuses.
**依賴：** 5.2.
**Owner／Wave：** Primary integrator / Wave 8.
**Gate／Evidence：** G11 release recommendation; `evidence/5.3/`.
**完成門檻：** No unresolved P0/P1 correctness, stale-generation, data-safety, or indefinite-loading issue remains; all changed files trace to scope; every completed leaf has valid evidence; no complete-suite claim is made.

- [x] 5.3.1 Review the final diff for scope, cache ownership, lock ordering, stale publication, unsafe deletion, and preservation of unrelated dirty-worktree edits.
- [x] 5.3.2 Re-run `openspec validate fix-shared-mft-folder-aggregate-lru --strict` and scan artifacts/evidence for incomplete markers, contradictions, stale records, and missing scenario traceability.
- [ ] 5.3.3 Finalize the evidence index with unique task records, hashes, timestamps, actual outcomes, and any A/B adjustment lineage.
- [x] 5.3.4 Write the final scoped verification report, including commands run, tests intentionally omitted, installed results, remaining non-blocking risks, and rollback procedure.
