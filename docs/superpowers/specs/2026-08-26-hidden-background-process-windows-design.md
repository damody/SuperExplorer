# Hidden Background Process Windows Design

## Goal

SuperExplorer must never flash or open a console window for commands that the product runs internally. Both debug and release SuperExplorer executables keep their own console while the product remains under development so diagnostics and unresolved failures stay observable. A separate console may be created only when the user explicitly invokes the product action that opens Command Prompt.

## Scope

The rule applies to production runtime child processes launched by SuperExplorer and its shipped helpers, including ADB discovery and operations, Lua automation executables, extension broker and worker processes, and future background command integrations. It does not apply to repository build scripts, test runners, or commands a developer starts directly from a terminal.

The existing `launch_command_prompt` action is an explicit user-facing exception. It continues to use `CREATE_NEW_CONSOLE` and opens in the current Explorer directory when possible.

## Architecture

Introduce one small Windows process-configuration helper at the lowest shared layer that can be consumed without creating dependency cycles. The helper applies `CREATE_NO_WINDOW` through `std::os::windows::process::CommandExt` to an existing `std::process::Command` and returns the command for normal argument, environment, working-directory, and standard-I/O configuration.

Every production background process launch must pass through this helper or apply an equivalent reviewed configuration when a crate boundary prevents reuse. Callers continue to use direct executable paths and argument arrays. The change must not introduce shell-string composition, detach processes from existing job-object lifecycle control, or discard redirected stdout and stderr.

The application entry-point subsystem configuration changes to keep the parent console visible in every build profile:

- Debug builds keep the SuperExplorer console visible.
- Release builds also keep the SuperExplorer console visible while the product remains under development.
- Background children are hidden in both debug and release builds.
- The explicit Open Command Prompt action creates a visible console in both build modes.

## Runtime Paths

The implementation will inventory all production `Command::new` sites and classify each one as background, explicit user-facing console, test-only, or build-time. Known background paths that require coverage are:

- ADB commands used during startup device discovery and later remote-file operations.
- Windows-contained automation commands, while preserving job assignment, cancellation, timeout, and bounded stdout/stderr capture.
- Platform-neutral automation fallback commands when compiled for Windows.
- Extension broker and worker launches, including diagnostic probes and worker-owned child processes.

Test-only commands and build scripts are outside the product behavior contract. They may independently hide windows when that improves test stability, but they must not be mistaken for production coverage.

## Errors and Diagnostics

Hiding a child window must not hide failures from SuperExplorer. Exit status, stdout, stderr, spawn errors, timeouts, cancellation, and typed extension terminals continue to flow through their existing APIs. Debug diagnostics remain visible through the parent SuperExplorer console or existing log path rather than through a child console.

## Verification

Add focused tests for the shared configuration and each production launcher. Windows integration coverage will start representative console-subsystem children with redirected output and prove that:

1. the child completes and its output is captured;
2. no visible top-level console window is created for the child;
3. timeout, cancellation, and job-object cleanup still work where supported;
4. the explicit Open Command Prompt action retains `CREATE_NEW_CONSOLE`;
5. source inventory rejects new unclassified production `Command::new` sites or missing hidden-window configuration.

Run focused crate tests first, followed by the relevant workspace architecture checks and headful Windows smokes that launch SuperExplorer in debug and release modes. Each smoke must confirm that the parent console remains present while startup ADB discovery and representative background commands create no additional visible console.

## Non-goals

- Removing or hiding the SuperExplorer parent console in either debug or release builds.
- Replacing direct process execution with PowerShell, `cmd.exe`, or another shell host.
- Changing command permissions, output policy, cancellation behavior, or remote-provider semantics.
- Suppressing an explicitly requested Command Prompt window.
