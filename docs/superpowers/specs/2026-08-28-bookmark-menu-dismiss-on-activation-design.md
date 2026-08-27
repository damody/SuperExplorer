# Bookmark Menu Dismiss-on-Activation Design

## Goal

Selecting a bookmark from a folder or overflow menu must immediately dismiss that menu, including when activation later fails.

## Design

At the start of the authoritative `ActivateBookmark` reducer branch, synchronously clear both browse-only bookmark popups: the active folder menu and overflow menu. Only then resolve the bookmark and dispatch navigation, file launch, remote login, or Lua execution. This ordering makes dismissal independent of target validity and provider outcome.

Child-folder clicks keep using `ToggleBookmarkFolderMenu`, so drill-in navigation does not dismiss the panel. Right-click action windows and management context menus retain their separate lifecycle.

## Verification

State tests cover clearing both browse menus. Reducer/source tests verify dismissal precedes bookmark lookup and therefore also covers stale or invalid targets. Focused bookmark tests and application compilation cover integration.
