## 1. Baseline and governance controls

### 1.1 Canonical unsafe inventory

**目的：** Freeze an immutable, target-aware baseline for every current unsafe diagnostic and every non-unsafe warning category.
**輸入：** Approved design, current dirty working tree, Rust 1.97.1 toolchain, locked workspace dependencies.
**產出：** `evidence/baseline.json`, owned-file SHA-256/scoped-diff snapshots, and broad-suppression inventory.
**依賴：** None.
**Owner／Wave：** Primary／1.
**Gate／Evidence：** UCG-BASE in `evidence/baseline.json`; task records in `evidence/index.json`.
**完成門檻：** Structured compiler diagnostics are normalized to stable location IDs, repeated targets are retained, all 116 canonical unsafe locations are reconciled including `ADJ-B-001`, non-unsafe counts are frozen, and every owned file has an attribution snapshot.

- [x] 1.1.1 Capture locked workspace compiler JSON and record command, revision, dirty-tree state, exit status, toolchain, target triple, Cargo configuration, enabled features, relevant build environment, and timestamp as the immutable baseline.
- [x] 1.1.2 Normalize `src/bin/../` paths and group repeated library/helper/service diagnostics into canonical unsafe locations with emitting target lists.
- [x] 1.1.3 Record warning-code counts excluding `unsafe_code` so later batches can prove they introduce no warning regression.
- [x] 1.1.4 Record SHA-256 and scoped pre-change diff snapshots for all 12 owned files, including attribution boundaries for unrelated working-tree edits.
- [x] 1.1.5 Inventory pre-existing crate-wide and module-wide unsafe suppressions outside the governed baseline with path, scope, and deferred residual risk.

### 1.2 Governance review procedure

**目的：** Make expectation scope, reason quality, and safety-comment review repeatable before source edits begin.
**輸入：** UCG-BASE inventory and unsafe-code-governance spec.
**產出：** `evidence/governance-review.json`, `evidence/schema.json`, and a fail-closed evidence validator with fixtures.
**依賴：** 1.1.
**Owner／Wave：** Primary／1.
**Gate／Evidence：** UCG-POLICY in `evidence/governance-review.json`.
**完成門檻：** The procedure rejects newly introduced broad suppression, generic reasons, missing adjacent safety invariants, and unfulfilled expectations; the validator rejects incomplete, duplicate, unknown, stale-unlinked, or hash-mismatched evidence.

- [x] 1.2.1 Define the source scan and manual review procedure for newly introduced suppression scope, concrete reasons, adjacent `SAFETY` invariants, callback panic/non-unwind coverage, and compare-before-every-write/immediate-hunk-verification enforcement.
- [x] 1.2.2 Define the evidence schema for stable location IDs, dispositions, mandatory task records, hashes, gates, timestamps, and stale-to-replacement lineage.
- [x] 1.2.3 Implement and fixture-test a fail-closed validator for one-disposition-per-location, one-current-passed-record-per-mandatory-task, known unique IDs, matching hashes, and valid replacement links.
- [x] 1.2.4 Run the governance procedure against the pre-change tree and record expected baseline failures plus deferred broad suppressions without modifying unrelated lint categories.

## 2. Small, focus, and journal boundaries

### 2.1 Small composition and extension boundaries

**目的：** Resolve unsafe diagnostics in small, independently reviewable process and extension boundaries.
**輸入：** UCG-BASE locations for `main.rs`, `application.rs`, `brokered_service.rs`, `remote_service.rs`, and `virtual_container_mutation.rs`.
**產出：** Minimal Rust edits and `evidence/batch-small.json`.
**依賴：** 1.2.
**Owner／Wave：** Primary／2.
**Gate／Evidence：** UCG-SMALL in `evidence/batch-small.json`.
**完成門檻：** All canonical unsafe locations in the package have a reviewed disposition, targeted checks pass, and non-unsafe warning counts do not increase.

- [x] 2.1.1 Compare current hashes and scoped diffs for the five owned files with UCG-BASE; invalidate and rebaseline drift before editing.
- [x] 2.1.2 After an immediate expected-hash/preimage check, audit and resolve `main.rs`, then verify the intended hunk and new hash.
- [x] 2.1.3 After an immediate expected-hash/preimage check, audit and resolve `application.rs`, then verify the intended hunks and new hash.
- [x] 2.1.4 After an immediate expected-hash/preimage check, audit and resolve `brokered_service.rs`, then verify the intended hunk and new hash.
- [x] 2.1.5 After an immediate expected-hash/preimage check, audit and resolve `virtual_container_mutation.rs`, then verify the intended hunks and new hash.
- [x] 2.1.6 After an immediate expected-hash/preimage check, audit and resolve `remote_service.rs`, then verify the intended hunks and new hash.
- [x] 2.1.7 Attribute every post-edit hunk, run targeted `explorer-app` and `explorer-extension-host` checks, and record warning delta plus changed-file hashes.

