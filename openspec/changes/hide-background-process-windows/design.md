## Context

The application binary currently declares the Windows GUI subsystem for non-debug builds, while debug builds retain a console. Separately, several production crates launch console-subsystem children. The extension broker paths already apply `CREATE_NO_WINDOW` in most places, but ADB and both automation process hosts do not. When these paths run from a GUI-subsystem parent, Windows can create a transient console; from a console-subsystem parent, a child can inherit the parent console unless explicitly suppressed. The required product policy is different for the parent and its children: SuperExplorer itself must keep a diagnostics console in debug and release builds during development, while all internal background children remain windowless.

Constraints include direct executable-plus-argument execution, redirected diagnostics, existing Windows Job Object ownership, cancellation and timeout semantics, no new external dependency, and preservation of the explicit user-facing Open Command Prompt action. Build scripts and test harness commands are outside the production runtime policy.

## Goals / Non-Goals

**Goals:**

- Keep the SuperExplorer parent console visible in debug and release builds.
- Hide every production background child on Windows with `CREATE_NO_WINDOW`.
- Preserve stdout, stderr, exit status, spawn failures, timeouts, cancellation, and process-tree cleanup.
- Classify all production process launch sites and prevent unreviewed additions.
- Preserve an explicitly requested visible Command Prompt.

**Non-Goals:**

- Hiding the SuperExplorer parent console before the product is ready to remove development diagnostics.
- Changing helper IPC, process permissions, package formats, ADB semantics, or automation policy.
- Converting process execution to shell strings or suppressing errors.
- Governing commands started directly by developers, build scripts, or standalone test runners.

## Decisions

### 1. Put the background-command configurator in `explorer-common`

Add a small `process` module that configures an existing `std::process::Command`. On Windows it imports `std::os::windows::process::CommandExt` and applies the named `CREATE_NO_WINDOW` constant; on other platforms it is a no-op. `explorer-automation` and `explorer-extension-broker` already depend on `explorer-common`; `explorer-automation-win` and `explorer-remote` will add the same internal dependency. This gives product launchers one auditable policy without adding a Win32 crate dependency or creating a cycle.

Alternative rejected: repeating the numeric flag in every crate is small initially but has already produced drift. A helper in `explorer-shell-win` would force unrelated process crates to depend on the Shell adapter layer and create an inappropriate architectural edge.

### 2. Configure children immediately before spawn

Each production launcher constructs its command, sets arguments, current directory and stdio, then invokes the common configurator before `spawn`, `output`, or `status`. The configurator changes only Windows creation flags. It does not change standard handles, environment, command-line quoting, ownership, or lifecycle. Existing explicit flags are combined by `CommandExt::creation_flags`; reviewed visible-console code does not call the background configurator.

Alternative rejected: setting environment variables such as `START /B` or routing through PowerShell/cmd does not reliably prevent windows and would violate the shell-free argument contract.

### 3. Make the parent a console-subsystem executable in every profile

Remove the release-only `windows_subsystem = "windows"` attribute from `explorer-app`. Running the release executable directly therefore retains or creates its diagnostics console just like debug. Broker and worker binaries remain Windows-subsystem helpers in release because they are internal children rather than the SuperExplorer parent; they also retain defensive `CREATE_NO_WINDOW` creation flags.

Alternative rejected: allocating a console dynamically only on failures can miss startup diagnostics and changes console attachment semantics. Keeping the existing release GUI subsystem contradicts the requirement that release diagnostics remain visible.

### 4. Classify rather than blindly rewrite every `Command::new`

The inventory gate classifies production sites as background or explicit-visible. Test-only and build-time sites are recorded as excluded by scope. `launch_command_prompt` remains explicit-visible with `CREATE_NEW_CONSOLE`; every background classification must call the common helper or an approved equivalent that is covered by a dedicated test. This avoids accidentally hiding the feature whose purpose is to show a terminal.

### 5. Verify behavior and diagnostics, not source shape alone

Focused tests prove configured commands still capture output and retain timeout/job behavior. A source inventory catches future unclassified sites. Windows headful checks run debug and release SuperExplorer processes and use Win32 process/window inspection to verify one visible parent console and no additional visible console owned by representative ADB, automation, broker, or worker children. If ADB is unavailable, a controlled console-subsystem fixture exercises the same production runner; the evidence records the ADB branch as passed or evidence-backed not-applicable rather than silently skipping it.

### 6. Govern implementation corrections

- **A — task refinement:** task order, command, ownership, or leaf splitting may change without altering requirements, gates, platforms, or evidence.
- **B — design/spec correction:** a wrong in-scope assumption pauses affected work; design, spec, tasks, and stale dependent evidence are updated together before work resumes.
- **C — material change:** scope, public behavior, parent/child visibility policy, platform, permission, destructive/external action, blocking gate, threshold, or required evidence changes require user approval.

No blocking gate or evidence requirement may be reduced through an A- or B-level correction. Superseded evidence remains linked in the evidence index.

## Risks / Trade-offs

- [Release now opens a persistent diagnostics console] → This is intentional during development and is captured as a product requirement; a future removal needs a separate approved change.
- [A child inherits or creates a console despite source flags] → Use Win32 runtime inspection in both profiles rather than relying only on source assertions.
- [The common dependency expands `explorer-remote`] → It is an existing internal primitive crate with no Win32 dependency; verify the dependency graph and architecture check.
- [Hidden children make failures less obvious] → Preserve redirected stdout/stderr, typed terminals and debug parent logging, and test failure propagation.
- [A newly added launcher bypasses policy] → Keep an inventory allowlist with explicit background/visible/test/build classifications and fail on unknown production sites.
- [Process flags interfere with Job Object cleanup] → Run existing cancellation, timeout and process-tree tests after applying the helper.

## Migration Plan

1. Add and test the common command configurator.
2. Migrate ADB and automation background launchers.
3. Audit and normalize broker/worker launchers, preserving helper subsystem settings.
4. Remove the release GUI-subsystem attribute only from `explorer-app`.
5. Add inventory and debug/release Windows runtime evidence, then run focused and architecture gates.

Rollback restores the prior `explorer-app` subsystem attribute and removes common-helper calls as one commit series. It must not leave only a subset of background paths hidden, because partial rollback would reintroduce unpredictable flashing.

## Open Questions

None. The user explicitly chose visible parent diagnostics consoles for both profiles and hidden production background children, and delegated remaining implementation decisions.
