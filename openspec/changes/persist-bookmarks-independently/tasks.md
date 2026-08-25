## 1. Persistence Contract and Baseline

### 1.1 Baseline and evidence contract

**目的：** Freeze the current bookmark/session ownership, package deletion surface, and auditable evidence format before changing behavior.
**輸入：** Approved source design, proposal, design, delta spec, current dirty worktree, session and installer tests.
**產出：** Baseline logs, overlap inventory, and `evidence/index.jsonl` records.
**依賴：** None.
**Owner／Wave：** Primary integrator / Wave 1.
**Gate／Evidence：** G1 baseline; `evidence/1.1/` and unique `task_id` records in `evidence/index.jsonl`.
**完成門檻：** Existing bookmark round-trip and session reset behavior is recorded, installer-owned deletion paths are inventoried, and unrelated dirty files are preserved.

- [x] 1.1.1 Record repository revision and dirty paths overlapping bookmark model, application persistence, session lifecycle/store, tests, and NSIS source.
- [x] 1.1.2 Run and save the focused pre-change bookmark serialization and session-store reset tests.
- [x] 1.1.3 Create the append-only evidence index schema with command, expected/actual result, exit status, hashes, gate, timestamp, and disposition fields.

### 1.2 Independent storage contract

**目的：** Freeze the exact path, files, schema, authority, migration, and reset ownership boundaries.
**輸入：** 1.1 baseline, compatibility-root tests, `Bookmarks` serde model, current session adapter.
**產出：** Implementable storage constants and a traceability note linking spec scenarios to tests.
**依賴：** 1.1.
**Owner／Wave：** Primary integrator / Wave 1.
**Gate／Evidence：** G2 storage contract; `evidence/1.2/`.
**完成門檻：** No ambiguity remains about empty-vs-absent, current/backup precedence, legacy fallback, bounds, sensitive diagnostics, or reset/package preservation.

- [x] 1.2.1 Inventory the exact existing session load, save, retry, reset, and bookmark mutation call paths.
- [x] 1.2.2 Record the approved `%LOCALAPPDATA%\RustGpuiExplorer\bookmarks\v1` artifact names and schema/authority rules.
- [x] 1.2.3 Map every normative scenario to one focused automated test or package contract assertion.

## 2. Independent Bookmark Store

### 2.1 Bounded recoverable adapter

**目的：** Implement the dedicated bookmark document with the same durability quality as session storage.
**輸入：** G2 storage contract, `RoadmapLimits`, `Bookmarks` serialization and repair behavior.
**產出：** Bookmark-store module exported by `explorer-app` and focused unit tests.
**依賴：** 1.2.
**Owner／Wave：** Primary integrator / Wave 2.
**Gate／Evidence：** G3 adapter correctness; `evidence/2.1/`.
**完成門檻：** Tests prove exact round trip, bounded rejection, atomic current/backup rotation, empty document authority, and privacy-safe errors.

- [x] 2.1.1 Add the versioned bookmark envelope, environment and injected-root constructors, and bounded current/backup reads.
- [x] 2.1.2 Add same-directory pending write, flush, atomic replacement, and last-known-good rotation.
- [x] 2.1.3 Add owned-file corruption quarantine and current-to-backup recovery without traversing or deleting unrelated paths.
- [x] 2.1.4 Add focused round-trip, empty collection, over-limit, corrupt-current, corrupt-both, and unrelated-file preservation tests.

### 2.2 Legacy migration and startup authority

**目的：** Restore independent bookmarks on every launch and migrate old profiles once without destructive fallback.
**輸入：** 2.1 adapter, loaded session outcome, application startup composition.
**產出：** Startup load/migration helper, diagnostics, and focused migration tests.
**依賴：** 2.1.
**Owner／Wave：** Primary integrator / Wave 2.
**Gate／Evidence：** G4 migration safety; `evidence/2.2/`.
**完成門檻：** Existing independent data always wins, absence migrates legacy data exactly once, and migration failure preserves and uses legacy data.

- [x] 2.2.1 Implement independent-first startup resolution with explicit present-empty semantics.
- [x] 2.2.2 Implement idempotent copy migration from the valid legacy session collection without modifying the session artifact.
- [x] 2.2.3 Add privacy-safe load/migration diagnostics and fallback to legacy/default bookmarks when independent storage is unavailable.
- [x] 2.2.4 Add focused precedence, first-launch migration, repeat-launch, empty-authority, and failed-migration tests.

