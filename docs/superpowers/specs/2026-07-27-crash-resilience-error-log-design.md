# Crash Resilience and `error.log` Design

## Goal

The Explorer application must keep its UI process usable when an application-controlled operation fails. Production code must not turn recoverable `Result`, `Option`, conversion, synchronization, parser, Shell, worker, or model errors into panics. Every handled failure must be appended to `error.log` with enough context to diagnose it.

This guarantee applies to failures the Rust application can safely handle. Native access violations, stack overflow, explicit process aborts, forced operating-system termination, and unrecoverable failures inside third-party native code cannot be safely recovered in-process.

## Scope

The audit covers production paths in all workspace crates:

- `explorer-app`
- `explorer-common`
- `explorer-jobs`
- `explorer-model`
- `explorer-search`
- `explorer-shell-win`
- `explorer-ui`

Test-only code and build scripts may retain `unwrap`, `expect`, and deliberate panics because a failed test or build must stop. Vendored dependencies are not modified. Application boundaries still install a panic reporter as a final diagnostic guard for unexpected panics from dependencies.

## Error Log Location

At startup, diagnostics select the first writable location in this order:

1. `error.log` beside the running executable.
2. `%LOCALAPPDATA%\RustGpuiExplorer\logs\error.log`.
3. The operating-system temporary directory under `RustGpuiExplorer\logs\error.log` when `LOCALAPPDATA` is unavailable or unusable.

Selection must not panic. If one candidate cannot be created or opened, initialization tries the next candidate. Failure to initialize every candidate must not panic or abort before the application makes its normal startup attempt; it falls back to `tracing::error!`/standard error where available.

The existing general diagnostics log may remain for lifecycle events. All errors and panic reports are also written to `error.log`.

## Log Record Contract

Each record is a single append-only line containing:

- timestamp;
- severity;
- subsystem and operation;
- human-readable error chain;
- source location when available;
- thread name;
- application version;
- panic payload and backtrace availability for panic reports.

Configured user-profile path prefixes are redacted using the existing diagnostics policy. A logging failure must never recursively invoke the same logger or panic. Poisoned log mutexes are recovered where safe, and failed writes are reported only through the fallback sink.

## Production Panic Removal

Production source is audited for `unwrap`, `expect`, `panic!`, `todo!`, and `unimplemented!`. Each occurrence is replaced according to its contract:

- Fallible I/O, Windows API, channel, worker, and conversion operations return a typed error or an `anyhow::Error` to the nearest recovery boundary.
- Optional user or Shell data uses explicit `None` handling and cancels only the affected operation.
- Mutex poisoning recovers the inner value only when the protected state remains valid; otherwise the operation is rejected and logged.
- Parser cursor access uses checked character extraction and returns a parse error instead of assuming an internal boundary.
- Model invariant failures become explicit errors at mutation boundaries. The invalid mutation is not committed, preserving the last valid window, tab, and navigation state.
- Fixed platform constants use compile-time-safe casts or validated helper functions instead of runtime panic paths.

Blind defaults are not used where they could create a false Shell identity, invalid handle, incorrect file operation, or corrupted UI/model state.

## Recovery Boundaries

Failures are contained at the smallest boundary that can preserve valid state:

- A UI command failure records the error, clears any transient busy state, and leaves the window open for the next command.
- A navigation, search, watcher, icon, clipboard, drag/drop, context-menu, or file-operation failure terminates only that request and reports a failure event to the UI.
- A background worker panic is caught at its worker entry point, recorded, and converted into a terminal failure event. Other workers and the UI remain alive.
- Startup failures after diagnostics initialization are recorded and cleaned up in reverse acquisition order. A prerequisite that makes the UI impossible to create may end the startup attempt gracefully; it must produce `error.log` rather than disappear without a diagnostic.
- The process panic hook writes a final report. Catching a panic does not imply continuing with potentially corrupted application state; recoverable code paths must use errors rather than panics.

## Verification

Automated verification includes:

1. A source-policy test that scans non-test workspace production code and rejects `unwrap`, `expect`, `panic!`, `todo!`, and `unimplemented!`.
2. Diagnostics tests for executable-directory selection, fallback selection, append behavior, redaction, poisoned state, and unwritable candidates.
3. Error-injection tests proving that UI commands, Shell requests, worker jobs, search parsing, and startup stages produce an error record and can process a subsequent valid operation.
4. A controlled unexpected-panic test proving that the panic report reaches `error.log` without recursive logging failure.
5. The existing workspace tests, Clippy checks, and Windows smoke tests.

## Acceptance Criteria

- No prohibited panic API remains in workspace production paths.
- Every converted failure has an explicit recovery action and logging context.
- A failed recoverable operation does not close the application window or prevent a subsequent operation.
- `error.log` is created beside the executable when writable and uses the documented fallback otherwise.
- Log creation and log writes cannot panic.
- Tests distinguish recoverable application failures from unrecoverable native/process termination.
