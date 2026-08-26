## ADDED Requirements

### Requirement: Remote mutation capability controls commands
The system SHALL derive create-folder, paste, delete, copy, and cut availability from the typed current location, selected item capabilities, and registered remote provider rather than URI or display-text heuristics.

#### Scenario: Writable remote background
- **WHEN** the current ADB or SFTP directory is writable and its provider exposes create-directory and upload
- **THEN** the background context menu SHALL expose New Folder and Paste when the clipboard contains supported file sources

#### Scenario: Remote selected items
- **WHEN** selected ADB or SFTP entries expose copy and delete capabilities
- **THEN** the item context menu SHALL expose Copy, Cut, and Delete through the same predicate used by keyboard commands

#### Scenario: Unsupported location fails closed
- **WHEN** the current virtual provider is unknown, unavailable, or does not expose a required mutation capability
- **THEN** the affected command SHALL be absent or rejected before provider dispatch

### Requirement: Remote folder creation and deletion are typed and cancellable
The system SHALL create remote folders and permanently delete remote files or trees through the resolved provider with validated components, cooperative cancellation, and exactly one terminal result.

#### Scenario: Create folder in ADB or SFTP
- **WHEN** a valid folder name is submitted in a writable ADB or SFTP directory
- **THEN** the resolved provider SHALL create that child directory and the affected directory SHALL refresh after success

#### Scenario: Invalid remote child name
- **WHEN** a folder name is empty, contains traversal, a separator, NUL, or violates the provider boundary
- **THEN** the request SHALL fail before invoking ADB, SFTP, Shell, or destructive cleanup

#### Scenario: Confirmed remote permanent delete
- **WHEN** the user confirms Delete for one or more ADB or SFTP items
- **THEN** the provider SHALL permanently delete each selected file or recursive tree and report item-level outcomes

#### Scenario: Remote root and identity are never deletable
- **WHEN** a delete target is empty, root, dot, parent, or mismatches provider, authority, container identity, or generation
- **THEN** deletion SHALL fail before provider dispatch and SHALL NOT invoke recursive cleanup

#### Scenario: Confirmation is immutable and stale-safe
- **WHEN** selection, tab, location, or generation changes after a remote delete dialog opens
- **THEN** confirmation SHALL either dispatch only its immutable typed target set with matching nonce and generation or reject the stale confirmation

#### Scenario: SFTP symlink deletion does not follow target
- **WHEN** a confirmed SFTP delete target is a symbolic link
- **THEN** the provider SHALL use link metadata and delete only the link rather than recursively deleting its target

#### Scenario: Local delete remains recyclable
- **WHEN** Delete targets only Local filesystem items
- **THEN** the operation SHALL retain the existing Windows Recycle Bin behavior rather than use remote permanent-delete semantics

#### Scenario: Cancelled remote mutation
- **WHEN** cancellation occurs before or during create or delete
- **THEN** no later unstarted destructive step SHALL start, completed items SHALL retain their true outcomes, unstarted items SHALL be Cancelled, and the request SHALL emit one aggregate terminal result

#### Scenario: Cancellation between permanent-delete items
- **WHEN** cancellation arrives after one selected item is permanently deleted and before the next item's destructive commit
- **THEN** the deleted item SHALL be Succeeded, unstarted items SHALL be Cancelled, and no completed irreversible effect SHALL be reported as Cancelled

### Requirement: Typed file clipboard is isolated from content clipboard
The system SHALL use typed Local or Virtual locations and Copy or Cut intent for file transfer without consuming or overwriting unrelated text, HTML, image, or unknown clipboard formats.

#### Scenario: Copy and cut remote entries
- **WHEN** the file view owns focus and the user invokes Ctrl+C or Ctrl+X on ADB or SFTP entries
- **THEN** the application SHALL store typed source locations and intent without fabricating Local paths

#### Scenario: Native remote clipboard token is authentic
- **WHEN** SuperExplorer publishes a remote file clipboard format
- **THEN** it SHALL contain only a host-minted 256-bit process/session-bound token resolving to an immutable internal record and SHALL NOT expose a directly executable Cut descriptor

#### Scenario: Forged or replayed remote clipboard token
- **WHEN** a token is malformed, foreign, from a previous process, already consumed, or replayed
- **THEN** Paste SHALL fail closed and SHALL NOT transfer or delete any source

#### Scenario: Paste typed mixed-provider sources
- **WHEN** the file view owns focus and Ctrl+V targets a writable Local, ADB, or SFTP directory
- **THEN** Paste SHALL dispatch the same cross-provider transfer request used by the context menu

