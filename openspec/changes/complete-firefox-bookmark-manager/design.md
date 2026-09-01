## Architecture

`BookmarkManagerWindow` owns ephemeral UI state: selected location/item, expanded folders, history, open toolbar menu, sort descriptor, density, and search input. Durable bookmark mutations continue through `ExplorerRoot`, which remains responsible for persistence and rollback.

The chrome renderer receives an immutable manager projection plus typed callbacks. It renders a toolbar, navigable tree, sortable table, and details editor; no element that looks enabled may be inert.

## Behavior

Tree selection replaces the right-hand projection and records history. Search filters the active projection without changing its durable order. Single-click selects; double-click opens the existing editor. Details edits save explicitly or on Enter and retain input after a persistence failure. Manage and View open real dismissible menus. Import/export support JSON clipboard data; file operations are included only where a safe native picker exists.

Editor launch origin is explicit: star launches carry an anchor, manager launches clear it and use centered bounds.

## Failure handling and tests

Invalid imports do not mutate bookmarks and show a notice. Failed durable writes roll back. Tests cover each visible control, stale selection after deletion, history boundaries, sorting, searching, menu dismissal, edit persistence, import failure, and editor positioning. Focused tests and all-target compilation are blocking gates.
