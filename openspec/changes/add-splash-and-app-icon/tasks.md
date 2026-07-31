## 1. Branding Assets

- [x] 1.1 Derive and visually validate the transparent splash PNG from the approved upper wordmark.
- [x] 1.2 Derive the lower square icon and package 16, 24, 32, 48, 64, 128, and 256 pixel ICO frames.
- [x] 1.3 Add automated asset validation for PNG alpha coverage and ICO frame dimensions.

## 2. Windows Application Identity

- [x] 2.1 Add the multi-resolution ICO to `app.rc` and update build rerun tracking.
- [x] 2.2 Build the application and verify the executable contains the expected icon resource.

## 3. GPUI Splash Lifecycle

- [x] 3.1 Add an application-local splash view, transparent popup window options, timing constants, and skip-policy tests.
- [x] 3.2 Open the main window and splash in the production startup path while preserving existing visual-fixture and auto-close behavior.
- [x] 3.3 Implement the 1 second hold, 180 millisecond fade, splash removal, main-window activation, and non-fatal diagnostics.

## 4. Verification

- [x] 4.1 Run formatting and focused `explorer-app` unit/integration tests.
- [x] 4.2 Run workspace checks needed to catch resource, asset, and GPUI integration regressions.
- [x] 4.3 Perform a headful startup smoke check and inspect the splash over the loaded main window.
