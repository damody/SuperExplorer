## Context

GPUI paints `text_input` selection backgrounds from the configured line box. The address and search editors currently center the normal typography line height inside a 40-pixel control, while inline rename calculates its own padding. That keeps glyph baselines centered but makes selected backgrounds too short and allows the lower edge to look clipped. The approved visual direction is a selected range that nearly fills the editor while retaining equal visible top and bottom margins.

## Goals / Non-Goals

**Goals:**

- Derive selection line height and vertical padding from one pure metric helper.
- Apply identical near-full-height selection behavior to address, search, and inline rename editors.
- Preserve correct glyph rendering, caret input, pointer selection, and DPI scaling while unifying every editor on the address editor's text, selected background, selected foreground, and caret palette.
- Add deterministic unit and headful UTIT evidence for the three editor surfaces.

**Non-Goals:**

- Changing font sizes, theme token definitions, focus-ring thickness, non-editing breadcrumbs, file-row selection, or multiline selection geometry.
- Replacing GPUI text shaping or selection painting.

## Decisions

### Fill the actual inner bounds for single-line selection

The shared helper subtracts both focus borders, reserves an equal small inset at the top and bottom, and assigns the remaining height to the single-line content box. The vendored GPUI editable painter uses that control's measured inner bounds for single-line selection backgrounds while retaining the typography line height for glyph shaping and multiline selections. This is required because high-DPI layout scales the content box independently of the absolute text line height. A separate absolute overlay was rejected because it would need to duplicate shaped-run geometry, scrolling, caret positions, bidirectional text, and partial selections.

### Share metrics, not typography tokens

The helper accepts control height, border width, minimum glyph line height, and desired inset, then returns equal padding plus selection line height. Address, search, and rename keep their existing font-size tokens. All three resolve editing colors through `editable_input_colors`, using the address editor as the canonical opaque selected background, selected foreground, normal foreground, and caret palette. This avoids changing global theme tokens while preventing three render paths from drifting.

### Clamp for constrained layouts

If the requested border and inset leave less than the minimum glyph line height, the helper reduces the inset to zero and clamps the line height to the available inner height. This keeps small controls valid without negative padding or layout overflow.

### Verify physical output in UTIT

Pure tests establish the arithmetic invariant. The editable pointer-input headful case will select text in each editor, capture the window, locate the focus border and selection-color run, and compare top and bottom physical-pixel margins with a one-pixel tolerance. Existing pointer/caret assertions remain part of the same case.

## Risks / Trade-offs

- [The larger line box could shift the baseline] → Keep font size unchanged, center the line box mathematically, and verify glyph bounds in screenshots.
- [Theme or anti-aliasing differences can change edge pixels] → Detect contiguous selection-color rows with color tolerance and allow one physical pixel of margin variance.
- [Rename uses a different height and border] → Pass its real dimensions to the same helper and cover them independently.
- [A near-full-height selection can resemble whole-field selection] → Keep horizontal painting limited to the selected shaped range; do not color the editor background.

## Migration Plan

No persisted data migration is required. The change is confined to render metrics and tests. Rollback consists of restoring the prior typography line height and centered-padding calls.

## Open Questions

None. The user approved the near-full-height C treatment and explicitly required UTIT coverage.
