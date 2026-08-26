## Why

SFTP currently requires hand-authored profile JSON and a separate credential helper. Direct host addresses should instead lead to an in-app secure login flow.

## What Changes

- Accept `sftp://host/` and the username-hint form `sftp://host@user/`.
- Canonicalize both to `sftp://host/` before persistence or logging.
- Add an in-app masked SFTP login surface with first-use host-key trust.
- Persist profiles automatically and store passwords only in Windows Credential Manager.
- Refresh the provider/navigation snapshot and navigate after successful login.

## Capabilities

### New Capabilities

- `sftp-address-login`: Direct-host SFTP login, canonicalization, trust, and secure persistence.

### Modified Capabilities

None.

## Impact

Address parsing/model contracts, GPUI address submission and modal state, application composition, SFTP provider probing, profile persistence, Windows Credential Manager, and navigation refresh.
