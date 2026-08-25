## ADDED Requirements

### Requirement: Independent authoritative bookmark document
The system SHALL store the complete bookmark collection in a bounded, versioned document under `%LOCALAPPDATA%\RustGpuiExplorer\bookmarks\v1`, with recoverable current, pending, and last-known-good artifacts, and SHALL treat a valid independent document as authoritative over the legacy session bookmark field.

#### Scenario: Valid current bookmark document
- **WHEN** startup finds both a valid independent current document and different bookmarks in the legacy session envelope
- **THEN** the system restores the independent document without replacing it from the session envelope

#### Scenario: Intentionally empty independent collection
- **WHEN** the independent document is valid and contains zero bookmarks while the legacy session contains bookmarks
- **THEN** the system restores the empty collection and does not remigrate legacy bookmarks

#### Scenario: Payload exceeds the configured bound
- **WHEN** a bookmark artifact exceeds the configured maximum persistence payload
- **THEN** the system rejects it without an unbounded allocation and attempts the last-known-good artifact

### Requirement: One-time legacy bookmark migration
When no valid independent bookmark artifact exists because the independent store has never been created, the system SHALL copy the valid legacy session bookmark collection to the independent store before making that collection authoritative, without deleting or modifying the legacy session snapshot.

#### Scenario: First launch after upgrade
- **WHEN** a valid legacy session contains bookmarks and no independent current, backup, or quarantined bookmark artifact exists
- **THEN** the system atomically writes the independent document and restores every valid legacy bookmark, folder, stable ID, payload, and order

#### Scenario: Migration write is unavailable
- **WHEN** the independent store cannot be created or written during first-launch migration
- **THEN** the system keeps the legacy session and its bookmarks intact, uses the legacy collection for that process, records a privacy-safe failure, and may retry on a later durable transition

### Requirement: Bookmark recovery is isolated and non-destructive
The system SHALL validate independent bookmark artifacts, recover from a valid last-known-good document when the current document is missing or corrupt, and quarantine only invalid files inside the owned bookmark directory.

#### Scenario: Corrupt current with valid backup
- **WHEN** the current bookmark document is corrupt and the last-known-good document is valid
- **THEN** the system quarantines the corrupt current artifact, restores the backup collection, and repairs the current document

#### Scenario: Both bookmark artifacts are corrupt
- **WHEN** both current and last-known-good bookmark documents are corrupt
- **THEN** the system quarantines only those artifacts, preserves session and unrelated user data, and starts with the valid legacy collection or defaults

### Requirement: Session operations cannot erase bookmarks
The system SHALL keep independent bookmark storage unchanged when resetting saved session state, view settings, Quick Access, or all saved Explorer state.

#### Scenario: Reset saved session
- **WHEN** the user confirms reset of saved windows and tabs
- **THEN** the system removes session artifacts but preserves the independent bookmark document byte-for-byte

#### Scenario: Reset all saved Explorer state
- **WHEN** the user confirms reset of all saved Explorer state
- **THEN** the system removes the resettable session state but restores the same independent bookmarks on the next launch

### Requirement: Durable bookmark mutations use background recovery
The system SHALL persist accepted bookmark additions, edits, moves, reorders, and deletions off the UI thread using atomic replacement and SHALL retain the latest dirty snapshot for bounded retry after a write failure.

#### Scenario: Successful bookmark mutation
- **WHEN** the UI accepts a bookmark mutation and the bookmark store is writable
- **THEN** the independent current document contains the complete updated collection after the persistence coordinator flushes

#### Scenario: Transient bookmark write failure
- **WHEN** the first independent bookmark write fails and a later retry succeeds
- **THEN** the coordinator retains the latest collection until it is durably written and reports the failure and recovery through existing persistence health counters

### Requirement: Packaging preserves bookmark user data
The installer, upgrader, repair path, and uninstaller SHALL not delete, empty, replace, or relocate `%LOCALAPPDATA%\RustGpuiExplorer\bookmarks\v1`.

#### Scenario: Upgrade or repair
- **WHEN** an existing installation with independent bookmarks is upgraded or repaired
- **THEN** the installed application restores the same bookmark collection

#### Scenario: Uninstall and reinstall
- **WHEN** the user uninstalls SuperExplorer and later installs it again under the same Windows profile
- **THEN** the bookmark directory remains present and the reinstalled application restores the same bookmark collection

### Requirement: Bookmark persistence protects sensitive content
The system SHALL persist only the existing bookmark model and SHALL not include credentials, file contents, or bookmark payload values in storage error diagnostics.

#### Scenario: Persistence error diagnostic
- **WHEN** bookmark load, migration, or save fails
- **THEN** diagnostics identify the operation and storage error category without recording bookmark names, target paths, remote secrets, or Lua source