#### Scenario: Editable text owns keyboard clipboard
- **WHEN** an editable address, search, rename, login, or other text input owns focus
- **THEN** Ctrl+C, Ctrl+X, and Ctrl+V SHALL remain text editing operations and SHALL NOT dispatch file transfer

#### Scenario: Text or image clipboard is ignored by file paste
- **WHEN** the native clipboard contains text, HTML, PNG, bitmap, or an unknown format without a supported file format
- **THEN** file Paste SHALL remain unsupported and SHALL NOT clear or alter the clipboard data

### Requirement: Cross-provider copy supports files and bounded directory trees
The system SHALL copy files and directory trees across Local, ADB, and SFTP boundaries while preserving relative structure, validating every destination component, and honoring conflict decisions and cancellation.

#### Scenario: Local and remote transfer matrix
- **WHEN** a file or directory is copied Local → ADB, ADB → Local, Local → SFTP, SFTP → Local, ADB → SFTP, or SFTP → ADB
- **THEN** the destination SHALL receive the source content through the appropriate Shell, upload, download, or staged transfer path

#### Scenario: Recursive directory copy
- **WHEN** the source is a directory containing nested files and directories
- **THEN** the system SHALL create the corresponding bounded destination tree and SHALL NOT follow symbolic-link targets that can escape or cycle

#### Scenario: Fixed traversal and staging limits
- **WHEN** traversal would exceed depth 64, 100000 nodes, 32 GiB actual bytes for one file, 64 GiB actual staging bytes for one operation, 128 GiB concurrent process staging, or the required max(2 GiB, 5 percent capacity) free-space reserve
- **THEN** N+1 SHALL fail before its next write and before source deletion regardless of provider-reported size

#### Scenario: Destination conflict
- **WHEN** a destination item already exists
- **THEN** the operation SHALL apply the selected Prompt, Skip, Replace, or KeepBoth decision and SHALL NOT silently overwrite

#### Scenario: Skipped descendant prevents move deletion
- **WHEN** any required descendant of a moved directory is Skipped
- **THEN** the item SHALL not be considered completely copied and its source tree SHALL remain undeleted

#### Scenario: Partial destination is not destructively rolled back
- **WHEN** a recursive copy partially writes a destination that also contains pre-existing user data
- **THEN** failure handling SHALL preserve existing and already-written destination data and SHALL NOT issue an unbounded recursive rollback delete

#### Scenario: Traversal or cancellation bound
- **WHEN** a directory exceeds the configured traversal bound or cancellation is observed
- **THEN** remaining enumeration, upload, download, and source deletion SHALL stop with a bounded Failed or Cancelled result

### Requirement: Remote-to-remote transfer uses scoped Local staging
The system SHALL use a unique RAII-owned Local temporary directory for each Remote → Remote transfer that cannot be selected by the user and whose name contains no remote authority or path data.

#### Scenario: Successful staged transfer
- **WHEN** ADB → SFTP or SFTP → ADB copy cannot stream provider-to-provider directly
- **THEN** the source SHALL download beneath the scoped staging root, the destination SHALL upload from it, and the staging tree SHALL be removed after terminal completion

#### Scenario: Failed or cancelled staged transfer
- **WHEN** download, upload, conflict handling, or cancellation terminates a staged transfer
- **THEN** the staging lease SHALL clean its owned tree without recursively deleting any path outside the verified staging root

#### Scenario: Malicious Windows child name
- **WHEN** a Remote name contains a separator, root or drive prefix, NUL, colon or ADS, reserved device name, trailing dot or space, dot/parent, normalization collision, or case-fold collision
- **THEN** Local staging SHALL reject or conflict the item before writing and SHALL verify canonical containment without crossing a symlink, junction, or reparse point

#### Scenario: Non-secret staging diagnostics
- **WHEN** staging creation or I/O fails
- **THEN** diagnostics SHALL omit passwords, file contents, and sensitive remote authority or path values

### Requirement: Cross-provider move is copy then conditional source deletion
The system SHALL implement Cut or cross-provider Move per source item as complete copy followed by source deletion only after the entire destination item tree succeeds.

#### Scenario: Successful move
- **WHEN** a source item's complete destination tree is written successfully and source deletion succeeds
- **THEN** the item outcome SHALL be Succeeded and both affected locations SHALL refresh

#### Scenario: Copy fails before deletion
- **WHEN** any copy, traversal, conflict, or cancellation step for a source item fails
- **THEN** the source SHALL remain undeleted and the item outcome SHALL be Failed or Cancelled

