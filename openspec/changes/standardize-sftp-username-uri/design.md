## Context

The remote model parses SFTP input separately from its canonical `RemoteAddress`. Before this change it split `host@username`, removed the transient username hint, and stored host-only addresses. UI navigation intercepts that input before profile resolution, while profiles, bookmarks, and history use the canonical remote address. This change crosses those layers and must retain existing host-only persisted data without putting credentials into location metadata.

The still-active `add-sftp-address-login` change and `docs/superpowers/specs/2026-08-26-sftp-address-login-design.md` document the reversed hint form. Those examples must be superseded in the same delivery so two active documents do not specify opposite orders.

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

Address navigation and login prefill must call the shared parser. This avoids a second authority swap in UI code. `ExplorerRoot::begin_address_navigation` forwards the raw string off the GPUI thread; `RemoteService::login_address` is the only application parser call site. Suggested username order remains: explicit hint, then saved profile username, then empty.

### Supersede reversed-order documentation

Do not leave `sftp://45.32.49.125@root/` as an accepted username-hint example in `add-sftp-address-login` or the 2026-08-26 design. Update those WHEN/example clauses to `sftp://root@45.32.49.125/` without changing credential, host-key, or persistence behavior.

## Risks / Trade-offs

- [Legacy reversed input becomes semantically different] → Document it as breaking, do not guess, and test the new parser order.
- [Existing host-only entries could stop resolving] → preserve host-only parsing and the existing credential-free canonical location.
- [Password accidentally reaches diagnostics] → parser rejects `:` in user-info and tests assert URI outputs contain no password.
- [Profile aliases differ from host names] → profile resolution remains alias-based after the parser supplies the host.

## Migration Plan

No stored credential or profile migration runs. Existing `sftp://host/path` data continues through the host-only path. New direct input uses the standard user-info order and is then canonicalized to the existing host-only location. Rollback is code-only: host-only saved data remains unaffected.

## Open Questions

None. The approved design intentionally treats legacy reversed authorities strictly as the new standard order to avoid unsafe inference.