### 2.2 Focus and journal boundaries

**目的：** Resolve named-pipe, security descriptor, overlapped I/O, handle, and USN journal unsafe boundaries.
**輸入：** UCG-BASE locations in `mft_focus.rs` and `mft_journal.rs`; successful UCG-SMALL evidence.
**產出：** Minimal Rust edits and `evidence/batch-focus-journal.json`.
**依賴：** 2.1.
**Owner／Wave：** Primary／3.
**Gate／Evidence：** UCG-FOCUS-JOURNAL in `evidence/batch-focus-journal.json`.
**完成門檻：** Every focus/journal boundary documents pointer, handle, overlapped-I/O, buffer, and cleanup invariants as applicable; targeted checks and available focused tests pass without non-unsafe warning growth.

- [x] 2.2.1 Compare current hashes and scoped diffs for `mft_focus.rs` and `mft_journal.rs` with UCG-BASE; invalidate and rebaseline drift before editing.
- [x] 2.2.2 After an immediate expected-hash/preimage check, audit `mft_focus.rs` extern and security-descriptor boundaries, then verify hunks and new hash.
- [x] 2.2.3 After an immediate expected-hash/preimage check, audit `mft_focus.rs` named-pipe and handle-ownership boundaries, then verify hunks and new hash.
- [x] 2.2.4 After an immediate expected-hash/preimage check, audit `mft_focus.rs` overlapped and cleanup boundaries, then verify hunks and new hash.
- [x] 2.2.5 After an immediate expected-hash/preimage check, audit `mft_journal.rs` volume-handle and `DeviceIoControl` boundaries, then verify hunks and new hash.
- [x] 2.2.6 After an immediate expected-hash/preimage check, audit `mft_journal.rs` buffer parsing and cleanup boundaries, then verify hunks and new hash.
- [x] 2.2.7 Attribute every post-edit hunk, run focused compilation/tests, and record canonical dispositions, warning delta, and changed-file hashes.

## 3. Migration, storage, and index boundaries

### 3.1 Migration, size-map, and SQLite audit

**目的：** Resolve filesystem migration, NTFS enumeration, raw stream, and SQLite callback unsafe boundaries without changing persistence semantics.
**輸入：** UCG-BASE locations for `mft_migration.rs`, `mft_size_map.rs`, and `mft_sqlite.rs`; completed focus/journal batch.
**產出：** Minimal Rust edits and `evidence/batch-storage-index.json`.
**依賴：** 2.2.
**Owner／Wave：** Primary／4.
**Gate／Evidence：** UCG-STORAGE-INDEX in `evidence/batch-storage-index.json`.
**完成門檻：** All package locations have reviewed dispositions, persistence and ownership invariants remain unchanged, targeted checks/tests pass, and non-unsafe warnings do not increase.

- [x] 3.1.1 Compare current hashes and scoped diffs for migration, size-map, and SQLite files with UCG-BASE; invalidate and rebaseline drift before editing.
- [x] 3.1.2 After an immediate expected-hash/preimage check, audit `mft_migration.rs`, then verify intended hunks and new hash.
- [x] 3.1.3 After an immediate expected-hash/preimage check, audit `mft_size_map.rs`, then verify intended hunks and new hash.
- [x] 3.1.4 After an immediate expected-hash/preimage check, audit `mft_sqlite.rs`, then verify intended hunks and new hash.
- [x] 3.1.5 Attribute every post-edit hunk, run focused compilation/tests, and record dispositions, warning delta, and changed-file hashes.

## 4. Query and service boundaries

### 4.1 High-volume MFT query and service audit

**目的：** Resolve the remaining high-volume pipe, buffer, service callback, control-handler, and handle boundaries.
**輸入：** UCG-BASE locations for `mft_query.rs` and `src/bin/mft_service.rs`; completed storage/index batch.
**產出：** Minimal Rust edits and `evidence/batch-query-service.json`.
**依賴：** 3.1.
**Owner／Wave：** Primary／5.
**Gate／Evidence：** UCG-QUERY-SERVICE in `evidence/batch-query-service.json`.
**完成門檻：** Every remaining canonical unsafe location has one reviewed disposition, helper/service targets emit no unsafe diagnostic, targeted checks/tests pass, and non-unsafe warnings do not increase.

