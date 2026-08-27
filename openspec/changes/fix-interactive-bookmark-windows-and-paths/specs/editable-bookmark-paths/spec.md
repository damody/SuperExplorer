## ADDED Requirements

### Requirement: Exact editable path text
The dedicated bookmark editor SHALL expose Folder and File target text as an editable control. Saving MUST preserve the exact non-empty user-authored text and target kind without requiring the path to exist or to parse successfully at save time.

#### Scenario: Save an unavailable path
- **WHEN** the user saves a non-empty offline, remote, virtual, not-yet-created, malformed, or otherwise unavailable target string
- **THEN** the system MUST persist that exact string and close the editor after durable success

#### Scenario: Restore an arbitrary path
- **WHEN** a session containing arbitrary target text is restarted and the bookmark is edited
- **THEN** the editor MUST display the exact previously saved text without normalization or truncation

#### Scenario: Reject only an empty target
- **WHEN** the user attempts to save a Folder or File bookmark whose target text is empty after trimming
- **THEN** the system MUST retain the editor and draft and display a validation notice without mutating bookmarks

### Requirement: Deferred target validation
The system SHALL defer target parsing and availability checks until a bookmark is activated. Activation failure MUST preserve the bookmark and exact target text and SHALL display an actionable error.

#### Scenario: Activate an invalid target
- **WHEN** the user activates a saved target that cannot be parsed or opened
- **THEN** the system MUST keep the bookmark unchanged and SHALL report that the target could not be opened

### Requirement: Backward-compatible target persistence
The bookmark store MUST restore existing structured Folder and File targets without data loss and SHALL round-trip newly authored raw targets with their stable IDs, names, logical parents, kinds, and sibling order.

#### Scenario: Restore a legacy structured bookmark
- **WHEN** the application loads a pre-change bookmark session containing a structured location descriptor
- **THEN** it MUST preserve the target and expose editable equivalent target text without requiring migration input from the user
