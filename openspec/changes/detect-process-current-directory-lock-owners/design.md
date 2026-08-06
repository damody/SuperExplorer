## Context

`LockOwnerQueryServiceV1` currently projects Windows Restart Manager results through `explorer-shell-win` into the application composition root, a short generation-scoped cache, and the Rust Lock owner example column. Restart Manager is authoritative for registered file resources but does not reliably report a console process whose only relationship to a directory is its process current directory. The implementation must remain Windows-only, discover-only, privacy-safe, bounded, cancellable, compatible with the current ABI, and testable with a real process through UTIT.

The approved source design is `docs/superpowers/specs/2026-08-06-lock-owner-process-current-directory-design.md`.

## Goals / Non-Goals

**Goals:**

- Report a process when its current directory equals or descends from a queried directory.
- Project the same process on every visible ancestor row at path-component boundaries.
- Preserve Restart Manager file-lock discovery and merge duplicate identities deterministically.
- Recompute through the existing F5/manual refresh generation and clear exited or moved owners.
- Bound process enumeration, remote reads, output count, latency, cancellation, and handle lifetime.
- Prove production behavior with focused unit/integration tests and a real `cmd.exe` headful UTIT.

**Non-Goals:**

- Process shutdown, termination, handle closure, or mutation.
- Command-line, environment, credential, native-handle, or full executable-path disclosure.
- Continuous polling or background process monitoring.
- Current-directory attribution for queried file rows.
- Replacing Restart Manager for genuine file owners.

## Decisions

### 1. Add a separate native current-directory discovery source

`explorer-shell-win` will expose one discover-only, Windows-specific function that accepts the whole already-authorized filesystem-resource batch plus a live cancellation token and one absolute deadline. It will enumerate at most 4,096 candidates from one Toolhelp process snapshot per batch, exclude the current SuperExplorer PID, open each candidate with the minimum query/read rights, obtain native or WOW64 process-environment metadata, and copy only the remote current-directory string after validating address, byte length, alignment, and a maximum of 32,768 UTF-16 code units (65,536 bytes). An over-limit remote string skips that malformed candidate. Exceeding the candidate-count bound returns a typed current-source `Unavailable` result instead of silently scanning or publishing a partial process set. The root `windows` feature manifest will explicitly enable the required Toolhelp bindings; no helper executable or new dependency is introduced.

The implementation will isolate pointer-width-specific layouts behind testable readers. Supported x64 Windows builds must exercise both native 64-bit and WOW64 32-bit real-process fixtures; an unknown layout may be skipped, but a supported WOW64 layout may not be waived. RAII owns snapshot and process handles. A cancellation or deadline check occurs before enumeration, before and after remote reads, and between candidates. Per-process access denial, protection, exit, malformed data, or layout race is a skipped candidate; snapshot/service-wide failure remains typed `Unavailable`/host error according to the terminal truth table below.

This is preferred over open-handle enumeration because a generic directory handle does not establish current-directory identity, and over recursive Restart Manager registration because it remains incomplete and unbounded.

### 2. Match normalized Windows path components

