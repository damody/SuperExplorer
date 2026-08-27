# Firefox-Style Bookmark Folder Menu Design

## Goal

Make a left click on a bookmark toolbar folder browse folder contents only. Rename, create-child, and delete commands must appear only in the existing right-click management menu.

## Design

The left-click panel lists only the active folder's immediate child folders and bookmarks. Child folders render as `📁 name ›` and switch the panel to that folder when clicked; bookmarks retain their normal activation behavior. The panel contains no folder title row that looks actionable and no mutation commands. Right-click continues to open the existing context menu with management commands.

This single-panel drill-in approach preserves the current one-active-folder state and avoids adding fragile nested popup positioning. Retaining the mixed panel is rejected because it violates pointer-button semantics; a multi-column hover cascade is deferred because it requires new popup geometry and dismissal state beyond this correction.

## Verification

Source-contract tests assert that left-click folder content contains child-folder navigation and excludes rename/add/delete IDs, while the right-click context retains those actions. Focused bookmark UI tests and application compilation cover integration.
