## Context

Super Explorer registers `gpui_elements::editable_text` bindings under a focused-input key context for its address, search, and rename editors. The shared component already models selection as a moving caret endpoint plus a stable anchor and contains character-selection handlers, but it has no line-selection actions or `Shift+Home`/`Shift+End` bindings. The fix must preserve Windows File Explorer semantics and must not let window navigation consume focused text commands.

## Goals / Non-Goals

**Goals:**

- Provide line-start, line-end, left, and right shifted selection through the shared editor action layer.
- Preserve the anchor while extending, shrinking, or reversing a selection.
- Cover direct state behavior, registered chords, and genuine-keyboard application behavior.

**Non-Goals:**

- Change pointer selection, selection rendering, clipboard ownership, or ordinary unshifted navigation.
- Add editor-specific shortcut implementations.
- Redefine document-selection or word-selection commands.

## Decisions

1. Add `SelectLineStart` and `SelectLineEnd` beside existing navigation and selection actions. Their handlers call the same linear selection primitive with a line boundary, preserving the existing caret/anchor model. This is preferred to reusing document actions because Home/End are line commands even if current application editors are single-line.
2. Add default `shift-home` and `shift-end` bindings on Windows/Linux and corresponding `cmd-shift-left` and `cmd-shift-right` bindings on macOS. Existing `shift-left` and `shift-right` remain shared bindings. Explorer continues to scope the complete binding collection to `EditableText`.
3. Add a dedicated headful UTIT item. It uses the established UI Automation launcher and genuine Windows key events, then replaces each selection with a typed marker and reads the resulting editor value as an exact semantic oracle. This avoids relying on GPUI-to-Win32 clipboard synchronization for the replacement itself while proving both that a selection exists and that its boundaries are correct. Representative address, search, and rename flows prove that the shared behavior is reachable in the real application.
4. Keep window-level key bindings unchanged, but add a narrowly scoped raw-key fallback on the shared editable element. The fallback accepts only Shift with Left, Right, Home, or End, uses the same shared `select_linear` primitive, and stops propagation after handling the key. Normal action dispatch remains authoritative; the fallback protects Windows non-text key delivery without adding editor-specific behavior or affecting file-list navigation.

## Risks / Trade-offs

- **Editor accessibility values can update asynchronously** → Use bounded setup retries and include the editor, sequence, expected value, and actual value in failures.
- **A binding can exist while its action is not registered on the element** → Unit tests cover state handlers and Explorer binding presence, while UTIT exercises the full dispatch path.
- **Multibyte text can expose byte-versus-character errors** → Reuse the component's grapheme boundary movement and retain/add multibyte unit coverage.
- **Headful editor discovery can vary with layout** → Reuse existing tested UI Automation predicates, a disposable ASCII fixture, bounded waits, and guaranteed process cleanup.

## Migration Plan

Ship the shared action additions and tests together. No stored data or configuration migration is needed. Rollback consists of reverting the action, binding, test, and manifest additions.

## Open Questions

None. Ambiguous behavior follows current Windows File Explorer editing conventions.
