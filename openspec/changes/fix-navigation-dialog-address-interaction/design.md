## Context

The breadcrumb child menu already synchronizes pointer movement with keyboard focus and uses neutral gray focus visuals, but navigation history retains an opening index without pointer synchronization. Permanent-delete confirmation is visually modal but Enter is hard-wired to Delete and its two buttons do not own a trapped focus model. The editable address uses the shared GPUI editable-text control; its focused border is not included in the content-height calculation, and selected glyph repaint must remain clipped to the actual selection rectangles.

## Goals / Non-Goals

**Goals:**

- Reuse one observable focus index for pointer and keyboard operation in history menus.
- Give Shift+Delete a visible, bounded, actionable two-button focus model.
- Preserve dark unselected address text and balance the selected line vertically inside the focused border.
- Make all behavior reproducible through manifest-driven UTIT.

**Non-Goals:**

- Replace GPUI controls with Win32 child controls.
- Change deletion requests, Shell flags, or confirmation wording.
- Redesign breadcrumb menus, navigation history contents, or address parsing.

## Decisions

1. Add a pointer-set history focus action and reuse the existing history index. This follows the breadcrumb pattern and avoids separate hover state that can disagree with keyboard focus.
2. Render history hover and focused index with the breadcrumb popup's neutral selected-inactive token. Blue selection implies persistent selection, while these popup rows are transient command focus.
3. Model permanent-delete focus in `AppViewState` as Cancel/Delete. Opening establishes a deterministic focus, Tab and Shift+Tab cycle, Enter/Space dispatch the focused action, and Escape remains Cancel. A reducer-owned value makes UIA, rendering, and keyboard routing consistent.
4. Keep the current IME-capable editable control. Compute vertical padding from the inner height after focused borders and verify that selected-glyph repaint is clipped to the true selection quads. This is smaller and safer than replacing address editing.
5. Extend focused headful coverage rather than relying only on source-shape assertions. Physical mouse movement, keyboard traversal, UIA focus, and pixel sampling are required evidence.

## Risks / Trade-offs

- [Mouse movement dispatch can produce repeated identical actions] → Make focus setters idempotent and keep the state update bounded.
- [A modal focus change could accidentally submit deletion] → Only Enter/Space on the focused Delete value dispatches confirmation; pointer hover alone never submits.
- [Editable geometry can regress search-box hit testing] → Share border-aware padding calculation while retaining existing horizontal padding and pointer-selection tests.
- [Headful pixel tests can be DPI-sensitive] → Sample relative to UIA bounds and compare color distance/insets rather than absolute desktop coordinates.

## Migration Plan

No persisted schema changes are required. Rollback is code-only and leaves file/session data untouched.

## Open Questions

None.
