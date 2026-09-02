## ADDED Requirements

### Requirement: Transfer speed is truthful and readable
The system SHALL derive a smoothed bytes-per-second value from monotonic byte progress and monotonic elapsed time for active Local, ADB, and SFTP Copy or Move operations, and SHALL NOT invent byte growth when no new provider progress exists.

#### Scenario: Second valid byte sample produces speed
- **WHEN** an active transfer receives a later monotonic byte sample with a positive elapsed interval
- **THEN** the bottom summary and expanded transfer row display a formatted B/s, KB/s, MB/s, or GB/s value

#### Scenario: First or unchanged sample has no fabricated speed
- **WHEN** a transfer is preparing, has only one byte sample, or receives an unchanged byte count
- **THEN** the system does not calculate a new non-zero speed from that sample

#### Scenario: Late progress cannot change terminal speed
- **WHEN** a progress event arrives after the operation has reached a terminal state
- **THEN** the event is rejected and neither progress nor speed changes

### Requirement: Active remote progress has a 200 millisecond publication cadence
The system SHALL publish the latest known active ADB and SFTP transfer state on a 200 millisecond cadence while preserving immediate phase, item-boundary, cancellation, and terminal events.

#### Scenario: ADB progress stream remains responsive
- **WHEN** adb push or pull remains active for longer than 200 milliseconds
- **THEN** the UI receives the latest known snapshot at intervals no greater than the scheduler tolerance around 200 milliseconds

#### Scenario: Sparse native output does not fabricate bytes
- **WHEN** adb emits no new byte or percentage information during a 200 millisecond tick
- **THEN** any keepalive snapshot repeats the last known completed byte count

#### Scenario: Cancellation bypasses periodic delay
- **WHEN** the user cancels an active ADB or SFTP transfer
- **THEN** cancellation is routed immediately and the unique terminal result is not delayed until the next periodic tick

### Requirement: Bottom status selects one foreground operation
The system SHALL render no more than one operation in the bottom transfer region and SHALL select the most recently started active Copy or Move operation before any terminal notice.

#### Scenario: Newest active transfer is foreground
- **WHEN** two or more transfers are active
- **THEN** the bottom region displays the most recently started active transfer

#### Scenario: Newer transfer finishes before older transfer
- **WHEN** the foreground transfer terminates while an earlier transfer remains active
- **THEN** the bottom region automatically displays the earlier active transfer

#### Scenario: No active transfer remains
- **WHEN** all transfers are terminal
- **THEN** the bottom region displays the latest eligible terminal notice and fades it out by eight seconds

### Requirement: Toolbar transfer center lists the current session
The system SHALL provide a Fluent-styled toolbar button that opens a scrollable panel containing all operation records from the current application execution, newest first, without persisting them across restart.

#### Scenario: Active count badge
- **WHEN** one or more Copy or Move operations are active
- **THEN** the toolbar button displays their count in an accent badge

#### Scenario: Expanded panel content
- **WHEN** the user activates the toolbar transfer button
- **THEN** the panel shows every current-session operation with state, progress, bytes, speed when known, and a determinate or indeterminate progress track

#### Scenario: Per-operation cancellation
- **WHEN** the user cancels one active row in the panel
- **THEN** only that request ID enters cancelling state and other operations continue

#### Scenario: Terminal destination action
- **WHEN** a terminal operation has a navigable local or remote location
- **THEN** its row exposes an action that navigates to the local parent or corresponding ADB/SFTP location

#### Scenario: Panel dismissal
- **WHEN** the panel is open and the user presses Escape, clicks outside, or activates the toolbar button again
- **THEN** the panel closes without changing operation state

#### Scenario: Application restart
- **WHEN** the application starts a new execution
- **THEN** no operation records from the previous execution appear in the panel

### Requirement: Permanent delete is silent until terminal
The system SHALL exclude Shift+Delete queued and running records from the bottom operation region and SHALL show a detailed bottom result only after the permanent delete becomes terminal.

#### Scenario: Confirmation and execution remain hidden
- **WHEN** a Shift+Delete request is awaiting confirmation or executing
- **THEN** no bottom operation summary, progress track, or Cancel control is rendered for that request

#### Scenario: Successful permanent deletion
- **WHEN** the permanent delete finishes successfully
- **THEN** the bottom region displays a completion message containing the operation and affected path and fades it out by eight seconds

#### Scenario: Partial or failed permanent deletion
- **WHEN** the permanent delete terminates partially or fails
- **THEN** the bottom region displays the detailed affected path, stage, native code when available, and user-readable reason

#### Scenario: Delete history remains inspectable
- **WHEN** a permanent delete reaches any terminal result
- **THEN** its terminal record remains in the toolbar panel until the application exits

### Requirement: Multi-operation state remains isolated
The system SHALL preserve request correlation, monotonic progress, unique terminal state, and independent cancellation for concurrent operations.

#### Scenario: One operation fails while another runs
- **WHEN** one concurrent operation fails or is cancelled
- **THEN** the other operation remains active and eligible for bottom foreground display

#### Scenario: Duplicate and stale events
- **WHEN** a duplicate terminal or regressing progress event is received
- **THEN** it is rejected without changing ordering, speed, badge count, or another operation
