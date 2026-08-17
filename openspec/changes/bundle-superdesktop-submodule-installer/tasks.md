# Component-Scoped SuperDesktop Installer Tasks

All leaves are mandatory. Every checked leaf must map to a unique evidence record under `evidence/index.jsonl`, or to an immutable shared artifact plus a unique subcheck. Failed, blocked, stale, or unexecuted leaves remain unchecked.

## 1. Establish Submodule and Build Admission

### 1.1 Register the exact SuperDesktop submodule

**目的：** Replace the untracked nested repository boundary with a parent-pinned, origin-verified SuperDesktop gitlink without disturbing unrelated parent changes.
**輸入：** Approved design, current nested `SuperDesktop` origin/HEAD, parent `.gitmodules`.
**產出：** `.gitmodules`, `SuperDesktop` gitlink, submodule identity evidence.
**依賴：** None.
**Owner／Wave：** Primary integrator／Wave 1.
**Gate／Evidence：** `G-SUBMODULE-ADMISSION`; `evidence/artifacts/1.1/submodule-identity.json`.
**完成門檻：** URL, parent gitlink, initialized checkout, and HEAD agree; only owned paths are staged.

- [x] 1.1.1 Capture and validate the nested SuperDesktop repository origin, exact HEAD, and clean Git metadata boundary.
- [x] 1.1.2 Register `SuperDesktop` in `.gitmodules` with the approved GitHub URL and preserve the validated checkout as the parent gitlink.
- [x] 1.1.3 Verify `git submodule status`, configured URL, parent tree mode `160000`, and checkout HEAD all agree.
- [x] 1.1.4 Record path-scoped parent status proving unrelated worktree changes were neither staged nor reverted.

### 1.2 Implement component and dirty-policy admission

**目的：** Resolve one component mode before any repository or build action and enforce the correct component-local cleanliness contract.
**輸入：** Existing `build/build_install.lua`, process/log helpers, 1.1 gitlink.
**產出：** Mode parser, selected-component predicates, formal/test admission helpers and diagnostics.
**依賴：** 1.1.
**Owner／Wave：** Lua build owner／Wave 1.
**Gate／Evidence：** `G-COMPONENT-ISOLATION`, `G-SUBMODULE-ADMISSION`; `evidence/artifacts/1.2/admission-fixtures.json`.
**完成門檻：** All three modes and both test allowances are explicit; every invalid or drifting formal state fails before compilation.

- [x] 1.2.1 Add exclusive `all`, `superexplorer`, and `superdesktop` option parsing with early rejection of unknown/conflicting modes.
- [x] 1.2.2 Preserve the existing relevant SuperExplorer source-status check for formal `all` admission.
- [x] 1.2.3 Add initialized-submodule, approved-origin, parent-gitlink, and checkout-HEAD checks for formal `all` admission.
- [x] 1.2.4 Add relevant SuperDesktop source-status rejection for formal `all` without treating ignored target/evidence output as source drift.
- [x] 1.2.5 Ensure SuperExplorer test admission does not execute SuperDesktop Git checks and SuperDesktop test admission does not execute parent cleanliness checks.
- [x] 1.2.6 Emit component-specific structured logs and stable diagnostics for each rejected admission state.

## 2. Route and Build Selected Products

### 2.1 Bind batch entry points to fixed modes

**目的：** Give each user-facing batch file one unambiguous installer product.
**輸入：** Existing two BAT files and Lua mode contract from 1.2.
**產出：** Updated `build_install.bat`, updated `build_test_install.bat`, new `build_desktop_test_install.bat`.
**依賴：** 1.2.
**Owner／Wave：** Build entry-point owner／Wave 2.
**Gate／Evidence：** `G-COMPONENT-ISOLATION`; `evidence/artifacts/2.1/batch-routing.json`.
**完成門檻：** Each BAT passes exactly its declared mode and reports check/build/launch outcomes accurately.

- [x] 2.1.1 Make `build_install.bat` pass formal `all` mode without a dirty allowance.
- [x] 2.1.2 Make `build_test_install.bat` pass `superexplorer` mode with only the existing SuperExplorer test dirty allowance.
- [x] 2.1.3 Add `build_desktop_test_install.bat` passing `superdesktop` mode with only the SuperDesktop test dirty allowance.
- [x] 2.1.4 Preserve argument forwarding and accurate `--check`/`--no-launch` success messages in all entry points.

### 2.2 Implement selected-component build pipelines

**目的：** Compile only the selected product set with reproducible commands and isolated logs.
**輸入：** Mode predicates, existing SuperExplorer finalizer/plugins, SuperDesktop Cargo workspace.
**產出：** Selected build stages and release binaries.
**依賴：** 2.1.
**Owner／Wave：** Lua build owner／Wave 2.
**Gate／Evidence：** `G-COMPONENT-ISOLATION`, `G-INSTALLER-INPUT`; `evidence/artifacts/2.2/build-stage-matrix.json`.
**完成門檻：** Each mode executes only its selected product build; SuperDesktop commands are locked/offline/all-targets.

