# Native context-menu baseline

Recorded: 2026-08-30T00:16:34.0876900+08:00

## Request and execution call graph

1. `explorer-ui/src/lib.rs` dispatches `ExplorerAction::ShowContextMenu` and releases GPUI pointer capture.
2. `explorer-ui/src/state.rs::begin_context_menu_request` resolves the selected target, keeps/clears selection as appropriate, and creates `ExplorerCommand::ShowContextMenu`.
3. The app service submits the command to the Shell STA context-menu worker.
4. `explorer-shell-win/src/context_menu.rs::show_with_deferred_replay` captures the cursor, creates the hidden owner, resolves `IContextMenu`, queries the HMENU, adds host commands, and calls `TrackPopupMenuEx`.
5. A selected Shell command is invoked through `invoke_host_owned`; host-owned Paste/Bookmark/Open commands return typed outcomes.
6. A cancelled menu destroys the owned HMENU before scheduling `DeferredMenuReplay`.

## Owner-message routing

`MenuOwnerState` contains the active `IContextMenu3`. The hidden owner window forwards `WM_INITMENUPOPUP`, `WM_DRAWITEM`, `WM_MEASUREITEM`, and `WM_MENUCHAR` to `IContextMenu3::HandleMenuMsg2`; unhandled messages use `DefWindowProcW`. Existing controlled owner-draw tests cover reentrancy and release ordering.

## Terminal resource paths

- Selection: `TrackPopupMenuEx` returns an ID, popup remains owned through lookup, then command invocation and owner/menu drop.
- Cancellation: popup is explicitly dropped before deferred replay scheduling.
- Requested verb: bypasses popup tracking and invokes the resolved canonical offset.
- Query/deadline error: Rust ownership drops menu, owner, and COM interfaces on return.
- Worker panic: outer worker boundary converts panic to a typed failure and releases apartment-owned values during unwind.
- Right-click replay: the low-level hook records only the latest complete owner-matched gesture; replay is posted after the old popup is destroyed.

## Visual isolation seams

- Custom remote menu rows and surface: `chrome.rs::remote_menu_text_command` and `chrome.rs::remote_context_menu`.
- File-list selection/hover colors: `chrome.rs::file_row_visual` and file-row render code near the details listing.
- Semantic colors: `theme.rs::SemanticColors`.

The remote style change must not edit `file_row_visual`, listing-row background/selection projections, or provider row text colors.

## Baseline hashes

- `context_menu.rs`: `FE512C9A611BE34D5108930BA16DB6E5D54B8FBEBC7F7978A1B5808709047EC3`
- `chrome.rs`: `6CCEFF25569709575A03A5DB5B1956CEB85EE6E1F693A128369C75D430877F64`

## Baseline tests

- `cargo test -p explorer-shell-win context_menu --lib`: PASS, 18 passed, 0 failed, 1 ignored because it requires an installed 7-Zip Shell extension.
- `cargo test -p explorer-ui remote_ --lib`: PASS, 11 passed, 0 failed.
