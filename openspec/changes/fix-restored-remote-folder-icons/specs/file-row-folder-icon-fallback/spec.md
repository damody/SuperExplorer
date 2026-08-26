## ADDED Requirements

### Requirement: File rows select folder visuals in stable priority order
The file view SHALL prefer an item-specific Shell icon or thumbnail and SHALL use the generic Windows Shell folder texture only when the entry is a container and the specific visual is unavailable.

#### Scenario: Specific folder visual is available
- **WHEN** a container row has both an item-specific visual and the generic folder texture
- **THEN** the row renders the item-specific visual

#### Scenario: Restored remote container has no specific visual
- **WHEN** a restored ADB or SFTP container row has no item-specific Shell visual
- **AND** the generic Windows Shell folder texture is available
- **THEN** the row renders the generic Windows Shell folder texture

#### Scenario: Local container visual is still loading
- **WHEN** navigation returns from a restored remote tab to a local directory
- **AND** a local container's item-specific visual is not yet available
- **AND** the generic Windows Shell folder texture is available
- **THEN** the row renders the generic texture until the specific visual arrives

### Requirement: Folder fallback remains type-safe and bounded
The file view MUST NOT use the generic folder texture for non-container entries and SHALL retain the existing vector placeholder when no eligible Shell texture is available.

#### Scenario: File lacks a specific visual
- **WHEN** a non-container file has no item-specific visual
- **THEN** the row does not render the generic folder texture
- **AND** the existing file placeholder remains available

#### Scenario: No Shell folder texture is available
- **WHEN** a container has neither an item-specific visual nor the generic folder texture
- **THEN** the row renders the existing folder placeholder
