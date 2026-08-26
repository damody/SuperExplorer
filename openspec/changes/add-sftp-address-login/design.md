## Context

Direct SFTP addresses currently resolve as saved aliases; unsaved hosts fail because profile creation, credential storage, trust, and provider refresh have no UI workflow. The approved source design is `docs/superpowers/specs/2026-08-26-sftp-address-login-design.md`.

## Goals / Non-Goals

**Goals:** accept direct hosts and username hints, canonicalize before persistence, authenticate through a masked modal, pin the first host key, securely persist successful login, and navigate without restart.

**Non-Goals:** key/agent auth, embedding ports or passwords in URLs, silent changed-key trust, or profile management beyond login/update.

## Decisions

- The host is the stable public alias. This avoids another required name and keeps canonical URIs reconstructable.
- A dedicated input parser extracts `@username`; core `RemoteAddress` remains user-info-free. Treating user-info as a general URI field was rejected because it could leak into history and logs.
- Windows Credential UI owns the masked username/password buffers for the modal call; UI/model state never owns the password. Prompt persistence is disabled.
- The application coordinator probes the key, authenticates, writes Credential Manager first, atomically replaces profile JSON, refreshes provider/navigation state, then navigates once.
- Explicit Login constitutes first-key trust. Changed keys are blocked; silent replacement is rejected.

## Risks / Trade-offs

- [Host used as alias can collide with an old alias] → update only the exact host alias and preserve all other profiles.
- [Credential succeeds but JSON write fails] → remove the just-written credential or retain the prior complete profile/credential pair.
- [Runtime provider was constructed before login] → add an application-owned refresh/reconfiguration seam.
- [Secret leaks through UI/debug] → use a secret wrapper/masked input and redaction tests.

## Migration Plan

Existing profiles remain readable. New direct-host profiles append/update the same versionless JSON array. Rollback leaves profile metadata and Credential Manager entries intact but unused.

## Open Questions

None; username precedence is explicit hint, then saved username, then empty, and port defaults to 22.
