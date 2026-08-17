## ADDED Requirements

### Requirement: Formal packaging must bind an exact SuperDesktop submodule
The formal installer build SHALL require `SuperDesktop` to be an initialized Git submodule using `https://github.com/damody/SuperDesktop.git`, with checkout HEAD equal to the parent repository gitlink and no relevant uncommitted product or build-source changes.

#### Scenario: Clean pinned submodule
- **WHEN** the configured origin, parent gitlink, submodule HEAD, and relevant source status all match
- **THEN** formal submodule admission passes and the combined build may proceed

#### Scenario: Missing or uninitialized submodule
- **WHEN** the `SuperDesktop` gitlink or initialized worktree is absent
- **THEN** the formal build fails before Cargo or NSIS execution with a submodule admission diagnostic

#### Scenario: Gitlink or origin drift
- **WHEN** the submodule HEAD differs from the parent gitlink or its configured origin differs from the approved URL
- **THEN** the formal build fails before compilation and reports the mismatched identity

#### Scenario: Dirty formal SuperDesktop source
- **WHEN** relevant tracked or untracked SuperDesktop product/build source differs from its committed revision
- **THEN** the formal build fails and does not create or replace an installer

### Requirement: Installer entry points must select exactly one declared component set
The build workflow SHALL expose `all`, `superexplorer`, and `superdesktop` modes, and each batch entry point MUST select its fixed mode without relying on filename inference.

#### Scenario: Formal entry point
- **WHEN** `build_install.bat` is invoked
- **THEN** Lua receives `all` and selects both SuperExplorer and SuperDesktop

#### Scenario: SuperExplorer test entry point
- **WHEN** `build_test_install.bat` is invoked
- **THEN** Lua receives `superexplorer`, permits the existing SuperExplorer test dirty policy, and does not inspect or build SuperDesktop

#### Scenario: SuperDesktop test entry point
- **WHEN** `build_desktop_test_install.bat` is invoked
- **THEN** Lua receives `superdesktop`, permits SuperDesktop test changes, and does not build SuperExplorer or its plugins

#### Scenario: Unknown or conflicting selection
- **WHEN** Lua receives an unknown mode or more than one component mode
- **THEN** it fails before repository inspection, compilation, publication, or launch

### Requirement: Dirty-tree allowances must remain component-local
The system MUST apply a dirty-tree allowance only to the selected test component and MUST NOT transfer either test allowance into formal combined packaging.

#### Scenario: Dirty unselected SuperDesktop during SuperExplorer test
- **WHEN** SuperDesktop contains changes and the selected mode is `superexplorer`
- **THEN** the build neither rejects nor inspects those changes and packages only SuperExplorer

#### Scenario: Dirty unselected SuperExplorer during SuperDesktop test
- **WHEN** SuperExplorer contains unrelated changes and the selected mode is `superdesktop`
- **THEN** the build neither rejects nor builds SuperExplorer and packages only SuperDesktop

#### Scenario: Dirty selected source during formal build
- **WHEN** either selected product has relevant source drift in `all` mode
- **THEN** formal packaging is rejected before any installer is published

### Requirement: Selected components must use reproducible builds and validated inputs
The orchestrator SHALL build only selected components, SHALL build SuperDesktop with `cargo build --workspace --all-targets --release --locked --offline`, and SHALL validate every selected executable and generated installer as a nontrivial Windows PE file.

#### Scenario: Combined build succeeds
- **WHEN** both products pass admission and all selected build commands and PE validations succeed
- **THEN** the orchestrator compiles the combined NSIS installer and atomically publishes it

#### Scenario: Skip-build uses existing artifacts
- **WHEN** `--skip-build` is supplied
- **THEN** compilation is skipped but every selected input is still required and fully PE-validated before NSIS executes

#### Scenario: Selected executable is missing or invalid
- **WHEN** any selected executable is absent, lacks an `MZ` signature, or is below the minimum size
- **THEN** the build fails before NSIS publication and leaves any prior successful output unchanged

#### Scenario: Unselected executable is absent
- **WHEN** an executable belonging only to an unselected component is absent
- **THEN** that absence does not block the selected component build

### Requirement: Check and launch controls must be side-effect bounded
All three component modes SHALL support `--check`, `--skip-build`, and `--no-launch` according to their declared semantics.

#### Scenario: Check mode
- **WHEN** `--check` is supplied for any component mode
- **THEN** selected tools, layout, admission, scripts, and NSIS inputs are checked without compiling, creating an installer, publishing output, or launching a process

#### Scenario: No-launch mode
- **WHEN** a selected installer build succeeds with `--no-launch`
- **THEN** the validated installer is published but is not started

#### Scenario: Default launch
- **WHEN** a selected installer build succeeds without `--no-launch`
- **THEN** only the newly published installer for that mode is launched

### Requirement: NSIS installation and uninstallation must match selected components
The NSIS sources SHALL install, register, shortcut, and uninstall exactly the component set selected at compile time, with shared SuperDesktop file definitions used by combined and desktop-only variants.

#### Scenario: Combined installer content
- **WHEN** NSIS compiles in `all` mode
- **THEN** the installer contains SuperExplorer, its declared runtime/plugins, and the five SuperDesktop-owned executables, with SuperDesktop adjacent to SuperExplorer

#### Scenario: SuperExplorer-only installer content
- **WHEN** NSIS compiles in `superexplorer` mode
- **THEN** the installer and uninstaller contain no SuperDesktop executable, shortcut, registry, or removal action

#### Scenario: SuperDesktop-only installer content
- **WHEN** `installer/SuperDesktop.nsi` compiles in `superdesktop` mode
- **THEN** the installer and uninstaller contain only SuperDesktop files, shortcuts, and metadata under the SuperDesktop installation root

#### Scenario: Failed replacement
- **WHEN** NSIS compilation or final PE validation fails
- **THEN** atomic publication does not replace the previous successful installer

### Requirement: Installing SuperDesktop must not enable Shell takeover
Every installer variant containing SuperDesktop MUST install it in preview-safe state and MUST NOT automatically apply a Windows login-Shell change, stop Explorer, reboot, or log off.

#### Scenario: Combined installation
- **WHEN** the combined installer installs SuperDesktop
- **THEN** it copies `shell-installer.exe` without invoking an applying command and launches SuperDesktop only through its default preview-safe shortcut

#### Scenario: Desktop-only installation
- **WHEN** the SuperDesktop-only test installer completes
- **THEN** Windows login-Shell registry state and the running Explorer session remain unchanged

#### Scenario: Static installer safety audit
- **WHEN** NSIS sources are scanned for SuperDesktop install actions
- **THEN** no `shell-installer --apply`, login-Shell registry write, Explorer termination, reboot, or logoff action is present
