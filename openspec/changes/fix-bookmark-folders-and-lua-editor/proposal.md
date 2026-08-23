## Why

The completed bookmark/Lua feature regressed in real UI use: opening the `+` Lua editor produces a modal without usable fields. Bookmarks also remain a flat list, so users cannot organize their favourites into Firefox-style folders or choose a destination when saving the current folder.

## What Changes

- Repair the Lua bookmark editor so its text fields are visible, focusable, editable, cancellable, and safely persisted.
- Replace flat bookmark placement with persistent bookmark folders and nested entries, including lossless upgrade of existing sessions.
- Add right-click management for bookmark folders: create, rename, add a subfolder, and delete with a non-empty-folder confirmation.
- Replace the star's immediate toggle with a bookmark editor that chooses a destination folder, edits an existing bookmark, or removes it.
- Make the selected-file/folder "add bookmark" action use the same destination picker and update toolbar, overflow, manager, and Quick access projections for folders.

## Capabilities

### New Capabilities

- `bookmark-folder-management`: Persistent nested bookmark folders and their accessible context-menu management.
- `bookmark-destination-editor`: Firefox-style bookmark save/edit dialog with a folder picker and removal action.

### Modified Capabilities

- `bookmark-toolbar`: Typed bookmarks become folder-aware and star/add actions open the destination editor.

## Impact

- Affects `explorer-model` bookmark/session contracts, `explorer-ui` state/actions/chrome/navigation projection, `explorer-app` session lifecycle, focused tests, and headful UI evidence.
- Lua runtime permissions and the removal of `.explorer.lua` automation are unchanged.
