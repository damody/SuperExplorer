## ADDED Requirements

### Requirement: Second right-click replacement is non-blocking
When a native Shell popup is visible, the application SHALL cancel that popup without synchronously tearing down its modal loop from a low-level input callback, and SHALL remain responsive while opening one replacement popup for a complete second right-click.

#### Scenario: User right-clicks a second visible item
- **WHEN** a native item popup is open and the user completes an untagged right-click on an unobscured different item in the same SuperExplorer window
- **THEN** the application SHALL asynchronously close the old popup, remain responsive, select the second item, and open exactly one native popup for that item

#### Scenario: Cancellation is requested from the input hook
- **WHEN** the low-level hook captures the matching right-button release over the originating application owner
- **THEN** it SHALL post one cancellation message to the popup owner and return without synchronously calling native modal-menu termination

### Requirement: Replacement request ordering is deterministic
The application MUST make the latest complete mouse replacement authoritative after the old native popup has terminated, MUST serialize non-mouse replacement requests, and MUST reject stale completion events by correlation.

#### Scenario: User rapidly right-clicks multiple targets
- **WHEN** more than one complete replacement gesture is received before the active popup finishes
- **THEN** only the latest valid mouse target SHALL remain pending and older targets SHALL NOT be reopened

#### Scenario: Mouse replay arrives after native teardown
- **WHEN** the captured mouse gesture is replayed after the worker has released the old popup and published its terminal
- **THEN** the replay request SHALL immediately supersede stale pending UI state without waiting for terminal-lane delivery

#### Scenario: Stale popup terminal arrives
- **WHEN** a completion event does not match the currently active context-menu request
- **THEN** it SHALL NOT clear, promote, or open any replacement request

### Requirement: Replacement preserves native menu behavior and resource bounds
The application SHALL preserve normal popup interaction, Shell extension behavior, and exact GPUI hit testing, and MUST release the old hook, popup, menu, and worker resources before replaying replacement input.

#### Scenario: User interacts with the existing popup
- **WHEN** the right-click lands on the popup or a submenu instead of unobscured SuperExplorer content
- **THEN** the gesture SHALL remain a native menu interaction and SHALL NOT trigger replacement replay

#### Scenario: Replacement command targets the second item
- **WHEN** the user invokes Copy from the replacement popup
- **THEN** the clipboard file-drop payload SHALL identify the second item rather than the old item, first row, or background

#### Scenario: Repeated alternating replacements
- **WHEN** the headful regression repeatedly alternates replacement between fixture items
- **THEN** every iteration SHALL remain responsive and hook, popup, menu, worker, thread, and handle counts SHALL stay within declared bounds
