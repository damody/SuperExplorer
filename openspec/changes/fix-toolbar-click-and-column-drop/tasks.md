## 1. Pointer Event Correction

- [x] 1.1 Remove the explorer root's unconditional left-release drag cancellation without changing unrelated focus or command behavior
- [x] 1.2 Defer outside-release cancellation on both the outer details header and nested sort control while retaining synchronous valid-drop commit
- [x] 1.3 Verify the existing drag reducer leaves a committed order unchanged when a deferred cancel runs after commit and adjust idempotence only if required

## 2. Automated Regression Coverage

- [x] 2.1 Add Rust unit or structural tests proving ordinary toolbar releases have no root cancel hook and both header hit areas use deferred fallback cancellation
- [x] 2.2 Run targeted explorer UI tests for command interaction, drag preview commit/cancel, persistence, and fixed `Name` behavior
- [x] 2.3 Add or extend an installed-app UTIT case using genuine pointer input for representative toolbar controls, valid committed drop, persisted order, outside cancellation, and fixed `Name`

## 3. Build and Installed-App Verification

- [x] 3.1 Build the release application and test installer with the repository build workflow
- [x] 3.2 Install or launch the produced test build and run the targeted UTIT case to completion
- [x] 3.3 Capture final screenshot and report evidence showing working toolbar interaction and a persisted reordered column
- [x] 3.4 Run strict OpenSpec validation, review the final diff for unrelated changes, and record all task results
