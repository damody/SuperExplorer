# Start menu taskbar alignment design

## Goal

Make the SuperDesktop Start menu follow the same left/center alignment setting as the taskbar. New and reset settings remain left aligned, while users can switch between Left and Center in Taskbar settings. This matches the Windows control model: Start position is governed by Taskbar alignment rather than an independent Start preference.

## Existing behavior and defect

`TaskbarSettings::default()` already selects `TaskbarAlignment::Left`, settings persistence already round-trips `left` and `center`, and the Taskbar settings page already exposes the choice. Taskbar buttons react to it, but `start_window_geometry` always centers the Start popup. The callback that opens Start also snapshots several settings when the taskbar window is created, so positioning must explicitly read the current persisted setting when the popup opens.

## Decision

Extend the Start geometry boundary with `TaskbarAlignment` and calculate the horizontal origin as follows:

- Left: place the popup at the monitor work area's left edge plus the existing Start horizontal margin, clamped so the popup remains inside the monitor.
- Center: preserve the current centered geometry.

Use the existing taskbar setting rather than adding a second Start setting. The settings row description will state that the value controls both taskbar buttons and Start, making the behavior discoverable without expanding the schema or introducing conflicting combinations.

At every Start-open action, read `persisted_settings.taskbar.alignment` and pass it to the shared window-options function. The taskbar click callback is also the entry point used by shell hotkeys, so mouse, Windows-key, and Win+S paths receive identical geometry. The monitor and DPI conversion remain inside the existing geometry function.

## Alternatives rejected

1. Add a separate Start-menu alignment field. This offers extra flexibility but diverges from Windows and permits surprising combinations between the Start button and popup.
2. Hard-code Start to the left. This fixes the default screenshot but violates the requested configurability and breaks the existing Center option.

## Failure and compatibility behavior

No schema migration is required. Missing or invalid alignment values continue to decode to Left. Extremely narrow work areas keep the existing width clamp, and the left origin is clamped to the available logical bounds. If settings cannot be saved, the existing settings error path remains authoritative and the last persisted alignment continues to apply.

## Verification

- Unit tests cover default Left, explicit Center, narrow monitors, taskbar row heights, shell/preview work-area differences, high DPI, and non-zero/negative monitor origins.
- Settings-model tests prove the alignment control toggles and its localized description identifies Start-menu positioning.
- A headful UTIT opens Start with the default Left setting, switches to Center through Taskbar settings, reopens Start through the keyboard path, and records bounds/trace evidence showing both states on the same monitor.
- Workspace tests, Clippy with warnings denied, release build, and installer generation remain blocking gates.

## Scope boundary

This change adjusts Start popup positioning and setting discoverability only. It does not redesign Start contents, change its width/height, add taskbar placement on other screen edges, or alter native Explorer settings.
