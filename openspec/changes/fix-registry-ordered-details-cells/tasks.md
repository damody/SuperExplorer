## 1. Baseline and ordered projection

### 1.1 Baseline and identity inventory

**目的：** Record the current header/row ordering divergence and exact descriptor/runtime identities.
**輸入：** Approved design, current dirty worktree, registry and details-render source.
**產出：** Baseline evidence with relevant diffs, IDs, orders, and focused commands.
**依賴：** None.
**Owner／Wave：** Primary integrator / wave 1.
**Gate／Evidence：** G1; `evidence/1.1-baseline.txt` and `evidence/evidence-index.jsonl`.
**完成門檻：** The observed swapped columns are reproduced or proven from both render paths, with unrelated edits preserved.

- [x] 1.1.1 Record registry order, row emission order, full extension `ColumnId` values, relevant dirty diffs, and owned files in `evidence/1.1-baseline.txt`.
- [x] 1.1.2 Run focused pre-change details-column tests and record the exact command, exit status, and any expected gap under task ID `1.1.2`.

### 1.2 Registry-ordered row projection

**目的：** Make row placement follow the same visible registry descriptor order as the header.
**輸入：** Baseline inventory, approved dynamic-column delta specification, existing specialized renderers.
**產出：** ID-keyed projection/dispatch implementation and focused source-level tests.
**依賴：** 1.1.
**Owner／Wave：** Primary integrator / wave 2.
**Gate／Evidence：** G2 blocking; `evidence/1.2-projection-tests.txt`.
**完成門檻：** Header and row IDs match for every tested combination; no cell is selected by renderer-family position.

- [x] 1.2.1 Introduce a visible descriptor projection shared by details-header and details-row placement without changing stable registry ordering.
- [x] 1.2.2 Dispatch built-in, Folder size, and Code lines-family cells by exact registry descriptor ID while retaining existing cell presentation behavior.
- [x] 1.2.3 Enforce fail-closed behavior for mismatched or stale runtime/visual descriptor IDs and preserve descriptor-owned loading/unavailable geometry.
- [x] 1.2.4 Add focused tests comparing ordered header and row IDs for Folder size plus Lua, Folder size plus Rust, and all four target extensions.
- [x] 1.2.5 Run the focused ordered-projection tests locked/offline and retain passing output under task ID `1.2.5`.

## 2. Extension lifecycle verification

### 2.1 Dynamic enable, disable, replacement, and recovery

**目的：** Prove descriptor-to-cell identity remains correct through production extension switches.
**輸入：** Registry-ordered projection implementation and existing extension lifecycle hooks.
**產出：** Lifecycle regression tests and auditable passing output.
**依賴：** 1.2.
**Owner／Wave：** Primary integrator / wave 3.
**Gate／Evidence：** G3 blocking; `evidence/2.1-lifecycle-tests.txt`.
**完成門檻：** Independent switches, install orders, stale runtimes, missing runtimes, and re-enable paths preserve alignment.

- [x] 2.1.1 Add tests covering different extension registration orders and independent disable/removal of Folder size, Lua, Rust, and Lock owners.
- [x] 2.1.2 Add tests covering stale runtimes after descriptor removal and visible descriptors whose runtime is temporarily unavailable.
- [x] 2.1.3 Add a remove-and-re-enable test proving retained visibility/width and registry position do not change neighboring identities.
- [x] 2.1.4 Run focused lifecycle tests locked/offline and retain passing output under task ID `2.1.4`.

## 3. Real-app evidence and completion

### 3.1 Headful extension-switch screenshot loop

**目的：** Demonstrate correct semantic header/cell alignment in the real Details view after extension switches.
**輸入：** Passing unit gates, built UITEST app, deterministic extension fixtures, existing smoke harness.
**產出：** Raw automation report, final screenshot, semantic and visual review records.
**依賴：** 2.1.
**Owner／Wave：** Primary integrator / wave 4.
**Gate／Evidence：** G4 blocking; `evidence/headful/` and `evidence/3.1-visual-review.md`.
**完成門檻：** After switches, Folder size shows sizes/bars, Lua Code lines shows counts, Lock owners shows owners, and Main code lines shows `Rust: 1,250` under their exact headers.

- [x] 3.1.1 Extend the headful scenario to exercise independent extension switches and assert ordered header/cell accessibility identities and semantic values.
- [x] 3.1.2 Build the checked-out UITEST app and run the headful scenario, retaining raw output and screenshots under task ID `3.1.2`.
- [x] 3.1.3 Inspect the screenshot for exact heading/data alignment, clipping, stale slots, bars, counts, owner values, and `Rust: 1,250`; record screenshot hashes and reviewer results.
- [x] 3.1.4 If any automated or visual subcheck fails, correct the defect, mark affected evidence stale, rerun dependent gates, and preserve supersession lineage; otherwise record evidence-backed `not-applicable`.

### 3.2 Final traceability and strict validation

**目的：** Close every normative scenario and task with indexed, reproducible evidence.
**輸入：** G1 through G4 artifacts and final source tree.
**產出：** Evidence index, strict validation output, final relevant diff review.
**依賴：** 3.1.
**Owner／Wave：** Primary integrator / wave 5.
**Gate／Evidence：** G5 blocking; `evidence/evidence-index.jsonl` and `evidence/final-validation.txt`.
**完成門檻：** All leaves are passed, evidence-backed not-applicable, or superseded; strict OpenSpec validation and focused gates pass without unexplained relevant diffs.

- [x] 3.2.1 Populate a unique evidence-index record for every resolved leaf with procedure, expected and actual result, exit status/reviewer, hashes, gate, timestamp, and adjustment ID when applicable.
- [x] 3.2.2 Run all focused automated gates, task-structure validation, strict OpenSpec validation, and artifact placeholder/contradiction scans; retain passing output.
- [x] 3.2.3 Review proposal-to-design-to-spec-to-task-to-evidence traceability and final relevant diffs, resolving every gap before completion.
- [x] 3.2.4 Confirm the final screenshot hash and visual review are indexed, then mark the change complete only after every blocking gate passes.
