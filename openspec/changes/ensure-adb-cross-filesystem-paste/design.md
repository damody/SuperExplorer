## Context

The approved source design is `docs/superpowers/specs/2026-08-27-adb-copy-to-any-filesystem-design.md`. `TransferEngine` already selects Local→Local, Local→Virtual, Virtual→Local, and Virtual→Virtual paths from typed descriptors. `RemoteExplorerService` also records remote clipboard ownership synchronously before asynchronously preparing native Windows clipboard data. The remaining work is to ensure the context-menu path and test suite preserve those generic contracts for ADB sources and every writable registered destination.

## Goals / Non-Goals

**Goals:**

- Keep Paste availability provider-independent and fail closed for unwritable destinations.
- Prove ADB→Local and ADB→Virtual use the current folder and the correct source/destination providers.
- Preserve immediate internal clipboard Paste without waiting for native clipboard staging.
- Preserve scoped staging, conflicts, cancellation, item outcomes, and Cut cleanup.

**Non-Goals:**

- Adding providers, direct provider-to-provider streaming, or new provider methods.
- Changing native Windows clipboard formats or credential handling.
- Changing conflict or Move semantics.

## Decisions

1. **Use typed capability routing.** UI availability uses valid file clipboard state and the active presentation's write capability. Transfer dispatch uses `LocationDescriptor` and provider registry resolution. Provider-name allowlists are rejected because they make future registered filesystems unusable.
2. **Keep one transfer engine.** ADB→Local uses source download; ADB→SFTP or another Virtual provider uses source download to RAII staging followed by destination upload. Pair-specific implementations are rejected because they duplicate cancellation, conflict, and cleanup behavior.
3. **Test observable provider calls.** Fake ADB and destination providers record download/upload calls and bytes. This proves registry routing rather than merely checking a successful terminal event.
4. **Current folder remains authoritative.** Context hit testing can change selection but never the Paste destination; the active tab's current location is submitted unchanged.

## Risks / Trade-offs

- **Remote→Remote requires Local temporary storage** → retain existing quotas, free-space reserve, containment validation, cancellation, and RAII cleanup.
- **A permissive UI could expose Paste for an unusable provider** → require destination write capability and let registry/provider resolution fail explicitly if runtime availability changes after menu creation.
- **Asynchronous native clipboard staging can race Paste** → application-owned clipboard remains the first source of truth.

## Migration Plan

No data migration is required. Land focused tests and any minimal routing correction, then run formatting, workspace all-target checks, focused test suites, and strict OpenSpec validation. Rollback consists of reverting this change; no stored data or provider contract changes.

## Open Questions

None. Existing typed descriptors and provider capability contracts cover the requested destinations.