- [x] 2.2.1 Gate the existing SuperExplorer release finalizer and eight plugin builds behind SuperExplorer selection.
- [x] 2.2.2 Add the SuperDesktop `--workspace --all-targets --release --locked --offline` Cargo build behind SuperDesktop selection.
- [x] 2.2.3 Give SuperDesktop and each existing SuperExplorer/plugin stage distinct structured log paths.
- [x] 2.2.4 Make `--check` skip all compilation and make `--skip-build` skip only selected compilation stages.

### 2.3 Validate inputs, version outputs, and publish atomically

**目的：** Reject incomplete/mis-scoped inputs and publish only a validated installer with an unambiguous name.
**輸入：** Selected release binaries, NSIS tool, existing version/publish helpers.
**產出：** Component-specific installer defines, temporary output, atomic final output.
**依賴：** 2.2.
**Owner／Wave：** Lua build owner／Wave 2.
**Gate／Evidence：** `G-INSTALLER-INPUT`; `evidence/artifacts/2.3/input-output-matrix.json`.
**完成門檻：** Selected PE inputs and installer pass validation; unselected files are not required; failed builds preserve prior outputs.

- [x] 2.3.1 Define and validate the five SuperDesktop-owned executable inputs without packaging a second SuperExplorer binary.
- [x] 2.3.2 Validate only selected component inputs for existence, `MZ` signature, and minimum size in normal and `--skip-build` paths.
- [x] 2.3.3 Generate formal, SuperExplorer-test, and SuperDesktop-test output names from the existing commit-date version.
- [x] 2.3.4 Pass only selected absolute input paths and component defines to NSIS with warnings-as-errors.
- [x] 2.3.5 Validate the temporary installer before atomic publication and launch only the newly published selected-mode output.

## 3. Compose Component-Scoped NSIS Installers

### 3.1 Centralize SuperDesktop NSIS content

**目的：** Keep combined and desktop-only SuperDesktop file/install/uninstall declarations synchronized.
**輸入：** Five validated SuperDesktop executables and approved layouts.
**產出：** Shared SuperDesktop NSIS include/macros.
**依賴：** 2.3.
**Owner／Wave：** NSIS owner／Wave 3.
**Gate／Evidence：** `G-INSTALLER-CONTENT`, `G-SHELL-SAFETY`; `evidence/artifacts/3.1/superdesktop-nsis-contract.json`.
**完成門檻：** One shared file set drives both variants and contains no applying Shell mutation.

- [x] 3.1.1 Define compile-time validation for all five SuperDesktop executable paths.
- [x] 3.1.2 Add shared install macros for SuperDesktop files, preview-safe shortcut, and component metadata.
- [x] 3.1.3 Add shared uninstall macros that remove only declared SuperDesktop files, shortcuts, and metadata.
- [x] 3.1.4 Add a static safety guard/test rejecting Shell apply, login-Shell writes, Explorer termination, reboot, or logoff actions.

### 3.2 Extend the SuperExplorer installer for isolated and combined builds

**目的：** Preserve the existing SuperExplorer installer while optionally composing adjacent SuperDesktop content in formal mode.
**輸入：** Existing `installer/SuperExplorer.nsi`, shared macros from 3.1, component defines.
**產出：** SuperExplorer-only and combined compile paths.
**依賴：** 3.1.
**Owner／Wave：** NSIS owner／Wave 3.
**Gate／Evidence：** `G-INSTALLER-CONTENT`; `evidence/artifacts/3.2/superexplorer-variant-contract.json`.
**完成門檻：** SuperExplorer-only contains zero Desktop actions; combined places Desktop binaries adjacent and uninstalls both sets.

- [x] 3.2.1 Add an explicit combined-mode define without changing existing SuperExplorer service/plugin behavior.
- [x] 3.2.2 Compose SuperDesktop install/shortcut metadata only in combined mode with executables adjacent to `SuperExplorer.exe`.
- [x] 3.2.3 Compose SuperDesktop removal only in combined-mode uninstall and keep SuperExplorer-only uninstall free of Desktop paths.
- [x] 3.2.4 Keep version metadata, elevation, compression, localization, and finish-page behavior valid in both variants.

### 3.3 Add the SuperDesktop-only test installer

**目的：** Produce an independently installable test package containing only SuperDesktop.
**輸入：** Shared macros from 3.1 and SuperDesktop component defines.
**產出：** `installer/SuperDesktop.nsi` and `SuperDesktop-Test-Setup-<version>-x64.exe`.
**依賴：** 3.1.
**Owner／Wave：** NSIS owner／Wave 3.
**Gate／Evidence：** `G-INSTALLER-CONTENT`, `G-SHELL-SAFETY`; `evidence/artifacts/3.3/superdesktop-only-contract.json`.
**完成門檻：** Installer/uninstaller touch only the SuperDesktop root/metadata and launch preview-safe app behavior.

