## ADDED Requirements

### Requirement: Host-owned Shell commands use a persistent STA
The application SHALL execute host-owned Shell commands through one bounded application-owned STA executor whose OLE apartment, message pump, and owner resources remain valid for the application lifetime.

#### Scenario: Properties invocation outlives InvokeCommand return
- **WHEN** a Properties handler continues asynchronous work after native invocation returns
- **THEN** the host STA and valid owner HWND SHALL remain available and the UI thread and visible-popup broker SHALL remain responsive

#### Scenario: Repeated host-owned commands
- **WHEN** Properties is invoked and dismissed ten times
- **THEN** the application SHALL retain one broker and one host executor without accumulating workers, host threads, owner windows, menus, or handles

#### Scenario: Application shutdown
- **WHEN** the application closes with no active property sheet
- **THEN** the host command queue SHALL close and its STA, owner window, OLE apartment, and thread SHALL terminate deterministically

### Requirement: Properties uses the immutable popup target and a real native menu
The application SHALL invoke Properties against the exact immutable selection that opened the popup by resolving and querying one host-side `IContextMenu` and invoking the Properties command from that same interface instance.

#### Scenario: Genuine-pointer Properties for supported targets
- **WHEN** the user physically right-clicks a file, filesystem folder, executable, script, or compatible multi-selection and physically activates Properties
- **THEN** Windows SHALL display the actual target-correct property sheet and SHALL NOT display a generic unavailable dialog

#### Scenario: Selection changes after popup delegation
- **WHEN** visible selection changes after the worker captured the popup target
- **THEN** Properties SHALL still apply to the immutable captured target rather than the later UI selection or first presentation row

#### Scenario: Valid Properties owner
- **WHEN** the host invokes the native Properties command
- **THEN** invocation metadata SHALL use the validated SuperExplorer window as UI owner and a Unicode-capable extended command structure

### Requirement: Context menu remains usable after Properties
Closing a Properties sheet SHALL leave subsequent genuine mouse context-menu gestures and commands fully functional.

#### Scenario: Right-click another item after Properties closes
- **WHEN** the user dismisses Properties and physically right-clicks a different non-first item
- **THEN** one complete target-appropriate native popup SHALL open for the new item without another click

#### Scenario: Invoke a command from the next popup
- **WHEN** the user physically selects a safe built-in command from that next popup
- **THEN** the command SHALL operate on the new item and produce its observable result

#### Scenario: Properties invocation fails
- **WHEN** native Properties resolution or invocation fails
- **THEN** only the correlated request SHALL fail and the next physical right-click SHALL still open a usable native menu
