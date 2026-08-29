## Why

The bookmark manager is currently rendered as a non-interactive overlay inside the Explorer surface, so its apparent edit, delete, reorder, and close controls cannot reliably receive focus or input. Filesystem bookmark targets are also read-only, preventing users from preserving offline, remote, virtual, not-yet-created, or intentionally invalid paths for later repair.

## What Changes

- Replace the in-surface bookmark manager overlay with a dedicated, focusable GPUI native window.
- Present bookmark-item right-click commands in the same compact inline menu style as logical bookmark folders, with dedicated delete confirmation.
- Replace the bookmark-folder rename overlay with a dedicated native editor window.
- Allow left-button toolbar bookmark dragging into logical folders and back to the root.
- Make left-click folder menus content-only and reserve folder mutation commands for right-click.
- Give Local, ADB, SFTP, and Lua bookmarks consistent source-specific icons.
- Dismiss bookmark folder and overflow menus immediately when a bookmark is selected.
- Show a solid focus-blue star for the current bookmarked location and open its compact dedicated editor when clicked.
- Present custom remote-file context commands in a square, classic Windows vertical menu style without changing command membership.
- Add only currently actionable remote item commands: folder new-tab opening, canonical URI copy, and bookmark creation.
- Add bookmark-toolbar background and item context menus for creating, renaming/editing, and deleting logical folders and path bookmarks.
- Make file and folder bookmark target text editable in the dedicated bookmark editor window.
- Persist non-empty path text without existence or parse validation, preserving the exact user-authored value; report errors only when activation fails.
- Reuse the existing durable bookmark mutation and rollback path for all window and context-menu operations.
- Keep web URLs, synchronization, import/export, and filesystem mutation outside this change.

## Capabilities

### New Capabilities

- `bookmark-manager-window`: Dedicated native-window lifecycle and interactive bookmark-tree management.
- `bookmark-toolbar-context-management`: Background, logical-folder, and bookmark context-menu CRUD from the toolbar.
- `editable-bookmark-paths`: Exact, editable, persistable filesystem/remote/virtual path text, including unavailable or invalid targets.
- `bookmark-action-window`: Superseded by the inline bookmark context-menu behavior in this change.
- `bookmark-inline-context-menu`: Folder-style compact right-click commands for bookmark entries.
- `bookmark-folder-editor-window`: Interactive bookmark-folder naming in a dedicated singleton native window.
- `bookmark-toolbar-folder-drag`: Firefox-style bookmark organization by native drag and drop.
- `bookmark-folder-content-menu`: Firefox-style left-click browsing with right-click management separation.
- `bookmark-provider-icons`: Shared provider-aware bookmark icon projection.
- `bookmark-menu-dismissal`: Deterministic browse-menu dismissal before bookmark activation.
- `bookmarked-location-star-editor`: Stateful current-location star and compact dedicated edit window.
- `classic-remote-context-menu`: Traditional vertical typography and geometry for the custom remote fallback menu.
- `remote-file-context-commands`: Capability-aware ADB/SFTP item commands backed by existing reducers.

### Modified Capabilities

None. Bookmark capabilities exist only in active historical changes and have not been promoted into `openspec/specs`.

## Impact

The change affects `explorer-model` bookmark payload serialization, `explorer-ui` bookmark state/actions/chrome and dedicated window components, `explorer-app` child-window creation and ownership, session persistence compatibility, and focused model/UI/application tests. No new dependency, external service, network permission, or destructive filesystem behavior is introduced.
