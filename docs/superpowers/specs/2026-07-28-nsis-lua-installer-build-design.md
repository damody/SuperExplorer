# NSIS Installer and Lua Build Design

## Goal

Add a reproducible Windows installer build whose public entry point is `build_install.bat` and whose orchestration runs through the repository-bundled Lua runtime at `D:\test\build\tools\lua\lua.exe`.

## Build Chain

1. `build_install.bat` validates the fixed Lua executable and invokes `build/build_install.lua` while forwarding arguments and exit status. Every success and failure path prints a final result and waits for a key press before the window closes.
2. `build/build_install.lua` resolves the repository root, derives version `1.YYYY.M.D` from the latest `HEAD` committer date, and locates NSIS from `PATH` or its standard installation directories.
3. Before any compilation or packaging, Lua rejects tracked or untracked `.rs` files reported by Git as uncommitted and prints every blocking path.
4. Lua calls `scripts/finalize_windows_artifact.ps1 -Profile release`. This reuses the existing locked Cargo build plus manifest, PE architecture, and version-resource validation.
5. Lua invokes `makensis.exe` with explicit definitions for the application version, finalized executable, and installer output path.
6. NSIS writes `dist/SuperExplorer-Setup-{version}-x64.exe`; Lua verifies that the installer exists and is non-empty before reporting success.

Every child-process failure must stop the build, preserve its log, and return a nonzero status through Lua and the batch entry point.
After displaying the result and waiting for input, the batch entry point must preserve that original status code.
All status labels, validation explanations, error summaries, and custom installer text owned by this build workflow use Traditional Chinese. Output produced directly by Cargo, Git, PowerShell, or NSIS remains in the tool's native language.

## Installer Behavior

The installer uses Unicode NSIS Modern UI and requests per-user execution so installation does not require elevation. It will:

- install `explorer-app.exe` as `SuperExplorer.exe` under `$LOCALAPPDATA\Programs\SuperExplorer`;
- create Start Menu and Desktop shortcuts;
- write an uninstaller into the installation directory;
- register uninstall metadata under HKCU, including display name, version, icon, publisher, install location, and repository URL;
- replace an existing per-user installation in place;
- remove installed files, shortcuts, the Start Menu folder, and uninstall metadata during uninstall;
- avoid removing user files outside the owned installation directory.

## Files

- `build_install.bat`: stable command-line entry point.
- `build/build_install.lua`: build orchestration and validation.
- `installer/SuperExplorer.nsi`: NSIS installer and uninstaller definition.
- `dist/`: generated installer output; this directory is build output and should be ignored by Git.

## Alternatives

- The selected approach keeps orchestration in Lua and installation rules in NSIS.
- A batch-driven build would underuse the required Lua runtime and make error handling less consistent.
- Reimplementing release finalization in Lua would duplicate the existing tested PowerShell path.

## Validation

- Run Lua syntax validation with the bundled runtime.
- Verify that a known Git committer date produces the expected four-component version.
- Verify that the dry check rejects the current workspace when a tracked or untracked Rust source is uncommitted.
- Verify that both batch success and failure paths print their result, pause, and retain the original status code.
- Verify tool discovery and required paths after the Rust-source guard passes.
- Build the release executable and NSIS installer when toolchain prerequisites are present.
- Check that the generated installer is a non-empty Windows executable containing the finalized x64 application and expected installer version information.
- Compile the NSIS script with warnings treated as errors when supported.

## Scope

This change adds installer tooling only. It does not alter application runtime behavior, the existing release-finalization contract, or unrelated uncommitted workspace changes.
