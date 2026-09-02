# Remote transfer cancellation and Fluent progress design

## Root cause

`RemoteExplorerService` owns ADB/SFTP transfer threads but does not intercept `ExplorerCommand::Cancel`. The command falls through to the inner Shell service, whose active-request registry cannot find the remote request. UI state therefore changes to `正在取消` while the original remote `CancellationToken` remains active.

## Cancellation design

- Add a request-scoped active remote registry mapping `RequestId` to the exact `CancellationToken` cloned into the worker.
- Register before spawning every remote operation-producing worker: remote file operation, paste, external drop, and staged transfer paths.
- Intercept Cancel in `RemoteExplorerService`; when the request is remote, cancel its stored token immediately and return success. Unknown requests continue to the inner service.
- Remove the entry on every worker terminal path, including provider failure, cancellation, panic containment, and submission rollback.
- ADB keeps its owned process kill/wait behavior; SFTP cancellable awaits stop the current network future; transfer-engine checks prevent later chunks, stages, and move cleanup.
- Exactly one correlated terminal event remains authoritative, and late progress is rejected.

## Fluent progress design

- Keep the fixed 250px cancel region.
- The right region renders a rounded neutral track with internal horizontal margin and a 4px visual height.
- Determinate progress uses a rounded accent fill clipped to the track, including a small visible minimum only after real progress begins.
- Indeterminate progress uses a shorter rounded accent segment rather than tinting the full track.
- Terminal success may fill the track; failure and cancellation preserve the last real ratio.
- Text remains above the track with sufficient spacing so the bar does not underline glyphs.

## Validation

- Unit test remote Cancel routing uses the same token and cleans the registry on terminal paths.
- Test unknown cancellation still delegates to Shell.
- Provider tests verify no later progress/stage/source deletion after cancellation.
- Render structure tests verify Fluent track/fill/segment containment and rounded styling.
- Final release build uses `build_test_install.bat`, followed by installed-app Local→ADB and Local→SFTP large-file cancellation checks from the user perspective.

## Non-goals

- Pause/resume, transfer speed, ETA, or public ABI changes.
