## Context

The workspace enables `unsafe_code = "warn"` and `unused_qualifications = "warn"` globally. A normal locked workspace check succeeds but currently emits roughly 194–212 `unsafe_code` diagnostics from 113 canonical locations. Most are in `explorer-app` MFT modules that are compiled into the library and repeated binary module graphs; two locations are in `explorer-extension-host`.

The current dirty working tree contains unrelated user work. This change must edit only audited unsafe boundaries and their immediately adjacent documentation. The approved source design is `docs/superpowers/specs/2026-08-26-unsafe-code-warning-governance-design.md`. Existing workspace-owned modules outside the baseline already contain broad `allow(unsafe_code)` attributes; they are inventoried as deferred residual risk and do not become remediation scope for this wave.

The affected operations cross Windows FFI, raw pointers, process-owned handles, overlapped I/O, service callbacks, and memory ownership boundaries. Although the change is diagnostic-focused, an incorrect edit can introduce memory unsafety or lifecycle regressions, so implementation and evidence are divided into risk-oriented batches.

## Goals / Non-Goals

**Goals:**

- Produce zero `unsafe_code` diagnostics from normal workspace library and binary compilation.
- Preserve `unsafe_code = "warn"` as the default workspace policy and add no new broad unsafe suppression.
- Give every unavoidable unsafe boundary in the governed 113-location default-feature normal-target baseline a narrow `#[expect(unsafe_code, reason = "...")]` and an adjacent soundness invariant.
- Remove genuinely unnecessary unsafe blocks when the compiler and existing safe APIs permit it without behavior changes.
- Preserve product behavior, public contracts, ABI, persistence, process topology, and non-unsafe warning counts.

**Non-Goals:**

- `dead_code` or general Clippy cleanup.
- Repair of the existing all-target test initializer failures.
- MFT crate extraction, module-graph redesign, dependency changes, or Windows API replacement.
- Auditing or removing pre-existing broad unsafe suppressions outside the default-feature normal-target baseline.
- Adding any new crate-wide, module-wide, or workspace-wide unsafe suppression.

## Decisions

### Use expression- or boundary-local lint expectations

Each unavoidable unsafe operation, extern block, or unsafe function receives the narrowest practical `#[expect(unsafe_code, reason = "...")]`. A function-level expectation is permitted only when multiple unsafe operations implement one inseparable invariant. This change may not add module-level or crate-level expectations. Pre-existing broad allows outside the baseline are listed in evidence and deferred.

This keeps new unsafe code visible and lets `unfulfilled_lint_expectations` reveal expectations whose unsafe operation was later removed. A broad module-level expectation would be faster but would silently absorb future unrelated unsafe code; changing the workspace lint to `allow` would destroy the regression signal.

### Separate necessity from soundness documentation

The expectation `reason` states why unsafe Rust is unavoidable. An adjacent `// SAFETY:` comment states why the operation is sound, including applicable pointer extent, initialization, handle validity and ownership, thread or apartment constraints, callback ABI and panic/non-unwind containment, return-code validation, and cleanup.

Existing adequate safety comments are retained. Generic reasons such as "FFI call" are rejected because they do not identify the required boundary.

### Classify each diagnostic before editing

Every canonical diagnostic has exactly one disposition:

1. remove an unnecessary unsafe block;
2. replace it with an already-available safe API without behavior change; or
3. document and expect an unavoidable unsafe boundary.

The inventory key is normalized source path, line/span identity, lint code, and target set. Repeated diagnostics caused by the binary module graph map to one canonical disposition.

### Implement in four risk-oriented batches

1. Small boundaries: application composition, broker, process entry, and extension virtual-container mutation.
2. Focus and journal boundaries.
3. Migration, size-map, and SQLite boundaries.
4. Query and service boundaries.

Each batch records changed canonical locations, disposition, expectation reason, safety-comment status, targeted check result, and non-unsafe warning delta before the next batch begins.

### Protect shared dirty-tree attribution

Before any source edit, record a SHA-256 hash and scoped pre-change diff for all 11 owned files. Immediately before every file write or patch, compare the current hash and relevant preimage with the expected value; unexpected drift invalidates the affected preservation map and dependent evidence, and the file is rebaselined before editing. Immediately after every write, verify the intended hunk and record the new expected hash. After each batch, attribute every new hunk to a baseline location or documentation invariant. Run formatting only on paths changed by this work, followed by a repository-wide format check that does not write files.

### Keep validation truthful under existing unrelated failures

The blocking gates for this change are targeted checks, `cargo check --workspace --lib --bins --locked`, and a normal locked workspace check with zero `unsafe_code` diagnostics. The existing `cargo check --workspace --all-targets --locked` missing-field failures are recorded as a pre-existing external blocker and are not represented as passing.

### Evidence correction policy

