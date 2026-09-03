# SFTP `username@host` URI standardization design

## Goal

SuperExplorer shall accept the conventional SFTP authority order `sftp://username@host/path` for direct address-bar entry and login prefilling.

## Accepted forms

- `sftp://username@host/path` supplies a username hint and a host.
- `sftp://host/path` remains valid. The application resolves an existing saved profile for the host or opens the login UI without a username hint.
- The former nonstandard `sftp://host@username/path` interpretation is removed. The parser must never guess which side of `@` is the host because that is ambiguous and can connect to the wrong endpoint.
- Passwords are never accepted in, displayed in, logged from, or persisted as part of an SFTP URI.

## Canonical representation

The parsed username is transient. The address is canonicalized to the existing credential-free `sftp://host/path` representation after login so profile aliases, history, bookmarks, copied paths, diagnostics, and credential targets remain stable and do not duplicate identity data.

The host portion retains existing IPv4, DNS-name, port, and IPv6 validation rules. Usernames must be non-empty when `@` is present. URI formatting must preserve the existing path normalization behavior.

## Affected flows

The shared SFTP address parser is the source of truth. Address-bar navigation and login prefilling must consume that behavior instead of independently swapping authority components.

## Compatibility and failure behavior

Existing host-only profiles, credentials, bookmarks, and history remain valid. A malformed user-info URI is rejected through the existing visible navigation/login error path without panicking or logging secrets. An old `host@username` string is intentionally interpreted according to the new standard (`username = host`, `host = username`) rather than through an unreliable heuristic; documentation and tests explicitly prevent the old convention from returning.

## Verification

Focused tests cover parsing, login prefilling, host-only compatibility, malformed authorities, and secret redaction. A final targeted workspace check is run only after implementation, followed by a user-perspective check of address entry, login, and navigation.
