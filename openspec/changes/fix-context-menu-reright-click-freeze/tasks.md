## 1. Native menu cancellation

- [x] 1.1 Add a testable one-shot asynchronous cancellation decision for a matched second right-button gesture.
- [x] 1.2 Post `WM_CANCELMODE` to the owned popup window and remove synchronous `EndMenu` from the low-level hook callback.
- [x] 1.3 Preserve popup/submenu pass-through, tagged replay rejection, wrong-owner cleanup, and bounded failure behavior.

## 2. Replacement ordering

- [x] 2.1 Make replayed mouse replacement immediately supersede stale pending state while retaining serialized keyboard replacement.
- [x] 2.2 Add stale-terminal and rapid multi-target tests that prove older requests cannot reopen.

## 3. UTIT regression coverage

- [x] 3.1 Strengthen the genuine-pointer replacement test with a responsiveness oracle, exact second-target Copy result, and one-popup assertion.
- [x] 3.2 Add repeated alternating replacement resource bounds and register the new requirement mapping in `uitest/manifest.json`.

## 4. Verification

- [x] 4.1 Run focused Rust tests, Shell/context-menu regressions, and the headful replacement UTIT.
- [x] 4.2 Run formatting, build/check, manifest validation, strict OpenSpec validation, and diff hygiene checks.
