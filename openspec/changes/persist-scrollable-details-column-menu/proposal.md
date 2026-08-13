## Why

The Details column chooser closes during immediate visibility changes and has no bounded vertical overflow, forcing users to reopen it for every column and making lower built-in or extension columns unreachable in short windows. The chooser must match File Explorer's persistent multi-toggle and scrolling behavior.

## What Changes

- Keep the Details column chooser open while enabled column rows are repeatedly checked and unchecked.
- Keep chooser actions owned by the file-view surface so rerenders and hiding the originating header do not dismiss the popup.
- Bound the chooser to the usable menu height and enable vertical wheel, touchpad, and scrollbar movement.
- Preserve the scroll offset across row toggles and keep `Name` fixed, checked, and disabled.
- Add Rust and installed-app UTIT coverage for repeated toggles, overflow scrolling, bottom-row interaction, dismissal, and persisted state.

## Capabilities

### New Capabilities

- `details-column-chooser`: Defines persistent multi-toggle behavior, fixed-column rules, bounded scrolling, popup ownership, dismissal, and persistence for the Details column chooser.

### Modified Capabilities

None.

## Impact

- `crates/explorer-ui/src/actions.rs`: chooser action focus ownership.
- `crates/explorer-ui/src/chrome.rs`: bounded scrollable popup composition and structural tests.
- `crates/explorer-ui/src/state.rs`: repeated-toggle state regression coverage.
- `scripts/` and `uitest/manifest.json`: installed-app genuine-pointer and wheel verification.
- No extension ABI, column registry order, settings schema, or dependency changes.
