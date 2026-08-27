## Why

The bookmark manager is currently rendered as a non-interactive overlay inside the Explorer surface, so its apparent edit, delete, reorder, and close controls cannot reliably receive focus or input. Filesystem bookmark targets are also read-only, preventing users from preserving offline, remote, virtual, not-yet-created, or intentionally invalid paths for later repair.

## What Changes

- Replace the in-surface bookmark manager overlay with a dedicated, focusable GPUI native window.
- Replace bookmark-item overlay context menus with a singleton native action window that requires explicit selection and confirmation.
- Replace the bookmark-folder rename overlay with a dedicated native editor window.
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
- `bookmark-action-window`: Confirmed bookmark-item right-click commands in a dedicated singleton native window.
- `bookmark-folder-editor-window`: Interactive bookmark-folder naming in a dedicated singleton native window.

### Modified Capabilities

None. Bookmark capabilities exist only in active historical changes and have not been promoted into `openspec/specs`.

## Impact

The change affects `explorer-model` bookmark payload serialization, `explorer-ui` bookmark state/actions/chrome and dedicated window components, `explorer-app` child-window creation and ownership, session persistence compatibility, and focused model/UI/application tests. No new dependency, external service, network permission, or destructive filesystem behavior is introduced.
