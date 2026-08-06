## 1. Core contracts and reference service

### 1.1 Snapshot model and semantic policy

**目的：** Freeze one normalized aggregate/tree contract and Explorer-safe traversal semantics.
**輸入：** Approved design, existing folder-size/size-map types, watcher generations.
**產出：** Core snapshot module, policy types, serialization schema, contract tests.
**依賴：** None.
**Owner／Wave：** Primary integrator / wave 1.
**Gate／Evidence：** G1; `evidence/1.1-contract-tests.txt`.
**完成門檻：** Aggregate/tree projections, statuses, reparse/hard-link policy, generations, and bounded serialization pass focused tests.

- [x] 1.1.1 Inventory current Folder Size and Size Map request/result/cache identities and record the migration map.
- [x] 1.1.2 Add normalized node, aggregate, tree, status, method, diagnostic, generation, and lease-key types.
- [x] 1.1.3 Add schema-versioned bounded snapshot encode/decode with corrupt/oversized rejection tests.
- [x] 1.1.4 Add deterministic reparse-point, hard-link, inaccessible-subtree, mutation, and deep-tree fixtures.
- [x] 1.1.5 Run contract tests and retain G1 evidence.

### 1.2 Coalescing service and recursive reference backend

**目的：** Provide one generation-safe service with a correctness reference backend.
**輸入：** 1.1 types/fixtures, existing QoS/cancellation primitives.
**產出：** `FolderSizeService`, recursive adapter, leases, LRU, counters, focused tests.
**依賴：** 1.1.
**Owner／Wave：** Primary integrator / wave 1.
**Gate／Evidence：** G2; `evidence/1.2-service-tests.txt`.
**完成門檻：** Compatible consumers share one physical scan; stale/cancelled/partial/cache lifecycle matrices pass.

- [x] 1.2.1 Implement bounded request coalescing and aggregate/tree subscriber leases.
- [ ] 1.2.2 Implement the non-reparse recursive reference adapter with progressive partial deltas.
- [ ] 1.2.3 Implement generation rejection, final-consumer cancellation, active-root pinning, and bounded LRU eviction.
- [x] 1.2.4 Implement privacy-safe scan/cache/subscriber/fallback/stale counters.
- [ ] 1.2.5 Test dual-consumer single-scan, disable-one, navigation, refresh, inaccessible, quota, and shutdown cases.
- [ ] 1.2.6 Run service tests and retain G2 evidence.

## 2. Accelerated backends and cache

### 2.1 MFT/UAC backend

**目的：** Reuse one validated elevated MFT index without elevating the main process.
**輸入：** G2 service adapter, existing helper/index format, Windows volume identity.
**產出：** MFT adapter, lazy prompt coordinator, validated helper ingestion, fallback tests.
**依賴：** 1.2.
**Owner／Wave：** Primary integrator / wave 2; privileged helper execution remains primary-owned.
**Gate／Evidence：** G3; `evidence/2.1-mft-tests.txt`.
**完成門檻：** Success, decline, timeout, malformed output, journal discontinuity, coalesced prompt, and fallback scenarios pass.

- [x] 2.1.1 Version and bound the MFT helper output contract and root/volume projection inputs.
- [ ] 2.1.2 Implement one lazy per-volume UAC prompt coordinator and non-elevated result validation.
- [ ] 2.1.3 Implement MFT normalized-tree projection with reparse and logical hard-link semantics.
- [ ] 2.1.4 Implement decline/timeout/missing-helper/malformed-index/journal-discontinuity fallback.
- [ ] 2.1.5 Run MFT unit plus opt-in elevated smoke tests and retain G3 evidence or evidence-backed environment disposition.

### 2.2 Everything backend and equivalence gate

**目的：** Use the installed Everything index only when it matches reference semantics.
**輸入：** G2 fixtures, adjacent Everything SDK/IPC, canonical path utilities.
**產出：** Everything adapter, validation filters, equality/performance report.
**依賴：** 1.2.
**Owner／Wave：** Primary integrator / wave 2.
**Gate／Evidence：** G4; `evidence/2.2-everything-tests.txt`, `evidence/2.2-profile.json`.
**完成門檻：** All correctness fixtures equal recursive output; unavailable/stale/escaped/reparse results fall back; profiling records cold/warm data.

