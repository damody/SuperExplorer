## 1. Contract and migration

### 1.1 Central cache-budget model

**目的：** One normalized versioned contract defines all approved budgets and slider stops.  
**輸入：** Approved design, current `ViewSettings`, session schema.  
**產出：** Model types/constants/normalizers and unit tests.  
**依賴：** None.  
**Owner／Wave：** Primary integrator / Wave 1.  
**Gate／Evidence：** G-CONTRACT; `evidence/index.jsonl`.  
**完成門檻：** All 14 descriptors match approved bounds and boundary tests pass.

- [x] 1.1.1 Add `CacheBudgetSettingsV1`, stable budget IDs, and exact descriptor defaults/minima/maxima.
- [x] 1.1.2 Add checked MB-to-byte normalization and approved logarithmic stop/interpolation helpers.
- [x] 1.1.3 Add boundary tests for all descriptors including 24 MB and four 16384 MB MFT maxima.

### 1.2 Persistence migration

**目的：** Legacy and new sessions round-trip normalized budgets.  
**輸入：** 1.1 contract and current session serde model.  
**產出：** Migration/serialization code and fixtures/tests.  
**依賴：** 1.1.  
**Owner／Wave：** Primary integrator / Wave 1.  
**Gate／Evidence：** G-MIGRATION; `evidence/index.jsonl`.  
**完成門檻：** Missing, legacy, valid, and out-of-range fixtures pass without losing unrelated settings.

- [x] 1.2.1 Embed the versioned aggregate budget object with backward-compatible serde defaults.
- [x] 1.2.2 Migrate legacy icon, thumbnail, and MFT LRU values into the aggregate contract.
- [x] 1.2.3 Add session fixture tests for missing fields, clamping, persistence, and restart round-trip.

## 2. Folder Options controls and commit correctness

### 2.1 Reusable number/slider editor

**目的：** Every configurable row has one accessible synchronized editor.  
**輸入：** 1.1 descriptors and current telemetry layout.  
**產出：** Shared GPUI component, automation IDs, interaction tests.  
**依賴：** 1.1.  
**Owner／Wave：** Primary integrator / Wave 2.  
**Gate／Evidence：** G-EDITOR; `evidence/index.jsonl`.  
**完成門檻：** Pointer, keyboard, arbitrary textbox value, snapping, and 400 px layout tests pass.

- [x] 2.1.1 Implement the 400 px logarithmic progress-slider and filtered endpoint-inclusive stops.
- [x] 2.1.2 Implement integer textbox validation and bidirectional textbox/slider draft synchronization.
- [x] 2.1.3 Implement arrow/Home/End accessibility behavior and stable editor/slider automation IDs.
- [x] 2.1.4 Render editors for exactly the 14 approved rows and wrap them without clipping.
- [x] 2.1.5 Add GPUI/UITEST coverage for stop order, 24 MB, pointer snapping, keyboard input, and scrolling.

### 2.2 Transactional Apply, OK, and Cancel

**目的：** Visible values and committed settings cannot diverge.  
**輸入：** 1.2 persistence and 2.1 editor drafts.  
**產出：** Aggregate action/commit path and regression tests.  
**依賴：** 1.2, 2.1.  
**Owner／Wave：** Primary integrator / Wave 2.  
**Gate／Evidence：** G-COMMIT; `evidence/index.jsonl`.  
**完成門檻：** Installed-equivalent UI tests prove 512??048 Apply and OK; Cancel changes nothing.

- [x] 2.2.1 Replace per-field commit callbacks with one normalized aggregate budget action.
- [x] 2.2.2 Restore invalid editors while atomically committing all other valid drafts on Apply/OK.
- [x] 2.2.3 Add reducer/state tests for Apply, OK, Cancel, persistence notification, and no stale 512 draft.

## 3. Runtime cache owners

### 3.1 In-process, Host, and renderer budgets

