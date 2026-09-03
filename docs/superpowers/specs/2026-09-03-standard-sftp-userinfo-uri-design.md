# SFTP `username@host` URI standardization design

## Goal

SuperExplorer shall use the conventional SFTP authority order `sftp://username@host/path` everywhere a user sees, enters, copies, or bookmarks an authenticated SFTP location.

## Accepted forms

- `sftp://username@host/path` supplies a username hint and a host.
- `sftp://host/path` remains valid. The application resolves an existing saved profile for the host or opens the login UI without a username hint.
- The former nonstandard `sftp://host@username/path` interpretation is removed. The parser must never guess which side of `@` is the host because that is ambiguous and can connect to the wrong endpoint.
- Passwords are never accepted in, displayed in, logged from, or persisted as part of an SFTP URI.

## Canonical representation

After credentials are known, user-facing SFTP addresses use `sftp://username@host/path`. Internal profile identifiers may remain stable so existing saved profiles and sessions do not require destructive migration. Host-only saved bookmarks remain readable; when the profile username is available, subsequent display and saved output are upgraded to the standard form.

The host portion retains existing IPv4, DNS-name, port, and IPv6 validation rules. Usernames must be non-empty when `@` is present. URI formatting must preserve the existing path normalization behavior.

## Affected flows

The shared SFTP address parser and formatter are the source of truth. Address-bar navigation, login prefilling, tab/breadcrumb display, copied remote paths, bookmarks, navigation history, and tests must consume that shared behavior instead of independently swapping authority components.

## Compatibility and failure behavior

Existing host-only profiles, credentials, bookmarks, and history remain valid. A malformed user-info URI is rejected through the existing visible navigation/login error path without panicking or logging secrets. An old `host@username` string is intentionally interpreted according to the new standard (`username = host`, `host = username`) rather than through an unreliable heuristic; documentation and tests explicitly prevent the old convention from returning.

## Verification

Focused tests cover parsing, canonical formatting, login prefilling, host-only compatibility, bookmarks/history round trips, malformed authorities, and secret redaction. A final targeted workspace check is run only after implementation, followed by a user-perspective check of address entry, login, navigation, bookmark restoration, and copied-path output.
