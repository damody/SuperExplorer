## 1. Geometry contract

- [x] 1.1 Extend Start geometry and window options to accept `TaskbarAlignment`
- [x] 1.2 Implement left-margin placement and work-area clamping while preserving center placement
- [x] 1.3 Add geometry unit tests for default left, center, narrow, offset, high-DPI, shell, and taskbar-row cases

## 2. Runtime settings flow

- [x] 2.1 Read the current persisted alignment immediately before opening Start without retaining a `RefCell` borrow
- [x] 2.2 Verify pointer and registered shell-hotkey activation continue through the shared Start callback
- [x] 2.3 Add regression assertions for current-setting reads and shared activation routing

## 3. Settings discoverability

- [x] 3.1 Update English and Traditional Chinese alignment descriptions to include taskbar buttons and Start
- [x] 3.2 Add settings-model tests for default Left, Left/Center toggling, and localized description text

## 4. Automated verification

- [x] 4.1 Run focused settings-store, taskbar-ui, and superdesktop-app tests
- [x] 4.2 Run the complete SuperDesktop workspace test suite
- [x] 4.3 Run workspace Clippy with warnings denied
- [x] 4.4 Run the SuperDesktop release build

## 5. GUI and packaging verification

- [x] 5.1 Add a headful UTIT case that proves default-left Start bounds
- [x] 5.2 Extend the headful case to change alignment, reopen through the keyboard path, and prove centered bounds
- [x] 5.3 Run the headful UTIT case and retain screenshot, bounds, trace, and console evidence
- [ ] 5.4 Build the installer and verify it contains the updated SuperDesktop executable

## 6. Integration and closure

- [ ] 6.1 Record verification results and artifact hashes in the OpenSpec evidence index
- [ ] 6.2 Validate the OpenSpec change strictly and mark every task complete only with passing evidence
- [ ] 6.3 Commit SuperDesktop changes, update the parent submodule pointer, and commit the parent OpenSpec integration
