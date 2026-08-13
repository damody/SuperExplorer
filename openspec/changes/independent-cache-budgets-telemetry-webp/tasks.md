## 1. Contracts and Compatibility

### 1.1 Independent budget and session contracts

**目的：** Persist and restore two independently normalized memory budgets without breaking prior sessions.  
**輸入：** Approved design; `ViewSettings`; schema-3 session fixtures; current cache actions.  
**產出：** Model constants/fields, session projection/migration, actions, focused tests.  
**依賴：** None.  
**Owner／Wave：** Primary agent / Wave 1.  
**Gate／Evidence：** G-SETTINGS; `target/openspec-evidence/independent-cache-budgets-telemetry-webp/1.1.*`.  
**完成門檻：** Current/prior session fixtures pass; defaults are icon 32 MiB and thumbnail 128 MiB; changing either value leaves the other byte-for-byte unchanged.

- [x] 1.1.1 Add independent icon and thumbnail constants, normalization, and `ViewSettings` fields.
- [x] 1.1.2 Add backward-compatible session serde defaults, projection, validation, and round-trip behavior.
- [x] 1.1.3 Add independent UI actions that mutate only their selected budget.
- [x] 1.1.4 Add unit and golden-fixture tests for defaults, bounds, prior-session restore, and independence.
- [ ] 1.1.5 Write G-SETTINGS evidence records for tasks 1.1.1–1.1.4.

### 1.2 Cache telemetry value contract

**目的：** Define a bounded, path-redacted Host telemetry snapshot with safe aggregation.  
**輸入：** Approved telemetry design and existing icon/thumbnail stats.  
**產出：** Stable IDs/categories, availability, values, subtotal helpers, validation tests.  
**依賴：** 1.1.  
**Owner／Wave：** Primary agent / Wave 1.  
**Gate／Evidence：** G-TELEMETRY-CONTRACT; `target/openspec-evidence/independent-cache-budgets-telemetry-webp/1.2.*`.  
**完成門檻：** The contract rejects oversized/untrusted collections, contains no path field, represents unavailable distinctly, and saturates totals deterministically.

- [x] 1.2.1 Define stable cache identity, category, availability, byte/limit/count/counter, and snapshot types.
- [x] 1.2.2 Implement bounded construction, deterministic ordering, saturating subtotals, and partial-total state.
- [x] 1.2.3 Add tests for available/unavailable entries, saturation, collection bounds, deterministic order, and redaction shape.
- [x] 1.2.4 Write G-TELEMETRY-CONTRACT evidence records for tasks 1.2.1–1.2.3.

## 2. Independent Runtime Caches and Host Reporting

### 2.1 Icon and thumbnail LRU enforcement

**目的：** Apply each configured budget to only its own runtime cache and evict immediately on reduction.  
**輸入：** 1.1 settings; visible icon cache; shared/base icon cache; thumbnail memory cache.  
**產出：** Independent budget functions, setters, runtime application, LRU tests.  
**依賴：** 1.1.  
**Owner／Wave：** Primary agent / Wave 2.  
**Gate／Evidence：** G-MEMORY-LRU; `target/openspec-evidence/independent-cache-budgets-telemetry-webp/2.1.*`.  
**完成門檻：** Each cache remains within its own byte limit after insertion and after a lower setting; the sibling cache's limit, entries, and counters do not change.

- [x] 2.1.1 Stop dividing one combined value and derive icon/thumbnail byte budgets independently.
- [x] 2.1.2 Apply icon reductions to visible/shared icon ownership without modifying thumbnail state.
- [x] 2.1.3 Apply thumbnail reductions through immediate LRU eviction without modifying icon state.
- [x] 2.1.4 Add deterministic insertion, promotion, reduction, oversized-entry, and sibling-isolation tests.
- [ ] 2.1.5 Write G-MEMORY-LRU evidence records for tasks 2.1.1–2.1.4.

### 2.2 Host memory and extension reporters

**目的：** Populate Host snapshots from UI-owned memory and Host-managed extension caches without plugin-controlled accounting.  
**輸入：** 1.2 contract; cache stats; extension persistent-cache ownership.  
**產出：** Reporters, registration/composition seam, extension byte accounting, tests.  
**依賴：** 1.2, 2.1.  
**Owner／Wave：** Primary agent / Wave 2.  
**Gate／Evidence：** G-HOST-REPORTERS; `target/openspec-evidence/independent-cache-budgets-telemetry-webp/2.2.*`.  
**完成門檻：** Snapshot values match owned bytes for visible/base icons, thumbnails, and Host extension memory/disk; synthetic plugin telemetry cannot override them.