A pure matcher will first require the queried resource to be a directory, normalize Windows case, repeated/trailing separators, drive roots, UNC/share roots and equivalent extended-length `\\?\` forms, then compare path components. Equality and true descendant relationships match; textual prefixes without a component boundary do not. Relative paths and unresolved `.`/`..` components are rejected rather than converted into drive-relative paths. Metadata races that change a queried directory into a file contribute no current-directory owner. The function will be independently unit tested and reused for every candidate/resource comparison.

### 3. Preserve stable owner identity and existing privacy projection

Current-directory owners will use the existing `LockOwner` model with PID plus process creation time as identity, a basename-only safe display name, Console application type for console images where derivable and Unknown otherwise, and no service/command/path payload. The application composition root will merge Restart Manager and current-directory records by identity, prefer Restart Manager metadata on collision, sort by process ID, creation time, case-folded display name and application type, then truncate to the existing owner limit. Input enumeration order cannot affect the retained set.

No public extension ABI changes. The plugin continues to receive only the existing bounded owned records.

### 4. Batch the internal service seam and contain ABI panics

The host-only `HostLockOwnerQueryServiceV1` seam will change from one `(path, relative timeout)` callback per item to one batch callback containing all resolved item/path pairs, one absolute deadline, and a live job-cancellation predicate. This is an internal Rust host seam, not the public `LockOwnerQueryServiceV1` ABI. The application performs one current-directory snapshot/walk for the batch, while Restart Manager resource queries consume the same remaining deadline. The maximum 128-item ABI request therefore cannot trigger 128 process snapshots.

`HostLockOwnerQueryAdapterV1::query` will delegate to `query_inner` inside `catch_unwind(AssertUnwindSafe(...))`. A panic in authorization, native discovery, result composition, or the injected host callback returns generation-preserving `HOST_ERROR`, drops all owned payloads/handles, and does not unwind across `abi_stable`.

### 5. Freeze terminal composition

The internal terminal model adds `DeadlineElapsed`. Composition is performed per item after both sources use the shared request context:

| Restart Manager | Current directory | Per-item result |
| --- | --- | --- |
| any | Cancelled | Cancelled |
| Cancelled | any | Cancelled |
| any non-cancelled | DeadlineElapsed | DeadlineElapsed |
| DeadlineElapsed | any non-cancelled | DeadlineElapsed |
| Ready | Ready/Empty/Unavailable/Failed | Ready with all available Ready owners |
| Empty/Unavailable/Failed | Ready | Ready with current-directory owners |
| Empty | Empty | Empty |
| Failed | Empty/Unavailable/Failed | HostError |
| Empty/Unavailable | Failed | HostError |
| Unavailable | Empty/Unavailable | Unavailable |
| Empty | Unavailable | Unavailable |

Cancellation dominates deadline because explicit lifecycle revocation must never publish. Deadline dominates every non-cancelled outcome. A Ready source remains useful if the other source is unavailable or failed; the available owners are published as Ready rather than regressing genuine file-lock results. Without a Ready source, HostError dominates Unavailable, which dominates Empty.

Because public `LockOwnerQueryOutcomeV1` has one status for a multi-item batch, the adapter includes every per-item Ready owner record and selects the batch status from ownerless items only: HostError dominates Unavailable, which dominates Empty; if no ownerless item exists the status is Ready. Thus an item with owners remains displayable even when another item failed, while an ownerless item is never falsely projected as Empty when any ownerless peer is Unavailable/HostError. This conservative V1 fallback can make a genuinely Empty peer appear unavailable, which is preferable to a false empty and requires no ABI expansion. Cancellation and deadline remain batch-wide and suppress every owner.

### 6. Reuse generation-scoped cache and refresh

The composition root will perform both discovery sources as one logical query and cache only the merged projection under the existing canonical resource identity, short TTL, and refresh generation. F5/manual refresh advances generation and reschedules the combined query. Late results are rejected by the current generation checks, so an older occupied value cannot replace a newer empty value.

### 7. Extend the production Lock owner UTIT

The existing `rust-lock-owner-headful` case will retain its real file-lock and stale-generation checks and add nested native and WOW64 fixtures. It launches native `cmd.exe` and the operating system's `%SystemRoot%\SysWOW64\cmd.exe` directly with explicit nested working directories without shell-string composition, verifies the second process with `IsWow64Process2`, verifies owner projection on the nested folder and visible parent row, captures evidence, exits both processes, invokes production F5, and verifies clearing. This uses the installed Windows WOW64 subsystem and requires no i686 Rust target or prebuilt fixture binary. A separate integration test changes a controlled process to a directory outside the subtree and verifies refresh clearing. The manifest requires the new screenshots/report fields so a mocked UI value cannot satisfy the gate. English and Traditional Chinese example documentation will describe ancestry, privacy/false-negative policy, refresh/TTL/deadline behavior and offline reproduction.

## Failure Handling and Security

- Remote reads use checked arithmetic and fixed maximum lengths; malformed or racing layouts never become pointers trusted by Rust references.
- Only basename display data leaves the shell adapter. Current-directory strings are used for matching and discarded.
- Current process exclusion prevents SuperExplorer's launch directory from self-marking a subtree.
- The discover-only public service remains unable to close or terminate any result.
- Deadline/cancellation releases all handles and suppresses stale publication.
- One inaccessible process is not a global error; global snapshot/setup failure remains observable through the existing typed status.

## Testing and Blocking Gates

- **G1 Native safety and matching:** focused `explorer-shell-win` tests pass for local/UNC/extended path ancestry, file/race bypass, native and WOW64 readers, malformed lengths, cancellation/deadline, access denial, process exit, self exclusion, handle cleanup, 4,096-candidate overflow and deterministic output.
- **G2 Composition and refresh:** focused model/app tests pass for the terminal truth table, identity merge precedence, deterministic bounds, one-snapshot maximum-item batching, cache generation, F5 invalidation, and stale-result rejection.
- **G3 ABI/security:** extension-host/API contract, injected-panic, cleanup and architecture checks prove no unwind, new process-control, or sensitive-data surface.
- **G4 Production UTIT:** `rust-lock-owner-headful` passes locally with real nested native and WOW64 processes, parent projection, evidence screenshots, F5 clearing, and existing file-lock/stale/disable assertions; EN/zh-TW docs reproduce offline.
- **G5 Regression:** format, affected crate tests, offline compile, manifest validation, and strict OpenSpec validation pass. Failures outside touched paths must be classified with reproducible evidence and may not be silently waived if introduced by this change.

## Planning and Evidence Adjustments

- **A — task refinement:** leaf split/order/command/evidence-path changes may be made without changing scope, requirements, gates, public contracts, or thresholds.
- **B — design/spec correction:** a correction within approved scope requires affected tasks to pause, design/spec/tasks to be updated and revalidated, and dependent evidence to be marked stale.
- **C — material change:** any new permission, process mutation, ABI change, polling, platform expansion, weakened gate, or reduced evidence requires user approval.

Every completed atomic task records command/procedure, expected and actual result, exit status, timestamp, hashes where applicable, and related gate in `openspec/changes/detect-process-current-directory-lock-owners/evidence/index.json`. Existing evidence is append-only; replacements identify superseded records.

## Risks / Trade-offs

- **[Undocumented process-layout variability]** → Isolate native/WOW64 layouts, validate every remote field, gate with real-process tests, and skip unsupported candidates rather than guessing.
- **[Protected or cross-architecture processes are unreadable]** → Treat per-process denial as a non-fatal omission and retain Restart Manager results.
- **[Process enumeration cost]** → Use one snapshot per authorized batch, bound candidates to 4,096, bound remote string size/owners/elapsed time, and perform no recursive filesystem traversal or continuous polling.
- **[PID reuse and process races]** → Bind identity to PID plus creation time and re-check through handle-derived metadata before projection.
- **[Self or prefix false positives]** → Exclude current PID and use component-aware normalized ancestry rather than string prefixing.

## Migration Plan

No data or ABI migration is required. Land the native probe and tests first, then composition merge/cache tests, then extend the fixture and UTIT manifest. Rollback removes the new discovery call while retaining Restart Manager, the existing cache schema, and plugin ABI. No persistent state needs conversion.

## Open Questions

None. Unsupported or inaccessible individual processes are explicitly skipped, and any proposal to broaden permissions or weaken the blocking real-process gate is a material change requiring approval.
