# SuperDesktop Submodule Installer Build Design

## Context

`D:\SuperExplorer` already builds SuperExplorer installers through two batch entry points, `build/build_install.lua`, and `installer/SuperExplorer.nsi`. SuperDesktop now lives at `D:\SuperExplorer\SuperDesktop` as an independent Git repository and must become a pinned Git submodule of the SuperExplorer repository.

The installer workflow needs three deliberately different products:

- the formal installer contains SuperExplorer and SuperDesktop;
- the existing SuperExplorer test installer contains only SuperExplorer;
- a new SuperDesktop test installer contains only SuperDesktop.

The parent repository has unrelated user changes. This change must not stage, revert, move, or otherwise alter those files.

## Goals

- Register `SuperDesktop` as a submodule using `https://github.com/damody/SuperDesktop.git`.
- Keep the existing NSIS plus bundled-Lua build architecture.
- Make `build_install.bat` produce a combined formal installer only when both product source trees are clean and the SuperDesktop checkout matches the parent gitlink.
- Keep `build_test_install.bat` scoped exclusively to SuperExplorer and allow its existing test-build dirty-tree behavior.
- Add `build_desktop_test_install.bat`, scoped exclusively to SuperDesktop and allowing SuperDesktop test changes.
- Validate every packaged executable as a nontrivial Windows PE file before invoking NSIS.
- Preserve `--check`, `--skip-build`, and `--no-launch` behavior in every applicable mode.
- Install and uninstall only the component selected by the generated installer.

## Non-Goals

- The installer does not automatically enable SuperDesktop as the Windows login Shell.
- The installer does not perform a reboot, logoff, or automatic Shell takeover.
- The test installers do not build or package the other product.
- This change does not modify SuperDesktop product code.
- This change does not clean or normalize unrelated SuperExplorer worktree changes.

## Build Entry Points

### Formal combined build

`build_install.bat` invokes the bundled Lua runtime with component mode `all`. The Lua orchestrator:

1. validates the SuperExplorer source policy;
2. verifies `SuperDesktop` is an initialized submodule whose HEAD matches the parent gitlink;
3. rejects relevant uncommitted SuperDesktop source changes;
4. builds SuperExplorer and its bundled plugins;
5. builds the SuperDesktop release workspace using its locked, offline build contract;
6. validates all package inputs;
7. produces and optionally launches the combined NSIS installer.

### SuperExplorer-only test build

`build_test_install.bat` invokes component mode `superexplorer` with the existing SuperExplorer dirty-tree allowance. This mode does not inspect, initialize, build, hash, or package SuperDesktop. It produces and optionally launches a SuperExplorer-only test installer.

### SuperDesktop-only test build

`build_desktop_test_install.bat` invokes component mode `superdesktop` with an explicit SuperDesktop dirty-tree allowance. This mode does not build or package SuperExplorer or its plugins. It builds SuperDesktop using the same release build command as the formal installer, validates its product binaries, and produces and optionally launches a SuperDesktop-only test installer.

## Lua Orchestration

`build/build_install.lua` remains the single command-line orchestrator. It gains a required internal component selection with these values:

- `all`: formal combined installer;
- `superexplorer`: SuperExplorer-only test installer;
- `superdesktop`: SuperDesktop-only test installer.

Batch files supply the mode so ordinary users do not need to remember it. Unknown or conflicting mode flags fail before any build starts.

Dirty-tree policy is component-specific:

- formal `all` rejects relevant uncommitted source changes in both products and rejects a SuperDesktop gitlink mismatch;
- `superexplorer` may allow SuperExplorer changes and never evaluates SuperDesktop cleanliness;
- `superdesktop` may allow SuperDesktop changes and never evaluates SuperExplorer cleanliness.

`--check` validates the selected mode's tools, repository layout, submodule state, NSIS scripts, and required build scripts without compiling, creating an installer, or launching a process. `--skip-build` requires already-built inputs and still performs complete PE/input validation. `--no-launch` creates the installer without starting it.

All subprocesses use the existing structured `process.run` logging. Each stage receives a distinct log path so failures identify the component, command, working directory, exit code, and output tail.

## SuperDesktop Package Inputs

The SuperDesktop build uses:

```text
cargo build --workspace --all-targets --release --locked --offline
```

