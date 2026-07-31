## 1. Placement Model and Windows Integration

- [x] 1.1 Add pure rectangle centering/clamping helpers for normal, edge-crossing, and oversized
  property sheets.
- [x] 1.2 Resolve the validated SuperExplorer owner rectangle and invocation-point monitor work area
  with a deterministic fallback contract.
- [x] 1.3 Add a Properties-only synchronized one-shot placement state and process WinEvent hook on the
  persistent Shell STA.
- [x] 1.4 Identify the first eligible top-level native dialog, position it with non-activating,
  non-sizing `SetWindowPos`, and clear state on success or failure.
- [x] 1.5 Scope hook installation exactly around host-owned Properties invocation without changing
  other verbs, native ownership, focus, or command errors.

## 2. Rust Regression Coverage

- [x] 2.1 Unit-test exact owner centering and all work-area clamp edges.
- [x] 2.2 Unit-test oversized dimensions, invalid rectangles, and fallback anchor calculations.
- [x] 2.3 Test hook-state lifecycle and command scoping so failed or completed sessions cannot poison
  the next Properties invocation.

## 3. Result-Based UTIT

- [x] 3.1 Extend native test interop to capture app, property-sheet, and selected monitor work-area
  rectangles in physical screen coordinates.
- [x] 3.2 Assert file, folder, multi-selection, executable, and script sheet centers are within the
  declared owner-relative tolerance and remain work-area safe.
- [x] 3.3 Preserve native-control/title assertions, Escape dismissal, ten post-Properties cycles,
  exact later Copy target, and one-broker/resource bounds.
- [x] 3.4 Persist placement evidence in the report and map every new requirement into the UTIT
  manifest coverage gate.

## 4. Verification and Delivery

- [x] 4.1 Pass focused Properties, direct/broker context-menu, persistent-broker, external-command,
  replacement, focus, and context resource-soak UTIT cases.
- [x] 4.2 Pass formatting, targeted Rust tests, workspace tests/check/clippy, coverage validation,
  and strict OpenSpec validation.
- [x] 4.3 Build debug and release artifacts, run focused Properties validation against release,
  rebuild the installer, and pass isolated installed-path validation.
- [x] 4.4 Commit implementation and UTIT independently while leaving unrelated untracked workspace
  content untouched.
