# Navigation, Dialog, and Address Interaction Parity

## Scope

Bring three existing interaction surfaces in line with Windows File Explorer: Back/Forward history menu hover, Shift+Delete confirmation focus, and editable address selection rendering. This change does not redesign navigation, replace GPUI controls, or alter deletion semantics.

## Interaction model

The Back/Forward history popup will use one state index for keyboard focus and pointer hover. Pointer movement updates the index, Up/Down/Home/End continue to update it, and Enter or a genuine click activates that entry. Both pointer hover and keyboard focus use the same neutral gray selected-inactive color as the breadcrumb popup; no entry remains blue merely because the menu opened.

The permanent-delete dialog will own a two-value focus state: Cancel and Delete. Opening the dialog establishes a visible focused button. Tab and Shift+Tab cycle only within those buttons, Enter/Space invokes the focused action, Escape cancels, and pointer movement/clicks visually and functionally target the button under the mouse. The focus surface remains modal until dismissal.

The editable address field will keep the existing IME-capable text input. Its focused border is included in the vertical content calculation so the text line and selection rectangle are not clipped asymmetrically. Selected glyph repainting remains clipped to actual selection quads, while unselected glyphs always retain the normal primary text color.

## Components

- `AppViewState` owns history hover/focus and permanent-delete button focus.
- `ExplorerAction` carries pointer focus changes and modal focus traversal.
- `chrome.rs` renders neutral hover/focus visuals and the corrected editable field geometry.
- The existing GPUI editable-text implementation remains responsible for caret and selection hit testing; focused visual tests guard its clipping contract.

## Testing

- Reducer tests cover pointer history focus, modal Tab/Shift+Tab cycling, focused Enter behavior, Escape, and lifecycle reset.
- Render-contract tests verify neutral gray focus tokens and focused-border-aware vertical geometry.
- A headful UTIT uses physical pointer movement and keyboard input, samples hover/focus pixels, activates dialog actions with Tab+Enter, and captures partial address selection to verify selected and unselected foreground colors plus balanced vertical inset.
- The UTIT case is manifest-driven and mapped to the OpenSpec requirements.

## Error and lifecycle handling

Opening a replacement popup or leaving the dialog clears stale focus state. Out-of-range history indices remain bounded by the current entry list. Destructive work is still submitted only through the existing explicit confirmation action.