- [ ] 2.2.1 Extend the Everything boundary to request bounded full-path, size, kind, and identity data required by folder snapshots.
- [ ] 2.2.2 Implement canonical-root, existence, generation, and reparse validation filters.
- [ ] 2.2.3 Test unavailable IPC, stale entries, escaped prefixes, reparse descendants, mutation, and resource bounds.
- [ ] 2.2.4 Run recursive-versus-Everything equality fixtures and block eligibility on any mismatch.
- [ ] 2.2.5 Record reproducible cold/warm `D:\SuperExplorer` profiling and retain G4 evidence.

### 2.3 Persistent cache and invalidation

**目的：** Reuse snapshots safely across consumers and sessions.
**輸入：** Service schema, watcher events, volume/root identity, optional USN checkpoint.
**產出：** Disk cache, invalidation/incremental policy, corruption and lifecycle tests.
**依賴：** 2.1, 2.2.
**Owner／Wave：** Primary integrator / wave 2.
**Gate／Evidence：** G5; `evidence/2.3-cache-tests.txt`.
**完成門檻：** Warm hits are correct; watcher/manual/schema/backend/journal changes invalidate; corrupt/oversized data fails closed.

- [ ] 2.3.1 Implement bounded disk keys and records including semantic policy and backend data version.
- [ ] 2.3.2 Implement watcher/manual refresh invalidation and affected-ancestor propagation.
- [ ] 2.3.3 Implement MFT USN continuity checks with rebuild on unproven continuity.
- [ ] 2.3.4 Test warm/cold, mutation, rename, schema bump, corruption, oversized, eviction, and active-lease retention.
- [ ] 2.3.5 Run cache tests and retain G5 evidence.

## 3. Consumer and ABI migration

### 3.1 Folder Size and Size Map integration

**目的：** Replace duplicate measurement with shared projections while preserving independent toggles.
**輸入：** G2-G5 service, existing GPUI visuals and Size Map renderer.
**產出：** Host wiring, render-only Folder Size, shared-tree Size Map, lifecycle tests.
**依賴：** 2.3.
**Owner／Wave：** Primary integrator / wave 3.
**Gate／Evidence：** G6; `evidence/3.1-consumer-tests.txt`.
**完成門檻：** Both consumers show equal current values, share one scan, remain independently switchable, and reject stale output.

- [ ] 3.1.1 Route Folder Size request/result sorting and rendering through aggregate snapshot subscriptions.
- [ ] 3.1.2 Route Size Map progressive nodes and terminal state through tree snapshot subscriptions.
- [ ] 3.1.3 Remove official Folder Size recursion/cache implementation and keep its renderer data-only.
- [ ] 3.1.4 Remove the independent official Size Map scan coordinator after shared-service parity.
- [ ] 3.1.5 Test enable/disable order, tab switching, F5, navigation, simultaneous consumers, and final-consumer cancellation.
- [ ] 3.1.6 Run consumer tests and retain G6 evidence.

### 3.2 Extension contracts, compatibility, and packaging

**目的：** Publish authorized data requirements and migrate without unsafe plugin coupling.
**輸入：** G6 consumers, current ABI registrar/manifests/fingerprint, installer.
**產出：** API/host validation, compatibility diagnostics, manifests, SDK docs/examples, packaged helper/DLL.
**依賴：** 3.1.
**Owner／Wave：** Primary integrator / wave 3.
**Gate／Evidence：** G7; `evidence/3.2-abi-packaging-tests.txt`.
**完成門檻：** Official packages use declared snapshot data only; undeclared requests reject; legacy fixture disposition is tested; ABI and installer validation pass.

- [ ] 3.2.1 Add `folder.aggregate` and `folder.tree` requirement descriptors and authority validation.
- [ ] 3.2.2 Add the bounded legacy visual-measure compatibility adapter and explicit diagnostics.
- [ ] 3.2.3 Migrate in-tree manifests, registrars, fixtures, SDK docs, samples, ABI schema, and UI fingerprint.
- [ ] 3.2.4 Verify MFT helper/service and Everything DLL build/install/hash paths and non-elevated main executable manifest.
- [ ] 3.2.5 Test undeclared/stale/disabled/update-incarnation authority rejection and compatibility behavior.
- [ ] 3.2.6 Run ABI, SDK, bundle, installer check, and packaging tests; retain G7 evidence.

