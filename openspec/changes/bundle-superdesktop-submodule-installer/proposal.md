## Why

SuperDesktop now lives inside the SuperExplorer checkout but is not yet represented as a pinned submodule or part of the established NSIS/Lua installer workflow. Without component-scoped builds, the formal installer cannot ship both products reproducibly and the two test installers cannot guarantee that they build and install only their intended product.

## What Changes

- Register `SuperDesktop` as the `https://github.com/damody/SuperDesktop.git` submodule and bind formal packaging to the parent gitlink.
- Extend the bundled-Lua installer orchestrator with `all`, `superexplorer`, and `superdesktop` component modes.
- Make `build_install.bat` reject relevant source drift in both products and create a combined SuperExplorer + SuperDesktop installer.
- Keep `build_test_install.bat` SuperExplorer-only, without inspecting or building SuperDesktop.
- Add `build_desktop_test_install.bat` as a SuperDesktop-only test installer entry point that permits SuperDesktop test changes.
- Extend NSIS sources with reusable SuperDesktop install/uninstall content, component-specific output names, and atomic publication.
- Validate selected inputs and generated installers as Windows PE files, retain check/skip-build/no-launch behavior, and prove component isolation with negative fixtures.
- Keep SuperDesktop Shell takeover disabled by default; installer execution does not apply login-Shell registry changes, stop Explorer, reboot, or log off.

## Capabilities

### New Capabilities

- `component-scoped-installer-build`: Defines pinned SuperDesktop submodule admission, the three installer component modes, component-specific clean-tree rules, selected-input validation, NSIS install/uninstall boundaries, and non-mutating Shell defaults.

### Modified Capabilities

None.

## Impact

- Affects `.gitmodules`, the `SuperDesktop` gitlink, `build_install.bat`, `build_test_install.bat`, new `build_desktop_test_install.bat`, `build/build_install.lua`, and installer NSIS/include sources.
- The formal release build additionally invokes the SuperDesktop locked offline Cargo workspace build.
- Formal builds become stricter because SuperDesktop must be initialized, pinned, origin-valid, gitlink-aligned, and clean.
- Test installer output names become component-explicit; no product runtime API changes are introduced.
- Existing unrelated SuperExplorer worktree changes remain outside this change and must not be staged or reverted.
