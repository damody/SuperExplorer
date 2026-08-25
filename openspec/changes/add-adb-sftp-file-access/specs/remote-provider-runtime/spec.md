## ADDED Requirements

### Requirement: Typed remote virtual locations
The system SHALL resolve `adb://<serial>/<normalized-path>` and
`sftp://<profile>/<normalized-path>` into validated `LocationDescriptor::Virtual`
values without embedding credentials or raw host names in SFTP locations.

#### Scenario: Invalid URI component
- **WHEN** an address contains `..`, NUL, or a path separator inside one component
- **THEN** navigation SHALL fail before any provider process or network I/O

### Requirement: Correlated provider operations
The remote runtime SHALL honour request cancellation and deadlines and SHALL
emit no more than one correlated terminal outcome for every accepted request.

#### Scenario: User navigates away during listing
- **WHEN** a directory listing is cancelled by a later navigation
- **THEN** its provider work SHALL stop and its late batches SHALL not update the active tab

### Requirement: Cross-provider transfer outcomes
The system SHALL report an item-level outcome for each local, ADB, or SFTP copy
or move, and SHALL mark a failed source deletion after a completed cross-provider
copy as partial rather than reporting a successful move.

#### Scenario: Move deletion fails
- **WHEN** a copied remote source cannot be deleted
- **THEN** destination data SHALL remain and the operation SHALL report partial completion

### Requirement: File clipboard format isolation
The system SHALL invoke file copy, cut, or paste only when the file view owns
the shortcut and the clipboard contains local file-drop data or the versioned
SuperExplorer remote-file format. Text, rich text, HTML, and image clipboard
formats SHALL remain available to their normal consumers and SHALL not start a
file operation.

#### Scenario: User pastes text into an editor
- **WHEN** an editable text control owns focus and the user presses Ctrl+V
- **THEN** the text control SHALL receive the paste and no file transfer request SHALL be created

#### Scenario: Clipboard contains an image
- **WHEN** the active directory receives Ctrl+V while the clipboard contains only image formats
- **THEN** no file operation SHALL be started and the image clipboard contents SHALL remain unchanged

### Requirement: Native Explorer drag interoperability
The system SHALL accept native filesystem drops into Local, ADB, and SFTP
destinations and SHALL expose Local, ADB, and SFTP selections as OLE drag sources
that Windows Explorer can copy or move according to the negotiated effect.

#### Scenario: Native files dropped on SFTP
- **WHEN** Windows Explorer drops `CF_HDROP` files onto an SFTP directory with copy effect
- **THEN** the files SHALL be uploaded through the cross-provider transfer engine

#### Scenario: Remote file dragged to Windows Explorer
- **WHEN** an ADB or SFTP file is dragged to a native Explorer folder
- **THEN** SuperExplorer SHALL stage or stream the file and complete an OLE file drop without exposing credentials
