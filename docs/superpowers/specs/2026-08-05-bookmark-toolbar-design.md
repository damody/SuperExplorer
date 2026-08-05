# Firefox-style bookmark toolbar design

## Goal

Add a persistent Firefox-style bookmark toolbar to SuperExplorer. Bookmarks may target a folder, a file, or a user-authored Lua command. Lua commands run on demand against the current filesystem folder. Replace, rather than extend, the existing directory-local `.explorer.lua` automation system.

## Scope

### Bookmark model and persistence

Introduce an application-owned bookmark collection in the persisted user session/configuration. Each bookmark has a stable ID, user-visible name, type, ordering value, and one type-specific payload:

- `Folder`: target location.
- `File`: target location.
- `LuaScript`: Lua source text.

Bookmark mutations update the live state, then use the existing background session persistence path. Restoring a session restores every bookmark and its order. Missing targets remain bookmarked so users can repair or remove them later.

### Toolbar and management UI

Render the toolbar below the address bar. The toolbar shows bookmarks in their persisted order, using distinct folder, file, and Lua/script-lightning icons. When width is insufficient, overflow entries move into a More Bookmarks menu.

Right-clicking a filesystem file or folder offers Add to Bookmarks. A toolbar `+` action creates a Lua bookmark. A dedicated Bookmark Manager provides the complete management surface:

- list all bookmark types and drag to reorder them;
- edit names and filesystem targets;
- edit a Lua bookmark's source text;
- delete bookmarks.

Folder clicks navigate the current tab. File clicks use the existing Windows Shell open behavior. Lua bookmark clicks run immediately and never navigate.

### On-demand Lua commands

Lua bookmarks use the existing built-in Lua runtime and asynchronous work scheduling infrastructure. They are manual commands, not folder-owned automation.

For each execution, SuperExplorer derives the current tab's filesystem directory and exposes it to Lua as the read-only `current_folder` string. No selected-item, file-operation, shell, or broader Explorer API is exposed by this feature. Non-filesystem locations reject execution with a clear notification. Runtime startup failures, Lua exceptions, and timeouts complete as non-blocking, user-readable failure results.

### Removal of legacy automation

Remove the existing automatic `.explorer.lua` discovery, loading, activation, lifecycle composition, UI bindings, and tests. The app must neither scan nor execute `.explorer.lua` on entering a folder. Existing user files are never changed or deleted; they simply become inert to SuperExplorer.

## Data flow

1. A user creates, edits, reorders, or removes a bookmark from the context menu, toolbar, or manager.
2. The UI validates the type-specific payload, updates bookmark state, then sends a session snapshot to the existing persistence coordinator.
3. Clicking a folder, file, or Lua bookmark dispatches to the corresponding handler.
4. A Lua handler verifies the current location is a filesystem folder, starts a bounded background job, injects `current_folder`, and publishes success or failure through the existing non-blocking status presentation.

## Error handling

- Preserve unavailable file and folder bookmarks and display an actionable open/navigation error.
- Refuse Lua execution outside a filesystem directory without starting a runtime.
- Surface runtime exceptions, startup failures, and timeouts without blocking the UI.
- Reject invalid persisted bookmark data safely; retain valid entries and report recovery through existing persistence diagnostics where appropriate.

## Verification

- Model and persistence tests cover serialization, restore, invalid data recovery, and stable order.
- UI/state tests cover typed icon selection, overflow, create/edit/delete/reorder operations, and context-menu eligibility.
- Command dispatch tests cover folder navigation, file opening, missing targets, and no navigation for Lua scripts.
- Lua execution tests cover `current_folder`, rejection for non-filesystem locations, success, error, and timeout presentation.
- Regression tests prove legacy `.explorer.lua` discovery and execution no longer occur.
