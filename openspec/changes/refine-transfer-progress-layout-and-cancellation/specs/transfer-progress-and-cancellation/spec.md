## ADDED Requirements

### Requirement: Active operation layout separates cancellation and progress
The operation center SHALL render an active cancellable transfer with a fixed 250px left cancellation region and a right progress region that fills all remaining width. The progress text and progress bar MUST derive from the same operation state, and terminal operations SHALL omit the cancellation region.

#### Scenario: Active transfer uses the two-region layout
- **WHEN** a cancellable transfer is active
- **THEN** a compact accessible Cancel control is contained within the 250px left region
- **AND** the summary and progress bar fill only the remaining right region

#### Scenario: Terminal transfer uses the full surface
- **WHEN** the transfer reaches Finished, Partial, Failed, or Cancelled
- **THEN** the cancellation region is absent
- **AND** terminal details use the available surface width and retain the existing fade behavior

### Requirement: Ordinary progress publication is bounded to 200 milliseconds
Transfer producers SHALL publish ordinary delivered-byte progress no more frequently than once per 200ms per request, while lifecycle boundaries and terminal events MUST publish immediately. Published bytes and items MUST remain monotonic and MUST NOT be synthesized to 100% on cancellation or failure.

#### Scenario: Rapid byte callbacks are coalesced
- **WHEN** multiple ordinary byte callbacks occur within one 200ms interval
- **THEN** the operation center receives at most one ordinary visible update for that interval
- **AND** a later eligible update contains the latest monotonic byte count

#### Scenario: Lifecycle event bypasses throttling
- **WHEN** Preparing, total discovery, item transition, Finalizing, or a terminal event occurs within a throttle interval
- **THEN** that event is published immediately without waiting for the interval

#### Scenario: Cancelled transfer preserves real progress
- **WHEN** a transfer is cancelled before all bytes arrive
- **THEN** its terminal state is Cancelled
- **AND** the last real bytes/items remain visible instead of changing to 100%

### Requirement: Cancellation is acknowledged immediately
The UI SHALL enter a request-correlated `正在取消` state immediately after a Cancel action is accepted, prevent duplicate cancellation actions, and clear that state on the first correlated terminal result or command-submission failure.

#### Scenario: User cancels an active transfer
- **WHEN** the user activates Cancel on an active transfer
- **THEN** the same operation immediately displays `正在取消`
- **AND** another cancellation cannot be dispatched for it

#### Scenario: Cancel submission fails
- **WHEN** cancellation command submission fails
- **THEN** the cancelling marker is cleared
- **AND** the operation surface exposes the concrete failure reason

### Requirement: Providers stop at the earliest safe interrupt boundary
Local, ADB, and SFTP providers MUST observe the request cancellation token at their streaming, process, recursive-item, and stage boundaries. ADB MUST terminate its owned transfer subprocess when cancellation wins; SFTP MUST stop scheduling chunks or entries; staged cross-provider transfer MUST NOT begin the next stage after cancellation.

#### Scenario: ADB transfer is cancelled
- **WHEN** cancellation occurs during ADB push or pull
- **THEN** the owned ADB process is terminated promptly
- **AND** no late byte progress is published after the terminal cancellation

#### Scenario: SFTP transfer is cancelled
- **WHEN** cancellation occurs during an SFTP upload or download
- **THEN** no subsequent chunk or recursive item starts after cancellation is observed
- **AND** the request terminates as Cancelled

#### Scenario: Staged cross-provider transfer is cancelled
- **WHEN** cancellation occurs during or between ADB/SFTP staging phases
- **THEN** no later provider stage starts
- **AND** temporary staging cleanup occurs without deleting the source

### Requirement: Cancelled moves preserve source data
Move source cleanup MUST occur only after the complete destination transfer succeeds. Cancellation, failure, or partial completion SHALL leave every unconfirmed source item intact.

#### Scenario: Move is cancelled before destination completion
- **WHEN** a Local, ADB, SFTP, or staged cross-provider move is cancelled before the destination is complete
- **THEN** source deletion is not invoked for the incomplete item
- **AND** the operation reports Cancelled with its last real progress
