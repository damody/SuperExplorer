## ADDED Requirements

### Requirement: Selecting a bookmark dismisses its browse menu immediately

The system SHALL close folder and overflow bookmark menus before resolving or activating the selected bookmark.

#### Scenario: Successful activation
- **WHEN** a bookmark in a browse menu is selected
- **THEN** the menu disappears before navigation or launch begins

#### Scenario: Failed or stale activation
- **WHEN** the selected bookmark is invalid, unavailable, or no longer exists
- **THEN** the menu still disappears and the normal failure notice may be shown

#### Scenario: Child-folder drill-in
- **WHEN** a child folder is selected
- **THEN** the browse menu remains open and switches to that folder's content
