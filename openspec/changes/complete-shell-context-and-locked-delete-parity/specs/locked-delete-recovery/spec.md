## ADDED Requirements

### Requirement: Structured locked-delete detection
Recycle and permanent delete failures SHALL distinguish Windows sharing and lock violations from unrelated file-operation failures without parsing localized user-facing text.

#### Scenario: Sharing violation
- **WHEN** deletion of an owned fixture fails with a Windows sharing or lock violation
- **THEN** the terminal SHALL retain the original operation identity and SHALL start one correlated lock-owner discovery request

#### Scenario: Unrelated delete failure
- **WHEN** deletion fails for access denied, invalid target, offline provider, cancellation, or another non-lock reason
- **THEN** the existing typed failure SHALL be shown and lock-owner discovery SHALL not run

### Requirement: Bounded Restart Manager owner discovery
The Windows adapter SHALL use Restart Manager to discover owners of bounded delete resources and SHALL return only owned, privacy-safe, generation-bound process records.

#### Scenario: One or more owners
- **WHEN** Restart Manager reports applications or services that currently use a delete target
- **THEN** the result SHALL include bounded application name, PID, process creation identity, application type, restart capability, and shutdown eligibility without exporting handles, paths, command lines, or credentials

#### Scenario: Empty or unavailable result
- **WHEN** owner discovery is empty, denied, cancelled, unavailable, malformed, or over budget
- **THEN** the UI SHALL preserve the original delete failure with Retry and Cancel and SHALL not claim that no process owns the file

#### Scenario: Stale discovery
- **WHEN** the tab navigates, selection/operation generation changes, the window closes, or a later request supersedes discovery
- **THEN** the late result SHALL be ignored and SHALL not open a dialog or close a process

### Requirement: Accessible lock-owner recovery dialog
The application SHALL present a modal, keyboard-accessible and UIA-exposed dialog that lists the locking applications and offers Close programs and retry, Retry, and Cancel.

#### Scenario: Keyboard and focus lifecycle
- **WHEN** the dialog opens
- **THEN** Tab and Shift+Tab SHALL remain within its process list and actions, Escape SHALL cancel, status changes SHALL be announced, and dismissal SHALL restore focus to the originating item

#### Scenario: Plain retry
- **WHEN** the user selects Retry
- **THEN** the original delete SHALL be resubmitted exactly once without requesting any process shutdown

#### Scenario: Cancel
- **WHEN** the user selects Cancel or dismisses the dialog
- **THEN** no process SHALL be closed, no delete SHALL be retried, and the selected file SHALL remain unchanged

### Requirement: Safe graceful close and retry
The application SHALL close external owners only after an explicit user action, SHALL revalidate each process identity and eligibility, and SHALL use graceful Restart Manager shutdown without force termination or elevation.

#### Scenario: Eligible owner closes
- **WHEN** the user selects Close programs and retry and every selected owner remains the same eligible process and closes within the deadline
- **THEN** the original recycle or permanent delete SHALL be retried exactly once with its original destructive semantics

#### Scenario: Protected or ineligible owner
- **WHEN** an owner is SuperExplorer or its helper, PID 0 or 4, system/critical/protected, elevated-inaccessible, identity-reused, or otherwise ineligible
- **THEN** the application SHALL not close it and SHALL present a truthful per-process result with Retry and Cancel

#### Scenario: Partial or refused shutdown
- **WHEN** one or more eligible owners refuse, time out, exit ambiguously, or Restart Manager returns a partial failure
- **THEN** the application SHALL not force terminate any process and SHALL keep the recovery state visible without reporting delete success

### Requirement: Locked-delete destructive and resource evidence
Tests SHALL exercise recycle and permanent deletion against contained owned fixtures with controlled lock holders and SHALL prove bounded resources, exactly-one terminals, and no unrelated process or file effects.

#### Scenario: Controlled graceful owner
- **WHEN** an owned helper holds the fixture open and accepts graceful shutdown
- **THEN** headful evidence SHALL show the owner, close it, retry deletion, verify the expected recycle/permanent outcome, and leave no helper process

#### Scenario: Adversarial recovery matrix
- **WHEN** tests inject multiple owners, stale PID identity, denied shutdown, unresponsive owner, cancellation, navigation, duplicate terminals, or window shutdown
- **THEN** process, thread, Restart Manager session, handle, request, and modal state SHALL remain bounded and no unsafe close or duplicate delete SHALL occur

