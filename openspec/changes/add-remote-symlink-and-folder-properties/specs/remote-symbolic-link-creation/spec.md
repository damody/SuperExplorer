## ADDED Requirements

### Requirement: Remote background menu exposes symbolic-link creation
For ADB and SFTP directory backgrounds, SuperExplorer SHALL display `新增捷徑` after `新增資料夾`
and SHALL preserve the accepted remote menu visuals, dismissal, accessibility, keyboard behavior,
and edge clamping.

#### Scenario: Background command membership
- **WHEN** the user opens an ADB or SFTP directory-background context menu
- **THEN** the menu contains `新增資料夾`, `新增捷徑`, and `內容` in that order
- **AND** item-only commands are not introduced into the background menu

#### Scenario: File item menu is unchanged
- **WHEN** the user opens an ADB or SFTP non-folder item context menu
- **THEN** the existing item command membership and order remain unchanged

#### Scenario: Folder item exposes direct shortcut creation
- **WHEN** the user opens an ADB or SFTP folder item context menu
- **THEN** the menu contains `新增捷徑`
- **AND** selecting it directly creates a sibling symbolic link without opening the editor
- **AND** the stored target is the clicked folder's display name
- **AND** the destination name is `原名稱 - 捷徑` or the first free numbered variant

### Requirement: Dedicated editor captures link name and target
Selecting `新增捷徑` SHALL open an owned, separately interactive window with editable
`捷徑名稱` and `目標路徑` fields plus Cancel and Create actions.

#### Scenario: Open editor
- **WHEN** the user selects `新增捷徑`
- **THEN** the context menu closes
- **AND** the dedicated editor opens without blocking the main window
- **AND** both fields can be edited before provider work begins

#### Scenario: Repeated invocation
- **WHEN** an editor already exists and the user invokes `新增捷徑` again
- **THEN** SuperExplorer replaces or focuses that owned editor instead of accumulating hidden windows

### Requirement: Link input validation preserves Linux target semantics
SuperExplorer MUST reject an invalid child link name before provider dispatch and MUST permit a
nonempty relative, absolute, whitespace-containing, or currently nonexistent target string.

#### Scenario: Invalid child name
- **WHEN** the name is empty, whitespace-only, `.`, `..`, or contains `/`, `\\`, or NUL
- **THEN** Create does not dispatch provider work
- **AND** the editor displays a correction message while preserving both fields

#### Scenario: Dangling target
- **WHEN** the target is nonempty but does not exist
- **THEN** SuperExplorer submits it unchanged and permits the provider to create a dangling link

### Requirement: Provider-native creation is safe and asynchronous
The system SHALL create ADB links through argument-safe fixed-script execution and SFTP links
through the SFTP symlink protocol operation, on a remote worker rather than the GPUI thread.

#### Scenario: Successful ADB creation
- **WHEN** a valid ADB link request completes
- **THEN** the remote entry is a symbolic link whose stored target exactly matches the submitted target

#### Scenario: Successful SFTP creation
- **WHEN** a valid SFTP link request completes
- **THEN** the remote entry is a symbolic link whose stored target exactly matches the submitted target

#### Scenario: Provider failure
- **WHEN** creation fails because of duplicate name, permission, connectivity, cancellation, or protocol error
- **THEN** no success state is fabricated
- **AND** the editor preserves both fields and displays the failure

### Requirement: Completion is scoped to the captured directory
Successful creation SHALL refresh only the captured current tab/generation/location and SHALL
select the new link when it appears; late results MUST NOT affect a replacement navigation state.

#### Scenario: Current completion
- **WHEN** creation succeeds and the captured directory is still current
- **THEN** the editor closes, the directory refreshes, and the new link becomes selected when listed

#### Scenario: Stale completion
- **WHEN** the user navigates or replaces the tab generation before completion
- **THEN** the late result does not refresh, select, or otherwise modify the replacement state

#### Scenario: Folder shortcut name collides concurrently
- **WHEN** a direct folder shortcut destination becomes occupied after menu construction
- **THEN** the provider failure is not reported as success
- **AND** no existing entry is overwritten
