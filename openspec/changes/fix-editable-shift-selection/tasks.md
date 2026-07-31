## 1. Shared Editable-Text Behavior

- [x] 1.1 Add line-start and line-end selection actions, platform bindings, handler hooks, and element registration.
- [x] 1.2 Implement line-boundary selection with the existing stable-anchor selection primitive.
- [x] 1.3 Add shared component tests for line selection and left/right extension, contraction, reversal, and grapheme behavior.

## 2. Explorer Binding Contract

- [x] 2.1 Add Explorer UI tests proving all four shifted selection chords remain scoped to editable text controls.

## 3. UTIT Coverage

- [x] 3.1 Add a headful keyboard-selection smoke script using genuine key input and exact selected-range replacement assertions.
- [x] 3.2 Add a dedicated test item to `uitest/manifest.json` covering address, search, and inline rename editing.

## 4. Verification

- [x] 4.1 Run focused shared-component and Explorer UI unit tests.
- [x] 4.2 Build the debug application and run the dedicated UTIT item on an interactive Windows desktop.
- [x] 4.3 Run formatting and relevant workspace regression checks, then record final task completion.
