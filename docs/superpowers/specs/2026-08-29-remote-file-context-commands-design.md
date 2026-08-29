# Remote File Context Commands Design

## Scope

Extend the existing ADB/SFTP custom context menu only with commands backed by current SuperExplorer behavior: open a selected folder in a new tab, copy the canonical remote URI, and add selected items to bookmarks. Existing Open, Cut, Copy, Paste, Rename, and Delete remain unchanged.

Commands requiring missing contracts—destination picker download, Android intent/APK installation, SFTP chmod, SSH terminal execution, and custom remote properties—are not rendered until their backends exist. This prevents visible no-op commands.

## Projection

The context snapshot records the focused row and whether it is a container. The menu projects Open in New Tab only for a single container and dispatches the existing `OpenItem { new_tab: true }` action. Copy Remote URI dispatches `CopySelectedPaths`; Add to Bookmarks dispatches `AddSelectedToBookmarks`. All rows keep the classic square vertical style and fixed icon gutter.

## Verification

Tests cover item/container/background membership, action identity, separator placement, remote URI copying, bookmark creation compatibility, menu dismissal, focused UI tests, application compilation, formatting, and strict OpenSpec validation.
