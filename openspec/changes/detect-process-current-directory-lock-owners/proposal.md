## Why

The Lock owners column currently relies on Windows Restart Manager, which does not report a console process merely using a directory as its current working directory. Consequently a visible folder such as `ComfyUI` incorrectly appears unoccupied while `cmd.exe` is working in that folder or one of its descendants, and F5 cannot repair the missing result.

## What Changes

- Add a bounded, read-only Windows process-current-directory discovery source alongside Restart Manager.
- Treat a process current directory as occupying that directory and every ancestor folder at a component boundary.
- Merge and deduplicate current-directory and Restart Manager owners without exposing new process-control capabilities or sensitive process data.
- Make the existing generation-scoped F5/manual-refresh path recompute and clear current-directory owners.
- Extend unit, integration, and blocking headful UTIT coverage with a real `cmd.exe` nested-current-directory fixture.

## Non-Goals

- No process shutdown, termination, handle closure, command-line/environment disclosure, continuous polling, or replacement of Restart Manager file ownership.
- No public extension ABI or capability change and no non-Windows implementation.

## Capabilities

### New Capabilities

None.

### Modified Capabilities

- `lock-owner-host-service`: Extend discover-only ownership to include bounded process current-directory ancestry, with existing privacy, cancellation, cache, and stale-generation guarantees.
- `source-example-plugin-suite`: Require the production Lock owner example and headful gate to demonstrate nested `cmd.exe` current-directory detection, parent projection, F5 refresh, and clearing after exit or directory change.

## Impact

- Windows-native discovery in `crates/explorer-shell-win`, including audited process snapshot, native/WOW64 remote process-parameter reads, handle cleanup, deadlines, and path matching, plus the exact root Windows binding feature.
- Internal batching, live cancellation and ABI panic containment in `crates/explorer-extension-host`, an internal deadline terminal in `explorer-model`, and lock-owner result composition/cache refresh in `crates/explorer-app`.
- Existing Lock owner extension fixture, Windows headful script, and `uitest/manifest.json` evidence contract.
- English and Traditional Chinese example documentation and offline package reproduction.
- No ABI break, no new extension capability, no process shutdown/termination path, no continuous polling, and no new external runtime dependency.