- **A — task refinement:** Commands, batching, task order, or evidence mechanics may be refined without changing scope, requirements, blocking gates, or public contracts. Record the adjustment in the evidence index.
- **B — design/spec correction:** If an unsafe site cannot meet the approved boundary rules without a behavior-preserving local correction, pause the affected batch, update design/spec/tasks and invalidate dependent evidence before continuing.
- **C — material change:** Any public behavior, ABI, persistence, process topology, dependency, permission, destructive action, scope, or blocking-gate change requires user approval.

No evidence correction may silently lower a gate or relabel a failed check as complete.

## Component and Evidence Flow

The compiler warning stream is captured as structured JSON, normalized into stable canonical location IDs, and stored in `openspec/changes/govern-unsafe-code-warnings/evidence/baseline.json`. Each batch consumes that inventory, performs a compare-before-edit check, edits only its owned locations, and appends an evidence record. Final validation recaptures compiler JSON, confirms the unsafe set is empty, compares non-unsafe warning categories to baseline, and writes `evidence/final-validation.json` plus `evidence/index.json`.

Each resolved atomic task maps to one evidence-index `task_id` or to a unique subcheck within an immutable shared command record. Records contain the command or review procedure, expected and actual result, exit status or reviewer, affected file hashes, related gate, timestamp, and adjustment identifier when applicable.

An evidence schema and fail-closed validator require exactly one disposition per baseline location ID, one current passed record per mandatory task, matching hashes, explicit stale-to-replacement lineage, and no duplicate or unknown IDs. `not-applicable` is valid only for an explicitly conditional task whose approved condition is recorded; superseded records are nonterminal and must link to a distinct current passed replacement.

## Failure Handling and Recovery

- If an expectation is unfulfilled, remove or relocate it rather than suppressing `unfulfilled_lint_expectations`.
- If a targeted check introduces a non-unsafe warning, the batch remains open until the warning is removed or a B-level correction is approved in the artifacts.
- If an existing safety comment is inaccurate, correct it before accepting the boundary; do not preserve misleading documentation for diff minimality.
- If a required safe replacement changes behavior or ownership, retain the unsafe implementation with a narrow expectation and document the invariant.
- If unrelated working-tree edits overlap a diagnostic span, preserve them and perform a minimal contextual edit. Stop only if the soundness invariant cannot be determined from the current state.

## Security and Compatibility

The change does not claim that an expectation proves soundness. Expectations are audit markers; safety still depends on code review and tests. No ABI types, exported symbols, protocol fields, service commands, on-disk formats, or dependency versions change. Within the governed baseline sources and outside inventoried pre-existing broad suppressions, future unsafe additions remain warnings unless separately reviewed and expected.

## Testing

- Capture compiler JSON rather than relying only on rendered text counts, and record toolchain, target triple, Cargo configuration, enabled features, and relevant build environment.
- Run targeted checks after every batch.
- Run relevant existing focused tests for changed modules when independently compilable.
- Run the locked workspace library/binary check and normal workspace check.
- Assert zero diagnostics with lint code `unsafe_code`.
- Compare all other warning-code counts against baseline and reject increases.
- Scan changed Rust sources to reject newly introduced crate- or module-level unsafe suppression and generic reasons; separately inventory all pre-existing broad suppressions as deferred residual risk.
- Run the final authoritative checks with locked dependencies and offline resolution.
- Run the evidence validator as a blocking gate after final records are written.

## Migration Plan

This is a source-only migration with no deployed data or runtime migration. Land batches in the documented order, retaining evidence for each. Rollback consists of reverting this change's boundary-local attributes, comment corrections, and any proven-unnecessary unsafe removals; no user data or binary compatibility action is required.

## Risks / Trade-offs

- [A broad expectation could hide future unsafe code] → Forbid module/crate scope and review expectation spans.
- [Mechanical edits could misstate soundness] → Require boundary-specific reasons, adjacent invariants, and batch review evidence.
- [Repeated module compilation inflates counts and causes duplicate work] → Normalize diagnostics by canonical source location before editing.
- [Line movement can make a baseline appear stale] → Match final success on lint code and canonical source, while preserving the original immutable baseline.
- [Unrelated dirty-tree changes can be overwritten] → Use minimal patches, review scoped diffs, and never revert unrelated lines.
- [Concurrent dirty-tree edits can invalidate attribution] → Hash and diff owned files before work, compare before every batch, invalidate stale evidence on drift, and use path-limited formatting.
- [Existing broad allows mean the workspace policy is not globally complete] → Inventory them explicitly and limit this wave's guarantee to the 113-location default-feature normal-target baseline while forbidding new broad suppression.
- [The workspace remains noisy from `dead_code`] → Compare warning codes and explicitly defer that category instead of hiding it.

## Open Questions

None. The approved design fixes the scope, expectation policy, and validation gates. Implementation discoveries are handled through the A/B/C correction policy above.
