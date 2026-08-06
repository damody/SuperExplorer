## MODIFIED Requirements

### Requirement: Restricted lock-owner query service
The host SHALL expose a read-only `LockOwnerQueryServiceV1` accepting a bounded list of capability-authorized item handles. For files it SHALL retain Windows Restart Manager ownership discovery. For directories it SHALL additionally discover a bounded set of processes whose current directory equals or descends from the queried directory at a normalized Windows path-component boundary. Results SHALL be merged by PID plus process creation time into bounded owned records containing PID, safe basename display name, application type and safe status, without exposing native handles, current-directory paths, command lines, environments or credentials.

#### Scenario: File is locked by multiple processes
- **WHEN** a provider queries a file held by two helper processes
- **THEN** the service returns two bounded owned records without exposing native handles

#### Scenario: Console uses a nested current directory
- **WHEN** `cmd.exe` has `D:\AI_Pic\ComfyUI\subfolder` as its current directory and the provider queries the `subfolder`, `ComfyUI`, or `AI_Pic` directory
- **THEN** each queried directory returns one privacy-safe `cmd.exe` owner record for that same process identity

#### Scenario: Similar path prefix is not ancestry
- **WHEN** a process current directory is `D:\AI_Picture` and the provider queries `D:\AI`
- **THEN** the process is not returned because the paths do not share a component-boundary ancestry relationship