**目的：** Committed memory and GPU budgets reach and constrain their owners.  
**輸入：** 1.1 contract and 2.2 commit event.  
**產出：** Runtime update APIs, bounded eviction, telemetry tests.  
**依賴：** 2.2.  
**Owner／Wave：** Primary integrator / Wave 3.  
**Gate／Evidence：** G-RUNTIME; `evidence/index.jsonl`.  
**完成門檻：** Icon/base-icon/thumbnail/extension/GPUI telemetry reports new effective maxima and over-budget tests evict.

- [x] 3.1.1 Apply icon, shared/base icon, and thumbnail memory budgets with immediate bounded eviction.
- [x] 3.1.2 Apply Host extension data-column memory budget with bounded LRU eviction.
- [x] 3.1.3 Add GPUI APIs and apply independent icon/thumbnail GPU budgets.
- [x] 3.1.4 Add owner-level tests for reducing, increasing, restart initialization, and telemetry limits.

### 3.2 Disk cache budgets

**目的：** BC7 and extension-column disk caches enforce independent persisted budgets safely.  
**輸入：** 1.2 persisted budgets and existing disk cache formats.  
**產出：** Budget policies, bounded pruning, atomicity tests.  
**依賴：** 1.2.  
**Owner／Wave：** Primary integrator / Wave 3.  
**Gate／Evidence：** G-DISK; `evidence/index.jsonl`.  
**完成門檻：** Each disk cache reaches its configured budget without deleting unrelated files or blocking UI.

- [x] 3.2.1 Apply independent Icon BC7 and Thumbnail BC7 disk policies with oldest-entry pruning.
- [x] 3.2.2 Apply the Host extension data-column disk budget and pruning policy.
- [x] 3.2.3 Add isolated-directory tests for bounds, atomic writes, cancellation, and unrelated-file preservation.

## 4. MFT service configuration and enforcement

### 4.1 Versioned configuration IPC

**目的：** MFT limits change immediately and survive reconnect without synthetic queries.  
**輸入：** 1.1 contract and existing framed MFT protocol.  
**產出：** Request/response, client retry, service handler, compatibility tests.  
**依賴：** 1.1, 2.2.  
**Owner／Wave：** Primary integrator / Wave 3.  
**Gate／Evidence：** G-MFT-IPC; `evidence/index.jsonl`.  
**完成門檻：** 512??048 is acknowledged and visible in diagnostics without navigation; old endpoint cases fail safely.

- [x] 4.1.1 Define versioned `SetCacheBudgets` framing and normalized response for all five MFT budgets.
- [x] 4.1.2 Implement service handling and remove folder-query side effects on configuration.
- [x] 4.1.3 Implement client application, pending/unavailable telemetry, and latest-snapshot reconnect retry.
- [x] 4.1.4 Add protocol tests for valid, boundary, malformed, old-client, old-service, timeout, and reconnect cases.

### 4.2 Independent structure trimming

**目的：** Each MFT structure obeys its own hard budget and records incompleteness.  
**輸入：** 4.1 effective settings and current BTree/index implementations.  
**產出：** Accounting, eviction/pruning, incomplete markers, stress tests.  
**依賴：** 4.1.  
**Owner／Wave：** Primary integrator / Wave 4.  
**Gate／Evidence：** G-MFT-TRIM; `evidence/index.jsonl`.  
**完成門檻：** Five independent over-budget fixtures reach limits and preserve service/process integrity.

- [x] 4.2.1 Add independent checked accounting for persisted, volume-index, file-data, aggregate, and LRU stores.
- [x] 4.2.2 Implement oldest/LRU hard trimming for the three memory index structures and result LRU.
- [x] 4.2.3 Implement atomic persisted-index pruning and recovery from interrupted replacement.
- [x] 4.2.4 Add per-structure incomplete-generation markers and repopulation completeness transitions.
- [x] 4.2.5 Add deterministic and stress tests for each budget, a single oversized record, 16384 MB bounds, and concurrent queries.

