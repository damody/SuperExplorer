## 1. Contracts and model foundations

### 1.1 Add durable built-in column identities

**目的：** Make both count columns first-class, persistable built-ins before runtime wiring.
**輸入：** Approved design, current `ColumnId`, descriptor registry, layout and session codecs.
**產出：** New IDs/descriptors, default-hidden migration, exhaustive model/UI match updates, focused tests.
**依賴：** None.
**Owner／Wave：** Primary integrator / Wave 1.
**Gate／Evidence：** `G-MODEL`; `target/openspec-evidence/add-mft-directory-count-columns/1.1.json`.
**完成門檻：** Stable IDs round-trip, legacy layouts remain intact, and focused model tests pass.

- [x] 1.1.1 Add `FileCount` and `FolderCount` identities, stable parsing, built-in descriptors, and default-hidden ordered-layout behavior.
- [x] 1.1.2 Extend session persistence/migration and every exhaustive built-in match without changing unrelated column behavior.
- [x] 1.1.3 Add and pass model tests for ID round-trip, descriptor semantics, legacy migration, hidden defaults, and retained layout preferences.

### 1.2 Define exact directory-facts contracts

**目的：** Carry exact recursive counts and availability through one generation-safe Host boundary.
**輸入：** MFT aggregate query/result types, folder-size runtime ports, request contexts.
**產出：** `DirectoryFactsV1` projection and request/result state with explicit exact/unavailable semantics.
**依賴：** 1.1.
**Owner／Wave：** MFT/application contract owner / Wave 1.
**Gate／Evidence：** `G-FACTS`; `target/openspec-evidence/add-mft-directory-count-columns/1.2.json`.
**完成門檻：** Bytes and counts share one typed result, root exclusion is explicit, and partial/stale data cannot appear exact.

- [x] 1.2.1 Extend the MFT-backed aggregate boundary to carry file count, root-inclusive directory count, generation, and partial state without a second query path.
- [x] 1.2.2 Add the Host projection that derives descendant Folder Count with saturating root subtraction and exposes exact facts only for complete MFT results.
- [x] 1.2.3 Count each reparse-point directory entry once without traversing its target and retain focused topology/count tests.
- [x] 1.2.4 Add unavailable, partial, cancellation, and stale-generation contract tests proving no filesystem fallback or zero coercion.

## 2. Shared runtime and built-in presentation

### 2.1 Implement the shared directory-facts coordinator

**目的：** Deduplicate MFT work and fan one current-generation fact value to every consumer.
**輸入：** Directory-facts contract, existing folder-size scheduling/cache/invalidation paths.
**產出：** Demand aggregation, deduplicated requests, context-scoped values, cancellation and invalidation.
**依賴：** 1.2.
**Owner／Wave：** Application runtime owner / Wave 2.
**Gate／Evidence：** `G-DEDUP`, `G-STALE`; `target/openspec-evidence/add-mft-directory-count-columns/2.1.json`.
**完成門檻：** One folder/generation causes at most one MFT query and obsolete facts never update current UI or dispatch work.

- [x] 2.1.1 Refactor the existing folder aggregate runtime to store and publish exact directory facts alongside folder bytes while preserving current byte consumers.
- [x] 2.1.2 Implement the original demand aggregation from visible count columns and enabled limited contributions; correction package 7.1 supersedes hidden-column acquisition.
- [x] 2.1.3 Deduplicate pending/cache requests by folder identity and generation and fan completed facts to all current consumers.
- [x] 2.1.4 Wire navigation, refresh, watcher, service recovery, cancellation, and MFT-generation invalidation with focused stale-result tests.

### 2.2 Render, sort, and persist the built-in columns

