## ADDED Requirements

### Requirement: Bookmark icons identify target sources consistently

The system SHALL render source-specific icons for bookmark entries across toolbar, overflow, folder content, manager, and navigation projections.

#### Scenario: Local bookmark
- **WHEN** a bookmark target is Local or has no recognized remote scheme
- **THEN** it displays the bookmark icon `🔖`

#### Scenario: Remote bookmark
- **WHEN** a structured or raw target is ADB or SFTP
- **THEN** it displays the phone icon `📱` for ADB or the remote-computer icon `🖥` for SFTP without validating availability

#### Scenario: Lua bookmark
- **WHEN** the target is Lua
- **THEN** it displays the unchanged, proportionally scaled official Lua logo from Lua.org
- **AND** the application does not require network access to render it
