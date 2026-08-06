## 1. Aggregation contracts

### 1.1 Distinct provider semantics

**目的：** Make Code lines total all languages and Main code lines select one aggregate language.
**輸入：** Approved design, provider and host fast paths, caches.
**產出：** Correct aggregation, cache versioning, and focused tests.
**依賴：** None.
**Owner／Wave：** Primary integrator / wave 1.
**Gate／Evidence：** G1; `evidence/1.1-aggregation-tests.txt`.
**完成門檻：** Mixed, single, tie, cold-cache and warm-cache tests pass with exact values.

- [ ] 1.1.1 Inventory Lua, Rust, host directory and cache aggregation paths.
- [ ] 1.1.2 Implement all-language Code lines total and greatest-language Main code lines contracts.
- [ ] 1.1.3 Separate or version semantic cache records.
- [ ] 1.1.4 Add mixed-language, single-language, tie and cache regression tests.
- [ ] 1.1.5 Run focused provider and host tests and retain G1 evidence.

## 2. Ordered layout and interaction

### 2.1 Name-fixed model and persistence

**目的：** Make OrderedColumnLayout authoritative and keep Name first through every lifecycle.
**輸入：** Existing layout, registry and session persistence.
**產出：** Model invariants and round-trip tests.
**依賴：** None.
**Owner／Wave：** Primary integrator / wave 1.
**Gate／Evidence：** G2; `evidence/2.1-layout-tests.txt`.
**完成門檻：** Move, restore, hidden, unknown and extension lifecycle matrices pass.

- [ ] 2.1.1 Canonicalize Name first in default, restore, reorder and move APIs.
- [ ] 2.1.2 Project header, rows and chooser from visible ordered layout.
- [ ] 2.1.3 Add move-left/right, Name rejection, hidden/unknown and persistence tests.
- [ ] 2.1.4 Run model/UI projection tests and retain G2 evidence.

### 2.2 File Explorer-style header drag

**目的：** Reorder non-Name columns by an accessible, cancellable pointer gesture.
**輸入：** G2 model contract and existing resize/sort header interaction.
**產出：** Drag state/actions, insertion cue, and interaction tests.
**依賴：** 2.1.
**Owner／Wave：** Primary integrator / wave 2.
**Gate／Evidence：** G3; `evidence/2.2-drag-tests.txt`.
**完成門檻：** Click sorts, drag reorders, resize remains isolated, Name stays fixed, Escape cancels.

- [ ] 2.2.1 Add bounded header-drag state and begin/move/drop/cancel actions.
- [ ] 2.2.2 Implement midpoint insertion and visible drop cue without splitter conflict.
- [ ] 2.2.3 Add gesture threshold, reorder, Name, resize-isolation and Escape tests.
- [ ] 2.2.4 Run focused interaction tests and retain G3 evidence.

## 3. UITEST and completion

### 3.1 Headful mixed-language reorder scenario

**目的：** Verify exact values, drag behavior and restart persistence in the real app.
**輸入：** G1-G3, UITEST binary and deterministic plugins.
**產出：** UITEST case, reports and screenshots.
**依賴：** 1.1, 2.2.
**Owner／Wave：** Primary integrator / wave 3.
**Gate／Evidence：** G4 blocking; `evidence/headful/`.
**完成門檻：** Unequal exact values, successful non-Name drag, rejected Name drag and restart order all pass.

- [ ] 3.1.1 Add deterministic mixed-language fixture and exact semantic assertions to UITEST.
- [ ] 3.1.2 Add header drag, Name rejection and restart persistence automation.
- [ ] 3.1.3 Build and run UITEST, retaining raw report and before/after/restart screenshots.
- [ ] 3.1.4 Review screenshots and rerun after any failure with supersession lineage.

### 3.2 Final validation

**目的：** Close every task and normative scenario with reproducible evidence.
**輸入：** G1-G4 artifacts.
**產出：** Evidence index and final validation record.
**依賴：** 3.1.
**Owner／Wave：** Primary integrator / wave 4.
**Gate／Evidence：** G5 blocking; `evidence/evidence-index.jsonl`, `evidence/final-validation.txt`.
**完成門檻：** All leaves resolved; focused builds/tests, strict OpenSpec and final review pass.

- [ ] 3.2.1 Populate one unique evidence record per leaf.
- [ ] 3.2.2 Run focused builds/tests, task validator, strict OpenSpec and diff checks.
- [ ] 3.2.3 Review requirement-to-evidence traceability and final screenshot hashes.
