# Remote File Context Commands

## ADDED Requirements

### Requirement: Remote item menus expose currently actionable common commands

ADB and SFTP item menus SHALL expose Download to Downloads, Copy Remote Path, Add to Bookmarks, and Properties using existing transfer, canonical path, durable bookmark, and namespace actions.

#### Scenario: Remote file is selected

- **WHEN** its custom context menu opens
- **THEN** Copy Remote Path copies the canonical `adb://` or `sftp://` URI
- **AND** Add to Bookmarks starts the existing bookmark editor for the selected target
- **AND** Download to Downloads copies the selected item through the cross-provider transfer engine to `%USERPROFILE%\Downloads`
- **AND** Properties uses the selected remote item's namespace metadata route

#### Scenario: Background menu opens

- **WHEN** no remote item is targeted
- **THEN** item-only URI, bookmark, and new-tab commands are absent

### Requirement: Remote folders can open in a new tab

The menu SHALL expose Open in New Tab only when the focused selected item is a container and SHALL dispatch the existing row-open action with `new_tab: true`.

#### Scenario: Folder is targeted

- **WHEN** an ADB or SFTP folder context menu opens
- **THEN** Open in New Tab is visible and opens that row in a new tab

#### Scenario: File is targeted

- **WHEN** an ADB or SFTP file context menu opens
- **THEN** Open in New Tab is absent

### Requirement: Unsupported remote operations are not advertised

Commands SHALL NOT be rendered until their required backend and failure contract exists.

#### Scenario: Current backend lacks a command contract

- **WHEN** the menu is projected
- **THEN** Android install/intent, SFTP chmod, and SSH terminal commands are absent
