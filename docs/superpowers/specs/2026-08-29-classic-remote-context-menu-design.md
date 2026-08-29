# Classic Remote Context Menu Visual Design

## Goal

Change the custom Local/ADB/SFTP fallback context menu from a Windows 11-style horizontal command strip to the traditional Windows vertical menu appearance shown by the reference. Command membership and behavior remain unchanged.

## Visual Contract

- Every command is a full-width vertical row with a left icon gutter and text label.
- Use the application's configured UI font at 12 logical pixels with 18 logical-pixel row height, a 14px icon slot, 4px icon-to-text gap, and 216px menu width so the active Windows DPI scale matches the supplied reference pixel-for-pixel.
- Use the menu surface, a one-pixel divider-colored border, fully square corners on both the menu and rows, and no oversized card spacing.
- Open is the first row and visually emphasized; a thin separator divides it from edit commands.
- Hover and pressed feedback cover the full row. Destructive text remains theme-danger colored.
- Do not copy third-party commands, labels, accelerators, or submenu contents from the reference image.

## Behavior and Verification

The existing actions—Open, Cut, Copy, Rename, Permanent Delete, conditional Paste, and background New Folder—continue to dispatch through the same reducer and persistence paths. Tests assert that there is no command strip, all item commands are vertical rows, command ordering and membership are retained, and menu lifecycle/accessibility behavior is unchanged.
