## ADDED Requirements

### Requirement: Complete target-appropriate Shell menu
The application SHALL display the complete classic Windows Shell context menu available through public APIs for a background, single file, single folder, or compatible multi-selection target, including native separators, enabled/default state, icons, owner-drawn entries, and nested third-party submenus.

#### Scenario: Ordinary item right-click
- **WHEN** the user right-clicks one or more compatible selected items
- **THEN** the disposable worker SHALL query the target-appropriate Explorer item-menu profile and show every command returned within the bounded command range

#### Scenario: Background right-click
- **WHEN** the user right-clicks an unoccupied file-view background
- **THEN** the disposable worker SHALL query the background profile without applying item-only flags and SHALL preserve provider-owned submenu behavior

#### Scenario: Installed third-party extension
- **WHEN** a compatible x64 Shell extension such as 7-Zip, WinRAR, or TortoiseGit is installed and returns commands for the target
- **THEN** its entries and submenus SHALL remain visible and invokable through their native Shell command identity

#### Scenario: User-invoked external application
- **WHEN** a Shell verb launches an external application such as 7-Zip, Code, or Git from the isolated worker
- **THEN** the external application SHALL escape the worker-only process-count limit while the worker remains subject to its memory, CPU, lifetime, and crash-containment limits

### Requirement: Explorer modifier and invocation parity
Ordinary pointer and keyboard context invocation SHALL use the normal complete profile, while Shift-modified invocation SHALL additionally request extended-only verbs without permanently changing later menus.

#### Scenario: Shift right-click
- **WHEN** the user invokes the item context menu while Shift is active
- **THEN** the correlated request SHALL enable extended verbs for that menu session only

#### Scenario: Later ordinary invocation
- **WHEN** a Shift-extended menu has closed and the next context menu is invoked without Shift
- **THEN** extended-only verbs SHALL not be requested for the new session

### Requirement: Native menu isolation and lifecycle
Shell resolution, menu query, modal presentation, `IContextMenu2/3` message forwarding, submenu population, and third-party command invocation SHALL remain inside a disposable broker worker and SHALL emit exactly one bounded terminal outcome. Built-in verbs whose semantics depend on long-lived application selection, clipboard, refresh, or editor ownership SHALL instead return a typed delegated terminal and execute through the application-owned operation pipeline.

#### Scenario: Escape or outside dismissal
- **WHEN** the native popup is visible and the user presses Escape or clicks outside it
- **THEN** the popup SHALL close, emit one cancelled terminal, and restore focus to the originating app surface

#### Scenario: Selection remains visible under native popup
- **WHEN** an item context menu owns foreground focus
- **THEN** the originating item SHALL retain an obvious active selection visual until the menu closes or its target is replaced

#### Scenario: Right-click another item while a menu is open
- **WHEN** the user right-clicks another item while the current native popup is visible
- **THEN** the current popup SHALL cancel and the same gesture SHALL select the new item and open its target-appropriate menu without requiring a second click

#### Scenario: Slow or crashing handler
- **WHEN** a handler hangs, crashes, returns malformed command data, or exceeds its deadline
- **THEN** the worker SHALL be cancelled or terminated, the handler SHALL be quarantinable, and navigation SHALL remain usable without a console window or orphan process

### Requirement: Host-owned built-in verbs
The built-in Cut, Copy, Create shortcut, Delete, Rename, and Properties commands returned by the native Shell menu SHALL produce the same observable result as the corresponding File Explorer operations while preserving broker isolation for third-party extensions.

#### Scenario: Built-in command selected from native popup
- **WHEN** the user selects Cut, Copy, Create shortcut, Delete, Rename, or Properties from an item context menu
- **THEN** the worker SHALL return its canonical non-localized verb and command offset without invoking it in the disposable process
- **AND** the application SHALL execute the typed command against the exact selection that opened the popup

#### Scenario: Selection-dependent command lifetime
- **WHEN** a built-in command owns clipboard data, an inline rename editor, a property sheet, or a file-operation refresh
- **THEN** that ownership SHALL remain valid after the disposable context-menu worker exits

#### Scenario: Create shortcut
- **WHEN** Create shortcut is selected for a writable filesystem item
- **THEN** Windows SHALL create one collision-safe `.lnk` beside the selected item and the file view SHALL refresh after completion

### Requirement: Shell menu parity evidence
The test system SHALL compare direct and brokered native Shell queries on the same Windows host for ordinary and Shift invocation across background, file, folder, and multi-selection targets.

#### Scenario: Provider unavailable
- **WHEN** a named third-party extension is not installed or does not support the fixture target
- **THEN** the provider-specific test SHALL report a prerequisite skip rather than a pass

#### Scenario: Provider available
- **WHEN** a compatible provider is available
- **THEN** the evidence SHALL record command counts and labels, validate submenu discovery and safe owned-fixture invocation, and show no broker-induced command loss

