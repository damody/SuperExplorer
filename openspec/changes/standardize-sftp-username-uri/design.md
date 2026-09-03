## Context

The remote model parses SFTP input separately from its canonical `RemoteAddress`. It currently splits `host@username`, removes the transient username hint, and stores host-only addresses. UI navigation intercepts that input before profile resolution, while profiles, bookmarks, and history use the canonical remote address. This change crosses those layers and must retain existing host-only persisted data without putting credentials into location metadata.

## Goals / Non-Goals

**Goals:**

- Make `username@host` the sole username-bearing SFTP URI interpretation.
- Preserve host-only profile aliases and persisted locations.
- Keep passwords outside URI parsing, logs, descriptors, history, bookmarks, and credential metadata.

**Non-Goals:**

- Change SFTP authentication methods, credential-vault storage, host-key policy, or profile-alias persistence.
- Infer or migrate the legacy reversed `host@username` convention.
- Add URL percent-encoding semantics beyond existing validated username and host values.

## Decisions

### Parse user-info before host

`SftpAddressInput::parse` shall split a single `@` into `(username, host)` and create a host-only `RemoteAddress` for profile lookup. This supports standard input while preserving the opaque stable profile identity. The rejected alternative is a heuristic that accepts both orders; hostnames and usernames overlap enough that it can silently target the wrong server.

### Centralize call sites

Address navigation and login prefill must call the shared parser. This avoids a second authority swap in UI code.

## Risks / Trade-offs

- [Legacy reversed input becomes semantically different] → Document it as breaking, do not guess, and test the new parser order.
- [Existing host-only entries could stop resolving] → preserve host-only parsing and the existing credential-free canonical location.
- [Password accidentally reaches diagnostics] → parser rejects `:` in user-info and tests assert URI outputs contain no password.
- [Profile aliases differ from host names] → profile resolution remains alias-based after the parser supplies the host.

## Migration Plan

No stored credential or profile migration runs. Existing `sftp://host/path` data continues through the host-only path. New direct input uses the standard user-info order and is then canonicalized to the existing host-only location. Rollback is code-only: host-only saved data remains unaffected.

## Open Questions

None. The approved design intentionally treats legacy reversed authorities strictly as the new standard order to avoid unsafe inference.
