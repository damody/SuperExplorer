## Why

Launching SuperExplorer while it is already open currently starts another full
application process, which does not match the familiar File Explorer interaction
and risks concurrent ownership of session and background resources. Repeated
launches should instead create another explorer window through the resident app.

## What Changes

- Coordinate ordinary launches per Windows user through a bounded, versioned
  local IPC endpoint.
- Keep the first process resident and translate each later launch into exactly
  one new top-level explorer window at `C:\`.
- Preserve first-launch session restoration and exclude explicit diagnostic,
  fixture, and test launches from redirection.
- Fall back to an independent normal launch when no healthy resident endpoint
  accepts the request.
- Add protocol, coordination, UI integration, and Windows headful coverage.

Non-goals are arbitrary path command-line launches, cross-user coordination,
multi-window session restoration, or changes to tab behavior.

## Capabilities

### New Capabilities

- `repeated-launch-window-coordination`: Per-user repeated-launch detection,
  resident-process request delivery, and creation of a fresh `C:\` explorer
  window.

### Modified Capabilities

None.

## Impact

- `explorer-app` gains Windows launch coordination and reusable main-window
  construction.
- GPUI startup owns a foreground-safe command receiver and multiple top-level
  explorer windows.
- Session persistence remains process-owned and continues to describe the
  initial/restored window only.
- Installer entry points require no argument or registration change.
- No public plugin ABI, SDK, persisted schema, or external service changes.
