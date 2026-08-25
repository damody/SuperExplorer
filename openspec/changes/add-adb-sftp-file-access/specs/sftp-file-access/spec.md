## ADDED Requirements

### Requirement: Secure SFTP profile connection
The system SHALL connect to a saved SFTP profile using password authentication,
store the password only in Windows Credential Manager, and pin the SSH host key
fingerprint after explicit first trust.

#### Scenario: Connection uses stored secret
- **WHEN** the user connects to a saved profile with a stored password
- **THEN** the session SHALL authenticate without persisting or displaying the password in application data

#### Scenario: Host key changes
- **WHEN** a server presents a fingerprint different from the profile's pin
- **THEN** the connection SHALL be blocked pending explicit user replacement of trust

### Requirement: SFTP file operations
The system SHALL list, refresh, create folder, rename, permanently delete, and
stream files for an active SFTP profile.

#### Scenario: Delete remote file
- **WHEN** the user confirms deletion of an SFTP file
- **THEN** SuperExplorer SHALL delete it remotely and refresh the affected directory without claiming local undo support
