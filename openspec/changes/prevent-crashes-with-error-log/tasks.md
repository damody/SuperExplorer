## 1. Error Diagnostics Foundation

- [x] 1.1 Extend `explorer-common` diagnostics configuration with injectable error-log candidates and production executable/LocalAppData/temp candidate discovery.
- [x] 1.2 Implement an independent append-only error sink and structured error-record API with timestamp, severity, subsystem, operation, error chain, thread, version, optional source, and path redaction.
- [x] 1.3 Make error-sink initialization, poisoned mutex handling, write/flush failure, and fallback reporting best-effort and non-recursive.
- [x] 1.4 Route the process panic hook and application startup/shutdown errors to `error.log` while preserving existing general lifecycle diagnostics.
- [x] 1.5 Add diagnostics unit and subprocess tests for preferred/fallback paths, all-candidates-fail behavior, append, redaction, poisoned state, write failure, and panic reporting.

## 2. Leaf Panic-Path Removal

- [x] 2.1 Replace production parser cursor `expect` calls in `explorer-search` with checked extraction and parse errors; test a later valid query after failure.
- [x] 2.2 Replace production size/index conversion and optional-value `expect` calls in Shell watcher, navigation, icon, clipboard, and drag/drop helpers with typed errors or safe cancellation.
- [x] 2.3 Replace production model window/tab/location invariant `expect` calls with fallible accessors and validate-then-commit mutations that preserve the prior state.
- [x] 2.4 Audit `explorer-jobs`, `explorer-ui`, and remaining application-owned production targets for panic-producing APIs and convert each hit without blind domain defaults.
- [x] 2.5 Update callers across crate boundaries to propagate the new errors to their nearest recovery boundary and add contextual subsystem/operation data.

## 3. Recovery Boundaries

- [x] 3.1 Ensure UI command failures clear only their transient state, retain the last valid view/model, record the error, and accept the next command.
- [x] 3.2 Ensure navigation, search, watcher, icon, clipboard, drag/drop, context-menu, and file-operation failures terminate only their request with a failure event.
- [x] 3.3 Add panic isolation to independently owned background worker entry points, converting unwind payloads to logged terminal failures and releasing worker resources.
- [x] 3.4 Ensure startup failure and partial shutdown failure paths log context and continue reverse-order cleanup without masking the original error.

## 4. Regression Tests and Policy

- [x] 4.1 Add a source-policy test that rejects `unwrap`, `expect`, `panic!`, `todo!`, and `unimplemented!` in workspace-owned non-test production targets while excluding tests, build scripts, and vendor code.
- [x] 4.2 Add model and UI error-injection tests proving a failed mutation/command preserves state and a subsequent valid command succeeds.
- [x] 4.3 Add Shell and worker error-injection tests proving one failed request or unwind does not stop unrelated workers or the UI request stream.
- [x] 4.4 Document the supported recoverable-failure guarantee and the exclusion of access violations, stack overflow, abort, OS termination, and unrecoverable native corruption.

## 5. Verification

- [x] 5.1 Run formatting and the source-policy scan, then resolve every production panic-API violation.
- [x] 5.2 Run focused diagnostics, model, search, jobs, Shell, UI, and application tests after each affected crate batch.
- [x] 5.3 Run the complete workspace test suite and Clippy gates, preserving unrelated working-tree changes.
- [x] 5.4 Run applicable Windows lifecycle and interaction smoke tests and record any environment-limited manual verification separately.
