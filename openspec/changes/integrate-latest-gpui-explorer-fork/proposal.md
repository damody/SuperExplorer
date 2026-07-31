## Why

SuperExplorer depends on four GPUI-CE extensions that are not available from the unmodified upstream dependency used by the application. The vendored submodule is behind upstream `main`, so the Explorer-specific changes need to be replayed on the latest upstream base and published from an explicitly owned fork before the host dependency can be upgraded safely.

## What Changes

- Create a traceable `damody/gpui-ce-explorer` fork history based on the latest `gpui-ce/gpui-ce` `main` commit.
- Replay and preserve Explorer-specific editable-text selection, accessibility, Windows external-drop negotiation, and related platform behavior.
- Resolve upstream conflicts without dropping public APIs used by SuperExplorer.
- Validate the fork itself and compile SuperExplorer against the integrated submodule revision.
- Push the validated result to `https://github.com/damody/gpui-ce-explorer.git` without rewriting published history.
- Update the parent repository submodule pointer and dependency source to the validated fork revision.

## Capabilities

### New Capabilities

- `gpui-explorer-fork-integration`: Defines reproducible upstream integration, required Explorer extensions, compatibility validation, and safe publication behavior for the GPUI fork.

### Modified Capabilities

None.

## Impact

This affects the `vendor/gpui-ce` submodule, the SuperExplorer workspace GPUI dependency source and lockfile, Windows platform integration, editable-text rendering, accessibility semantics, external drag/drop negotiation, CI build validation, and the `damody/gpui-ce-explorer` remote repository.
