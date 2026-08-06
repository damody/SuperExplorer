## 1. Baseline and host coexistence

### 1.1 Baseline preservation

**目的：** Establish the exact overlapping edits and focused validation baseline without changing unrelated work.
**輸入：** Approved design, current worktree, existing code-line tests and smoke scripts.
**產出：** Baseline record and owned-file inventory.
**依賴：** None.
**Owner／Wave：** Primary integrator / wave 1.
**Gate／Evidence：** G1; `evidence/evidence-index.jsonl` and `evidence/1.1-baseline.txt`.
**完成門檻：** Relevant diffs and tests are recorded with no unrelated changes reverted.

**?桃?嚗?* Establish the exact overlapping edits and focused validation baseline without changing unrelated work.
**頛詨嚗?* Approved design, current worktree, existing code-line tests and smoke scripts.
**?Ｗ嚗?* Baseline record and owned-file inventory.
**靘陷嚗?* None.
**Owner嚗ave嚗?* Primary integrator / wave 1.
**Gate嚗vidence嚗?* G1; `evidence/evidence-index.jsonl` and `evidence/1.1-baseline.txt`.
**摰??瑼鳴?** Relevant diffs and tests are recorded with no unrelated changes reverted.

- [x] 1.1.1 Record relevant worktree diffs, stable IDs, descriptors, host maps, fixture metadata, and existing test commands in `evidence/1.1-baseline.txt`.
- [x] 1.1.2 Run the focused pre-change unit/contract baseline and record exact command, exit status, and failures under task ID `1.1.2`.

### 1.2 Independent host routing

**目的：** Lua and Rust code-line columns coexist through registration, values, rendering, refresh, cache, and sorting.
**輸入：** Baseline inventory and dynamic-column delta specification.
**產出：** Focused host implementation and regression tests.
**依賴：** 1.1.
**Owner／Wave：** Primary integrator / wave 2.
**Gate／Evidence：** G2; `evidence/1.2-host-tests.txt`.
**完成門檻：** Focused host tests prove separate identity/state and pass offline.

**?桃?嚗?* Lua and Rust code-line columns coexist through registration, values, rendering, refresh, cache, and sorting.
**頛詨嚗?* Baseline inventory and dynamic-column delta specification.
**?Ｗ嚗?* Focused host implementation and regression tests.
**靘陷嚗?* 1.1.
**Owner嚗ave嚗?* Primary integrator / wave 2.
**Gate嚗vidence嚗?* G2; `evidence/1.2-host-tests.txt`.
**摰??瑼鳴?** Focused host tests prove separate identity/state and pass offline.

- [x] 1.2.1 Replace any single active-code-line routing slot with complete stable-column-identity routing while preserving unrelated host behavior.
- [x] 1.2.2 Add host tests proving both descriptors register, remain visible, receive independent results/render plans, and sort independently.
- [x] 1.2.3 Add host tests proving disable/re-enable of one provider does not mutate the sibling column and retained layout follows stable identity.
- [x] 1.2.4 Run focused host tests offline and retain the passing output under task ID `1.2.4`.

## 2. Fixture behavior and metadata

### 2.1 Exact column names

**目的：** All user-visible metadata and descriptors consistently expose `Code lines` and `Main code lines`.
**輸入：** Approved exact names and existing package/locale metadata.
**產出：** Updated descriptors, locales, manifests, and contract assertions.
**依賴：** 1.1.
**Owner／Wave：** Primary integrator / wave 2.
**Gate／Evidence：** G3; `evidence/2.1-name-contract.txt`.
**完成門檻：** Repository scan and contract tests find only the approved names on the two relevant surfaces.

**?桃?嚗?* All user-visible metadata and descriptors consistently expose `Code lines` and `Main code lines`.
**頛詨嚗?* Approved exact names and existing package/locale metadata.
**?Ｗ嚗?* Updated descriptors, locales, manifests, and contract assertions.
**靘陷嚗?* 1.1.
**Owner嚗ave嚗?* Primary integrator / wave 2.
**Gate嚗vidence嚗?* G3; `evidence/2.1-name-contract.txt`.
**摰??瑼鳴?** Repository scan and contract tests find only the approved names on the two relevant surfaces.

- [x] 2.1.1 Update Lua metadata to `Code lines` and Rust metadata/descriptor to `Main code lines` without changing either stable identity.
- [x] 2.1.2 Update scripts and tests that select the two columns and add exact-name/distinct-identity assertions.
- [x] 2.1.3 Run the focused name/descriptor contracts and retain passing output under task ID `2.1.3`.

