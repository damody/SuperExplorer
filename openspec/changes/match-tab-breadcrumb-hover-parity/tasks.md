## 1. Tab Surface Parity

- [x] 1.1 Add pure semantic color mapping tests for active/content and inactive/chrome surfaces.
- [x] 1.2 Render the active tab with the content surface and inactive tabs with the strip surface.
- [x] 1.3 Cover the strip divider only beneath the active tab while preserving keyboard focus and
  inactive-strip separation.

## 2. Breadcrumb Pointer Focus

- [x] 2.1 Add a typed bounded pointer-focus action and reducer transition for the open breadcrumb
  menu.
- [x] 2.2 Dispatch pointer focus from each stable breadcrumb child row without changing click or
  accessibility activation.
- [x] 2.3 Use the gray hover token for pointer and keyboard current-row presentation and retain the
  pressed token.
- [x] 2.4 Unit-test same-index no-op, pointer replacement, closed-menu rejection, and out-of-range
  rejection.

## 3. Result-Based UTIT

- [x] 3.1 Add a focused headful test that creates two tabs and records active/content continuity,
  inactive/strip equality, and bottom-edge divider pixels.
- [x] 3.2 Drive a real pointer across two breadcrumb rows and prove the gray highlight moves while
  the previous row returns to menu fill.
- [x] 3.3 Persist screenshots and coordinate/color evidence in a JSON report.
- [x] 3.4 Register the case and all new requirements in the UTIT manifest coverage gate.

## 4. Verification and Delivery

- [x] 4.1 Pass targeted Rust tests, formatting, Clippy, workspace check, and strict OpenSpec
  validation.
- [x] 4.2 Pass focused headful tab/breadcrumb UTIT plus existing breadcrumb, mouse-control, keyboard,
  and accessibility regressions.
- [x] 4.3 Commit implementation and UTIT while leaving unrelated `SteamLibrary/` content untouched.
