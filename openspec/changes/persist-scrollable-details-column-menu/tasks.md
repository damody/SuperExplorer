## 1. Popup Ownership and Scrolling

- [x] 1.1 Keep visibility, auto-size, and chooser display actions on the file-view focus surface while preserving existing explicit dismissal actions
- [x] 1.2 Add a bounded maximum height and vertical overflow scrolling to the complete Details column chooser
- [x] 1.3 Preserve popup and scroll identity across rerenders, adding a view-owned scroll handle only if verification proves the stable element ID insufficient

## 2. Automated Regression Coverage

- [x] 2.1 Add Rust action/state tests for checked-unchecked-checked repetition, originating-header removal, open-menu preservation, fixed `Name`, and explicit dismissal
- [x] 2.2 Add structural tests for bounded height, vertical overflow, and row click propagation ownership
- [x] 2.3 Run targeted explorer UI tests covering chooser actions, per-tab persistence, extension ordering, scrolling composition, and existing column interactions
- [x] 2.4 Add an installed-app UTIT case with genuine pointer and wheel input for repeated toggles, overflow to the final row, retained bottom scroll position, upward recovery, and screenshots

## 3. Build and Installed-App Verification

- [x] 3.1 Build the debug/release application and test installer through the repository workflows
- [x] 3.2 Install the produced test build and run the targeted UTIT case to completion with top and bottom screenshot evidence
- [x] 3.3 Run formatting and strict OpenSpec validation, review the scoped diff, and record all task results