#### Scenario: Local, UNC and extended roots normalize safely
- **WHEN** equivalent drive-root, UNC/share-root or `\\?\` absolute paths use mixed case, repeated separators or trailing separators
- **THEN** the service compares their normalized components without converting a root to a drive-relative path or crossing into a sibling share

#### Scenario: Relative traversal input is unresolved
- **WHEN** a candidate or resource path is relative or retains unresolved `.` or `..` components
- **THEN** the current-directory source rejects that comparison rather than guessing an absolute ancestry relationship

#### Scenario: Current directory is queried as a file
- **WHEN** a provider queries a file path while a process current directory is the file's parent
- **THEN** the current-directory source contributes no owner and file ownership remains governed by Restart Manager

#### Scenario: Directory metadata races to a file
- **WHEN** the queried directory becomes a file or disappears before current-directory matching
- **THEN** the current-directory source contributes no owner and does not publish a stale directory match

### Requirement: Deadline, cancellation and cleanup
Every bounded-list query SHALL enforce one live cancellation source and one absolute overall deadline across all items and both discovery sources, a maximum of 4,096 process candidates per single process snapshot, a maximum remote current-directory length of 32,768 UTF-16 code units (65,536 bytes), maximum input/results and guaranteed Restart Manager session, process snapshot and process-handle cleanup across success, error, panic, access-denied, malformed-data and process-exit races. A per-process access denial, protected process, unknown layout, over-limit remote string or exit race SHALL skip only that candidate. Exceeding 4,096 candidates or failing process-snapshot setup SHALL produce current-source Unavailable rather than a silent partial process set. Supported x64 Windows SHALL inspect both native and WOW64 process layouts.

#### Scenario: Query is cancelled
- **WHEN** cancellation occurs after a Restart Manager session or process snapshot starts
- **THEN** every session, snapshot and process handle is released and the result is Cancelled rather than a plugin fault

#### Scenario: Maximum item request shares one process snapshot
- **WHEN** a provider submits the maximum 128 authorized items in one lock-owner query
- **THEN** the host performs at most one current-directory process snapshot/walk, decrements one shared deadline across all work, and observes live job cancellation during native reads

#### Scenario: Candidate count exceeds the bound
- **WHEN** the process snapshot contains more than 4,096 candidates
- **THEN** the current-directory source returns Unavailable without publishing a partial process set or inspecting an unbounded tail

#### Scenario: Native and WOW64 owners use the same ancestry contract
- **WHEN** supported x64 Windows runs native `cmd.exe` and `%SystemRoot%\SysWOW64\cmd.exe`, verified by `IsWow64Process2`, in nested fixture directories
- **THEN** both processes are discovered and projected through the same normalized ancestor matching and privacy rules without requiring an i686 Rust toolchain or prebuilt fixture binary

#### Scenario: One protected process cannot be inspected
- **WHEN** one candidate denies the minimum query/read access while another accessible `cmd.exe` matches the queried directory
- **THEN** the inaccessible candidate is skipped and the accessible owner is still returned

#### Scenario: Remote process data is malformed or races with exit
- **WHEN** a remote pointer or length is invalid or the process exits during inspection
- **THEN** no unchecked memory is read, all acquired handles are closed, and that candidate cannot corrupt or fail other results

### Requirement: Deterministic mixed-source terminal composition
The host SHALL compose Restart Manager and current-directory results per item with the following precedence: explicit cancellation dominates every outcome; deadline elapsed dominates every non-cancelled outcome; any Ready source produces Ready containing every available Ready owner even when the other source is Empty, Unavailable or HostError; without Ready, HostError dominates Unavailable and Unavailable dominates Empty. Ready owners SHALL be deduplicated by PID plus creation time, prefer Restart Manager metadata on collision, sorted by process ID, creation time, case-folded display name and application type, and only then truncated to the existing maximum.

For a public multi-item `LockOwnerQueryOutcomeV1`, cancellation or deadline SHALL remain batch-wide and return no owners. Otherwise the adapter SHALL include every per-item Ready owner and derive the single batch status from ownerless items: HostError dominates Unavailable, which dominates Empty; when no ownerless item exists the batch status is Ready. This conservative V1 projection SHALL NOT report an ownerless item as Empty when any ownerless peer is Unavailable or HostError, and SHALL NOT require a public ABI expansion.

#### Scenario: Current-directory setup fails while Restart Manager is ready
- **WHEN** the process snapshot is unavailable but Restart Manager returns a genuine file owner before the shared deadline
- **THEN** the item returns Ready with that available owner rather than suppressing valid Restart Manager data

#### Scenario: Deadline expires after one source becomes ready
- **WHEN** either source has owners but the shared overall deadline expires before the combined query completes
- **THEN** the item returns DeadlineElapsed with no owners and no late result is published

#### Scenario: Both sources fail without an owner
- **WHEN** neither source is Ready and at least one source returns HostError
- **THEN** HostError is returned; otherwise Unavailable is returned when at least one source is Unavailable, and Empty is returned only when both sources are Empty

#### Scenario: Input enumeration order changes before truncation
- **WHEN** the same owners arrive from either source in different enumeration orders and exceed the maximum result count
- **THEN** identity merge, the frozen sort key and truncation retain the same owner set and ordering

#### Scenario: Batch contains Ready, Empty and Unavailable items
- **WHEN** one item has Ready owners, one item is Empty and one ownerless item is Unavailable
- **THEN** the batch includes the Ready item's owner records and has Unavailable status, so the owner remains displayable and neither ownerless item is falsely reported as definitively Empty

### Requirement: Lock-owner ABI panic containment
The ABI-facing lock-owner service SHALL contain every panic from authorization, native discovery, result composition or the internal host callback before it can unwind across `abi_stable`. It SHALL return generation-preserving HostError with no owner payload and release all sessions, snapshots, process handles and temporary records.

#### Scenario: Native reader panics at the service boundary
- **WHEN** an injected native-reader, composition or host-callback panic reaches the lock-owner ABI adapter
- **THEN** the adapter returns HostError with the request item/location generations, no owners, no leaked resources and a still-usable host process

### Requirement: Short cache and shared refresh path
Merged Restart Manager and process-current-directory values SHALL use the same short TTL. F5 and the extension's manual refresh command SHALL use the same host cache-invalidation/reschedule path and increment the current location refresh generation.

#### Scenario: Lock state changes around F5
- **WHEN** a helper acquires a file lock, F5 is pressed, then releases the lock and F5 is pressed again
- **THEN** the column first displays the owner name and then clears it after the second refresh

#### Scenario: Console leaves a directory before F5
- **WHEN** `cmd.exe` is displayed for a parent directory, then exits or changes to a directory outside that parent subtree before F5
- **THEN** the refreshed merged query clears `cmd.exe` and an older occupied result cannot restore it

### Requirement: Stale lock results are rejected
Restart Manager and process-current-directory request/results SHALL carry the same location/item refresh generation. Switching folder/tab, disabling the feature or pressing F5 again SHALL cancel or ignore older work from either source.

#### Scenario: F5 is pressed rapidly
- **WHEN** an older current-directory or Restart Manager query finishes after the newest refresh query
- **THEN** its generation mismatch prevents it from overwriting the current cell

#### Scenario: Folder changes during process enumeration
- **WHEN** the user navigates away while an older process snapshot is still being inspected
- **THEN** no owner discovered for the previous folder is published into the new folder
