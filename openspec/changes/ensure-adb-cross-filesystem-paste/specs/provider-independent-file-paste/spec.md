## ADDED Requirements

### Requirement: Internal file Paste is provider-independent
The system SHALL allow application-owned ADB clipboard items to be pasted into the current Local directory or any current writable Virtual directory whose registered provider can accept uploads, without provider-name allowlists.

#### Scenario: ADB to Local
- **WHEN** an ADB item is copied and Paste targets a writable Local current directory
- **THEN** the ADB provider SHALL download the item into that directory through the shared transfer engine

#### Scenario: ADB to SFTP
- **WHEN** an ADB item is copied and Paste targets a writable SFTP current directory
- **THEN** the ADB provider SHALL download into scoped staging and the SFTP provider SHALL upload the staged content into the current directory

#### Scenario: ADB to another registered provider
- **WHEN** an ADB item is copied and Paste targets another writable registered Virtual provider
- **THEN** transfer routing SHALL resolve both providers from typed descriptors and SHALL NOT require an ADB/SFTP-specific pair branch

#### Scenario: Immediate internal Paste
- **WHEN** Paste is invoked before asynchronous native Windows clipboard staging completes
- **THEN** the application SHALL use its internal typed clipboard and begin the transfer without waiting for native staging

### Requirement: Paste destination is the active current directory
The system SHALL construct a Paste request using the active tab's current location regardless of the row hit by the context-menu invocation.

#### Scenario: Context menu hit variants
- **WHEN** the user invokes Paste after right-clicking the background, a file, or a directory row
- **THEN** all three requests SHALL use the same active current directory as destination

### Requirement: Unsupported destinations fail closed
The system SHALL omit or reject file Paste when the clipboard is invalid, the current location is not writable, or the required provider cannot be resolved, without changing clipboard ownership or deleting a source.

#### Scenario: Read-only destination
- **WHEN** the active destination is read-only
- **THEN** context-menu Paste SHALL not be offered as available

#### Scenario: Destination upload failure
- **WHEN** an ADB download succeeds but the destination provider upload fails
- **THEN** the operation SHALL report failure, retain Copy clipboard ownership, preserve the ADB source, and clean only its scoped staging directory