- [x] 4.1.1 Compare current hashes and scoped diffs for `mft_query.rs` and `src/bin/mft_service.rs` with UCG-BASE; invalidate and rebaseline drift before editing.
- [x] 4.1.2 After an immediate expected-hash/preimage check, audit `mft_query.rs` extern, pipe-open/mode, and handle boundaries, then verify hunks and new hash.
- [x] 4.1.3 After an immediate expected-hash/preimage check, audit `mft_query.rs` request/response and overlapped boundaries, then verify hunks and new hash.
- [x] 4.1.4 After an immediate expected-hash/preimage check, audit `mft_query.rs` raw buffer/view boundaries, then verify hunks and new hash.
- [x] 4.1.5 After an immediate expected-hash/preimage check, audit `mft_service.rs` callback ABI and panic/non-unwind boundaries, then verify hunks and new hash.
- [x] 4.1.6 After an immediate expected-hash/preimage check, audit `mft_service.rs` dispatcher/control/status boundaries, then verify hunks and new hash.
- [x] 4.1.7 After an immediate expected-hash/preimage check, audit `mft_service.rs` process/job/memory and remaining Win32 boundaries, then verify hunks and new hash.
- [x] 4.1.8 Attribute every post-edit hunk, run `explorer-app` library/all normal binary checks plus focused tests, and record dispositions, warning delta, and changed-file hashes.

## 5. Integration and release evidence

### 5.1 Source policy and workspace integration gates

**目的：** Prove formatting, expectation policy, target coverage, and warning-regression requirements across the integrated change.
**輸入：** All four batch evidence files and changed Rust sources.
**產出：** `evidence/integration-validation.json` and formatted source.
**依賴：** 4.1.
**Owner／Wave：** Primary／6.
**Gate／Evidence：** UCG-INTEGRATION in `evidence/integration-validation.json`.
**完成門檻：** Formatting passes, policy scan passes, affected targets compile, zero unsafe diagnostics remain, and no non-unsafe warning code exceeds baseline.

- [x] 5.1.1 Format only Rust paths changed by this work, then verify `cargo fmt --all --check` without repository-wide writes.
- [x] 5.1.2 Run the governance scan and manual review to reject newly introduced broad suppression, generic reasons, missing safety invariants, and unfulfilled expectations; verify the deferred-suppression inventory.
- [x] 5.1.3 Run targeted locked checks for `explorer-extension-host` and every `explorer-app` library/binary target and record exit statuses.
- [x] 5.1.4 Run `cargo check --workspace --lib --bins --locked --offline` and record structured diagnostics and exit status.
- [x] 5.1.5 Run normal `cargo check --workspace --locked --offline`, assert zero `unsafe_code` diagnostics, and compare every non-unsafe warning-code count to UCG-BASE.

### 5.2 External blocker and final traceability review

**目的：** Record the out-of-scope all-target state truthfully and close every requirement-to-task-to-evidence link.
**輸入：** UCG-INTEGRATION results, proposal, design, spec, tasks, and all evidence files.
**產出：** `evidence/all-target-status.json`, `evidence/final-validation.json`, and `evidence/index.json`.
**依賴：** 5.1.
**Owner／Wave：** Primary／7.
**Gate／Evidence：** UCG-FINAL in `evidence/final-validation.json` and `evidence/index.json`.
**完成門檻：** The unrelated all-target outcome is accurately classified, every leaf has unique evidence, all blocking requirements pass, scoped diff review finds no unrelated code changes, and strict OpenSpec validation passes.

- [x] 5.2.1 Run `cargo check --workspace --all-targets --locked --offline` and record the existing missing-field initializer failure as out-of-scope unless current external state allows it to pass.
- [x] 5.2.2 Build the evidence index with one current passed record per mandatory task; permit `not-applicable` only for explicitly conditional tasks with an approved condition, and require every nonterminal superseded record to link to a distinct passed replacement.
- [x] 5.2.3 Review the scoped source diff for behavior, ABI, persistence, dependency, process-topology, dead-code, and unrelated-working-tree changes.
- [x] 5.2.4 Run the fail-closed evidence validator and resolve every missing, duplicate, unknown, hash-mismatched, or stale-unlinked record.
- [x] 5.2.5 Trace every unsafe-code-governance requirement and scenario to its gate and evidence record, then run `openspec validate govern-unsafe-code-warnings --strict`.
- [x] 5.2.6 Record the final revision, toolchain/build environment, dirty-tree preservation summary, validation results, residual non-unsafe warnings, deferred suppressions, and unresolved out-of-scope failures in `evidence/final-validation.json`.