- [x] 3.3.1 Add required defines, product metadata, pages, languages, and `$PROGRAMFILES64\SuperDesktop` installation root.
- [x] 3.3.2 Install the shared five-file set and create only SuperDesktop shortcuts/uninstall metadata.
- [x] 3.3.3 Remove only SuperDesktop-owned files and metadata during uninstall.
- [x] 3.3.4 Configure finish-page execution without an applying Shell installer command or automatic SuperExplorer fallback.

## 4. Verify Isolation, Safety, and Release Outputs

### 4.1 Add deterministic routing and negative fixtures

**目的：** Prove mode selection, repository admission, and unselected-component isolation without relying only on successful full builds.
**輸入：** Lua/BAT/NSIS implementation and controlled fixtures.
**產出：** Test script(s), raw logs, summarized fixture matrix.
**依賴：** 3.2, 3.3.
**Owner／Wave：** Verification owner／Wave 4.
**Gate／Evidence：** All four gates; `evidence/artifacts/4.1/fixture-matrix.json`.
**完成門檻：** Every success/failure boundary has deterministic evidence and stable expected diagnostic.

- [x] 4.1.1 Verify direct Lua and all three BAT routes select exactly one expected component mode.
- [x] 4.1.2 Verify unknown/conflicting modes fail before Git, Cargo, NSIS, publication, or launch.
- [x] 4.1.3 Verify missing initialization, wrong origin, gitlink mismatch, and dirty formal SuperDesktop fixtures are rejected.
- [x] 4.1.4 Verify SuperExplorer-only mode performs no SuperDesktop Git/Cargo/input step.
- [x] 4.1.5 Verify SuperDesktop-only mode performs no parent cleanliness, SuperExplorer finalizer, or plugin step.
- [x] 4.1.6 Verify missing/invalid selected PE fails while missing unselected PE does not block the build.

### 4.2 Build and inspect all installer variants

**目的：** Execute the real toolchain and prove each installer contains only its declared components with safe uninstall behavior.
**輸入：** Clean formal sources, test-mode sources, NSIS/Lua/Cargo tools.
**產出：** Three PE installers, build logs, content/safety manifest.
**依賴：** 4.1.
**Owner／Wave：** Primary integrator／Wave 4.
**Gate／Evidence：** `G-INSTALLER-INPUT`, `G-INSTALLER-CONTENT`, `G-SHELL-SAFETY`; `evidence/artifacts/4.2/installer-build-matrix.json`.
**完成門檻：** Check mode and no-launch builds pass for each variant, hashes/content are indexed, and no Shell mutation is observed.

- [x] 4.2.1 Run `--check` for formal combined mode and prove zero compilation/publication/launch.
- [x] 4.2.2 Run `--check` for SuperExplorer-only and SuperDesktop-only modes and prove unselected isolation.
- [x] 4.2.3 Build the SuperExplorer-only test installer with `--no-launch` and validate PE/content hashes.
- [x] 4.2.4 Build the SuperDesktop-only test installer with `--no-launch` and validate PE/content hashes.
- [x] 4.2.5 Build the clean formal combined installer with `--no-launch` and validate PE/content hashes.
- [x] 4.2.6 Inspect all three install/uninstall tables and confirm component-scoped removal plus zero automatic Shell takeover.

### 4.3 Final quality, traceability, and handoff

**目的：** Close the change only after source quality, OpenSpec, evidence, and path ownership are all auditable.
**輸入：** All implementation and verification artifacts.
**產出：** Evidence index/coverage, final validation report, clean owned-path handoff.
**依賴：** 4.2.
**Owner／Wave：** Primary integrator／Wave 5.
**Gate／Evidence：** All gates; `evidence/artifacts/4.3/final-verification.json`.
**完成門檻：** All tasks have valid evidence, strict validation passes, no placeholders remain, and unrelated worktree state is unchanged.

- [x] 4.3.1 Run focused Lua/build tests and NSIS warnings-as-errors checks with zero failures.
- [x] 4.3.2 Run SuperExplorer formatting/static checks affected by the installer scripts and SuperDesktop locked offline release build validation.
- [x] 4.3.3 Generate requirement/scenario/gate/task/evidence coverage and hash every retained artifact.
- [x] 4.3.4 Run `openspec validate bundle-superdesktop-submodule-installer --strict` and detailed task/evidence validation.
- [x] 4.3.5 Verify owned-path Git diff/staging contains no unrelated parent changes or SuperDesktop product-code edits.
- [x] 4.3.6 Publish the final passed disposition for `G-SUBMODULE-ADMISSION`, `G-COMPONENT-ISOLATION`, `G-INSTALLER-INPUT`, `G-INSTALLER-CONTENT`, and `G-SHELL-SAFETY`.
