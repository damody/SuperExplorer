## Why

The ADB/SFTP context menu now follows the Windows classic command structure, but its 15px text is visibly larger than the native Windows menu font and its implicit line box makes the row rhythm and icon/text proportions look wrong. The remote menu needs a DPI-aware typography contract instead of screenshot-specific physical pixels.

## What Changes

- Calibrate the shared fallback context-menu font size from 15 logical pixels to the Windows menu-sized 12 logical pixels.
- Make the GPUI remote menu use the existing `TypographyTokens::menu` family, size, 16px line height, and 400 weight explicitly.
- Preserve the existing 23px logical row height, icon gutter, icon position, separators, colors, shadow, commands, and behavior.
- Add focused contract tests for typography source, values, vertical fit, theme independence, and owner-draw fallback compatibility.

## Capabilities

### New Capabilities

- `context-menu-typography-layout`: Testable typography and row-layout calibration for application-owned ADB/SFTP context menus and the Windows owner-draw font fallback.

### Modified Capabilities

None.

## Impact

- Model contract: `crates/explorer-model/src/context_menu.rs`.
- Remote renderer: `crates/explorer-ui/src/chrome.rs`.
- Windows fallback consumer: `crates/explorer-shell-win/src/immersive_popup.rs` remains behaviorally unchanged but consumes the corrected shared fallback value.
- No API, persistence, provider, clipboard, command, or dependency change.
