## Context

SuperExplorer already owns an NSIS installer driven by the repository-bundled Lua runtime. SuperDesktop has been moved beneath the SuperExplorer checkout as an independent repository, but the parent does not yet register it as a submodule and the installer does not understand its binaries. The approved source design is `docs/superpowers/specs/2026-08-17-superdesktop-submodule-installer-build-design.md`.

The parent worktree contains unrelated user edits and generated evidence. This change owns only its OpenSpec artifacts, `.gitmodules`/the `SuperDesktop` gitlink, the three installer batch entry points, `build/build_install.lua`, focused build tests, and NSIS sources. It must not stage, revert, or rewrite unrelated paths.

## Goals / Non-Goals

**Goals:**

- Pin SuperDesktop as a verifiable submodule.
- Produce a clean-tree combined formal installer and two isolated test installers.
- Reuse the bundled Lua, structured subprocess logging, NSIS warnings-as-errors, and atomic publish flow.
- Prove selected-component build, validation, install, and uninstall boundaries.
- Keep SuperDesktop installed but Shell takeover disabled until a separate explicit operation.

**Non-Goals:**

- No SuperDesktop runtime code changes.
- No automatic Shell enable, Explorer termination, reboot, logoff, or installer-time recovery experiment.
- No cleanup of unrelated SuperExplorer worktree changes.
- No replacement of NSIS or the bundled Lua runtime.

## Decisions

### 1. One Lua orchestrator with three explicit component modes

`build/build_install.lua` accepts exactly one mode: `all`, `superexplorer`, or `superdesktop`. Batch entry points provide the mode. Selection is resolved before repository checks or subprocess execution, so an unselected component cannot accidentally be inspected or built.

The alternative of separate Lua scripts duplicates tool detection, versioning, PE validation, logging, and publication. Implicit mode inference from the batch filename is rejected because it is fragile and untestable when Lua is invoked directly.

**Blocking gate `G-COMPONENT-ISOLATION`:** trace/log evidence must show SuperExplorer-only mode performs no SuperDesktop Git/Cargo/NSIS input operation, and SuperDesktop-only mode performs no SuperExplorer/plugin build operation.

### 2. Formal admission binds both repositories; test admission is component-local

Formal `all` verifies the SuperDesktop directory is an initialized Git submodule, its configured URL is the approved GitHub URL, its HEAD equals the parent gitlink, and relevant source changes are absent in both products. The existing SuperExplorer source filter remains the basis for parent cleanliness; the SuperDesktop filter covers tracked modifications and untracked product/build-source files while excluding generated target/evidence outputs that its own repository already ignores.

`build_test_install.bat` passes SuperExplorer-only mode plus the existing parent dirty allowance. It never evaluates SuperDesktop. `build_desktop_test_install.bat` passes SuperDesktop-only mode plus a desktop dirty allowance and never evaluates parent source cleanliness.

The alternative of allowing a formal installer from dirty submodule content would make the parent commit insufficient to reproduce the artifact.

**Blocking gate `G-SUBMODULE-ADMISSION`:** missing initialization, wrong URL, gitlink mismatch, or relevant dirty SuperDesktop content must reject formal packaging before compilation.

### 3. Selected components own their complete build and validation flow

SuperExplorer uses its existing release finalizer and eight plugin builds. SuperDesktop uses `cargo build --workspace --all-targets --release --locked --offline` from the submodule root. The selected binary set is validated for existence, `MZ` signature, and minimum size before NSIS runs. `--skip-build` skips compilation only; it does not skip validation.

`--check` resolves tools, scripts, repository/submodule admission, and selected NSIS inputs without compiling, publishing, or launching. `--no-launch` publishes without starting the installer.

**Blocking gate `G-INSTALLER-INPUT`:** every selected input and generated installer must pass PE validation, and no unselected input may be required.

### 4. NSIS composition is explicit and shares SuperDesktop file macros

`installer/SuperExplorer.nsi` compiles either SuperExplorer-only or combined content based on explicit defines. `installer/SuperDesktop.nsi` compiles the desktop-only test product. A shared include owns the SuperDesktop file list, shortcuts, registry metadata, and removal statements so the combined and desktop-only variants cannot drift silently.

The combined installer places SuperDesktop executables beside `SuperExplorer.exe` under `$PROGRAMFILES64\SuperExplorer`, satisfying the adjacent executable resolver. The desktop-only test installer uses `$PROGRAMFILES64\SuperDesktop`; SuperExplorer integration remains truthfully unavailable unless configured or supplied adjacent.

Each output identifies its contents: formal combined, SuperExplorer test, or SuperDesktop test. Temporary output is atomically published only after validation.

**Blocking gate `G-INSTALLER-CONTENT`:** compiled installer content and uninstall declarations must contain exactly the selected component set.

### 5. Installation never opts into Shell takeover

NSIS copies `shell-installer.exe` but does not execute an applying command. It does not write Windows login-Shell values, stop Explorer, reboot, or log off. Shortcuts launch SuperDesktop in its default preview-safe behavior.

**Blocking gate `G-SHELL-SAFETY`:** static NSIS inspection and dry-run installer evidence must show no automatic Shell mutation path.

### 6. Evidence and adjustment governance

Implementation evidence is stored beneath `openspec/changes/bundle-superdesktop-submodule-installer/evidence/`. Each completed task maps to a unique task ID or shared immutable artifact plus unique subcheck and content hash.

- **A — task refinement:** commands, leaf splits, or ordering may change without altering modes, outputs, gates, or public behavior.
- **B — design/spec correction:** a faulty assumption within approved scope requires design/spec/tasks updates, affected evidence marked stale, and gates rerun.
- **C — material change:** component definitions, formal cleanliness, required evidence, installer technology, submodule origin, Shell mutation behavior, permissions, or destructive/external actions require user approval.

No adjustment may silently lower a blocking gate.

## Risks / Trade-offs

- **[Parent worktree is heavily dirty]** → Use path-scoped reads, edits, staging, tests, and commits; never normalize unrelated state.
- **[Nested repository is not yet a true submodule]** → Resolve the exact existing HEAD and approved origin before replacing the parent entry with a gitlink; reject unexpected origin or history.
- **[Combined and desktop-only layouts differ]** → Centralize the SuperDesktop file set in a shared NSIS include and test both compile paths.
- **[Cargo feature unification changes binary hashes]** → Use the same workspace/all-targets/locked/offline command for formal and desktop test builds.
- **[Test dirty allowances leak into formal builds]** → Make mode and allowance explicit, reject conflicting flags, and add negative routing fixtures.
- **[Uninstaller removes another installation's files]** → Scope registry/uninstall metadata and removal statements to the generated component mode; validate installer content before release.

## Migration Plan

1. Verify the nested SuperDesktop repository origin and HEAD, then register that exact revision as the parent submodule gitlink.
2. Add mode routing and testable admission helpers to the Lua orchestrator.
3. Add the desktop test batch entry and make existing batch files pass explicit modes.
4. Add shared SuperDesktop NSIS content and the desktop-only NSIS source; extend combined compilation.
5. Run argument, admission, isolation, PE, warnings-as-errors, and installer content gates.
6. Commit only owned paths. Rollback removes the submodule registration and installer changes; it does not mutate installed Shell state.

## Open Questions

None. Component membership, output identity, installation layout, submodule origin, dirty policies, and Shell safety behavior are fixed by the approved design.
