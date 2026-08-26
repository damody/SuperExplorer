# Unsafe Code Warning Governance Design

## Context

The Rust workspace currently completes `cargo check --workspace --locked`, but emits hundreds of warnings. The first cleanup wave targets only the `unsafe_code` lint. `dead_code` and broader Clippy cleanup are deliberately deferred so warning reduction does not become an uncontrolled deletion or refactoring exercise.

The baseline inventory found 113 canonical unsafe locations across 11 source files. Because the MFT modules are compiled into more than one target, those locations produce roughly 194–212 warning diagnostics in a workspace build. Most locations are Windows FFI calls, raw handle operations, or pointer conversions that cannot be expressed entirely in safe Rust.

## Goals

- Remove all `unsafe_code` diagnostics from normal workspace library and binary compilation.
- Preserve the workspace-level `unsafe_code = "warn"` policy and do not introduce any new crate-wide or module-wide suppression. Existing broad suppressions outside the 113-location normal-target baseline are inventoried as deferred residual risk rather than treated as remediated.
- Make every accepted unsafe boundary in the governed 113-location baseline state why unsafe is unavoidable and why the operation is sound.
- Avoid changing runtime behavior, ABI layout, process boundaries, or MFT persistence behavior.
- Avoid increasing any non-unsafe warning category.

## Non-goals

- Removing `dead_code` warnings in this change.
- Making the entire workspace Clippy-clean.
- Repairing the existing unrelated all-target test compilation failures.
- Reorganizing all MFT modules into a new crate or replacing the current Windows APIs.
- Auditing or removing pre-existing broad unsafe suppressions outside the 113-location normal-target baseline.
- Applying any new crate-wide, module-wide, or workspace-wide `allow(unsafe_code)` or `expect(unsafe_code)` attribute.

## Chosen Approach

Use narrow `#[expect(unsafe_code, reason = "...")]` attributes at individual unsafe operations, unsafe extern blocks, or unsafe functions. A function-level expectation may cover multiple operations only when they form one inseparable safety boundary with one invariant. This change may not add module-level expectations because they would silently accept future unrelated unsafe code. Existing broad suppressions elsewhere are recorded for a later cleanup wave.

When an unsafe block is unnecessary, remove the block instead of adding an expectation. Existing behavior must remain unchanged.

Each accepted boundary has two complementary explanations:

- The `reason` field explains why the operation must use unsafe Rust, such as invoking a Win32 API that accepts a raw process-owned handle.
- The adjacent `// SAFETY:` comment explains the soundness invariant: pointer validity, buffer extent, handle ownership, thread or apartment requirements, return-code checks, and cleanup behavior as applicable.

Generic reasons such as "FFI call" or "required by Windows" are insufficient.

## Scope and Ordering

Work proceeds in risk-oriented batches:

1. Small composition boundaries: `main.rs`, `application.rs`, `brokered_service.rs`, and `explorer-extension-host/src/virtual_container_mutation.rs`.
2. Focus and journal boundaries: `mft_focus.rs` and `mft_journal.rs`.
3. Migration, size-map, and SQLite boundaries: `mft_migration.rs`, `mft_size_map.rs`, and `mft_sqlite.rs`.
4. High-volume query and service boundaries: `mft_query.rs` and `src/bin/mft_service.rs`.

The implementation will preserve user changes already present in the working tree and will not rewrite unrelated lines. Before editing, it records each owned file's SHA-256 hash and scoped pre-change diff. Immediately before every file write or patch, it compares the expected hash and relevant preimage; immediately afterward, it verifies the intended hunk and records the new expected hash. Any drift invalidates or rebaselines dependent evidence before further edits. Formatting is path-limited to files changed by this work.

## Review Rules

For every diagnostic location, the implementation must choose exactly one outcome:

1. Remove an unnecessary unsafe block.
2. Replace the operation with an existing safe API without changing behavior.
3. Add a narrow, reasoned expectation and confirm or improve the adjacent safety comment.

Review must pay particular attention to:

- Raw pointer construction and the byte or element length associated with it.
- Win32 handles, including invalid sentinel values and the party responsible for closing them.
- Out-parameters and initialization before values are read.
- UTF-16 buffers and terminating NUL requirements.
- Overlapped I/O lifetimes and event ownership.
- ABI callback signatures, thread requirements, and panic boundaries.
- Integer conversions used as buffer lengths or API parameters.

## Validation

Run formatting only on changed Rust sources, then validate in increasing scope:

1. Targeted `cargo check` for `explorer-extension-host` and `explorer-app` library/binary targets.
2. `cargo check --workspace --lib --bins --locked` must succeed.
3. `cargo check --workspace --locked` must emit no diagnostic whose lint code is `unsafe_code`.
4. Compare warning categories against the saved baseline; non-unsafe warnings must not increase.
5. Run relevant existing unit tests for modified safety-boundary modules when they compile independently.
6. Validate the disposition and evidence schema fail-closed: every baseline location has exactly one disposition, every mandatory task has one current passed record, hashes match, stale records link to replacements, and duplicate or unknown IDs fail.
7. Run the final authoritative Cargo checks with `--offline` and record toolchain, target triple, Cargo configuration, features, and relevant build environment.

`cargo check --workspace --all-targets --locked` currently fails on two unrelated missing-field initializers in `explorer-extension-host` tests. This cleanup records that pre-existing blocker but does not broaden scope to repair it. The final report must not claim the all-target gate passes unless that external state changes.

## Acceptance Criteria

- Zero `unsafe_code` warnings in normal workspace compilation.
- No new crate-wide or module-wide unsafe suppression, and every pre-existing broad suppression outside the baseline is inventoried as deferred residual risk.
- Every remaining unsafe boundary in the governed 113-location default-feature normal-target baseline has a specific expectation reason and an adequate safety invariant.
- No intentional `dead_code` cleanup or unrelated refactor is included.
- Targeted and workspace library/binary checks pass.
- Non-unsafe warning totals do not increase from the captured baseline.
