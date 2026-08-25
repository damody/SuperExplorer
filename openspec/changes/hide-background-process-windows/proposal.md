## Why

SuperExplorer launches console-subsystem tools such as ADB and automation executables without suppressing their windows, so ordinary startup and background work can flash a console over the desktop. The application is still under development and must retain its own debug console in every build profile, while child-process diagnostics must flow through captured output instead of visible child consoles.

## What Changes

- Define one Windows process-window policy that keeps the SuperExplorer parent console visible in debug and release builds.
- Require every production background child process launched by SuperExplorer or a shipped helper to use `CREATE_NO_WINDOW` while preserving arguments, environment, working directory, redirected output, cancellation, timeout, and job-object behavior.
- Apply the policy to ADB discovery and operations, Lua automation execution, and extension broker/worker/helper paths.
- Preserve `CREATE_NEW_CONSOLE` only for the explicit user-facing Open Command Prompt action.
- Add source-inventory and Windows runtime gates that reject unclassified production process launch sites and visible background console windows.

## Capabilities

### New Capabilities

- `windows-process-window-policy`: Defines parent-console visibility, hidden production background children, the explicit visible-terminal exception, diagnostics preservation, inventory rules, and Windows verification behavior.

### Modified Capabilities

- `lua-extension-registrar-and-tool-execution`: Requires extension-owned tool and helper processes to run without a visible console window on Windows while retaining the existing process lease and terminal-result contracts.

## Impact

Affected code includes the `explorer-app` entry point, shared Windows process configuration, `explorer-remote` ADB runner, `explorer-automation` and `explorer-automation-win` process hosts, and `explorer-extension-broker` process creation. Tests and architecture checks will classify production `Command::new` sites and exercise debug and release parent/child window behavior. No public API, command permission, remote-provider semantic, package format, or user-facing terminal action is removed.
