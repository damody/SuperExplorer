# Editable Shift Selection Design

## Goal

Make `Shift+Home`, `Shift+End`, `Shift+Left`, and `Shift+Right` update the selection in every Super Explorer text editor with Windows File Explorer semantics. This includes the address editor, search editor, and inline rename editor.

## Behavior

- `Shift+Left` moves the active caret endpoint one grapheme to the left while preserving the selection anchor.
- `Shift+Right` moves the active caret endpoint one grapheme to the right while preserving the selection anchor.
- `Shift+Home` moves the active caret endpoint to the beginning of the current logical line while preserving the selection anchor.
- `Shift+End` moves the active caret endpoint to the end of the current logical line while preserving the selection anchor.
- Reversing direction shrinks the current selection and can cross the anchor, after which selection grows in the opposite direction.
- At a boundary, the command is a no-op and does not discard an existing selection.
- For Super Explorer's single-line editors, `Shift+Home` and `Shift+End` therefore select to the start or end of the complete value. In particular, `Home` followed by `Shift+End` selects the full value.
- Movement remains grapheme-aware so an emoji or combined character is selected as one user-visible character.

## Architecture

The behavior belongs in the vendored shared `gpui_elements::editable_text` action layer. Add explicit line-selection actions, default platform key bindings, handler hooks, action registration, and state transitions there. The Explorer application continues to register the shared binding collection under the existing `EditableText` key context, so all editor surfaces inherit one implementation.

The window-level Explorer action dispatcher must not implement these commands. Keeping selection inside the focused editable element prevents navigation shortcuts from consuming text-editing chords and avoids separate behavior for address, search, and rename.

## Alternatives Considered

1. **Shared editable-text actions (chosen).** One implementation covers every current and future editor, preserves the existing anchor model, and correctly distinguishes line from document boundaries.
2. **Explorer-local bindings to document-selection actions.** Smaller initially, but gives incorrect semantics for multiline editors and hides the missing reusable line actions.
3. **Raw window key interception.** Can force the four shortcuts to fire but duplicates editor state logic and risks conflicts with file-list and navigation shortcuts.

## Validation

1. Shared component unit tests verify line-start and line-end selection, left/right extension and contraction, anchor crossing, and grapheme boundaries.
2. Explorer UI binding tests verify that all four Windows chords are present in the scoped editable-text binding set and are not replaced by window commands.
3. A dedicated UTIT manifest item launches the real app, focuses editable controls, emits genuine Windows keyboard input, and uses copy-to-clipboard as the selection oracle. It verifies full-line selection with both Home/End directions and one-character extension/contraction with Left/Right. The headful test exercises the shared behavior through representative address, search, and inline rename editors.

## Failure Handling

The UTIT script uses a disposable fixture directory, bounded UI Automation waits, explicit focus restoration, and guaranteed process cleanup. A mismatch reports the editor, chord sequence, expected selected text, and clipboard result so failures are actionable.

## Scope

This change only repairs keyboard text selection and its automated coverage. It does not alter mouse selection, editor styling, navigation shortcuts outside focused text inputs, or clipboard implementation.
