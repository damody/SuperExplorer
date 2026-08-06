## 1. Pointer lifecycle correction

- [x] 1.1 Add details-column drag update, commit, and cancellation to the passive pointer-action classification and cover editor-preservation plus ordinary click-outside behavior with focused tests.
- [x] 1.2 Keep root-level drag cancellation capture-safe, make inactive cancellation focus-neutral, and add structural/action coverage for ordinary pointer releases.
- [x] 1.3 Run focused explorer-ui tests, formatting checks, diff checks, and strict OpenSpec validation.

## 2. UTIT regression coverage

- [x] 2.1 Add a UTIT manifest scenario using genuine pointer input to enter address editing, type into the surviving editor, and cancel back to the resolved breadcrumb.
- [x] 2.2 Extend the scenario with `Ctrl+L`, `Alt+D`, valid `Enter` submission, and an outside-release details-column drag cleanup assertion.
- [x] 2.3 Run the focused UTIT case and preserve its passing report, coverage record, logs, and screenshot evidence.

## 3. Product verification

- [x] 3.1 Build `build_test_install.bat --no-launch`, install the resulting package, and verify installed/release executable hashes match.
- [x] 3.2 Perform installed-app address-bar pointer and keyboard editing verification and capture final screenshot evidence.
