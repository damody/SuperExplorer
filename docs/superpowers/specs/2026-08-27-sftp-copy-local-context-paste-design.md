# SFTP Copy to Local Context Paste Design

## Goal

After copying items in an SFTP folder, every context menu opened in a writable local folder shall expose Paste when the application clipboard is usable. Paste always targets the current tab's folder, regardless of whether the context click hit the background, a file, or a child folder.

## Existing Flow

`RemoteExplorerService` already owns remote copy intent and routes a later paste through `TransferEngine`. A virtual source and filesystem destination are downloaded directly through the selected remote provider. `ExplorerState::begin_paste_request` already derives its destination from the active tab history, so it does not use the context-menu hit item. The missing behavior is context-menu projection: a native item menu normally has no Paste command, while the custom remote item menu currently omits Paste.

## Design

- Add `Paste` to `ContextMenuHostCommand` with the stable wire name `paste`.
- Add a `paste_available` fact to `ContextMenuRequest`. The UI computes it from an owned/external clipboard plus a writable current presentation.
- The isolated native Shell menu appends one host-owned Paste command when `paste_available` is true. It does this for background and item targets, and delegates selection to the application instead of invoking a Shell handler.
- The remote custom menu exposes Paste for background and item targets. Existing action enablement remains authoritative.
- Map the delegated host command to `ExplorerAction::Paste`. The action calls the existing `begin_paste_request`, preserving current-folder destination, operation-center tracking, conflict handling, cancellation, partial outcomes, and remote cut semantics.
- Do not reinterpret a clicked directory as the destination. Selection may change for the context gesture, but paste destination remains the active history location.

## Failure and Lifetime Rules

- A stale, unsupported, or empty clipboard does not produce an enabled native Paste item.
- A read-only current location does not produce an enabled native Paste item.
- Remote clipboard intent remains authoritative even if asynchronous publication of the external Windows clipboard staging object is incomplete.
- Download, conflict, cancellation, and partial failures use existing typed operation terminals and never delete copy sources.

## Verification

- Model wire-name round-trip includes Paste.
- Shell menu tests cover appended/delegated Paste for background and item targets and absence when unavailable.
- UI tests prove background/file/folder context hits all submit Paste to the active history location.
- Remote-service tests prove copied SFTP items route to a filesystem destination through the internal remote clipboard.
- Run focused tests, `cargo fmt --all --check`, and locked/offline workspace checks.
