## Why

Launching SuperExplorer while it is already open currently restores the saved
session instead of reliably presenting a fresh system-drive window. Repeated
launches should behave like File Explorer by opening another independent window
at `C:\`.

## What Changes

- Detect repeated ordinary launches through a login-session-scoped Windows
  marker held for each process lifetime.
- Make each later launch create exactly one independent top-level explorer
  window at `C:\`.
- Preserve first-launch session restoration and exclude explicit diagnostic,
  fixture, and test launches from redirection.
- Report marker-creation failures through controlled startup diagnostics.
- Permit concurrent extension-host startup by sharing the verified private
  staging-root directory handle while retaining unique import children.
- Keep installed Start Menu and desktop shortcuts free of diagnostic arguments,
  including installers built with test diagnostics enabled.
- Make in-place upgrades quiesce only SuperExplorer processes running from the
  selected install directory and fail closed when quiescence cannot be proven.
- Add launch-classification, initial-location, and Windows headful coverage.

Non-goals are arbitrary path command-line launches, cross-user coordination,
multi-window session restoration, or changes to tab behavior.

## Capabilities

### New Capabilities

- `repeated-launch-window-coordination`: Login-session repeated-launch
  detection and creation of a fresh `C:\` explorer window.

### Modified Capabilities

None.

## Impact

- `explorer-app` gains Windows launch classification and an explicit startup
  path override.
- Each main window remains independently process-owned; existing GPUI window
  composition is unchanged.
- Session persistence remains process-owned; explicit repeated-launch location
  suppresses saved-tab restoration for that new window.
- Installer shortcuts use ordinary launch arguments; a test build may use the
  diagnostics argument only for the finish-page launch.
- No public plugin ABI, SDK, persisted schema, or external service changes.