#### Scenario: Source deletion fails after copy
- **WHEN** destination copy completes but source deletion fails
- **THEN** the destination SHALL remain, the source SHALL remain, and the item outcome SHALL be Partial with a non-secret diagnostic

#### Scenario: Stale view after successful move
- **WHEN** a matching Move operation succeeds after its originating view generation becomes stale
- **THEN** snapshots and selection SHALL remain unchanged, but the operation generation SHALL idempotently consume completed Cut items so Paste cannot replay the Move

#### Scenario: Partial move retains only incomplete intent
- **WHEN** a Cut request has mixed Succeeded, Skipped, Failed, or Cancelled item outcomes
- **THEN** completed items SHALL be consumed once and only incomplete items SHALL remain eligible for a later Paste

### Requirement: Internal and Windows Explorer drag and drop share transfer semantics
The system SHALL route application-internal Local, ADB, and SFTP drops through the same capability and Transfer Engine semantics, and SHALL bridge native Windows Explorer file drags without exposing Virtual locations as fake Local paths.

#### Scenario: Internal cross-provider drag
- **WHEN** the user drops Local, ADB, or SFTP items on another writable Local, ADB, or SFTP destination inside SuperExplorer
- **THEN** the selected Copy or Move effect SHALL dispatch the same transfer contract as clipboard Paste

#### Scenario: Native Local files dragged into remote
- **WHEN** Windows Explorer supplies supported Local file paths to an ADB or SFTP drop target
- **THEN** the system SHALL convert them to typed Local sources and upload them through the Transfer Engine

#### Scenario: Remote items dragged to Windows Explorer
- **WHEN** ADB or SFTP entries are dragged out to Windows Explorer
- **THEN** the system SHALL fully materialize them in an owned staging lease before publishing Local file paths, retain the lease until Shell consumption ends, and then clean it
- **AND** the offered native effect SHALL be Copy only

#### Scenario: COM staging lease remains valid
- **WHEN** DoDragDrop returns or the SuperExplorer window closes while Windows Explorer still holds the data object
- **THEN** cleanup SHALL wait for both drag terminal and final COM Release, and COM ownership, STA affinity, STGMEDIUM release, and callback panic conversion SHALL remain valid and exactly once

#### Scenario: Drag-out materialization fails
- **WHEN** remote staging cannot fully materialize the dragged selection
- **THEN** the drag SHALL be cancelled with one bounded error and SHALL NOT publish incomplete or nonexistent Local paths

### Requirement: Results, refresh, and stale-state handling are deterministic
The system SHALL produce item-level Succeeded, Skipped, Partial, Failed, or Cancelled outcomes, one terminal event per request, and refresh only affected current locations while rejecting stale generations. Any mixed request containing a Skipped, Partial, Failed, or Cancelled item SHALL retain aggregate Partial or failure information rather than presenting complete success.

#### Scenario: Mixed item outcomes
- **WHEN** a multi-item request contains both successful and failed items
- **THEN** every input item SHALL have one outcome and the aggregate operation SHALL preserve Partial or failure information

#### Scenario: Navigation during transfer
- **WHEN** a tab navigates or advances generation before a transfer result arrives
- **THEN** the stale result SHALL NOT mutate the new directory snapshot, clipboard intent, or current selection

#### Scenario: Current affected locations refresh
- **WHEN** a remote mutation or transfer reaches a successful or Partial terminal state
- **THEN** each still-current affected source or destination directory SHALL refresh once without generating an operation retry loop

#### Scenario: Deadline before or during transfer
- **WHEN** the request deadline expires before dispatch, enumeration, download, upload, or source-delete commit
- **THEN** no later step SHALL start, already committed item outcomes SHALL remain truthful, unstarted items SHALL terminate without deletion, and exactly one request terminal SHALL be emitted

### Requirement: Destructive and native integration evidence is contained
The system SHALL run destructive integration only inside marker-verified owned fixtures and SHALL require real Windows Explorer evidence for native OLE interoperability.

#### Scenario: Owned remote destructive fixture
- **WHEN** an integration test creates or deletes real ADB or SFTP data
- **THEN** the target SHALL be a unique pre-created subtree with a verified marker and cleanup SHALL fail closed if canonical containment or the marker does not match

#### Scenario: Real Explorer drag evidence
- **WHEN** native drag-in or drag-out is accepted as passing release evidence
- **THEN** a real Windows Explorer process and disk/content oracle SHALL verify the transfer; synthetic input SHALL NOT substitute, and an incapable environment SHALL report the gate Blocked
