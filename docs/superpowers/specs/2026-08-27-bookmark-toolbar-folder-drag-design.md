# Bookmark Toolbar Folder Drag Design

## Goal

Allow a path or Lua bookmark on the bookmark toolbar to be moved with the left mouse button into a logical bookmark folder, matching Firefox-style bookmark organization. Dropping on the toolbar background moves it back to the root.

## Design

Reuse GPUI's typed `BookmarkDrag` already used by the bookmark manager. Toolbar bookmark projections become drag sources; logical folder buttons and the toolbar root become drop targets. A new typed action carries the bookmark ID and destination parent folder. The authoritative bookmark model validates the parent, changes `parent_id`, appends the bookmark at the end of the destination, normalizes both sibling groups, and returns the existing rollback mutation.

Folder buttons stop drop propagation so the toolbar root cannot overwrite their destination. A valid target uses the existing hover styling as a drop cue. Same-parent drops are no-ops. This change moves bookmark entries only; dragging logical folders is excluded to avoid tree-cycle semantics outside the request.

## Failure and verification

Durable persistence uses the existing notification and rollback path. If persistence fails, the bookmark returns to its original folder and order. Model tests cover root-to-folder, folder-to-root, invalid target, same-parent no-op, ordering, and rollback. UI source-contract tests cover toolbar drag sources and both drop targets; focused bookmark tests and application compilation verify integration.
