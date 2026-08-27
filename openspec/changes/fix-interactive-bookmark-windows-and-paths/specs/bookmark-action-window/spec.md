## ADDED Requirements

### Requirement: Confirmed bookmark right-click action window
Right-clicking a bookmark item SHALL open or update a dedicated focusable native action window. The window SHALL show only commands applicable to the target type and MUST NOT execute a command until the user selects it and activates Confirm.

#### Scenario: Open a folder bookmark action window
- **WHEN** the user right-clicks a folder bookmark
- **THEN** the singleton action window SHALL show Open, Open in New Tab, Edit Name and Path, and Delete with Open initially selected

#### Scenario: Open a file bookmark action window
- **WHEN** the user right-clicks a file bookmark
- **THEN** the singleton action window SHALL omit Open in New Tab and show the remaining applicable commands

#### Scenario: Confirm an edit command
- **WHEN** the user selects Edit Name and Path and activates Confirm
- **THEN** the action window SHALL close and the dedicated bookmark editor window SHALL open for the same stable bookmark ID

### Requirement: Non-mutating cancellation and confirmed deletion
Cancel, Escape, and closing the action window MUST close it without changing bookmark state. Selecting Delete and activating Confirm MUST enter a distinct delete-confirmation stage, and deletion MUST occur only after the user activates Confirm Delete.

#### Scenario: Cancel a selected destructive command
- **WHEN** the user selects Delete and then cancels or closes the action window before Confirm Delete
- **THEN** the bookmark collection MUST remain unchanged

#### Scenario: Confirm deletion
- **WHEN** the user selects Delete, activates Confirm, and then activates Confirm Delete
- **THEN** only the logical bookmark SHALL be durably removed and its filesystem target MUST remain untouched

### Requirement: Singleton and stale-target lifecycle
At most one bookmark action window SHALL exist per Explorer owner. A new right-click while it is open SHALL replace its target snapshot, reset selection to Open, and activate it. Confirmation of a target that no longer exists MUST execute no bookmark command.

#### Scenario: Retarget an open action window
- **WHEN** the action window is open for bookmark A and the user right-clicks bookmark B
- **THEN** the existing window SHALL activate, display bookmark B, and reset its selected command to Open

#### Scenario: Confirm a stale target
- **WHEN** the target bookmark is removed through another projection before action confirmation
- **THEN** the action window MUST close without executing the selected command or mutating another bookmark
