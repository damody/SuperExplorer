# Bookmark Inline Context Menu Design

## Goal

Make bookmark right-click use the same compact inline context-menu style as logical bookmark folders instead of opening the large Bookmark Action native window.

## Design

Store an active bookmark ID plus pointer coordinates in root view state. Every bookmark projection dispatches the existing open-context action; the reducer validates the ID and opens one inline menu while closing folder, overflow, and folder-context popups. Chrome renders commands with the same positioning, surface, border, radius, spacing, hover, and danger colors used by the folder context menu.

Commands are Open, Open in New Tab for folder targets, Edit Name and Path, and Delete. Open/edit dispatch immediately and dismiss the menu. Delete dismisses the menu and opens the existing dedicated confirmation window. Clicking outside, Escape, stale IDs, or another context menu dismisses it. The previous `BookmarkActionWindow` remains unused by right-click and can be removed in a later cleanup without widening this behavioral change.

## Verification

State tests cover validated open/close and popup exclusivity. Source/render tests cover shared styling, command applicability, dismissal, and absence of action-window presentation from the right-click reducer. Focused bookmark tests and application compilation cover integration.