### 3.3 False-zero correctness recovery

**目的：** Recover correctness after installed-build evidence disproved service and accelerated-index assumptions.
**輸入：** Installed SCM evidence, MFT cache contract, shared snapshot service, data-column runtimes, and headful scripts.
**產出：** Checked installer service lifecycle, completeness gates, Host-owned persistent caches, tests, and screenshots.
**依賴：** 2.1, 2.2, 3.1, 3.2.
**Owner／Wave：** Primary integrator / correction wave.
**Gate／Evidence：** G7/G8; installed SCM state, cache records, focused tests, installer CRC, and headful screenshots.
**完成門檻：** No false exact zero; service is RUNNING; unchanged modified dates hit Host cache; changed dates recalculate.

- [x] 3.3.1 Retain evidence for missing SCM service and incomplete accelerated results.
- [x] 3.3.2 Make MFT service create/configure/start/RUNNING verification blocking in the installer.
- [x] 3.3.3 Reject truncated MFT projections and incomplete Everything snapshots.
- [x] 3.3.4 Enforce the exact-zero completeness invariant and invalidate incompatible cached records.
- [x] 3.3.5 Add recursive/MFT equality, false-zero fallback, installer, and UITEST coverage.
- [x] 3.3.6 Build and install production artifacts; verify service state, nonzero folder values, Size Map totals, and screenshots.
- [x] 3.3.7 Cache complete folder snapshots by canonical path and modified date; add reuse/invalidation tests.
- [x] 3.3.8 Move persistent data-column cache policy/storage to the Host and remove plugin-side production cache lookup/write paths.
- [x] 3.3.9 Add a once-per-volume MFT aggregate index computed with at most eight worker threads.
- [x] 3.3.10 Route Folder Size through aggregate-only lookup while retaining full tree projection for Size Map.
- [x] 3.3.11 Expose Host cache/MFT service/recursive backend activity and render it at the status bar's bottom-right edge.
- [x] 3.3.12 Add aggregate equality, reuse, eight-thread bound, fallback-status, and status rendering tests.
- [x] 3.3.13 Build/install the production artifacts and retain a timed `D:\\` profile plus screenshot showing the backend status.

## 4. End-to-end evidence and release gate

### 4.1 UITEST, profiling, and final validation

**目的：** Prove real-app correctness, shared work, fallback, interaction, performance, and clean lifecycle.
**輸入：** G1-G7 artifacts and production binaries.
**產出：** Headful report/screenshots, backend profile, evidence index, final validation record.
**依賴：** 3.2.
**Owner／Wave：** Primary integrator / wave 4.
**Gate／Evidence：** G8 blocking; `evidence/headful/`, `evidence/evidence-index.jsonl`, `evidence/final-validation.txt`.
**完成門檻：** Exact values and one-scan counter pass with both consumers; UAC-decline/Everything-off recursive fallback passes; screenshots reviewed; builds/tests/OpenSpec/diff checks pass.

- [ ] 4.1.1 Add deterministic mixed tree/reparse/inaccessible fixtures and UITEST manifest entries.
- [ ] 4.1.2 Add dual-consumer one-scan, independent-toggle, refresh, cancellation, and stale-result headful assertions.
- [ ] 4.1.3 Add UAC-decline, Everything-unavailable, and recursive-fallback headful scenarios without elevating the main process.
- [ ] 4.1.4 Build production binaries and run headful scenarios, retaining raw reports and screenshots.
- [ ] 4.1.5 Run equal-result backend profiling and compare against the same-environment recursive reference.
- [ ] 4.1.6 Populate one unique evidence-index record per resolved leaf with hashes and supersession lineage.
- [ ] 4.1.7 Run focused/full tests, installer check, task validator, strict OpenSpec, diff checks, and final screenshot review.
