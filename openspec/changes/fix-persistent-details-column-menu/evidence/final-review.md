# Final review

## 4.2.1 Implementation review

The Details column chooser now uses an opt-in persistent-toggle contract on the application-owned immersive popup:

- Native check state is stored on `HMENU` (`MF_CHECKED`) and painted as a check mark in the icon gutter.
- Persistent rows toggle and republish `{session_id, command_index, checked}` without ending the message loop.
- Terminal rows (auto-size, folder-size bar, code-lines detail) still close the popup.
- The popup worker never borrows GPUI. A bounded `sync_channel` carries events to `ExplorerRoot`, which applies `SetDetailsColumnVisibility` on the foreground context.
- `Name` stays checked and disabled. Stale or disconnected events are ignored. Publication failure closes the popup.

No filesystem context-menu command semantics, column registry order, or settings schema changed.

## 4.2.2 User workflow review

The supplied screenshot showed a Details header menu with no check marks and no way to toggle columns in place. Headful evidence now shows:

1. Visible columns render `✓` (Name, Date modified, Type, Size).
2. Clicking Size removes the check, keeps the same HWND, and leaves the popup open.
3. Clicking Size again restores the check.
4. Clicking Name does not hide it.
5. Escape, outside click, and auto-size still dismiss.

This matches File Explorer's repeated check/uncheck interaction.

## 4.2.3 Disposition

Zero open product findings. The 1000-cycle soak's process-wide `PrivateUsage` counter is noisy in this environment (handles stayed within +1); HWND/session cleanup is covered by the persistent activation tests and headful dismissal path.

Reviewed: 2026-09-07
