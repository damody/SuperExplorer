## Why

SuperExplorer currently uses the reversed, nonstandard SFTP username form `sftp://host@username/`. This causes confusing address entry and fails to prefill the login identity using ordinary SFTP URL syntax.

## What Changes

- Accept the standard SFTP authority form `sftp://username@host/path` and pass its username into the login flow.
- Preserve host-only SFTP paths for existing profiles, bookmarks, history, and login entry.
- **BREAKING** Stop treating `sftp://host@username/path` as a reversed username hint; any authority containing `@` is interpreted only as `username@host`.
- Reject credential-bearing or malformed authorities without persisting or exposing passwords.

## Capabilities

### New Capabilities

- `standard-sftp-userinfo-addresses`: Defines standard username-bearing SFTP URI parsing, formatting, compatibility, and safe failure behavior.

### Modified Capabilities

None.

## Impact

- `crates/explorer-model/src/remote.rs`: shared SFTP parsing and canonical formatting contract.
- `crates/explorer-ui/src/lib.rs`: address-bar interception and navigation tests.
- `crates/explorer-app/src/remote_service.rs`: host-based profile resolution after login prefill.
- Existing host-only saved metadata remains compatible; no credential-vault migration or external service change is required.
