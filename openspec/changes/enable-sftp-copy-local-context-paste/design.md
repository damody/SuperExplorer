## Context

The approved source design is `docs/superpowers/specs/2026-08-27-sftp-copy-local-context-paste-design.md`. `RemoteExplorerService` already stores SFTP copy intent and `TransferEngine` already supports virtual-to-filesystem download. `ExplorerState::begin_paste_request` already uses the active history location. The gap is that the disposable native item menu cannot see application clipboard state and normally exposes no Paste verb.

## Goals / Non-Goals

**Goals:**

- Show an enabled Paste command in every writable current-folder context menu when clipboard state is usable.
- Route native and custom menu Paste through the existing application action.
- Preserve active-folder destination regardless of the context hit.
- Preserve existing remote transfer, collision, cancellation, and cut cleanup semantics.

**Non-Goals:**

- Pasting into the clicked child folder.
- Replacing the native Shell context menu.
- Changing SFTP credentials, remote provider APIs, external Windows clipboard formats, or transfer conflict policy.

## Decisions

1. `ContextMenuHostCommand::Paste` is an internal, stable named command. It never invokes an extension handler in the disposable worker.
2. `ContextMenuRequest` carries `paste_available`, computed by the UI from clipboard validity plus current-presentation writability. The worker uses this immutable fact only to compose the popup.
3. The native menu appends a custom Paste item after `IContextMenu::QueryContextMenu` for both background and item targets. Its reserved ID is outside the Shell-reported command offsets and is recognized structurally before native invocation.
4. The custom remote menu includes Paste in both target shapes. Runtime action enablement and `begin_paste_request` remain the final authorization boundary.
5. Paste mapping ignores the delegated Shell target and builds its destination from active tab history. This exactly matches command-bar and keyboard paste.
6. Internal remote clipboard state takes precedence over asynchronous external clipboard staging, so an immediate local Paste does not race the staging download.

Alternatives rejected: always showing Paste would advertise an unusable command; invoking a native Shell Paste would lose remote clipboard intent; treating a clicked folder as destination contradicts the approved interaction contract.

## Risks / Trade-offs

- [Custom command ID overlaps Shell offsets] → Allocate after the returned command count, bounds-check the reserved range, and detect the exact ID before calling `InvokeCommand`.
- [Clipboard changes while native menu is open] → Treat `paste_available` as a display snapshot; `begin_paste_request` rechecks current clipboard state before submission.
- [Remote staging is incomplete] → Route internal remote clipboard directly through `TransferEngine`, never through staged `CF_HDROP` for in-app paste.
- [Request shape affects broker fixtures] → Update every constructor and wire/round-trip test, then run focused broker and context-menu tests.

## Migration Plan

No data migration is required. Add the internal enum/request fields, update all constructors, then compose and delegate Paste. Rollback removes those additions without touching stored clipboard or remote data.

## Open Questions

None.
