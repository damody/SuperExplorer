## ADDED Requirements

### Requirement: Restored active tab loads during startup
The application SHALL submit the restored active tab's current resolved location to the directory service during root construction, without requiring refresh or another user action.

#### Scenario: Active restored filesystem tab
- **WHEN** SuperExplorer starts with a valid restored session whose active tab targets a filesystem directory
- **THEN** the tab SHALL enter the normal loading lifecycle and display its directory contents without showing a persistent disconnected state

#### Scenario: Restored location falls back during resolution
- **WHEN** the saved active location is unavailable and session resolution selects an existing ancestor or configured fallback
- **THEN** the application SHALL automatically load that resolved location through the same startup path

### Requirement: Idle restored background tab loads on first activation
The application SHALL submit exactly one normal navigation request when a restored background tab in `DirectoryState::Idle` first becomes active.

#### Scenario: Pointer activates restored background tab
- **WHEN** the user selects an idle restored background tab with the pointer
- **THEN** the application SHALL load that tab's current location without requiring F5

#### Scenario: Keyboard cycling activates restored background tab
- **WHEN** next-tab or previous-tab keyboard navigation makes an idle restored tab active
- **THEN** the application SHALL load that tab's current location through the same activation policy

#### Scenario: Closing active tab reveals idle restored tab
- **WHEN** closing the active tab makes an idle restored tab active
- **THEN** the application SHALL automatically load the newly active tab

### Requirement: Automatic activation loading is duplicate-safe
The application MUST NOT submit an automatic activation load for an active tab whose directory state is `Loading`, `Ready`, or `Error`.

#### Scenario: Tab is activated again while loading
- **WHEN** a restored tab is activated again before its first directory request completes
- **THEN** no second navigation request SHALL be submitted

#### Scenario: Ready tab is revisited
- **WHEN** the user returns to a restored tab that already has a completed directory snapshot
- **THEN** the existing snapshot SHALL remain visible and no activation reload SHALL occur

#### Scenario: Terminal error tab is revisited
- **WHEN** the user returns to a restored tab whose directory request ended in an error
- **THEN** activation SHALL preserve the error and SHALL NOT retry until the user invokes an explicit recovery action such as F5

### Requirement: Submission failures are truthful and recoverable
An automatic load command that cannot be admitted by the directory service SHALL use the existing correlated failure handling and MUST NOT leave the tab indefinitely represented as an unconnected idle directory.

#### Scenario: Directory service rejects command admission
- **WHEN** the automatic activation command is rejected as overloaded or disconnected
- **THEN** the active tab SHALL expose the existing retryable directory error and F5 SHALL remain available for an explicit retry

### Requirement: Restart behavior has headful UTIT evidence
The repository SHALL include a two-process Windows UTIT that restores multiple filesystem tabs and proves active and background restored tabs load automatically.

#### Scenario: Restart and activate background tab
- **WHEN** the UTIT persists two distinct filesystem tabs, closes SuperExplorer, restarts it, and activates the restored background tab through UI Automation
- **THEN** both restored locations SHALL display their expected fixture contents without manual refresh and the report SHALL record that `Directory service is not connected` did not remain visible

#### Scenario: Required restart artifacts
- **WHEN** the restart UTIT completes
- **THEN** it SHALL emit a machine-readable report, logs, and screenshots of the loaded restored active tab and loaded restored background tab
