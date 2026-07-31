## Why

Editable text selection currently hugs or clips the glyph band near the bottom of the address bar instead of filling the control with balanced vertical spacing. Address, search, and inline rename editors need one Explorer-like selection geometry so the visual treatment remains consistent across every editing mode and DPI scale.

## What Changes

- Add a shared near-full-height selection metric for single-line editable controls.
- Center the selection line box with equal top and bottom insets inside the focus border.
- Apply the shared metric to the address editor, search editor, and inline file or folder rename editor.
- Use the address editor's opaque selected background, selected-text foreground, normal text, and caret colors in every editing mode while preserving focus borders, pointer selection, and keyboard editing behavior.
- Extend unit, structural, and UTIT coverage to verify glyph coverage and symmetric physical-pixel margins across all three editors.

## Capabilities

### New Capabilities

- `editable-selection-vertical-metrics`: Defines consistent near-full-height, vertically symmetric selected-text geometry for every supported single-line editor.

### Modified Capabilities

None.

## Impact

- `explorer-ui` editable control rendering and metric helpers plus the vendored GPUI single-line selection painter.
- Editable pointer-input UTIT scripts and manifest evidence.
- No public API, dependency, persistence, or file-format changes.
