## ADDED Requirements

### Requirement: Folder Options exposes a single search engine

The system SHALL show Everything, MFT, and file enumeration as a single-select group on Folder Options → General. The default preference SHALL be Everything.

#### Scenario: Default preference is Everything

- **WHEN** a new or legacy session is loaded without a stored search engine
- **THEN** the runtime preference is Everything

#### Scenario: User selects file enumeration

- **WHEN** file enumeration is available and the user selects it, then applies Folder Options
- **THEN** subsequent searches use only filesystem enumeration

### Requirement: Unsupported engines are visible but unselectable

The system SHALL probe availability when Folder Options opens. Unsupported radios SHALL remain visible, keep a selected-but-disabled state when they are the stored preference, ignore clicks, and show a text hint.

#### Scenario: Remote location disables all engines

- **WHEN** the current location is not a local filesystem path
- **THEN** all three radios are disabled with hints, and Apply of other Folder Options still succeeds

#### Scenario: Stored Everything is unavailable locally

- **WHEN** Everything is unavailable and file enumeration is available
- **THEN** Apply is rejected until the user selects an available engine

### Requirement: Search uses only the selected engine

The system SHALL pass the selected engine on `StartSearch` and SHALL NOT fall back to another backend.

#### Scenario: Everything fails closed

- **WHEN** the selected engine is Everything and SDK or IPC is unavailable
- **THEN** the search publishes Everything unavailable and a failed terminal without filesystem results

### Requirement: MFT filename search reconstructs paths

When MFT is selected and the volume index exists, the system SHALL reconstruct paths from parent references and match with the shared search matcher.

#### Scenario: MFT hit under the search root

- **WHEN** the index contains `C:\Users\報告.txt` and the user searches `報告` in `C:\Users`
- **THEN** the result set includes `報告.txt` and excludes names that do not match
