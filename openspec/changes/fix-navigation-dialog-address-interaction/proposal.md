## Why

Back/Forward history menus, Shift+Delete confirmation, and editable address selection currently expose inconsistent hover, focus, and text-selection visuals. The mismatches make pointer targets unclear, prevent reliable keyboard confirmation, and can render unselected address text with the selected foreground color.

## What Changes

- Make Back/Forward history menu focus follow pointer hover and use Explorer-like neutral gray for both hover and keyboard focus.
- Give the Shift+Delete dialog an explicit visible button focus, trapped Tab/Shift+Tab traversal, focused Enter/Space activation, Escape cancellation, and matching pointer hover/click behavior.
- Correct editable address vertical geometry and selected-glyph clipping so selection has balanced top/bottom space and unselected text keeps its normal foreground.
- Add reducer, render-contract, and headful UTIT coverage for all three interaction paths.

## Capabilities

### New Capabilities

- `navigation-dialog-address-interaction`: Explorer-like history popup hover, modal confirmation focus, and editable-address selection rendering.

### Modified Capabilities

None.

## Impact

- `explorer-ui` actions, view state, keyboard routing, and chrome rendering.
- GPUI editable-text geometry or clipping only where required by the focused address-field contract.
- UTIT manifest and a focused headful pointer/keyboard/visual regression case.
