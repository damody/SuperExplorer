## ADDED Requirements

### Requirement: Transient owned transfer window
SuperExplorer SHALL render the transfer center in a reusable native tool window owned by the invoking explorer window. The transfer window SHALL appear above ordinary owner content and menus, SHALL remain below owner modal dialogs, and SHALL NOT appear in the Windows taskbar or Alt+Tab list.

#### Scenario: User opens transfer center above file content
- **WHEN** the user activates the transfer button while file rows and details headers are visible
- **THEN** the complete transfer window is visible above those owner surfaces without clipping or occlusion

#### Scenario: Modal opens while transfer window exists
- **WHEN** an owner login or delete-confirmation modal is shown
- **THEN** the modal remains above the transfer window and receives modal interaction

#### Scenario: Windows enumerates application surfaces
- **WHEN** the transfer window is visible
- **THEN** it has an owner/tool-window policy and is absent from the taskbar and Alt+Tab application list

### Requirement: Transfer window focus and lifecycle
The transfer-window coordinator SHALL implement idempotent show, hide, reposition and close operations. It SHALL hide the window on Escape, repeated transfer-button activation, owner minimization, or loss of focus by the complete owner/tool-window group, and SHALL close it when its owner closes.

#### Scenario: Focus moves between owner and transfer window
- **WHEN** focus moves from the owner into its transfer tool window or back
- **THEN** the transfer window remains visible and the main UI remains responsive

#### Scenario: Application group loses focus
- **WHEN** neither the owner nor its transfer tool window remains the foreground/focused window after the focus transition settles
- **THEN** the transfer tool window hides without blocking subsequent owner input

#### Scenario: Owner terminates
- **WHEN** the owner explorer window closes
- **THEN** the coordinator closes the owned transfer window and discards its handle

### Requirement: Screen-safe anchored placement
The transfer window SHALL anchor to the transfer button in screen coordinates and SHALL remain within the current monitor work area across DPI and monitor changes.

#### Scenario: Space is available below the button
- **WHEN** the requested transfer-window bounds fit below the anchor
- **THEN** its right edge aligns with the anchor right edge subject to work-area clamping

#### Scenario: Bottom space is insufficient
- **WHEN** the window would extend below the monitor work area and more usable space exists above
- **THEN** the transfer window opens above the anchor and remains wholly inside the work area

### Requirement: Session operation source remains singular
The transfer tool window SHALL render newest-first records from the existing session `OperationCenterState` and SHALL dispatch typed per-record actions without maintaining a second operation-history model.

#### Scenario: Active and terminal records update
- **WHEN** progress or terminal events update session records while the tool window is visible
- **THEN** the existing window refreshes its rows without native-window recreation and preserves newest-first ordering

#### Scenario: Application restarts
- **WHEN** a new SuperExplorer process starts
- **THEN** the transfer window contains no operation records from the previous process

### Requirement: Cancelled terminal is explicit and stable
User-requested cancellation SHALL resolve to an explicit `Cancelled` terminal after provider cancellation completes. Progress and terminal events received after cancelling or terminal completion SHALL NOT replace the cancelled state.

#### Scenario: Cancellation completes before any item succeeds
- **WHEN** the user cancels and the provider terminates with zero completed items
- **THEN** the row displays `已取消` and does not display a partial-completion summary

#### Scenario: Cancellation follows completed items
- **WHEN** cancellation completes after X of Y items have succeeded
- **THEN** the row displays `已取消（已完成 X/Y）` and remains terminal

#### Scenario: Late callback arrives after cancellation
- **WHEN** a progress, finished, failed or partial callback arrives after the cancelled terminal
- **THEN** the operation remains cancelled and its terminal display is unchanged

### Requirement: ADB native progress parsing
ADB push and pull SHALL drain pseudo-terminal output continuously and SHALL parse carriage-return, newline, ANSI-controlled, fragmented percent and byte-pair progress frames into monotonic latest snapshots. Byte-pair observations SHALL take precedence over percent-derived bytes when both are available.

#### Scenario: CLI emits carriage-return percent frames
- **WHEN** adb emits multiple percent updates separated by carriage returns
- **THEN** each newer observation updates the latest snapshot without waiting for process exit

#### Scenario: CLI frame is fragmented and contains ANSI controls
- **WHEN** a progress frame spans reader chunks or includes ANSI cursor/status sequences
- **THEN** the parser reconstructs the progress observation and ignores control traffic without producing regressions

#### Scenario: Output repeats or regresses
- **WHEN** adb repeats an observation or emits a lower byte/percent value
- **THEN** the adapter retains the highest valid monotonic snapshot and does not invent transfer speed

### Requirement: ADB progress publication cadence and cancellation
During an active ADB transfer, SuperExplorer SHALL publish the latest available native progress snapshot to the operation UI at intervals no greater than 200 ms under normal scheduling. Phase boundaries, cancellation and terminal results SHALL publish immediately, and cancellation SHALL terminate and reap the adb child rather than merely hiding UI.

#### Scenario: Native output advances rapidly
- **WHEN** adb produces several valid observations within one 200 ms interval
- **THEN** the next scheduled publication contains the newest observation and the PTY reader was not blocked by publication

#### Scenario: Native output is temporarily unchanged
- **WHEN** no newer frame arrives during an active 200 ms interval
- **THEN** the publisher may repeat the latest snapshot while preserving monotonic bytes and stable speed semantics

#### Scenario: User cancels ADB transfer
- **WHEN** the user activates cancel during push or pull
- **THEN** SuperExplorer kills and reaps the adb child, publishes cancellation immediately, ignores later output and reaches the explicit cancelled terminal

#### Scenario: PTY creation falls back
- **WHEN** a pseudo-terminal cannot be created
- **THEN** SuperExplorer records a specific fallback diagnostic, uses the bounded pipe runner and retains the latest-snapshot publication contract

### Requirement: Blocking validation and packaging
The change SHALL NOT be considered complete until transient-window behavior, cancellation semantics, ADB parser/cadence, real ADB and SFTP transfer behavior, installed UI behavior and release/installed binary identity have passed their required gates.

#### Scenario: Final validation succeeds
- **WHEN** all focused tests, real-device/profile checks, user-perspective checks and `build_test_install.bat` complete successfully
- **THEN** evidence records each task result and the release and installed `SuperExplorer.exe` SHA-256 values are identical

#### Scenario: A blocking check fails
- **WHEN** any layer/focus, cancellation, 200 ms cadence, real transfer, packaging or hash gate fails
- **THEN** affected tasks remain incomplete and implementation continues without weakening the gate
