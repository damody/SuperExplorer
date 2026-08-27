## ADDED Requirements

### Requirement: Detailed typed operation summary
The system SHALL display the latest file operation with its operation type, relevant full source or target location, destination when applicable, item count, progress and terminal result.

#### Scenario: Copy or move operation
- **WHEN** a copy or move operation is active or terminal
- **THEN** the message identifies the operation, item count, first full source location, remaining source count and full destination location

#### Scenario: Create or rename operation
- **WHEN** a create or rename operation is active or terminal
- **THEN** the message identifies the operation and the complete target path when it can be safely derived

#### Scenario: Delete or shortcut operation
- **WHEN** a delete or shortcut operation is active or terminal
- **THEN** the message identifies the operation, item count and source location summary

### Requirement: Safe Local and remote path display
The system MUST display Local paths in Windows form and ADB or SFTP locations as canonical URIs without including passwords or other authentication secrets.

#### Scenario: Remote operation summary
- **WHEN** an operation contains an ADB or SFTP location
- **THEN** the visible summary contains its canonical URI and no credential secret

#### Scenario: Multiple source items
- **WHEN** an operation contains more than one source item
- **THEN** the summary shows the first full location and the number of additional items instead of expanding every path

### Requirement: Terminal message eight-second lifecycle
The system SHALL keep an active operation fully visible, keep its terminal message fully visible through the first seven seconds, fade it linearly during the eighth second, and stop rendering the operation message at eight seconds.

#### Scenario: Active operation
- **WHEN** the latest operation has not reached a terminal phase
- **THEN** its message remains visible at full opacity without an expiration deadline

#### Scenario: Terminal operation before seven seconds
- **WHEN** less than seven seconds have elapsed since the latest accepted terminal event
- **THEN** its message remains visible at full opacity

#### Scenario: Terminal operation during eighth second
- **WHEN** elapsed terminal age is at least seven seconds and less than eight seconds
- **THEN** its opacity decreases linearly from one to zero

#### Scenario: Terminal operation reaches eight seconds
- **WHEN** at least eight seconds have elapsed
- **THEN** the operation message is not rendered and its layout height is released

#### Scenario: New operation starts
- **WHEN** another operation becomes the latest record before the previous notice expires
- **THEN** the previous notice is replaced and the new operation receives its own lifecycle

### Requirement: Terminal outcome details
The system SHALL distinguish success, cancellation, partial success and failure while retaining bounded actionable details.

#### Scenario: Partial or failed operation
- **WHEN** the latest operation is partial or failed
- **THEN** the summary includes the result and reason, and partial detail rows are limited to five

#### Scenario: Hover over terminal message
- **WHEN** the pointer remains over a terminal operation message
- **THEN** the eight-second deadline continues without pausing
