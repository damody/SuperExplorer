## 1. Baseline and Test Oracle

- [x] 1.1 Reproduce the open-popup second-right-click failure with exact launched process IDs, popup HWNDs, selected UIA item, cursor coordinates, and clipboard state.
- [x] 1.2 Bind popup discovery to the launched SuperExplorer process tree and distinguish the original session from the replacement session without relying only on HWND inequality.
- [x] 1.3 Extend the focused UTIT to physically invoke Copy and prove the file-drop clipboard target is the second item rather than the old target, first row, or background.

## 2. Replacement Gesture State

- [x] 2.1 Add a small testable state machine for untagged right-button down/up capture, owner validation, tagged-input rejection, incomplete gesture cleanup, and completed replay points.
- [x] 2.2 Update the scoped low-level hook to capture and suppress only a matched second gesture on the originating SuperExplorer root while leaving popup/submenu input unchanged.
- [x] 2.3 End the old native menu only after a valid replacement release and clear every thread-local capture field when the hook/session exits.

## 3. Deferred Ordered Input Replay

- [x] 3.1 Replace synchronous `mouse_event` replay with a tagged two-record `SendInput` batch at the captured physical screen point.
- [x] 3.2 Validate owner/window/point liveness, restore foreground, and wait within a small bound for `VK_RBUTTON` release after `TrackPopupMenuEx` and old popup teardown.
- [x] 3.3 Treat partial injection, stale owner, wrong-owner point, or release timeout as bounded cancellation/failure without poisoning the next genuine gesture.
- [x] 3.4 Add Rust unit tests for state transitions, input construction, recursion prevention, cleanup, and failure isolation.

## 4. Explorer-Compatible UTIT

- [x] 4.1 Test Alpha-popup to Beta-popup replacement with exact Beta selection and physically invoked Copy result.
- [x] 4.2 Test compatible multi-selection preservation, popup/submenu interaction, Escape dismissal, outside-left-click dismissal, and subsequent fresh right-click behavior.
- [x] 4.3 Repeat alternating item replacement ten times and assert one broker plus bounded worker, hook, popup, menu, thread, and handle counts.
- [x] 4.4 Map every new OpenSpec requirement to result-based UTIT manifest coverage and validate coverage strictly.

## 5. Verification and Delivery

- [x] 5.1 Pass the focused replacement headful case and existing direct/broker, persistent-broker, Properties lifecycle, focus, worker, and context resource-soak suites.
- [x] 5.2 Run formatting, targeted Rust tests, workspace check/clippy, and strict OpenSpec validation.
- [x] 5.3 Build debug and release artifacts, run the focused flow against release, rebuild the installer, and pass isolated installed-path validation.
- [x] 5.4 Commit the implementation and UTIT as an independently revertible change while leaving unrelated and untracked workspace content untouched.
