## Why

Remote ADB/SFTP transfers display `正在取消` but continue because the Cancel command is delegated to the Shell service instead of cancelling the token owned by `RemoteExplorerService`. The progress surface also needs a Fluent track/fill treatment rather than a hard underline.

## What Changes

- Route Cancel to the exact active remote request token and clean registry entries on every terminal path.
- Preserve ADB/SFTP/local staged cancellation safety and reject late progress.
- Render determinate and indeterminate progress with rounded Fluent track and accent segments.
- Verify both endpoints using the installed release build.

## Capabilities

### New Capabilities

- `remote-cancel-and-fluent-progress`: Remote cancellation routing and Fluent progress presentation.

### Modified Capabilities

None.

## Impact

`explorer-app` remote service routing and tests, `explorer-ui` operation center rendering/tests, release packaging and headful endpoint verification. No public ABI or credential format changes.
