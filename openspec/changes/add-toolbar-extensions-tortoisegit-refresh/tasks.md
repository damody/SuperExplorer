## 1. Windows Capability Discovery

- [x] 1.1 Add a side-effect-free TortoiseGit candidate-path detector in `explorer-shell-win` with existing, missing, and duplicate-path tests.
- [x] 1.2 Inject TortoiseGit availability from the application composition root into `ExplorerRoot` and `AppViewState` without adding a Shell dependency to `explorer-ui`.

## 2. Command Bar and Typed Menu State

- [x] 2.1 Replace the More icon with the visible/accessibility label `其它` while preserving its popup identity and commands.
- [x] 2.2 Add typed Extensions menu actions and mutually exclusive state transitions, including keyboard execution and cancellation.
- [x] 2.3 Render the `擴充功能` button to the right of `其它`, with an enabled TortoiseGit refresh item or disabled unavailable placeholder.

## 3. Scoped Overlay Refresh

- [x] 3.1 Implement a root-level overlay refresh transition that advances beyond global/per-item epochs and invalidates overlay-dependent positive, negative, pending, and thumbnail presentation state only.
- [x] 3.2 Re-submit active-folder and navigation Shell icon requests without changing association generation, base icons, selection, navigation, view, or scroll state.
- [x] 3.3 Add root/reducer tests for epoch advancement, stale completion protection, preserved state, action enablement, and cache clearing.

## 4. Headful and Verification

- [x] 4.1 Add render-contract coverage for toolbar order, labels, popup identities, TortoiseGit command, and unavailable placeholder.
- [ ] 4.2 Add or extend a Windows headful UITEST for Extensions menu invocation and truthful installed/unavailable behavior, then map every new requirement in the manifest.
- [ ] 4.3 Run targeted format, Clippy, model/UI/Shell tests, UITEST coverage validation, headful interop coverage where available, and strict OpenSpec validation.