The packaged runtime set is:

- `superdesktop-app.exe`
- `superdesktop-guardian.exe`
- `shell-installer.exe`
- `shell-provider-host.exe`
- `notification-area-host.exe`

The combined installer uses the SuperExplorer binary built by the parent project rather than embedding a second copy from the SuperDesktop package helper. The SuperDesktop-only installer packages the five SuperDesktop-owned executables and treats SuperExplorer integration as unavailable until SuperExplorer is installed separately.

Each selected input must exist, begin with the `MZ` signature, and exceed the existing minimum executable size check. NSIS receives absolute paths through defines; it never searches PATH for package inputs.

## NSIS Layout

The installer sources share component macros rather than duplicating file lists and uninstall behavior.

- `installer/SuperExplorer.nsi` remains the SuperExplorer installer source and supports SuperExplorer-only or combined compilation through explicit build defines.
- `installer/SuperDesktop.nsi` is the SuperDesktop-only installer source.
- A shared NSIS include owns the SuperDesktop file list, shortcuts, registry metadata, and uninstall file removal used by both sources.

The combined installer places SuperDesktop's executables beside `SuperExplorer.exe` under `$PROGRAMFILES64\SuperExplorer`. This satisfies SuperDesktop's production adjacent-executable resolver without writing user settings or depending on the development-only `D:\SuperExplorer` path. The SuperDesktop-only test installer uses `$PROGRAMFILES64\SuperDesktop`; in that mode the fixed SuperExplorer entry truthfully remains unavailable until a path is configured or an adjacent executable is supplied.

The combined installer writes both products and creates both Start Menu shortcuts. Its uninstaller removes both products that it installed. The SuperExplorer-only test installer touches only SuperExplorer files and metadata. The SuperDesktop-only test installer touches only `$PROGRAMFILES64\SuperDesktop` files and SuperDesktop-specific metadata.

SuperDesktop installation is file-only and disabled-by-default with respect to Shell takeover. The NSIS installer does not call `shell-installer.exe --apply`, modify the Windows login Shell value, stop Explorer, or schedule a reboot.

## Versioning and Outputs

The existing commit-date version format remains authoritative. Output names identify their contents:

- formal: `SuperExplorer-Setup-<version>-x64.exe`;
- SuperExplorer test: `SuperExplorer-Test-Setup-<version>-x64.exe`;
- SuperDesktop test: `SuperDesktop-Test-Setup-<version>-x64.exe`.

Publishing remains atomic through a temporary NSIS output followed by the existing publish helper. A failed build must not replace a prior successful installer.

## Error Handling

The build fails before NSIS compilation when:

- the bundled Lua runtime, NSIS, selected NSIS source, or selected build script is missing;
- the formal build finds relevant dirty source in either product;
- the formal build finds an uninitialized SuperDesktop submodule, wrong origin, gitlink mismatch, or detached content not represented by the parent repository;
- a selected build command fails;
- a selected executable is missing or fails PE validation;
- an unsupported option or component combination is supplied.

Test modes relax only their explicitly selected source cleanliness rule. They do not relax tool, build, PE, path, or NSIS validation.

## Verification

Verification covers:

1. batch-to-Lua argument routing for all three entry points;
2. `--check` for all three modes with no installer creation or launch;
3. negative fixtures for unknown modes, missing submodule, gitlink mismatch, dirty formal SuperDesktop source, and missing binaries;
4. proof that SuperExplorer-only mode does not run SuperDesktop Git or Cargo stages;
5. proof that SuperDesktop-only mode does not run SuperExplorer/plugin stages;
6. formal combined build with clean-tree enforcement;
7. NSIS compilation with warnings as errors;
8. PE validation of every generated installer;
9. installer content inspection showing each mode contains only its declared components;
10. uninstall script review confirming component-scoped removal and no automatic Shell takeover.

Tests and build outputs remain outside source ownership. No verification step stages or commits unrelated existing worktree changes.

## Rollback

Reverting this change removes the new test entry point, component-aware Lua orchestration, SuperDesktop NSIS sources, and the SuperDesktop submodule registration. Existing SuperExplorer source and user worktree changes remain untouched. Installed products are removed through the matching component-aware uninstaller; rollback of this source change does not itself mutate an installed Windows Shell configuration.
