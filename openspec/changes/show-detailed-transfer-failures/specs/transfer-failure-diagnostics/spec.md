## ADDED Requirements

### Requirement: Transfer failures retain item context
The system SHALL associate every failed or partially completed Local, ADB or SFTP transfer item with its logical source, logical destination and failed stage.

#### Scenario: Destination upload fails
- **WHEN** a Local, ADB or SFTP source is successfully staged but the destination provider upload fails
- **THEN** the item outcome contains the logical source, computed destination target and destination-upload stage
- **AND** the outcome retains the provider's available diagnostic chain

#### Scenario: Remote source download fails
- **WHEN** a remote source cannot be downloaded into owned local staging
- **THEN** the item outcome identifies the source-download stage
- **AND** the displayed route uses the remote source and logical destination rather than the temporary staging path

#### Scenario: Move source deletion fails
- **WHEN** copy succeeds but deleting the source for a move fails
- **THEN** the result is partial rather than failed
- **AND** its diagnostic identifies source deletion and retains the deletion error

### Requirement: Detailed failure messages are actionable
The operation message UI SHALL display a distinct detail row for each reported failed or partial item, including source, destination, stage and safe underlying reason.

#### Scenario: Multiple items fail differently
- **WHEN** multiple copied items fail with different provider diagnostics
- **THEN** each visible detail row names its own item route and actual reason
- **AND** the rows do not repeat a context-free generic transfer sentence

#### Scenario: Native code is available
- **WHEN** a failed item's error contains a native error code
- **THEN** the detail row displays that code with the safe reason

#### Scenario: Provider supplies no reason
- **WHEN** the provider diagnostic is empty after sanitization
- **THEN** the detail row displays an explicit `未提供底層錯誤` fallback

### Requirement: Transfer diagnostics do not expose secrets
The system MUST sanitize transfer diagnostics before storing them in UI operation state and MUST NOT display passwords, authentication tokens, secrets or URI userinfo.

#### Scenario: Diagnostic contains SFTP userinfo
- **WHEN** a provider error contains an SFTP URI with username or password userinfo
- **THEN** the displayed diagnostic retains the host and path but replaces the userinfo with `[已隱藏]`

#### Scenario: Diagnostic contains credential assignments
- **WHEN** a provider error contains a password, token or secret assignment
- **THEN** the corresponding value is replaced with `[已隱藏]`
- **AND** non-sensitive provider error text remains visible

### Requirement: Existing operation-message behavior remains stable
Detailed transfer failures SHALL preserve existing cancellation semantics, partial-row limit and terminal-message lifetime.

#### Scenario: Cancellation occurs
- **WHEN** cancellation is observed before or during transfer
- **THEN** the result remains cancelled and is not rewritten as a provider failure

#### Scenario: More than five items fail
- **WHEN** a terminal operation contains more than five item outcomes
- **THEN** OperationCenter renders no more than the existing five detail rows

#### Scenario: Failure notice expires
- **WHEN** a detailed failed or partial operation reaches its terminal state
- **THEN** it follows the existing seven-second hold, one-second fade and removal at eight seconds
