## Context

Production currently places eight DLLs in `$INSTDIR\plugins` and loads them through fixed shortcut arguments even though the SDK already publishes complete `.sepack` archives and the host owns bounded import, validation, sealing, resolution, desired-state admission, durable callback markers, and native lifecycle. This change makes `.sepack` the only automatic production source and retains direct DLL loading solely for explicit development tests.

The authoritative product design is `docs/superpowers/specs/2026-08-27-plugin-directory-autoload-safe-mode-design.md`.

## Goals / Non-Goals

**Goals:**

- Discover executable-relative, direct-child `.sepack` archives deterministically at startup.
- Enable a valid newly discovered package by default and honor persisted global/package/feature overrides.
- Make caught panics, abnormal callback exits, and stale markers latch fail-closed global Safe Mode.
- Expose explicit recovery in Folder Options while preserving individual desired state.
- Migrate installed layout and shortcuts without breaking explicit development DLL loading.

**Non-Goals:**

- Recursive scanning, loose-DLL production discovery, filesystem watching, or hot-loading Rust DLLs.
- Automatic retry after a fault, deletion of plugin packages, or resetting individual switches.
- Loose-DLL automatic discovery, recursive scanning, symlink traversal, or bypassing package/ABI validation.

## Decisions

### Production source and admission

The application resolves `plugins` from the executable parent, inspects at most 1,024 direct entries, ignores symlinks/non-files/non-`.sepack` content, and supplies sorted archives to the existing host import configuration. The importer extracts into private staging, validation seals accepted content, resolution applies dependencies and desired state, and native lifecycle admits eligible roots. Only the validated manifest supplies package identity. Explicit `--plugin-dll` paths remain a separate development/test source.

### Desired-state persistence

`feature-state-v1.json` remains the only persisted user intent store. Absence means enabled, including packages first seen after a file copy or upgrade. Folder Options uses a snapshot/draft transaction; Apply/OK atomically saves before runtime transition, while Cancel leaves disk untouched. Effective state is never serialized.

### Global Safe Mode latch

A separate checksummed/versioned file under the extension host state root records `{latched, incident_kind, incident_id}` without paths or plugin payload. Before production discovery/admission, startup reconciles stale durable markers into this latch. Caught unwind boundaries latch before returning a typed plugin failure. Unreadable, corrupt, unsupported, or unsuccessfully replaced latch state fails closed for plugin execution.

While latched, the host may parse and validate metadata for a non-executing catalog but MUST NOT call DLL entrypoints, Lua registrars, skin code, or bundled tools. The latch dominates desired state and existing per-package quarantine.

### Recovery

Extensions options exposes an explicit confirmation action. Successful recovery atomically records the latch as clear and removes only reconciled incident markers. It preserves global/package/feature desired state and reports restart required; no Rust plugin is hot-loaded. A failed write or marker cleanup leaves the latch active.

### Adaptation and evidence

A-level refinements may split tasks or adjust commands without changing requirements or gates. B-level corrections within scope update design/spec/tasks and invalidate dependent evidence. C-level changes to discovery roots, trust policy, fault threshold, recovery authority, required gates, external writes, or destructive behavior require user approval. Gates cannot be silently weakened.

## Risks / Trade-offs

- **One faulty plugin disables healthy plugins** → This is intentional fail-closed policy; the catalog identifies the incident and preserves individual choices for controlled recovery.
- **Package migration changes installer layout** → Build/package validation verifies every bundled package and an installed-path upgrade test proves old shortcut compatibility and new shortcut replacement.
- **Fault while writing the latch** → Durable markers remain the fallback and startup treats ambiguity as latched.
- **Directory replacement or reparse attacks** → Resolve from the executable, reject reparse points, validate handles/content, and retain sealed admission.
- **User copies only a DLL** → It remains unloaded and appears as an invalid candidate diagnostic; documentation requires a complete package directory.

## Migration Plan

1. Add executable-relative `.sepack` discovery/latch composition and keep explicit DLL arguments compatible for development.
2. Build and package each bundled fixture, install the resulting `.sepack`, and rewrite shortcuts without Plugin arguments.
3. Roll out with import/admission and restart tests. Rollback may restore explicit arguments while retaining state files; no user state deletion is required.

## Open Questions

None. Product policy decisions are fixed by the authoritative design.
