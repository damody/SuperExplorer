# Bookmark Provider Icons Design

## Goal

Give bookmark entries a bookmark-specific icon and visually distinguish Local, ADB, and SFTP targets consistently across every bookmark projection.

## Design

Add one presentation helper that classifies a `BookmarkTarget` as Local, ADB, SFTP, or Lua. Structured locations use their filesystem/provider identity. Raw editable paths use only a case-insensitive `adb://` or `sftp://` prefix; they are never existence-validated. Local bookmarks render `🔖`, ADB renders `🤖`, SFTP renders `⇄`, and Lua retains `⚡`. Logical bookmark folders remain `📁`.

Toolbar entries, overflow entries, left-click folder content, manager rows, and bookmark navigation rows call the same helper. Centralizing classification avoids divergent icons and preserves arbitrary erroneous path data.

## Verification

Unit tests cover structured and raw Local/ADB/SFTP classification plus Lua. Source-contract tests verify all bookmark projections use the shared helper. Focused bookmark tests and application compilation cover integration.
