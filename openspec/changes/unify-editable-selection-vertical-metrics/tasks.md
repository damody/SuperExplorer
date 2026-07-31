## 1. Shared Metrics

- [x] 1.1 Add a pure editable selection metric type and helper with border, inset, minimum line-height, and constrained-height clamping.
- [x] 1.2 Add unit tests for normal address/search geometry, rename geometry, symmetric accounting, and constrained controls.

## 2. Editor Integration

- [x] 2.1 Apply shared selection line height and padding to the address and search editing fields without changing font size.
- [x] 2.2 Apply the same helper to inline rename using its own height and border, and route all editor foreground, selection, selected-text, and caret colors through the address editor palette.
- [x] 2.3 Add a structural regression test proving all three editor paths use the shared metrics.

## 3. UTIT Coverage

- [x] 3.1 Extend editable pointer-input automation to create partial selections in address, search, and inline rename editors.
- [x] 3.2 Capture per-editor evidence and measure selection height, glyph coverage, and symmetric physical-pixel margins with one-pixel tolerance.
- [x] 3.3 Update the UTIT manifest or artifact declarations for the new evidence and assertions.

## 4. Verification

- [x] 4.1 Run focused `explorer-ui` unit and structural tests.
- [x] 4.2 Run the editable pointer-input UTIT case and inspect its screenshots and report.
- [x] 4.3 Run formatting, workspace tests or scoped equivalents, Clippy, and strict OpenSpec validation.
- [x] 4.4 Mark tasks complete and commit only the intended implementation, tests, and OpenSpec artifacts.
