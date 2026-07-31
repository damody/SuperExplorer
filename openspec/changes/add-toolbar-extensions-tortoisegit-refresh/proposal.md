## Why

The command bar's ellipsis-only More button is unclear, and there is no explicit surface for installed Shell extension actions. Users with TortoiseGit also need a deterministic way to discard stale in-app Git overlay pixels and re-query the current folder without changing its navigation or selection state.

## What Changes

- Replace the More button's ellipsis icon with the visible Traditional Chinese label `其它` while preserving its existing menu and command identity.
- Add an `擴充功能` dropdown immediately to the right of `其它`.
- Detect a valid TortoiseGit installation through owned Windows adapter code and expose that capability to the UI without coupling presentation code to Win32 APIs.
- Show `更新 TortoiseGit 狀態` when TortoiseGit is installed; otherwise show a disabled `沒有可用的擴充功能` placeholder.
- Refresh the active folder's Shell overlay icons by advancing the overlay epoch, invalidating overlay-dependent caches and consumers, and reusing the existing Shell icon pipeline.
- Add reducer, render, cache-invalidation, Windows detection, and headful UITEST coverage.

## Capabilities

### New Capabilities

- `toolbar-shell-extensions`: Covers the labeled Other button, extension dropdown behavior, TortoiseGit discovery, and scoped overlay-icon refresh.

### Modified Capabilities

None.

## Impact

- `explorer-shell-win`: Windows TortoiseGit installation discovery.
- `explorer-app`: capability injection at the composition root.
- `explorer-ui`: toolbar rendering, extension-menu state/actions, keyboard behavior, overlay cache invalidation, and refresh orchestration.
- UITEST manifest and a Windows headful extension-menu/overlay regression.
- No new third-party dependency, Git parser, external process invocation, or file mutation.
