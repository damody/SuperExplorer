## 1. Theme contract

- [x] 1.1 Add exhaustive file-row hover, active-selection, inactive-selection, focus-outline, and selected-text semantic slots and getters.
- [x] 1.2 Define measured light-theme values, readable dark-theme values, and Windows high-contrast system-role mappings.
- [x] 1.3 Extend theme completeness and contrast tests for every new file-row role.

## 2. File-row rendering

- [x] 2.1 Extend `FileRowVisual` and `file_row_visual` to project normal hover, active selection, inactive selection, selected text, and one-pixel outline states.
- [x] 2.2 Apply the projected fill, text, hover, outline color, and outline width in the shared file-row renderer without changing event handlers or geometry.
- [x] 2.3 Replace the old outline-only test contract with exact light-theme RGB, state precedence, dark-theme, and high-contrast assertions.

## 3. Verification

- [x] 3.1 Run Rust formatting and focused theme/file-row visual tests.
- [x] 3.2 Run the relevant `explorer-ui` package tests and distinguish failures outside the touched visual paths (398 passed, 8 unchanged out-of-scope failures, 2 ignored; all touched theme/file-row tests passed).
- [x] 3.3 Produce or inspect rendered light-theme evidence and compare hover/focus fill and outline against the four supplied reference screenshots.
- [x] 3.4 Review the final diff against every `file-row-interaction-visuals` requirement and confirm unrelated semantic roles, selection logic, and persisted state are unchanged.

## 4. Details column boundary correction

- [x] 4.1 Make Details row containers use the existing visible-column `render_item_width` instead of full viewport width, without changing other view modes.
- [x] 4.2 Add regression coverage for narrow/wide viewports and built-in/extension visible-column width changes.
- [x] 4.3 Run formatting and focused/full `explorer-ui` tests, recording failures outside the touched layout path. (`cargo fmt --all --check` and focused row-boundary/visual tests pass; full suite: 399 passed, 8 pre-existing unrelated failures, 2 ignored.)
- [x] 4.4 Rebuild and capture hover/focused-selection evidence proving the right edge stops at the last visible column while the calibrated colors remain exact.
- [x] 4.5 Revalidate OpenSpec and review the final diff for row hit boundary, horizontal scrolling, background interaction, and non-Details isolation.

## 5. Header-divider pixel alignment

- [x] 5.1 Include the shared leading horizontal inset in the Details row visual/hit width without changing persisted column widths or other view modes.
- [x] 5.2 Add a regression assertion that the row boundary equals the visible-column sum plus exactly one layout inset.
- [x] 5.3 Rebuild hover/focus evidence and verify the row edge and final header divider share the same physical x coordinate. (Both end at physical x=1663 at 175% DPI.)
- [x] 5.4 Run formatting, focused tests, strict OpenSpec validation, and final diff checks.
