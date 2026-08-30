## ADDED Requirements

### Requirement: Remote background menu exposes current-directory Properties
ADB and SFTP directory-background menus SHALL expose `內容`, which SHALL describe the directory
currently displayed rather than any selected child.

#### Scenario: Open current-directory Properties
- **WHEN** the user selects background `內容`
- **THEN** the menu closes and SuperExplorer requests metadata for the captured current remote location
- **AND** no synthetic selected listing row is created

### Requirement: Properties uses authoritative provider metadata
The current-directory Properties window SHALL show the public ADB/SFTP path, display name,
directory type, permissions, modification time, and size when authoritative provider metadata
contains them; unavailable fields SHALL be labeled unavailable.

#### Scenario: Complete metadata
- **WHEN** the provider returns all supported metadata fields
- **THEN** the existing owned remote Properties window displays those authoritative values

#### Scenario: Partial metadata
- **WHEN** directory size or another optional field is unavailable
- **THEN** the window says the field is unavailable
- **AND** SuperExplorer does not start an implicit recursive scan or invent a value

#### Scenario: Metadata failure
- **WHEN** the provider cannot read the current directory metadata
- **THEN** SuperExplorer surfaces the provider failure without opening a false Properties snapshot

### Requirement: Properties completion rejects stale navigation
Metadata completion MUST be matched to the captured tab, generation, and location before opening
or replacing the owned Properties window.

#### Scenario: Current metadata completion
- **WHEN** metadata completes and the captured directory remains current
- **THEN** SuperExplorer opens or replaces the owned remote Properties window

#### Scenario: Stale metadata completion
- **WHEN** navigation or tab replacement changes the captured context before metadata completes
- **THEN** the late result does not open or replace a Properties window for the new location

### Requirement: Item Properties and menu interaction remain stable
Adding background Properties MUST NOT change selected-item Properties semantics or regress remote
menu visual and interaction behavior.

#### Scenario: Selected item Properties
- **WHEN** the user invokes `內容` from an ADB or SFTP item menu
- **THEN** the existing selected-item metadata route and displayed item remain unchanged

#### Scenario: Interaction regression suite
- **WHEN** the remote menu interaction suite runs after this change
- **THEN** hover, pressed, Escape, outside-click dismissal, right-click replacement, keyboard activation, accessibility roles, and edge clamping pass