### 4.3 Partial result propagation

**目的：** Trimmed data is never displayed as an exact folder size.  
**輸入：** 4.2 markers and current folder-size/Size Map results.  
**產出：** Typed partial contracts, UI presentation, sorting tests.  
**依賴：** 4.2.  
**Owner／Wave：** Primary integrator / Wave 4.  
**Gate／Evidence：** G-PARTIAL; `evidence/index.jsonl`.  
**完成門檻：** Details and Size Map visibly label partial values and exact paths remain unchanged.

- [x] 4.3.1 Carry typed partial lineage through service, Host cache, visual column, and Size Map contracts.
- [x] 4.3.2 Render and sort partial values without formatting them as exact.
- [x] 4.3.3 Add tests for trimmed aggregate, raised limit, journal repopulation, stale result, and exact unaffected query.

## 5. Integration, packaging, and evidence

### 5.1 Full automated validation

**目的：** All contracts and regressions pass in the workspace.  
**輸入：** Phases 1??.  
**產出：** Test logs and task-index evidence.  
**依賴：** 1.2, 2.2, 3.1, 3.2, 4.3.  
**Owner／Wave：** Primary integrator / Wave 5.  
**Gate／Evidence：** G-AUTO; `evidence/index.jsonl` plus raw logs.  
**完成門檻：** Formatting/diff checks and targeted model/UI/app/shell/UITEST suites exit 0.

- [x] 5.1.1 Run model/session/slider/reducer tests and save raw logs.
- [x] 5.1.2 Run cache-owner/disk/MFT protocol/trimming/partial tests and save raw logs.
- [x] 5.1.3 Run UITEST automation for every editor, slider, Apply/OK/Cancel, wrap, and scroll behavior.
- [x] 5.1.4 Run formatting, `git diff --check`, and affected workspace package checks.

### 5.2 Test installer and installed-build validation

**目的：** The packaged app and SYSTEM service exhibit the approved behavior.  
**輸入：** 5.1 passing binaries and installer scripts.  
**產出：** Installer, hashes, install logs, screenshots, telemetry evidence.  
**依賴：** 5.1.  
**Owner／Wave：** Primary agent only / Wave 6.  
**Gate／Evidence：** G-INSTALL; `evidence/index.jsonl`, screenshots, installer hash.  
**完成門檻：** `build_test_install.bat` succeeds; installed 512??048 applies without navigation and persists after restart; representative budgets from every owner class update.

- [x] 5.2.1 Build the test installer and record exit status, binary versions, and SHA-256.
- [x] 5.2.2 Install/upgrade the test package and verify the MFT Windows Service binary/version/startup identity.
- [ ] 5.2.3 Capture before/after evidence that Apply and OK change MFT LRU 512??048 without navigation.
- [ ] 5.2.4 Change representative UI/Host/GPU/disk/MFT budgets, restart, and capture persistence/telemetry evidence.
- [ ] 5.2.5 Verify partial presentation with a bounded test fixture and capture Details/Size Map evidence.

### 5.3 Final traceability review

**目的：** Every requirement and task has current auditable evidence.  
**輸入：** All prior evidence.  
**產出：** Completed evidence index and final validation report.  
**依賴：** 5.2.  
**Owner／Wave：** Primary integrator / Wave 6.  
**Gate／Evidence：** G-FINAL; `evidence/index.jsonl`, `evidence/final-report.md`.  
**完成門檻：** Strict OpenSpec validation passes, no placeholders remain, and every leaf has a unique passed/not-applicable/superseded record.

- [ ] 5.3.1 Map proposal outcomes and normative scenarios to gates, task IDs, and immutable evidence records.
- [ ] 5.3.2 Revalidate task atomicity and mark stale/superseded evidence lineage explicitly.
- [x] 5.3.3 Run strict OpenSpec validation and write the final report with unresolved risks, if any.

