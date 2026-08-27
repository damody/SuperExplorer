## Context

The approved design at `docs/superpowers/specs/2026-08-27-normal-workspace-warning-cleanup-design.md` identifies 17 warnings in the normal locked/offline workspace build. Implementation evidence subsequently identified 28 equivalent import and qualification warnings in test targets. Most are lexical simplifications, but the two `event_guard` bindings deliberately hold Win32 handles until scope exit and must not be deleted.

## Goals / Non-Goals

**Goals:**

- Reach zero warnings and zero errors in both normal and all-target locked/offline workspace checks.
- Preserve runtime behavior, public interfaces, control flow, resource lifetime, and lint governance.
- Make each edit manually and review it in context.

**Non-Goals:**

- Running `cargo fix`.
- Cleaning feature-only or Clippy-only diagnostics.
- Refactoring modules or relocating MFT source files.

## Decisions

1. Redundant qualification warnings are fixed by using names already imported in each module. Adding new imports is allowed only when it is the smallest equivalent edit.
2. The unused `HashMap` import and unnecessary `mut` are removed after checking the complete enclosing item for conditional or platform-specific use.
3. `event_guard` becomes `_event_guard`. This suppresses only the unused-binding warning while preserving construction, ownership, destructor timing, and handle cleanup.
4. The unused cancellation error pattern becomes `Err(_)` because the branch uses only the cancellation state. Matching order and returned terminal state remain unchanged.
5. No lint allowances or expectations are introduced. The compiler output itself is the final inventory, and every newly exposed normal-build warning stays in scope.
6. All-target test warnings that only remove imported names or redundant qualifications use the same manual equivalence rules. The 1,080 Clippy pedantic diagnostics discovered during validation are recorded as a separate governance concern because many require API, numeric-conversion, documentation, and decomposition decisions.

Alternatives rejected: `cargo fix` could alter unrelated dirty-worktree sites; lint suppression hides actionable diagnostics; deleting unread guards would close handles early.

## Risks / Trade-offs

- [Conditional compilation hides a use] → Compile the complete normal workspace on the supported Windows host after each batch.
- [RAII lifetime accidentally changes] → Rename guards in place without changing their declaration position or scope, then run focused MFT focus tests.
- [A qualification edit resolves to another symbol] → Use only an existing import whose resolved item is the same standard-library symbol and rely on compilation/type checking.

## Migration Plan

Apply one local batch, format it, run the focused MFT test, then run the normal workspace check. Rollback is the reversal of only these lexical edits; no stored data or deployment migration exists.

## Open Questions

None.

## Validation Observation

The workspace all-target test run compiled without warnings. After fetching the lockfile-pinned `fiat-crypto` package required by an offline metadata test, that architecture test passed. The full suite later exposed pre-existing environment-sensitive `explorer-shell-win` native test failures (real Shell fixtures resolving under missing `D:\test`, lock-owner behavior, mutex poisoning, and one parallel access violation). These failures are unrelated to the lexical warning edits and are not suppressed or reclassified by this change.
