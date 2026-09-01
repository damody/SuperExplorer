# Light-theme file-row visual review

## Environment

- Windows headful GPUI fixture, actual window DPI 168 (175%).
- Theme: light.
- Fixture: deterministic populated/drag-cue directory.
- Capture metadata and diagnostics are stored beside each screenshot.

## Hover

- Screenshot: `hover/screenshot.png`
- SHA-256: `61184025D5A174B0BDEA79C4F992C0DF9A0D7ED664C24F00E8C240956C14385F`
- Injected logical point: `(400, 212)`, within the first file row.
- Interior samples at physical `(1000, 370)` and row edge `(540, 343)`: `#E5F3FF`.
- Reference Explorer dominant hover fill: `#E5F3FF`.
- Result: exact RGB match; no focus outline is rendered.

## Focused selection

- Screenshot: `selection/screenshot.png`
- SHA-256: `A45140C5C513BC5128F6F1DD32D76E18FBAE9C35647AAC4E1062E2BFC7FFAD7B`
- Interior sample at physical `(1000, 370)`: `#CCE8FF`.
- Top/left outline sample at physical `(540, 343)`: `#000000`.
- Reference Explorer dominant selection fill and outline: `#CCE8FF`, opaque black one-pixel outline.
- Result: exact RGB match; the outline is one logical pixel and remains crisp at 175% DPI.

## Visible-column boundary

- Rebuilt hover screenshot: `bounded-hover/screenshot.png`
- Hover SHA-256: `b24e2f66370378ae6f148a1c51dd447acbedbd081c960a152ecbc02dacb704c9`
- Rebuilt focused-selection screenshot: `bounded-selection/screenshot.png`
- Focused-selection SHA-256: `ab53257093ee4e4509b8ed4009b41fe2d0c9500a98f4cee8d78135018138e712`
- The row's last painted physical pixel is x=1648, aligned with the right edge of the visible Size column; x=1650 and x=1700 remain the file-view background `#FFFFFF` in both states.
- Hover remains `#E5F3FF` at `(1000, 370)` and `(1648, 370)`.
- Focused selection remains `#CCE8FF` at `(1000, 370)`, with an opaque `#000000` right outline at `(1648, 370)`.
- Result: fill, outline, hover hit area, and row click target stop at the final visible data column rather than extending to the viewport edge.

## Disposition

PASS. The rendered light-theme hover and focused-selection states match the supplied Explorer reference colors, outline behavior, and final-visible-column boundary. Typography, icons, and selection state were not changed.

## Final divider-center alignment

- Hover screenshot: `divider-aligned-hover/screenshot.png`
- Hover SHA-256: `e56bfb95ecd75b065ade6710ad0089ab3f2c4c3198be45047fe148c82663961e`
- Focused-selection screenshot: `divider-aligned-selection/screenshot.png`
- Focused-selection SHA-256: `bed1339fe56690dddb2637ea2d547130d743a9bf6823f9eab2d1cc7e2c09752a`
- At 175% DPI, the visible final header divider occupies physical x=1662..1663.
- Hover fill and focused-selection outline both end at physical x=1663; x=1664 is the ordinary white file-view background.
- Result: exact zero-pixel difference between the row's right edge and the visible final header divider.