**目的：** Deliver complete optional Details-column behavior from the shared facts.
**輸入：** New descriptors and coordinator value state.
**產出：** Column chooser entries, cells, sorting, sizing, filtering disposition, and UI tests.
**依賴：** 1.1, 2.1.
**Owner／Wave：** GPUI Details owner / Wave 2.
**Gate／Evidence：** `G-DETAILS`; `target/openspec-evidence/add-mft-directory-count-columns/2.2.json`.
**完成門檻：** Both columns toggle independently, display only exact folder values, sort numerically, and persist without changing file rows.

- [x] 2.2.1 Wire File Count and Folder Count through Details chooser, headers, widths, reorder, auto-size, accessibility, and session persistence.
- [x] 2.2.2 Render exact unsigned values for eligible folders, blank file/ineligible rows, and `—` for unavailable eligible folders.
- [x] 2.2.3 Route both columns through exact optional-integer sorting with existing missing-value ordering and no filesystem I/O in render/sort.
- [x] 2.2.4 Add and pass UI/state/render tests for independent toggles, values, unavailable cells, sorting, hidden defaults, and restored layouts.

## 3. Extension admission and Code Lines

### 3.1 Add and validate contribution admission metadata

**目的：** Make folder limits a compatible, validated extension contract.
**輸入：** Manifest contribution schema, public column descriptors, package validators and tooling fixtures.
**產出：** Optional policy fields, typed validation diagnostics, compatible decode and public API propagation.
**依賴：** 1.2.
**Owner／Wave：** Extension contract owner / Wave 3.
**Gate／Evidence：** `G-MANIFEST`; `target/openspec-evidence/add-mft-directory-count-columns/3.1.json`.
**完成門檻：** Valid limits survive registration; invalid/inapplicable limits reject before callbacks; omitted fields preserve behavior.

- [x] 3.1.1 Add optional inclusive `max_file_count` and `max_folder_count` fields to validated data-column contribution metadata and its public/host projections.
- [x] 3.1.2 Validate the full `u64` JSON domain, zero, AND semantics, and folder applicability while rejecting malformed, non-column, and file-only uses.
- [x] 3.1.3 Preserve decoding/registration behavior for every existing manifest that omits the policy.
- [x] 3.1.4 Add and pass manifest, package-validation, ABI/source-shape, and plugin-tooling tests for valid, boundary, invalid, and compatibility cases.

### 3.2 Enforce Host-side admission before dispatch

**目的：** Ensure limited folder work cannot enter extension callbacks without exact admissible facts.
**輸入：** Validated policy, shared directory facts, extension batch job/runtime state.
**產出：** Pending/admitted/over-limit/unavailable/stale admission states and dispatch guards.
**依賴：** 2.1, 3.1.
**Owner／Wave：** Extension Host/application integrator / Wave 3.
**Gate／Evidence：** `G-ADMISSION`, `G-STALE`; `target/openspec-evidence/add-mft-directory-count-columns/3.2.json`.
**完成門檻：** Only exact in-limit folder work dispatches; rejected states are Host-owned and callback counters remain zero.

- [x] 3.2.1 Add a pure admission evaluator for optional inclusive limits with AND semantics and typed pending/unavailable/over-limit/admitted outcomes.
- [x] 3.2.2 Integrate admission before folder job creation/submission while leaving files and unlimited contributions on their current path.
- [x] 3.2.3 Keep admission presentation states generation-safe, non-plugin-produced, and invalidated with their directory facts.
- [x] 3.2.4 Add and pass dispatch-spy tests for pending, partial, unavailable, stale, over-limit, both-limit, admitted, file, and unlimited cases.

### 3.3 Configure and present Code Lines dependency behavior

**目的：** Apply the reusable gate to both official Code Lines contributions at the exact 999/1000 boundary.
**輸入：** Admission contract, Rust/Lua fixture manifests, Code Lines visuals/runtime, package tooling.
**產出：** `max_file_count = 999`, three Host-owned states, synchronized fixtures and bundle metadata.
**依賴：** 3.2.
**Owner／Wave：** Code Lines integration owner / Wave 3.
**Gate／Evidence：** `G-CODE-LINES`; `target/openspec-evidence/add-mft-directory-count-columns/3.3.json`.
**完成門檻：** Folder callbacks/tool launches occur at 999 but not 1000 or unavailable; file behavior is unchanged.