- [x] 2.2.1 Add visible icon, shared/base icon, and thumbnail memory reporters using existing byte-cost contracts.
- [x] 2.2.2 Add Host extension data-column memory and persistent-storage accounting at the ownership boundary.
- [x] 2.2.3 Compose reporters into one immutable snapshot without exposing implementation references to Folder Options.
- [x] 2.2.4 Add reporter accuracy, plugin-override rejection, and unavailable-source tests.
- [ ] 2.2.5 Write G-HOST-REPORTERS evidence records for tasks 2.2.1–2.2.4.

### 2.3 Single-flight disk sampler

**目的：** Measure known cache roots asynchronously without recursive UI-thread I/O or overlapping samples.  
**輸入：** 1.2 contract; known icon, thumbnail, extension, and MFT disk roots.  
**產出：** Bounded sampler, latest-snapshot state, cancellation/single-flight tests.  
**依賴：** 1.2.  
**Owner／Wave：** Primary agent / Wave 2.  
**Gate／Evidence：** G-DISK-SAMPLER; `target/openspec-evidence/independent-cache-budgets-telemetry-webp/2.3.*`.  
**完成門檻：** A slow sample never overlaps another, UI-thread instrumentation observes no recursive scan, cancellation terminates, and inaccessible roots report unavailable.

- [x] 2.3.1 Implement bounded accounting for only registered cache roots with cancellation and error isolation.
- [x] 2.3.2 Implement latest-completed snapshot storage and a single-flight admission guard.
- [x] 2.3.3 Add slow-sample, inaccessible-root, cancellation, symlink/reparse, saturation, and non-overlap tests.
- [ ] 2.3.4 Write G-DISK-SAMPLER evidence records for tasks 2.3.1–2.3.3.

## 3. WebP Shell Cache Persistence

### 3.1 Codec dependency and safety gate

**目的：** Select and prove a production-suitable WebP implementation before activating writers.  
**輸入：** Locked workspace dependencies; repository license/offline policy; representative alpha and thumbnail fixtures.  
**產出：** Codec decision, dependency changes if required, raw gate evidence.  
**依賴：** None.  
**Owner／Wave：** Primary agent / Wave 1.  
**Gate／Evidence：** G-CODEC (blocking); `target/openspec-evidence/independent-cache-budgets-telemetry-webp/3.1.*`.  
**完成門檻：** Windows locked offline build, approved license, lossless alpha, quality-80 encode, corrupt-input rejection, and decoded-resource controls all pass; otherwise writer activation remains blocked.

- [x] 3.1.1 Inventory the locked `image` WebP feature/API and record whether another codec is required.
- [x] 3.1.2 Verify dependency license/provenance and locked offline Windows build behavior.
- [x] 3.1.3 Prove lossless alpha icon and quality-80 thumbnail encode/decode on representative fixtures.
- [ ] 3.1.4 Prove truncated, corrupt, oversized-dimension, and decoded-byte-limit rejection behavior.
- [ ] 3.1.5 Record the G-CODEC activation decision and evidence for tasks 3.1.1–3.1.4.

### 3.2 Versioned WebP envelope and atomic storage

**目的：** Replace raw-RGBA persistence with bounded cache-kind-aware WebP entries while retaining isolated quotas.  
**輸入：** Passing G-CODEC; existing `ShellIconDiskCache`; approved envelope contract.  
**產出：** New schema/codec, `.webp` paths, atomic publication, independent quota/stat APIs.  
**依賴：** 3.1.  
**Owner／Wave：** Primary agent / Wave 3.  
**Gate／Evidence：** G-WEBP-STORAGE; `target/openspec-evidence/independent-cache-budgets-telemetry-webp/3.2.*`.  
**完成門檻：** Valid entries round-trip; every specified corruption class fails closed; concurrent writers leave one valid entry; cleanup never crosses cache roots.

