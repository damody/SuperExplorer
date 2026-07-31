## 1. Interaction state and routing

- [x] 1.1 Add idempotent pointer-driven navigation-history focus and keyboard activation coverage.
- [x] 1.2 Add permanent-delete Cancel/Delete focus state, lifecycle reset, Tab/Shift+Tab traversal, and focused Enter/Space routing.

## 2. Explorer-like rendering

- [x] 2.1 Render history pointer/keyboard focus with neutral gray and synchronize pointer movement to the focused entry.
- [x] 2.2 Render permanent-delete buttons with matching hover/focus gray and actionable mouse semantics.
- [x] 2.3 Correct editable-field inner-height padding and selected-glyph clipping so partial address selection preserves unselected foreground.

## 3. UTIT and verification

- [x] 3.1 Add a headful pointer/keyboard/visual UTIT for history hover, Shift+Delete button focus, and partial address selection.
- [x] 3.2 Register the test in the UTIT manifest and map every new requirement.
- [x] 3.3 Pass focused Rust tests, manifest coverage, headful UTIT, workspace checks, formatting, clippy, and strict OpenSpec validation.
- [x] 3.4 Commit the completed change without touching unrelated workspace content.
