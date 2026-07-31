## ADDED Requirements

### Requirement: Restored drive roots use absolute Shell parent identity
The system SHALL normalize a successfully resolved bare Windows drive designator to an absolute drive-root filesystem descriptor while preserving other explicit filesystem paths and opaque Shell namespace descriptors.

#### Scenario: Legacy bare drive root is restored
- **WHEN** a persisted tab restores a filesystem location such as `D:` and Shell resolution succeeds for the drive root
- **THEN** the active history location SHALL become `D:\` and the next persisted session SHALL store that absolute root

#### Scenario: Ordinary explicit path remains stable
- **WHEN** Shell resolves an explicit filesystem path that is not a bare drive designator
- **THEN** the published descriptor SHALL retain the requested explicit path identity

### Requirement: Restored drive-root item menus remain usable
The system SHALL open the native Shell item context menu for an item shown in a restored drive-root tab under normal foreground-window conditions.

#### Scenario: Physical right click after multi-tab high-DPI restore
- **WHEN** the application restores multiple tabs at high DPI with a bare drive-root tab active and the user physically right-clicks a visible non-first item
- **THEN** that item SHALL become selected and exactly one native context-menu popup bound to the launched process tree SHALL appear without topmost assistance