## 3. Lifecycle and Package Isolation

### 3.1 Background persistence integration

**目的：** Persist bookmarks independently on accepted durable transitions while retaining existing coalescing and retry behavior.
**輸入：** 2.1 adapter, 2.2 startup authority, `PersistenceCoordinator` runtime snapshots.
**產出：** Coordinator bookmark-store dependency and lifecycle retry/reset tests.
**依賴：** 2.1 and 2.2.
**Owner／Wave：** Primary integrator / Wave 3.
**Gate／Evidence：** G5 durable lifecycle; `evidence/3.1/`.
**完成門檻：** Successful flush writes the independent collection, transient failure retries the latest snapshot, and every session reset scope leaves bookmark storage unchanged.

- [x] 3.1.1 Extend the persistence worker to write independent bookmarks before the transitional session snapshot.
- [x] 3.1.2 Keep reset requests scoped exclusively to `SessionStore` and preserve bookmark bytes across Session and AllRoadmapState resets.
- [x] 3.1.3 Update production composition and test stores without blocking or serializing on the UI thread.
- [x] 3.1.4 Add coordinator tests for successful bookmark writes, transient retry/recovery, latest-snapshot coalescing, and reset isolation.

### 3.2 Installer and compatibility contract

**目的：** Prevent install, upgrade, repair, uninstall, product rename, or future cleanup edits from claiming bookmark user data.
**輸入：** G2 path contract, current NSIS install/uninstall sections, product identity tests.
**產出：** NSIS preservation declaration and source-level contract tests.
**依賴：** 1.2 and 2.2.
**Owner／Wave：** Primary integrator / Wave 3.
**Gate／Evidence：** G6 package preservation; `evidence/3.2/`.
**完成門檻：** Tests prove the compatibility root and bookmark namespace remain stable and no package deletion target reaches the bookmark directory.

- [x] 3.2.1 Add an explicit NSIS comment/detail contract preserving per-user bookmark data across uninstall and reinstall.
- [x] 3.2.2 Extend product/installer source tests to assert the exact compatibility path and absence of bookmark-directory deletion.
- [x] 3.2.3 Verify installer compilation or the repository's focused NSIS contract command and record the result.

## 4. Verification and Delivery

### 4.1 Focused and workspace validation

**目的：** Demonstrate model, storage, lifecycle, packaging, and formatting correctness without disturbing unrelated worktree changes.
**輸入：** Completed implementation packages and all G3–G6 tests.
**產出：** Command logs, hashes, and evidence records for focused and broader checks.
**依賴：** 3.1 and 3.2.
**Owner／Wave：** Primary integrator / Wave 4.
**Gate／Evidence：** G7 build/test quality; `evidence/4.1/`.
**完成門檻：** Formatting check and all relevant crate/test targets pass; any unrelated pre-existing failure is isolated with reproducible evidence and does not mask an in-scope failure.

- [x] 4.1.1 Run Rust formatting and the focused bookmark-store, session-lifecycle, session-store, bookmark-model, and installer/product contract tests.
- [x] 4.1.2 Run `cargo check` for the affected application target and record warnings or failures attributable to this change.
- [x] 4.1.3 Inspect the final diff for secrets, unbounded reads, destructive path expansion, UI-thread I/O, and unrelated edit overlap.

### 4.2 Traceability and final review

**目的：** Close every approved requirement with evidence and leave the OpenSpec change implementation-complete.
**輸入：** G1–G7 evidence, proposal, design, delta spec, tasks, final diff.
**產出：** Complete evidence index, final review report, validated OpenSpec status, and task dispositions.
**依賴：** 4.1.
**Owner／Wave：** Primary integrator / Wave 4.
**Gate／Evidence：** G8 final acceptance; `evidence/4.2/`.
**完成門檻：** Every leaf has a passed or evidence-backed terminal disposition, every scenario traces to passing evidence, strict OpenSpec validation passes, and no unresolved P0/P1 issue remains.

- [x] 4.2.1 Populate unique evidence-index records and hashes for every completed atomic task.
- [x] 4.2.2 Run strict OpenSpec validation, detailed-task validation, and placeholder/contradiction scans.
- [x] 4.2.3 Write the final traceability and risk review, resolve all P0/P1 findings, and mark implementation tasks complete only after evidence passes.