### 2.2 Rust main-language aggregation

**目的：** Rust directory results select the deterministically largest per-language aggregate.
**輸入：** Bounded directory-pack format and Rust tokei provider.
**產出：** Aggregator, cache invalidation/version update, and unit tests.
**依賴：** 1.1.
**Owner／Wave：** Primary integrator / wave 2.
**Gate／Evidence：** G4; `evidence/2.2-rust-provider-tests.txt`.
**完成門檻：** Aggregation, tie, malformed/unsupported, and cache tests pass offline.

**?桃?嚗?* Rust directory results select the deterministically largest per-language aggregate.
**頛詨嚗?* Bounded directory-pack format and Rust tokei provider.
**?Ｗ嚗?* Aggregator, cache invalidation/version update, and unit tests.
**靘陷嚗?* 1.1.
**Owner嚗ave嚗?* Primary integrator / wave 2.
**Gate嚗vidence嚗?* G4; `evidence/2.2-rust-provider-tests.txt`.
**摰??瑼鳴?** Aggregation, tie, malformed/unsupported, and cache tests pass offline.

- [x] 2.2.1 Implement per-language `CodeStats` accumulation and greatest-code selection with ascending-name tie resolution.
- [x] 2.2.2 Invalidate or version Rust cache records so older directory semantics cannot be reused.
- [x] 2.2.3 Add tests for multi-file aggregation, differing largest-file versus largest-language outcomes, ties, malformed packs, and no-supported-source directories.
- [x] 2.2.4 Run Rust fixture unit tests locked/offline and retain passing output under task ID `2.2.4`.

### 2.3 Rust visible formatting and sorting

**目的：** Rust labels use `Language: N` with comma grouping while sorting stays numeric.
**輸入：** Selected-language payload and visual renderer.
**產出：** Deterministic formatter, renderer changes, and tests.
**依賴：** 2.2.
**Owner／Wave：** Primary integrator / wave 3.
**Gate／Evidence：** G5; `evidence/2.3-renderer-tests.txt`.
**完成門檻：** Exact labels including `Rust: 1,250` pass and the sort value remains `1250`.

**?桃?嚗?* Rust labels use `Language: N` with comma grouping while sorting stays numeric.
**頛詨嚗?* Selected-language payload and visual renderer.
**?Ｗ嚗?* Deterministic formatter, renderer changes, and tests.
**靘陷嚗?* 2.2.
**Owner嚗ave嚗?* Primary integrator / wave 3.
**Gate嚗vidence嚗?* G5; `evidence/2.3-renderer-tests.txt`.
**摰??瑼鳴?** Exact labels including `Rust: 1,250` pass and the sort value remains `1250`.

- [x] 2.3.1 Implement dependency-free comma grouping and render `Language: count` for the Rust column only.
- [x] 2.3.2 Add boundary formatting tests and verify optional detail contains only selected-language aggregate statistics.
- [x] 2.3.3 Verify provider results retain the selected raw code count as `StableSortValueV1::U64` and retain output under task ID `2.3.3`.

## 3. Packaging and real-app verification

### 3.1 Dual-package integration

**目的：** Current deterministic Lua and Rust fixture packages install and load together.
**輸入：** Passing host and fixture changes, bundle build scripts, package manifests.
**產出：** Rebuilt packages and package-validation evidence.
**依賴：** 1.2, 2.1, 2.2, 2.3.
**Owner／Wave：** Primary integrator / wave 4.
**Gate／Evidence：** G6; `evidence/3.1-package-validation.txt` and package SHA-256 records.
**完成門檻：** Both current packages validate and the app loads both contributions in one run.

**?桃?嚗?* Current deterministic Lua and Rust fixture packages install and load together.
**頛詨嚗?* Passing host and fixture changes, bundle build scripts, package manifests.
**?Ｗ嚗?* Rebuilt packages and package-validation evidence.
**靘陷嚗?* 1.2, 2.1, 2.2, 2.3.
**Owner嚗ave嚗?* Primary integrator / wave 4.
**Gate嚗vidence嚗?* G6; `evidence/3.1-package-validation.txt` and package SHA-256 records.
**摰??瑼鳴?** Both current packages validate and the app loads both contributions in one run.

