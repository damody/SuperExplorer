# Lock Owner Process Current Directory Design

## Goal

Make the Lock owners Details column report a process whose current working directory is the queried folder or any descendant of that folder. For example, when `cmd.exe` has `D:\AI_Pic\ComfyUI\subfolder` as its current directory, rows for `subfolder`, `ComfyUI`, and `AI_Pic` are occupied by `cmd.exe`. Pressing F5 must recompute the result, and the owner must disappear after the process exits or changes to a directory outside the queried subtree.

## Existing Problem

The host currently delegates discovery to Windows Restart Manager. Restart Manager reports applications that hold registered file resources, but a console process merely using a directory as its current working directory is not reliably returned. Re-registering more files cannot solve that gap and is unbounded for large directory trees.

## Selected Approach

Keep Restart Manager for real file-resource ownership and add a bounded, read-only native Windows current-directory probe. The probe enumerates candidate processes, reads the process current-directory state through audited Windows process/PEB access, projects privacy-safe `LockOwner` records, and merges them with Restart Manager results.

Alternatives rejected:

- Enumerating every open directory handle cannot reliably distinguish the current-directory handle from unrelated handles and can create false positives.
- Recursively registering all descendant files with Restart Manager remains unable to detect a process that only changed directory, while introducing unacceptable I/O and resource growth.
- WMI and command-line helpers do not expose a reliable current-directory contract and would add external execution and parsing paths.

## Matching Semantics

Paths are normalized with Windows case-insensitive component semantics before matching. A candidate process matches a queried directory when its current directory is equal to that directory or is a descendant separated at a path-component boundary. String-prefix matches such as `D:\AI` against `D:\AI_Pic` are rejected.

For a queried file, the new current-directory probe contributes no owner; file ownership remains Restart Manager's responsibility. For a queried directory, a process at any depth below that directory contributes one owner. The SuperExplorer process itself is excluded so its launch directory cannot mark the application tree as occupied.

## Native Probe Boundary

The Windows-specific implementation lives in `explorer-shell-win` behind one discover-only function. It:

- enumerates a bounded process snapshot;
- opens candidates with only the query/read rights required for discovery;
- supports native and WOW64 process layouts when the operating system exposes them;
- reads only the process parameters needed to obtain the current directory;
- validates every remote address and length before copying;
- observes cancellation and an overall deadline between candidates;
- closes every snapshot and process handle through RAII;
- never exposes a native handle, command line, environment block, credentials, or process-control operation.

Access-denied, protected, exiting, malformed, and racing processes are skipped individually. Failure to inspect one process must not turn an otherwise valid result into `Unavailable`. A failure to establish the bounded snapshot or another service-wide failure follows the existing typed error contract.

## Result Composition

The application lock-owner service performs the existing Restart Manager query and current-directory query within the same generation-scoped request. Results are merged by stable process identity: PID plus process creation time. Restart Manager metadata wins when both sources return the same identity; otherwise the current-directory probe supplies a safe executable display name and application type.

The merged result remains bounded by the existing maximum-owner contract and is sorted deterministically. Empty, unavailable, cancellation, deadline, and stale-generation behavior retain the current extension-host ABI.

## Refresh and Cache

The current short-TTL cache remains keyed by canonical resource identity and refresh generation. Current-directory results are stored only as part of the merged result. F5 and the extension refresh action advance the generation, invalidate the prior lookup, and schedule a new combined query. Results from an older generation cannot overwrite a newer empty or occupied value.

No background polling is introduced. The column updates through its existing initial query, short TTL, and explicit refresh paths.

## Testing

Unit tests cover:

- exact-directory and descendant matching;
- recursive parent reporting;
- component-boundary rejection;
- case-insensitive and trailing-separator normalization;
- files bypassing current-directory matching;
- self-process exclusion;
- merge deduplication and deterministic bounds;
- cancellation, malformed remote data, access-denied and process-exit races;
- F5 generation invalidation and stale-result rejection.

A blocking Windows UTIT launches a real `cmd.exe` with its current directory set to a nested fixture folder. It verifies that the nested folder and its visible parent row show `cmd.exe`, captures evidence, changes or exits the console process, presses F5, and verifies that the owner is cleared. The test must use the production Lock owner plugin and host service rather than a mocked cell value.

## Non-Goals

- Closing or terminating a process discovered only through its current directory.
- Exposing process command lines, environment variables, native handles, or full executable paths to extensions.
- Continuous real-time polling of process working directories.
- Treating a similarly prefixed sibling path as a descendant.
