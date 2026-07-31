## Why

Recoverable failures in production paths can still be promoted to panics by `unwrap`, `expect`, or invariant assertions, causing the Explorer window to disappear. The application needs explicit recovery boundaries and a dependable `error.log` so failures remain diagnosable without terminating otherwise usable UI and worker services.

## What Changes

- Add an append-only `error.log` with executable-directory, LocalAppData, and temporary-directory fallback locations.
- Record structured, redacted error and panic context without allowing logger initialization, mutex poisoning, or write failures to panic recursively.
- Remove panic-producing APIs from non-test workspace production paths and replace them with typed propagation, checked optional handling, or safe request cancellation.
- Contain failures at UI command, model mutation, Shell request, and background-worker boundaries so a failed operation does not prevent the next valid operation.
- Add policy scans and error-injection tests that prevent regressions and distinguish recoverable application failures from unrecoverable native or process termination.

## Capabilities

### New Capabilities

- `resilient-error-diagnostics`: Selects a writable `error.log`, emits safe structured records, redacts sensitive paths, and preserves a final panic report.
- `recoverable-operation-boundaries`: Defines panic-free production paths and per-operation recovery semantics across UI, model, search, Shell, workers, startup, and shutdown.

### Modified Capabilities

None. The repository has no promoted baseline specs; the existing foundation requirements remain historical change artifacts.

## Impact

The change affects all production workspace crates, with the largest changes in `explorer-common` diagnostics, the `explorer-app` composition root, Shell worker entry points, model mutation APIs, and UI command handling. It adds no required third-party dependency or public network service. Existing general lifecycle diagnostics may remain, while errors and panic reports also flow to `error.log`.
