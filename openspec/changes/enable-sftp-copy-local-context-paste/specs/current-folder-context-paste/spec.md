## ADDED Requirements

### Requirement: Paste appears for every context hit in a writable current folder
When clipboard state is usable and the current presentation is writable, the application SHALL expose Paste in context menus opened on the folder background, a file, or a child folder.

#### Scenario: Background context hit
- **WHEN** an SFTP copy is active and the user opens the context menu on a writable local folder background
- **THEN** the menu exposes an enabled host-owned Paste command

#### Scenario: File or child-folder context hit
- **WHEN** an SFTP copy is active and the user opens the context menu on a file or child folder in a writable local folder
- **THEN** the menu exposes the same enabled host-owned Paste command

#### Scenario: Paste is unavailable
- **WHEN** clipboard state is empty, unsupported, or stale, or the current presentation is not writable
- **THEN** the application does not expose an enabled host-owned Paste command

### Requirement: Context Paste always targets the current folder
Context-menu Paste MUST derive its destination from the active tab's current history location and MUST NOT use the context-menu hit item as the destination.

#### Scenario: Child folder was clicked
- **WHEN** the user right-clicks a child folder and invokes Paste
- **THEN** the transfer destination is the folder currently displayed by the active tab, not the clicked child folder

#### Scenario: File was clicked
- **WHEN** the user right-clicks a file and invokes Paste
- **THEN** the transfer destination is the folder currently displayed by the active tab

### Requirement: SFTP copy pastes directly into a local folder
An in-application Paste with SFTP clipboard items and a filesystem destination SHALL use the internal remote clipboard and remote transfer engine without waiting for external Windows clipboard staging.

#### Scenario: Immediate copy then paste
- **WHEN** a user copies one or more SFTP items and immediately invokes Paste in a writable local folder
- **THEN** each item is downloaded to the current local folder and a successful typed terminal is emitted

#### Scenario: Transfer failure
- **WHEN** an SFTP download, conflict decision, or cancellation prevents completion
- **THEN** the operation emits the existing failed, partial, skipped, or cancelled typed outcome without deleting a copied source