- [x] 3.2.1 Define the new magic/schema/kind/digest/dimension/length/checksum envelope and bounded parser.
- [x] 3.2.2 Implement lossless icon WebP encoding and validated owned-RGBA decoding.
- [x] 3.2.3 Implement quality-80 thumbnail WebP encoding and validated owned-pixel decoding.
- [x] 3.2.4 Preserve same-directory temporary publication, concurrency behavior, access metadata, and per-root LRU quotas.
- [ ] 3.2.5 Expose independent icon/thumbnail disk usage, limits, counts, hits, and misses to Host reporting.
- [ ] 3.2.6 Add round-trip, alpha, quality, corruption, bomb, concurrent-write, and quota-isolation tests.
- [ ] 3.2.7 Write G-WEBP-STORAGE evidence records for tasks 3.2.1–3.2.6.

### 3.3 Lazy raw-cache migration

**目的：** Make obsolete `.rgba` entries harmless misses without startup conversion.  
**輸入：** 3.2 WebP reader/writer; old cache fixtures.  
**產出：** Schema switch, lazy regeneration, obsolete cleanup behavior, migration tests.  
**依賴：** 3.2.  
**Owner／Wave：** Primary agent / Wave 3.  
**Gate／Evidence：** G-WEBP-MIGRATION; `target/openspec-evidence/independent-cache-budgets-telemetry-webp/3.3.*`.  
**完成門檻：** Old entries are never decoded as WebP, startup performs no bulk conversion, provider regeneration succeeds, and cleanup stays scoped.

- [x] 3.3.1 Switch production icon and thumbnail cache roots/schema to `.webp` without reading `.rgba` as hits.
- [ ] 3.3.2 Regenerate WebP entries lazily through existing provider paths and include obsolete files in scoped quota cleanup.
- [ ] 3.3.3 Add old-entry miss, no-startup-scan, lazy-regeneration, and scoped-cleanup tests.
- [ ] 3.3.4 Write G-WEBP-MIGRATION evidence records for tasks 3.3.1–3.3.3.

## 4. MFT Diagnostics and Folder Options

### 4.1 Fixed-size MFT diagnostics IPC

**目的：** Report aggregate Service cache telemetry without exposing index contents.  
**輸入：** Existing local named-pipe query protocol; Service LRU/counters; telemetry contract.  
**產出：** Diagnostics discriminator/codec, counters, client, local ACL tests.  
**依賴：** 1.2.  
**Owner／Wave：** Primary agent / Wave 3.  
**Gate／Evidence：** G-MFT-DIAGNOSTICS; `target/openspec-evidence/independent-cache-budgets-telemetry-webp/4.1.*`.  
**完成門檻：** Authorized round-trip returns fixed aggregate fields; malformed/remote/unauthorized requests fail closed; response shape contains no path or record payload.

- [x] 4.1.1 Add fixed-size versioned diagnostics request/response codecs with a distinct discriminator.
- [x] 4.1.2 Add saturating LRU bytes/limit/count/index-bytes/hit/miss/generation counters in the Service.
- [x] 4.1.3 Add bounded Host client timeout and map connection failure to unavailable telemetry.
- [ ] 4.1.4 Add local round-trip, malformed, truncated, unauthorized/remote, overflow, and redaction-shape tests.
- [ ] 4.1.5 Write G-MFT-DIAGNOSTICS evidence records for tasks 4.1.1–4.1.4.

### 4.2 Folder Options controls and live telemetry

**目的：** Let users independently tune both caches and inspect all three cache sections live.  
**輸入：** 1.1 settings, 1.2 snapshots, 2.x reporters, 4.1 diagnostics, existing Folder Options window.  
**產出：** Two controls, usage groups/rows/totals, one-second lifecycle, accessibility labels.  
**依賴：** 1.1, 1.2, 2.1–2.3, 4.1.  
**Owner／Wave：** Primary agent / Wave 4.  
**Gate／Evidence：** G-FOLDER-OPTIONS; `target/openspec-evidence/independent-cache-budgets-telemetry-webp/4.2.*`.  
**完成門檻：** Controls update independently; values refresh after one second; slow samples do not overlap; close cancels updates; unavailable and partial states render accessibly.

- [x] 4.2.1 Add independent Icon 32 MB and Thumbnail 128 MB controls using existing Folder Options interaction patterns.
- [x] 4.2.2 Render Memory, Disk, and MFT Service rows with bounded/unbounded/unavailable formatting and partial subtotals.
- [x] 4.2.3 Implement one-second window-scoped refresh, single-flight delivery, stale-generation rejection, and close cancellation.
- [x] 4.2.4 Add model/UI tests for control independence, formatting, live refresh, no re-entry, stale results, and close lifecycle.
- [ ] 4.2.5 Add UITest selectors and a deterministic telemetry seam for headful verification.
- [ ] 4.2.6 Write G-FOLDER-OPTIONS evidence records for tasks 4.2.1–4.2.5.