### Requirement: Exact pointer target and actionable command audit
The item identity hit by the secondary-button gesture SHALL remain the authoritative selection and command target, and every actionable native menu entry SHALL be classified as either an application-owned command or a provider-owned command before invocation.

#### Scenario: Right-click a non-first row
- **WHEN** the first row is selected and the user right-clicks a different visible row
- **THEN** the hit row SHALL become the focused selection before the popup appears
- **AND** any selected command SHALL operate on that hit identity rather than the first presentation row

#### Scenario: Right-click a different visible row while a native popup is open
- **WHEN** a native item popup is open and the user right-clicks an unobscured part of another row
- **THEN** the first popup SHALL end and the complete secondary-button gesture SHALL be replayed through the application input path
- **AND** the other row SHALL become the exact focused target of one replacement native popup

#### Scenario: Application-owned Shell command
- **WHEN** Open, Copy path, Share, or Quick access pin is selected from the native popup
- **THEN** the worker SHALL return a typed delegated terminal carrying the exact popup target
- **AND** the long-lived application SHALL execute the matching Explorer action after restoring that target

#### Scenario: Provider-owned Shell command
- **WHEN** an actionable command is not owned by the application
- **THEN** the same live `IContextMenu` instance that produced the command SHALL invoke its command offset in the isolated worker
- **AND** external-UI or credential-dependent commands SHALL be covered by invocation/lifecycle contracts without unsafe unattended side effects

### Requirement: Native Properties, inline rename, and Start pin ownership
Properties and Pin to Start SHALL execute through the long-lived Shell STA against the exact popup selection, and pointer editing inside the inline rename text box SHALL remain inside the editor.

#### Scenario: Real item property sheet
- **WHEN** the user selects Properties from an item context menu or presses Alt+Enter
- **THEN** the application SHALL invoke the selected item’s native `properties` command on its live `IContextMenu`
- **AND** Windows SHALL show the actual property sheet rather than a “properties unavailable” error dialog

#### Scenario: Pointer positions the inline rename caret
- **WHEN** F2 rename is active and the user clicks or drags inside the edit text box
- **THEN** the text control SHALL update its caret or selection without committing, cancelling, or restarting rename
- **AND** clicking outside the editor SHALL retain the existing blur-commit behavior

#### Scenario: Pin to Start
- **WHEN** the native popup returns and the user selects canonical verb `PinToStartScreen`
- **THEN** the worker SHALL delegate the immutable popup target to the application
- **AND** the long-lived Shell STA SHALL invoke the native command without worker quota or lifetime interference

### Requirement: Explorer navigation history menus and inherited new tabs
Back and Forward SHALL expose their available committed destinations from a secondary-button menu, and every new-tab entry point SHALL create an independent tab initialized with the active tab's complete committed navigation history.

#### Scenario: Secondary-click Back or Forward
- **WHEN** the user right-clicks an enabled Back or Forward button
- **THEN** a focused menu SHALL list that direction's destinations from nearest to farthest with folder icons and identifiable labels
- **AND** Escape, outside click, or activation SHALL dismiss the menu using the shared popup lifecycle

#### Scenario: Jump to an older history destination
- **WHEN** the user selects a destination more than one step away
- **THEN** one correlated navigation SHALL resolve that destination and atomically move all crossed entries to the opposite stack
- **AND** a failed or stale resolution SHALL leave the committed current, Back, and Forward history unchanged

#### Scenario: New tab from pointer or Ctrl+T
- **WHEN** the user clicks `+` or presses Ctrl+T
- **THEN** exactly one active tab with a new identity SHALL open at the current location
- **AND** its Back and Forward stacks SHALL initially equal the source tab while later navigation remains independent

### Requirement: Target-complete Properties and Explorer-like editable text
Properties SHALL open the real Windows property sheet for every supported immutable popup target, and focused address/search inputs SHALL support DPI-correct pointer caret placement, drag selection, and strong Explorer-like selection contrast.

#### Scenario: File, folder, and compatible multi-selection Properties
- **WHEN** the user selects Properties from a genuine-pointer context menu for a file, filesystem folder, or compatible multi-selection
- **THEN** the long-lived Shell STA SHALL open the actual property sheet for that exact popup target
- **AND** a generic unavailable dialog, wrong target, or missing property-page controls SHALL be treated as failure

#### Scenario: Pointer edits an existing address input
- **WHEN** address edit mode is already active and the user clicks or drags over its text
- **THEN** the existing input entity SHALL place the caret or extend the selection without being reset or reselecting the entire document

#### Scenario: Search pointer coordinates include inner padding and scroll
- **WHEN** the user clicks or drags within a padded or horizontally scrolled search field
- **THEN** glyph hit-testing SHALL subtract the inner text origin and scroll offset exactly once and select the characters under the pointer

#### Scenario: Focused text selection contrast
- **WHEN** address or search text is selected while focused
- **THEN** it SHALL use an opaque semantic Highlight background and legible HighlightText foreground comparable to File Explorer
- **AND** Windows high-contrast Highlight and HighlightText roles SHALL be preserved without alpha dilution
