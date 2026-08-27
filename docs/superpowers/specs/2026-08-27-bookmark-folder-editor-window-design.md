# Bookmark Folder Editor Window Design

## Problem

The bookmark-folder rename form is rendered as an overlay inside the explorer or bookmark-manager window. That overlay can leave the owner window unable to interact and does not match the required native-window interaction.

## Decision

Create a dedicated, non-resizable GPUI `WindowKind::Normal` for bookmark-folder creation and rename. The existing bookmark-folder draft remains the single source of truth. Main and manager windows only dispatch the add/edit action; the application-owned observer opens or activates one editor window.

The alternatives were retaining the overlay (rejected because it reproduces the freeze), or opening an unlimited transient window for every action (rejected because the global draft permits only one coherent edit). A single reusable native editor window matches the existing bookmark action and manager window lifecycle.

## Interaction and data flow

- Add creates and persists the default folder, starts its rename draft, then opens the editor window.
- Rename starts a draft and opens the editor window without placing an input or modal layer in the owner window.
- Text changes update the existing draft through the owner `ExplorerRoot`.
- Save uses the existing reducer and persistence rollback path. A validation or persistence failure keeps the window and text available.
- Cancel, Escape, and closing the window cancel the draft without changing the stored name.
- Reopening while an editor exists replaces its snapshot, selects the current name, and activates it.

## Scope and verification

The old folder-editor overlay and its duplicate inputs are removed from both host windows. Folder deletion confirmation is unchanged. Source-contract tests verify native-window construction, reducer routing, and absence of the old overlay; focused bookmark tests and an application compile check cover integration.
