# SFTP Address Login Design

## Purpose

Allow a user to type `sftp://<host>/` or `sftp://<host>@<username>/` in the
address bar and complete a secure password login without manually editing a
profile file. The host is also the stable public profile alias. User-info is an
input hint only and is removed before the address reaches history, tabs,
bookmarks, diagnostics, or provider locations.

## Address contract

- `sftp://45.32.49.125/` opens the login surface with an empty username unless
  a saved profile supplies one.
- `sftp://45.32.49.125@root/` opens the same surface with `root` prefilled and
  immediately canonicalizes the visible/persistable address to
  `sftp://45.32.49.125/`.
- A port may be entered in the login surface; it defaults to 22 and is not part
  of the public alias.
- Passwords and transient user-info are never serialized as locations.
- Existing alias-based profiles remain readable; a host-created profile uses
  the host string as its alias.

## Components and data flow

1. A model parser recognizes the optional SFTP username hint separately from
   the canonical `RemoteAddress`.
2. Address submission asks an application-owned SFTP connection coordinator
   whether the canonical host profile has a usable saved credential.
3. If it does, navigation proceeds. Otherwise UI state opens an SFTP login
   surface containing host, port, username, masked password, and status.
4. Submit probes the SSH host key. A first-seen fingerprint is displayed as
   part of the login status and accepted by the explicit Login action. A
   changed fingerprint blocks connection and is never silently replaced.
5. Successful authentication atomically writes non-secret profile JSON and
   stores the password under `SuperExplorer/SFTP/<host>` in Windows Credential
   Manager. The runtime provider/navigation snapshot is refreshed, then the
   original canonical location is navigated.

The connection coordinator owns filesystem, Credential Manager, host-key, and
provider mutations. UI state owns only non-secret draft fields; the password
is held by a masked input entity until submission and is cleared on completion,
cancel, or failure teardown.

## UI behavior

The login surface is the Windows native Credential UI owned by SuperExplorer.
It shows the host, prefilled editable username, masked password, Login and
Cancel controls, and blocks only the owning Explorer window while active.
Windows persistence is disabled for this prompt; SuperExplorer stores the
credential only after SSH authentication and host-key validation succeed.

## Errors and security

- Parse errors remain address-bar navigation errors without network access.
- Missing username/password stays in the login surface with field-level text.
- Authentication/network errors use redacted messages; neither password nor
  full credential payload is formatted with `Debug` or tracing.
- First trust occurs only on explicit Login. A later host-key mismatch blocks
  login and requires a separate trust-replacement feature outside this change.
- Profile persistence is replace-via-temporary-file so a crash cannot leave
  malformed JSON. Credential write failure prevents profile activation.

## Targeted verification

- Parser accepts both requested forms and canonical output contains no user.
- Address submission opens login rather than submitting Shell/remote navigation
  when credentials are absent.
- Username hint and saved username precedence are deterministic: explicit hint,
  then saved profile, then empty.
- Password is absent from address/history/profile JSON/debug output.
- Successful submission stores the credential/profile, refreshes the remote
  registry, and navigates once to the canonical host URI.
- Authentication and host-key mismatch leave no active profile and do not
  navigate.
