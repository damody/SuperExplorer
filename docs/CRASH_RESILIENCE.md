# Crash Resilience and Error Logs

The Explorer keeps its window and remaining services usable when an application-controlled operation fails. Navigation, search, watcher, icon, clipboard, drag/drop, context-menu, file-operation, UI-command, and isolated worker failures terminate only the affected request whenever valid state can be preserved.

Every handled error is appended to `error.log`. The application tries these locations in order:

1. beside the running executable;
2. `%LOCALAPPDATA%\RustGpuiExplorer\logs\error.log`;
3. `%TEMP%\RustGpuiExplorer\logs\error.log`.

When `EXPLORER_LOG_DIR` is set for a controlled run, its `error.log` is used. Records contain the severity, subsystem, operation, error chain, thread, application version, timestamp, and source location when available. Configured user-profile path prefixes are redacted. Failure to create or write the error log is handled on a best-effort basis and does not recursively invoke the logger.

Production crate roots deny Clippy's `unwrap_used`, `expect_used`, `panic`, `todo`, and `unimplemented` lints whenever they are compiled without `cfg(test)`. Tests and build scripts retain deliberate fail-fast behavior.

This guarantee covers recoverable Rust errors and unwindable panics inside explicitly isolated workers. No in-process program can safely guarantee recovery from native access violations, stack overflow, explicit process abort, forced operating-system termination, or unrecoverable corruption inside a native dependency. The panic hook still attempts to write a final redacted diagnostic before the normal terminal panic behavior.
