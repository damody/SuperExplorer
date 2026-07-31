# Shell context menu and locked-delete parity design

Date: 2026-07-29
Scope: full Windows Shell context menus, locked-file diagnostics, graceful process close, retry, and final UTIT closure

## Goals

1. A file, folder, multi-selection, or background context menu exposes the complete Windows Shell command set available for that target, including installed third-party extensions, separators, disabled/default state, owner-drawn items, and nested submenus.
2. Normal right-click shows the complete classic Shell menu. Shift+right-click additionally requests extended-only verbs.
3. A recycle or permanent delete that fails because a resource is in use identifies the current locking applications and offers an Explorer-like, explicit “Close programs and retry” flow.
4. No GPUI callback performs Shell COM, Restart Manager, process inspection, IPC, or blocking waits.
5. Final UTIT execution is a release gate; product failures are fixed and the relevant cases rerun before the complete gate is accepted.

## Chosen approach

### Context menu

Keep the existing native `IContextMenu3` popup inside the disposable broker worker. Extend the typed request with an invocation profile rather than serializing a menu tree back into GPUI. Item menus query with the public Explorer/item/synchronous-cascade flags in addition to rename support. Background menus use the applicable Explorer profile without item-only flags. Shift state adds `CMF_EXTENDEDVERBS`; ordinary invocation does not.

This preserves handler-owned submenu population, owner drawing, canonical command identifiers, keyboard navigation, icons, focus, and invocation semantics. The worker continues forwarding `IContextMenu2/3` messages during the modal loop and returns only a bounded terminal outcome.

Rejected alternatives:

- Rebuilding the menu in GPUI would lose owner-drawn content, lazy third-party submenus, native accessibility, canonical command offsets, and handler message routing.
- Always enabling extended verbs would differ from Explorer and could expose maintenance/debug commands during ordinary right-click.
- Returning arbitrary menu payloads over IPC would increase the attack surface and still fail to preserve provider-owned native state.

### Locked delete

Classify delete failures by Windows error/HRESULT. Only sharing and lock violations start lock-owner discovery. A background Windows adapter uses Restart Manager (`RmStartSession`, `RmRegisterResources`, `RmGetList`, `RmEndSession`) to return a bounded, owned list containing PID, application/service display name, application type, session identity, restart capability, and shutdown eligibility. It never exports native handles.

The reducer binds the result to the failed operation request, tab generation, and exact item identities. Late, cancelled, duplicate, or post-navigation results are ignored. The modal lists lock owners and offers:

- **Close programs and retry**: request graceful Restart Manager shutdown only for eligible, same-or-lower-integrity non-system processes, wait within a fixed deadline, then resubmit the original delete exactly once.
- **Retry**: retry without closing applications.
- **Cancel**: close the modal and leave files untouched.

SuperExplorer, its broker/workers, system/critical/protected processes, elevated processes not controllable by the current token, services without an allowed graceful Restart Manager path, and processes whose identity changed through PID reuse are never closed. There is no `TerminateProcess` fallback. Partial shutdown or delete remains visible and retryable.

## Typed contracts and data flow

1. Pointer/keyboard input creates `ContextMenuRequest` with target, owner, screen point, invocation source, and `extended_verbs`.
2. App service serializes the bounded request to the broker; the worker resolves the exact parent/child Shell identities, queries and shows the native popup, invokes the chosen offset, and emits exactly one terminal result.
3. `IFileOperation` maps lock-related failure to a structured delete terminal containing the original request identity instead of parsing localized error text.
4. UI requests lock owners through a background command. The Windows adapter returns `LockOwnerDiscoveryTerminal` with a bounded owner list or a typed unavailable/error result.
5. A confirmed close action carries the discovery generation and selected process start identities. The adapter revalidates every process, requests graceful shutdown, reports per-process outcomes, and the reducer retries the original delete only when safe.

## Error, focus, and accessibility behavior

- Escape or outside click dismisses the native context menu through its normal modal semantics.
- Context-menu timeout/crash returns focus to the originating file view and leaves navigation usable.
- The locked-file dialog is modal, traps Tab within its actions and process list, has UIA dialog/list/status roles and names, announces discovery and close results, and returns focus to the original item after Cancel or completion.
- Empty or unavailable Restart Manager results show the ordinary delete failure with Retry/Cancel; they never claim that no process owns the file.
- Process paths and command lines are not displayed or persisted. Diagnostics use PID/start identity plus redacted application metadata.

## Bounds and security

- Maximum registered resources, returned owners, IPC bytes, retries, discovery duration, and shutdown duration are centralized and non-zero bounded.
- Lock discovery and shutdown cancellation are tied to tab/window shutdown.
- Process identity includes creation time to prevent PID-reuse mistakes.
- No external process is closed without a visible, explicit user action.
- No force termination, privilege elevation, credential persistence, or system-wide handle enumeration is introduced.

## Tests and evidence

- Unit tests for invocation profiles, Shift mapping, target-specific flags, generation suppression, lock HRESULT classification, bounded owner lists, PID reuse, protected/elevated/self exclusion, close/retry/cancel, partial outcomes, and exactly-once retry.
- Controlled Restart Manager fixture with an owned helper that holds a test file open, accepts graceful close, and proves the subsequent recycle/permanent delete succeeds.
- Negative fixtures for stale owner, denied shutdown, unresponsive owner, cancellation, multiple owners, mixed eligible/ineligible owners, and non-lock delete failure.
- Real Shell differential captures ordinary and Shift menus for file, folder, multi-selection, and background; validates command count/labels against direct Shell query and verifies installed 7-Zip/WinRAR/TortoiseGit entries when available, otherwise records a prerequisite skip.
- Headful UTIT validates pointer/keyboard menu dismissal, submenu activation, locked-delete UIA/focus, Close-and-retry, Retry, and Cancel.
- Final gates: format, locked all-target check, Clippy warnings denied, workspace tests/doc tests, architecture/OpenSpec/manifest coverage, relevant interop/headful cases, then one complete UTIT run. Any discovered product failure is fixed and rerun before acceptance.

## Rollback

The context-menu invocation profile can fall back to the previous normal Shell query without changing model identity. Lock-owner discovery can be disabled independently; delete then retains the existing safe failure notice and never closes another process. Neither rollback changes files or user data.