- [ ] 3.1.1 Rebuild the Lua and Rust tokei packages through the approved offline deterministic packaging path.
- [ ] 3.1.2 Validate both packages, record their paths and SHA-256 hashes, and reject stale package selection.
- [ ] 3.1.3 Run the non-headful dual-registration integration gate and retain passing output under task ID `3.1.3`.

### 3.2 Headful screenshot loop

**目的：** The real Details view visibly proves both columns and correct Rust output simultaneously.
**輸入：** Validated packages, mixed-language fixture, checked-out UITEST binary and manifest.
**產出：** Final screenshot, raw UITEST output, and visual-review record.
**依賴：** 3.1 and all non-headful gates.
**Owner／Wave：** Primary integrator / wave 5.
**Gate／Evidence：** G7 blocking; `evidence/headful/` and `evidence/3.2-visual-review.md`.
**完成門檻：** A reviewed screenshot clearly shows both exact headers and populated cells, including a comma-grouped `Language: N`; any observed defect is fixed and the entire affected gate chain is rerun.

**?桃?嚗?* The real Details view visibly proves both columns and correct Rust output simultaneously.
**頛詨嚗?* Validated packages, mixed-language fixture, checked-out UITEST binary and manifest.
**?Ｗ嚗?* Final screenshot, raw UITEST output, and visual-review record.
**靘陷嚗?* 3.1 and all non-headful gates.
**Owner嚗ave嚗?* Primary integrator / wave 5.
**Gate嚗vidence嚗?* G7 blocking; `evidence/headful/` and `evidence/3.2-visual-review.md`.
**摰??瑼鳴?** A reviewed screenshot clearly shows both exact headers and populated cells, including a comma-grouped `Language: N`; any observed defect is fixed and the entire affected gate chain is rerun.

- [x] 3.2.1 Update the headful fixture/script to enable both packages together and produce deterministic mixed-language main-language counts.
- [x] 3.2.2 Run the checked-out repository UITEST case and retain raw output plus screenshots under task ID `3.2.2`.
- [x] 3.2.3 Inspect the screenshot for simultaneous exact headers, populated values, `Language: N` formatting, clipping, stale/missing results, and ambiguous layout; record each subcheck under task ID `3.2.3`.
- [x] 3.2.4 If visual review fails, correct the defect, mark dependent evidence stale, rerun affected automated/package/headful gates, and retain supersession lineage; otherwise record `not-applicable` with the passing review hash.

## 4. Final evidence and review

### 4.1 Traceable completion

**目的：** Every requirement and task has auditable passing local evidence and the change is ready to archive later.
**輸入：** G1–G7 outputs and final source tree.
**產出：** Evidence index, strict validation output, final diff review.
**依賴：** 3.2.
**Owner／Wave：** Primary integrator / wave 6.
**Gate／Evidence：** G8 blocking; `evidence/evidence-index.jsonl`, `evidence/final-validation.txt`.
**完成門檻：** All leaves resolve to passed, evidence-backed not-applicable, or superseded; strict OpenSpec validation and final focused tests pass with no unexplained relevant diff.

**?桃?嚗?* Every requirement and task has auditable passing local evidence and the change is ready to archive later.
**頛詨嚗?* G1–G7 outputs and final source tree.
**?Ｗ嚗?* Evidence index, strict validation output, final diff review.
**靘陷嚗?* 3.2.
**Owner嚗ave嚗?* Primary integrator / wave 6.
**Gate嚗vidence嚗?* G8 blocking; `evidence/evidence-index.jsonl`, `evidence/final-validation.txt`.
**摰??瑼鳴?** All leaves resolve to passed, evidence-backed not-applicable, or superseded; strict OpenSpec validation and final focused tests pass with no unexplained relevant diff.

- [x] 4.1.1 Populate one unique evidence-index record per resolved leaf with command/procedure, expected and actual result, exit status/reviewer, hashes, gates, timestamp, and any adjustment ID.
- [x] 4.1.2 Run all focused automated gates, `openspec validate separate-lua-rust-code-lines-columns --strict`, and artifact placeholder/contradiction scans; retain passing output.
- [x] 4.1.3 Review proposal-to-design-to-spec-to-task-to-evidence traceability and final relevant diffs, resolving every gap before completion.
- [ ] 4.1.4 Confirm the final screenshot hash and visual review are indexed, then mark tasks complete only when every blocking gate passes.
