## ADDED Requirements

### Requirement: Workspace unsafe lint remains active
The workspace SHALL retain `unsafe_code` as an active warning-level lint for workspace-owned Rust crates, and the change MUST NOT introduce new crate-wide or module-wide suppression of that lint. Pre-existing broad suppressions outside the 113-location default-feature normal-target baseline MUST be inventoried as deferred residual risk.

#### Scenario: A future unreviewed unsafe operation is added
- **WHEN** a governed baseline source file adds an unsafe operation outside a reviewed expectation boundary and outside a pre-existing inventoried suppression
- **THEN** normal compilation emits an `unsafe_code` diagnostic for that operation

#### Scenario: A broad suppression is proposed
- **WHEN** this change introduces a crate-level or module-level `allow` or `expect` for `unsafe_code`
- **THEN** the unsafe-governance validation gate fails

#### Scenario: A pre-existing broad suppression is discovered
- **WHEN** baseline inventory finds an existing broad unsafe suppression outside the governed locations
- **THEN** evidence records its path, scope, and residual risk without claiming it was remediated

### Requirement: Every unsafe diagnostic has a canonical disposition
The implementation SHALL normalize repeated target diagnostics to canonical source locations and MUST assign every baseline location exactly one disposition: unnecessary unsafe removed, behavior-preserving safe API used, or unavoidable unsafe boundary accepted.

#### Scenario: The same MFT source is compiled into multiple targets
- **WHEN** compiler output reports the same source boundary through library and binary module paths
- **THEN** the inventory records one canonical location with every emitting target rather than requiring contradictory duplicate dispositions

#### Scenario: An unnecessary unsafe block is found
- **WHEN** an unsafe block can be removed without changing behavior or ownership
- **THEN** the block is removed and no lint expectation is added for that location

### Requirement: Accepted unsafe boundaries are narrowly documented
Every unavoidable unsafe operation in the governed 113-location default-feature normal-target baseline SHALL have the narrowest practical `#[expect(unsafe_code, reason = "...")]`; its reason MUST identify why unsafe Rust is required, and an adjacent `// SAFETY:` invariant MUST explain why the operation is sound.

#### Scenario: A Windows FFI call is unavoidable
- **WHEN** a reviewed operation must pass raw pointers or handles to a Windows ABI
- **THEN** its expectation identifies the specific ABI boundary and its safety comment covers applicable pointer, buffer, ownership, lifecycle, thread, panic/non-unwind, return-code, and cleanup invariants

#### Scenario: A generic reason is used
- **WHEN** an expectation reason states only a generic phrase such as "FFI call" or "required by Windows"
- **THEN** the boundary review gate fails until the reason identifies the concrete necessity

#### Scenario: An unsafe operation is later removed
- **WHEN** a source edit leaves a lint expectation without a matching unsafe diagnostic
- **THEN** compilation reports the unfulfilled expectation and the stale attribute is removed rather than suppressing the report

### Requirement: Unsafe warning cleanup preserves behavior and warning quality
The cleanup MUST NOT intentionally change runtime behavior, public API, ABI, persistence formats, process boundaries, dependencies, or unrelated user edits, and it SHALL NOT increase any non-`unsafe_code` warning category from the captured baseline.

#### Scenario: A safe replacement would alter ownership behavior
- **WHEN** replacing an unsafe operation with a safe abstraction would change handle ownership, cleanup, ABI, or runtime behavior
- **THEN** the existing operation is retained behind a narrow reviewed expectation instead of expanding the change

#### Scenario: A batch introduces another warning
- **WHEN** post-batch compiler output contains more diagnostics for any non-`unsafe_code` lint than the immutable baseline
- **THEN** the batch remains incomplete until the regression is removed or an in-scope design/spec correction is recorded and revalidated

#### Scenario: Dirty-tree work overlaps a diagnostic
- **WHEN** an unsafe diagnostic occurs next to unrelated uncommitted user changes
- **THEN** the implementation preserves those changes and limits edits to the unsafe boundary and its documentation

#### Scenario: An owned file drifts before a batch edit
- **WHEN** the file's current SHA-256 hash or scoped diff differs from its recorded batch input
- **THEN** affected preservation and dependent evidence become stale, the file is rebaselined, and no edit occurs until attribution is restored

#### Scenario: An owned file drifts during a batch
- **WHEN** the expected hash or relevant preimage differs immediately before any file write or patch
- **THEN** that write does not occur, affected evidence becomes stale, and attribution is restored before work resumes

### Requirement: Normal workspace compilation has zero unsafe diagnostics
The final implementation SHALL pass targeted affected-crate checks and locked normal workspace library/binary compilation with zero diagnostics whose lint code is `unsafe_code`.

#### Scenario: Final structured compiler inventory is captured
- **WHEN** the final locked workspace check completes
- **THEN** its structured diagnostic inventory contains zero `unsafe_code` entries and its non-unsafe warning counts do not exceed baseline

#### Scenario: An unsafe diagnostic remains in a repeated binary target
- **WHEN** a source boundary is clean in the library but still emits `unsafe_code` from a helper or service target
- **THEN** the zero-unsafe gate fails until every normal library and binary target is clean

#### Scenario: The unrelated all-target gate remains broken
- **WHEN** `cargo check --workspace --all-targets --locked` reaches the pre-existing missing-field test initializer errors
- **THEN** evidence records the gate as a pre-existing out-of-scope failure and does not claim that it passed

### Requirement: Validation evidence is traceable and recoverable
Every resolved implementation task SHALL map to an evidence-index record containing the procedure, expected and actual result, exit status or reviewer, related gate, affected file hashes, timestamp, and any adjustment identifier. A fail-closed validator MUST prove that every baseline location has exactly one disposition and every mandatory task has one current passed record.

#### Scenario: A batch validation succeeds
- **WHEN** an implementation batch satisfies its review and compiler gates
- **THEN** its task records are marked passed and reference immutable evidence or unique shared-record subchecks

#### Scenario: A prior result becomes stale
- **WHEN** a later edit changes a file or gate covered by completed evidence
- **THEN** the affected task is reopened, the old evidence is retained as stale lineage, and replacement evidence is linked before completion

#### Scenario: Evidence contains duplicate or unknown identifiers
- **WHEN** the evidence index contains a duplicate location, duplicate current task, unknown identifier, hash mismatch, or stale record without a replacement
- **THEN** the final evidence validator fails closed

#### Scenario: A blocking gate fails
- **WHEN** a required validation command exits unsuccessfully or its expected diagnostic count is not met
- **THEN** the task remains incomplete and the gate is not weakened or relabeled without the required approval
