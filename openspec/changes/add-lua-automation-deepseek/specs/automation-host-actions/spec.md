## ADDED Requirements

### Requirement: Task-relative file access and atomic output
The system SHALL provide typed text, JSON, byte, append, and read APIs whose relative paths resolve against the immutable task cwd.

#### Scenario: Atomic text replacement succeeds
- **WHEN** Lua writes UTF-8 text using `atomic_replace`
- **THEN** the destination contains the complete new text and no temporary file remains

#### Scenario: Atomic text replacement is cancelled
- **WHEN** a write is cancelled or fails before replacement
- **THEN** the previous destination remains valid and no partial destination is reported as complete

### Requirement: Confirmation-gated removal
The system SHALL display a non-scriptable confirmation for every built-in remove, recycle, or permanent-delete request.

#### Scenario: User rejects deletion
- **WHEN** the user rejects the confirmation
- **THEN** the task receives `DeletionDenied`, no target is removed, and later queue items remain runnable

### Requirement: Controlled direct executable launch
The system SHALL accept a direct executable and separate argument array, SHALL reject shell/script hosts, and SHALL capture bounded stdout, stderr, exit, timeout, and cancellation results.

#### Scenario: Shell host is requested directly
- **WHEN** Lua passes cmd, PowerShell, pwsh, wscript, or cscript to the direct executable API
- **THEN** the request is rejected before a process starts

### Requirement: Controlled BAT CMD and PowerShell launch
The system SHALL execute BAT, CMD, and PowerShell files only through a dedicated API that chooses a fixed interpreter and scans exact content and statically resolvable nested scripts for deletion behavior.

#### Scenario: Script may delete
- **WHEN** scanning finds definite, possible, or dynamically indeterminate deletion behavior
- **THEN** the system requires deletion confirmation before starting the interpreter

### Requirement: Child process lifetime containment
The system SHALL assign launched processes to a Windows Job Object and terminate the process tree on timeout, cancellation, script disable/reload, or application shutdown.

#### Scenario: Script process times out
- **WHEN** a launched script exceeds its timeout after creating a descendant process
- **THEN** both parent and descendant terminate and the task reports a timeout

### Requirement: Clipboard UI and logging host APIs
The system SHALL provide typed clipboard text/file reads, notifications, summary presentation, and structured logging while excluding sensitive raw values from persistent diagnostics by default.

#### Scenario: Script shows notification
- **WHEN** a Lua handler calls the notification API
- **THEN** the UI receives an owned presentation request without the Lua VM accessing GPUI objects
