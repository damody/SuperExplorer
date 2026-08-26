## ADDED Requirements

### Requirement: Direct-host SFTP address login
The system SHALL turn an unsaved `sftp://<host>/` address into an in-app login flow instead of submitting an unavailable remote navigation.

#### Scenario: Host-only address
- **WHEN** the user submits `sftp://45.32.49.125/` without a usable saved credential
- **THEN** the login surface SHALL open for host `45.32.49.125` with port 22

#### Scenario: Username hint
- **WHEN** the user submits `sftp://45.32.49.125@root/`
- **THEN** the login surface SHALL prefill `root` and every persistable/display address SHALL be canonicalized to `sftp://45.32.49.125/`

### Requirement: Secure automatic persistence
The system SHALL persist a successfully authenticated host profile automatically while storing its password only in Windows Credential Manager.

#### Scenario: Successful first login
- **WHEN** the user explicitly submits valid username and password and accepts the first presented host key through Login
- **THEN** the system SHALL save the non-secret profile, store the credential, refresh SFTP runtime/navigation state, and navigate once to the canonical address

#### Scenario: Secret isolation
- **WHEN** login state, profile JSON, history, bookmarks, debug output, or diagnostics are serialized or formatted
- **THEN** none SHALL contain the password or transient `@username` input

### Requirement: SFTP login failure safety
The system SHALL keep failed authentication recoverable without activating an invalid profile or silently replacing host trust.

#### Scenario: Authentication failure
- **WHEN** authentication fails
- **THEN** the login surface SHALL remain open, clear the password, show a redacted error, and SHALL NOT navigate

#### Scenario: Host key changed
- **WHEN** a saved host presents a different fingerprint
- **THEN** login SHALL be blocked and the stored fingerprint SHALL remain unchanged