## 5. Integration, Performance, and Release Evidence

### 5.1 Cross-component automated verification

**目的：** Prove contract, Shell, Host, Service, UI, and session behavior together.  
**輸入：** Completed implementation packages and repository test runners.  
**產出：** Test logs, hashes, evidence index entries.  
**依賴：** 1.1–4.2.  
**Owner／Wave：** Primary agent / Wave 5.  
**Gate／Evidence：** G-AUTOMATED; `target/openspec-evidence/independent-cache-budgets-telemetry-webp/5.1.*`.  
**完成門檻：** Focused and affected full suites pass offline with no unexplained failure or stale evidence.

- [ ] 5.1.1 Run explorer-model session/settings tests and current/prior golden fixtures.
- [ ] 5.1.2 Run explorer-jobs memory-cache tests and explorer-shell-win WebP/disk-cache tests.
- [ ] 5.1.3 Run explorer-app MFT diagnostics, Host telemetry, and disk-sampler tests.
- [ ] 5.1.4 Run explorer-ui Folder Options tests and affected UITest manifest cases.
- [ ] 5.1.5 Run affected workspace checks, formatting, `git diff --check`, and strict OpenSpec validation.
- [ ] 5.1.6 Write G-AUTOMATED evidence records for tasks 5.1.1–5.1.5.

### 5.2 Headful and Release memory profile

**目的：** Verify the visible settings experience and bounded cache behavior in the optimized product.  
**輸入：** Passing G-AUTOMATED; Release binary; representative image/folder fixture; optional installed Service.  
**產出：** Screenshots, raw samples, build hashes, comparison report.  
**依賴：** 5.1.  
**Owner／Wave：** Primary agent / Wave 6.  
**Gate／Evidence：** G-RELEASE-PROFILE (blocking); `target/openspec-evidence/independent-cache-budgets-telemetry-webp/5.2.*`.  
**完成門檻：** UITest screenshots show both controls and three updating sections; repeated navigation leaves each owned cache within its selected limit; no sustained monotonic cache growth remains; WebP disk use and responsiveness are reported against the raw baseline.

- [ ] 5.2.1 Build the locked offline Release app, MFT Service, and test installer and record binary hashes.
- [ ] 5.2.2 Capture Folder Options screenshots showing independent controls, all cache sections, live value change, and unavailable-Service behavior.
- [ ] 5.2.3 Profile repeated navigation/thumbnail workloads and record per-cache bytes, working set, Private Bytes, handles, and threads over time.
- [ ] 5.2.4 Compare WebP disk bytes, encode/decode latency, and navigation responsiveness with the raw-RGBA baseline.
- [ ] 5.2.5 Verify cache limits settle independently and investigate any sustained monotonic growth before passing the gate.
- [ ] 5.2.6 Write G-RELEASE-PROFILE evidence records for tasks 5.2.1–5.2.5 and index screenshot hashes.

### 5.3 Final traceability and handoff

**目的：** Close every normative scenario with auditable evidence and leave implementation-ready release notes.  
**輸入：** All prior gates and evidence records.  
**產出：** Evidence index, traceability matrix, final review, rollback/handoff notes.  
**依賴：** 5.2.  
**Owner／Wave：** Primary agent / Wave 7.  
**Gate／Evidence：** G-FINAL; `target/openspec-evidence/independent-cache-budgets-telemetry-webp/final-review.md`.  
**完成門檻：** Every leaf has passed or evidence-backed terminal disposition, every scenario traces to evidence, no P0/P1 issue remains, and rollback/migration notes match shipped behavior.

- [ ] 5.3.1 Build the proposal-to-design-to-spec-to-task-to-evidence traceability matrix.
- [ ] 5.3.2 Scan artifacts and implementation for placeholders, contradictions, stale evidence, and undocumented deviations.
- [ ] 5.3.3 Resolve every conditional leaf as passed, not-applicable with evidence, or superseded with a replacement link.
- [ ] 5.3.4 Complete final architecture/security/performance review and resolve every P0/P1 finding.
- [ ] 5.3.5 Record migration, rollback, cache-clearing, and user-facing settings notes.
- [ ] 5.3.6 Write G-FINAL evidence and mark the change complete only when no required work remains.
