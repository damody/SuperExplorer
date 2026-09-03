## ADDED Requirements

### Requirement: Standard username-bearing SFTP address input
The system SHALL interpret a SFTP URI authority containing one `@` as `username@host`, use the username only as a transient login hint, and use the host for remote profile resolution.

#### Scenario: Standard URI prefills login
- **WHEN** the user navigates to `sftp://root@45.32.49.125/home/linuxuser`
- **THEN** the login flow receives `root` as its username hint and resolves `45.32.49.125` as the host

#### Scenario: Host-only URI remains compatible
- **WHEN** the user opens `sftp://45.32.49.125/home/linuxuser`
- **THEN** the system resolves a matching saved profile or presents login without a username hint

#### Scenario: Reversed legacy authority is not inferred
- **WHEN** the user submits `sftp://45.32.49.125@root/`
- **THEN** the system treats `45.32.49.125` as the username and `root` as the host without applying a legacy-order heuristic

### Requirement: Safe SFTP user-info validation
The system SHALL reject empty usernames, multiple `@` separators, and authorities containing a password delimiter without persisting or exposing password text.

#### Scenario: Password-bearing URI is rejected safely
- **WHEN** the user submits `sftp://root:secret@45.32.49.125/`
- **THEN** navigation fails through the visible invalid-address path and no canonical address, profile, history, bookmark, or diagnostic output contains `secret`