- [x] 3.3.1 Declare `max_file_count = 999` for Rust and Lua Code Lines and update exact tooling validators, templates, and bundle inventory inputs.
- [x] 3.3.2 Render `等待 File Count…`, `File Count 超過限制，因此未啟動`, and `依賴 File Count，因此未啟動` from Host admission state without localization resources.
- [x] 3.3.3 Preserve ordinary file Code Lines and implement the original hidden File Count dependency acquisition; correction package 7.1 supersedes that acquisition behavior.
- [x] 3.3.4 Add and pass Rust/Lua boundary, callback/tool non-dispatch, unavailable dependency, hidden-column, and file-regression tests.

## 4. Integration verification and evidence

### 4.1 Run automated correctness and compatibility gates

**目的：** Prove every contract across model, MFT, Host, application, UI, SDK, and examples.
**輸入：** Integrated implementation and deterministic fixtures.
**產出：** Focused test logs, format/check results, package validation, and indexed evidence.
**依賴：** 2.2, 3.3.
**Owner／Wave：** Primary integrator / Wave 4.
**Gate／Evidence：** `G-MODEL`, `G-FACTS`, `G-DEDUP`, `G-DETAILS`, `G-MANIFEST`, `G-ADMISSION`, `G-CODE-LINES`; `target/openspec-evidence/add-mft-directory-count-columns/4.1.json`.
**完成門檻：** Every independently failing focused suite passes and evidence records exact commands and outcomes.

- [x] 4.1.1 Run formatter and focused `explorer-model`, `explorer-app` MFT/folder service, `explorer-extension-host`, and `explorer-ui` tests.
- [x] 4.1.2 Run extension API/broker, package tooling, Rust/Lua Code Lines fixture, and bundle-manifest validation tests using locked/offline commands where supported.
- [x] 4.1.3 Run relevant workspace checks/clippy/tests and classify any unrelated pre-existing failure with reproducible evidence.
- [x] 4.1.4 Write the evidence index with unique task IDs/subchecks, commands, expected/actual outcomes, exit status, hashes, source state, and timestamps.

### 4.2 Validate real Details and boundary behavior

**目的：** Demonstrate the user-visible columns and pre-dispatch protection in the real application.
**輸入：** Built/test-installed application, deterministic nested NTFS fixtures at 999 and 1000 files.
**產出：** Headful test report and screenshots/logs showing values, sorting, persistence, and Code Lines states.
**依賴：** 4.1.
**Owner／Wave：** Primary integrator / Wave 4.
**Gate／Evidence：** `G-HEADFUL`; `target/openspec-evidence/add-mft-directory-count-columns/4.2.json`.
**完成門檻：** Real UI evidence proves optional columns and exact Code Lines admission/non-dispatch behavior without fallback scanning.

- [x] 4.2.1 Add or extend deterministic UITEST fixture/actions for nested counts, unavailable MFT, hidden dependency, and 999/1000 Code Lines folders.
- [x] 4.2.2 Build/install the candidate through the repository-standard validation path and run the focused headful case with exact binary/service identities.
- [x] 4.2.3 Capture and index screenshots/logs proving column toggles/values/sorting/persistence and all three Code Lines dependency states.

## 5. Final reconciliation

### 5.1 Complete traceability and apply readiness

**目的：** Close the change only when artifacts, implementation, tests, and evidence agree.
**輸入：** All prior outputs and approved thresholds.
**產出：** Completed checklist, strict validation, traceability/evidence disposition, and final status.
**依賴：** 4.2.
**Owner／Wave：** Primary integrator / Wave 5.
**Gate／Evidence：** All gates; `target/openspec-evidence/add-mft-directory-count-columns/5.1.json`.
**完成門檻：** Every scenario traces to passing evidence, no unfinished marker or contradiction remains, and OpenSpec reports all tasks complete.

