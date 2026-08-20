## Context

`TaskbarSettings` already owns a persisted `TaskbarAlignment::{Left, Center}` value, defaults it to Left, and exposes it through the Taskbar settings window. `TaskbarView` uses the value to arrange buttons. The Start popup is opened by a shared taskbar callback, but `start_window_geometry` always centers it and the callback currently captures taskbar row count at taskbar construction time.

The implementation must remain correct for preview and owned-shell modes, multiple monitors (including negative origins), per-monitor DPI, narrow work areas, taskbar row counts, and both pointer and shell-hotkey activation.

## Goals / Non-Goals

**Goals:**

- Make Start horizontal placement follow the current taskbar alignment.
- Keep Left as SuperDesktop's default.
- Apply a saved alignment the next time Start opens without restarting the shell.
- Keep every Start activation path on one geometry implementation.
- Make the existing setting's Start-menu effect explicit in English and Traditional Chinese UI text.

**Non-Goals:**

- Adding an independent Start-only preference.
- Changing Start width, height, contents, or visual styling.
- Moving the taskbar to another screen edge or modifying Explorer registry settings.

## Decisions

### Reuse `TaskbarAlignment`

Start SHALL consume `settings.taskbar.alignment`. This mirrors Windows' single Taskbar alignment control and avoids invalid or surprising combinations. Adding `StartSettings::alignment` was rejected because it would require migration and diverge from the requested Explorer-compatible behavior. Hard-coding Left was rejected because Center must remain selectable.

### Make alignment an explicit geometry input

`start_window_geometry` and its `WindowOptions` adapter will accept `TaskbarAlignment`. Left alignment uses the logical monitor work-area left plus `START_HORIZONTAL_MARGIN`; Center preserves the existing formula. Both results are clamped to the logical work-area bounds after width clamping. Keeping the arithmetic in this pure helper makes DPI, monitor-origin, narrow-screen, shell, and row behavior independently testable.

### Read the setting at open time

The Start callback will retain a shared reference to persisted settings and borrow it only long enough to copy the alignment immediately before `open_window`. It will not hold a `RefCell` borrow across GPUI callbacks. This prevents stale positioning after settings changes and avoids re-entrant borrow panics.

The existing callback remains the single activation boundary for taskbar clicks and registered shell hotkeys, so all activation paths receive the same current alignment and selected monitor.

### Clarify the existing settings row

The alignment row remains in Taskbar behaviors and continues toggling Left/Center. Its description changes from taskbar-buttons-only wording to wording that explicitly includes the Start menu. No new row or schema field is introduced.

## Risks / Trade-offs

- [Risk] A margin calculation can place Start outside a narrow or offset monitor. → Clamp the horizontal origin against logical work-area bounds and cover narrow, positive-origin, negative-origin, and high-DPI monitors in unit tests.
- [Risk] Reading shared settings while opening a GPUI window can cause a re-entrant `RefCell` panic. → Copy the alignment into a local value before invoking `open_window`; never retain the borrow in a closure.
- [Risk] A settings change can update taskbar buttons before Start geometry. → Resolve alignment from the authoritative persisted settings on each open rather than from the `TaskbarView` snapshot.
- [Trade-off] Start cannot be centered independently from left-aligned buttons. → This is intentional Windows parity and keeps one understandable control.

## Migration Plan

No data migration is needed. Existing `left` and `center` values retain their meaning; absent or invalid values continue to default to Left. Deployment uses the normal SuperDesktop build and installer. Rollback is a code rollback because the persisted format is unchanged.

## Observability and Verification

Unit tests will assert exact logical geometry for both alignments and relevant monitor boundaries. Settings-model tests will verify the toggle and localized description. Headful UTIT evidence will record left and centered popup bounds after changing the setting and reopening through the keyboard path. Workspace tests, warnings-denied Clippy, release build, and installer generation are blocking gates.

## Open Questions

None. The user's instruction to match Explorer behavior resolves the setting ownership and coupling decision.
