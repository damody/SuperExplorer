## ADDED Requirements

### Requirement: Open native popup is replaced by a second genuine right-click
When a native Shell context menu is visible, the application SHALL treat a complete untagged right-button gesture on a different visible SuperExplorer item outside that popup as a request to close the old popup and open one replacement native popup for the new item target.

#### Scenario: User right-clicks a different visible item
- **WHEN** an item native popup is open and the user physically right-clicks an unobscured part of a different visible file or folder
- **THEN** the old popup SHALL close, the second item SHALL become the exact selected target, and one complete replacement native popup SHALL open for that item

#### Scenario: User interacts with the existing popup
- **WHEN** a right-button gesture lands on the current popup or one of its submenus
- **THEN** the gesture SHALL remain a native-menu interaction and SHALL NOT be replayed against content behind the popup

### Requirement: Replacement input has a clean bounded lifecycle
The application MUST capture a matched physical right-button down/up pair for the validated app owner, suppress that pair while the old popup owns the modal loop, release the old popup session, and only then replay one tagged ordered right-button pair through the normal Win32 input path.

#### Scenario: Complete owner gesture is captured
- **WHEN** an untagged right-button down and matching up both resolve to the originating SuperExplorer top-level window
- **THEN** the application SHALL end the old native menu, restore the live owner, wait within a bound for physical release, and replay exactly one tagged down/up pair at the captured physical point

#### Scenario: Gesture is incomplete or leaves the owner
- **WHEN** the captured gesture lacks a matching release, resolves to another top-level window, or the originating owner is no longer valid
- **THEN** the session SHALL cancel without replay and SHALL retain no gesture state for the next context-menu session

#### Scenario: Hook observes tagged replay input
- **WHEN** a replacement hook observes input tagged by the application replay path
- **THEN** it SHALL ignore that input for replacement capture so the replay cannot recursively create additional sessions

#### Scenario: Input injection fails
- **WHEN** the operating system does not accept the complete tagged replay batch within the bounded attempt
- **THEN** the correlated replacement SHALL terminate as cancellation or structured failure and the next genuine pointer gesture SHALL remain usable

### Requirement: Replacement preserves exact Explorer selection semantics
The replayed gesture SHALL use existing UI hit-testing and stable item identity so selection, multi-selection, sorting, virtualization, and command targeting remain consistent with File Explorer.

#### Scenario: Second item was not selected
- **WHEN** the replacement gesture targets a visible item outside the current selection
- **THEN** that item alone SHALL become the focused context-menu target before the replacement popup opens

#### Scenario: Second item belongs to a compatible multi-selection
- **WHEN** the replacement gesture targets an item already in the active compatible multi-selection
- **THEN** the existing multi-selection SHALL remain intact and the popup SHALL target that selection

#### Scenario: Replacement command is invoked
- **WHEN** the user physically invokes Copy from the replacement popup for the second item
- **THEN** the clipboard file-drop payload SHALL identify the second item and SHALL NOT fall back to the first row, old target, or background

### Requirement: Replacement remains isolated and resource bounded
Native popup replacement SHALL retain the existing broker/worker isolation and MUST NOT create another broker, reuse the old target's `IContextMenu`, or accumulate hooks, popup windows, menu handles, threads, or workers.

#### Scenario: Ten alternating replacement cycles
- **WHEN** the focused headful test alternates first and second item replacement ten times and closes or invokes every replacement popup
- **THEN** every cycle SHALL target the requested item while broker count remains one and worker, hook, popup, thread, menu, and handle counts remain within declared bounds

#### Scenario: Escape or outside left-click dismisses the popup
- **WHEN** the user presses Escape or physically left-clicks outside a native popup without a second right-button gesture
- **THEN** the popup SHALL close without replacement and subsequent genuine right-clicks SHALL remain functional