- [x] 5.1.1 Reconcile proposal, design, delta specs, implementation, manifests, tests, and evidence against the approved source design.
- [x] 5.1.2 Run the detailed-task validator, unfinished-marker/contradiction scan, `openspec validate --strict`, and final apply-status checks.
- [x] 5.1.3 Record final residual risks and rollback disposition, then mark only evidence-backed tasks complete.

## 6. Restored-layout chooser correction

### 6.1 Reconcile newly introduced built-ins into persisted layouts

**目的：** Ensure sessions saved before File Count and Folder Count existed can discover and enable both columns from the real Details chooser.
**輸入：** Approved design correction, persisted extensible layout decoder, current built-in registry descriptors, reported chooser screenshot.
**產出：** Idempotent built-in layout reconciliation, regression tests, rebuilt installer, and real chooser evidence.
**依賴：** 1.1, 2.2, 5.1.
**Owner／Wave：** Primary integrator / Correction wave.
**Gate／Evidence：** `G-MODEL`, `G-DETAILS`, `G-HEADFUL`; `target/openspec-evidence/add-mft-directory-count-columns/6.1.json`.
**完成門檻：** An eight-built-in persisted layout retains its preferences and exposes both new unchecked chooser rows; a current layout remains unchanged; installed headful evidence confirms both rows.

- [x] 6.1.1 Add an idempotent model helper that appends every missing current built-in descriptor with default width and hidden visibility without changing saved entries.
- [x] 6.1.2 Apply reconciliation to restored extensible layouts and add tests for the reported eight-column session plus already-current layouts.
- [x] 6.1.3 Run focused model/UI/session tests, strict OpenSpec validation, and bundle verification.
- [x] 6.1.4 Rebuild/install through the standard path and capture a real restored-session chooser showing File Count and Folder Count as independently toggleable rows.

## 7. Visibility-driven activation correction

### 7.1 Make visible count columns the sole directory-facts demand authority

**目的：** Start MFT count acquisition immediately when a count column appears and perform no count-only query when both built-in count columns are hidden.
**輸入：** Approved visibility-driven design correction, current Details action lifecycle, directory-facts request deduplication, extension admission policy, reported all-dash visible columns.
**產出：** Explicit visibility lifecycle, hidden-column admission guard, focused request/dispatch tests, rebuilt installer, and real UI evidence with populated counts.
**依賴：** 2.1, 2.2, 3.2, 3.3, 6.1.
**Owner／Wave：** Primary integrator / Correction wave 2.
**Gate／Evidence：** `G-DEMAND`, `G-ADMISSION`, `G-HEADFUL`; `target/openspec-evidence/add-mft-directory-count-columns/7.1.json`.
**完成門檻：** Showing either count column immediately yields MFT-backed values without refresh; both hidden yields zero count queries even with Code Lines enabled; hidden File Count yields zero folder Code Lines dispatch.

- [x] 7.1.1 Add explicit count-visibility demand and transition helpers, start current-row requests immediately on hidden-to-visible/restored-visible/navigation states, and suppress or cancel count-only work after the last column is hidden.
- [x] 7.1.2 Gate extension folder admission on visibility of every corresponding built-in count column and prevent cached hidden facts from admitting work while preserving ordinary file and unlimited-contribution behavior.
- [x] 7.1.3 Add and pass focused request-spy and dispatch-spy tests for visible immediate start, restored-visible start, two-column deduplication, both-hidden zero-query, last-hidden suppression, and hidden File Count Code Lines non-dispatch/status.
- [ ] 7.1.4 Run format, focused model/application/UI/extension tests, bundle verification, strict OpenSpec/task validation, and write indexed correction evidence.
- [ ] 7.1.5 Rebuild and install through the standard path, then capture real headful evidence that enabling File Count/Folder Count populates MFT-backed values without refresh and that hiding both prevents new count activity.
