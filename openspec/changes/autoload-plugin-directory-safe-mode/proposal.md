## Why

Installed plugins are currently loaded only because the installer writes a fixed set of `--plugin-dll` arguments into shortcuts, so adding a valid plugin to the installation does nothing and per-user extension choices cannot reliably govern the next launch. Production startup needs package-directory discovery tied to durable desired state and a fail-closed global recovery path after plugin faults.

## What Changes

- Discover bounded direct-child `.sepack` archives from the executable-relative `plugins` directory and pass them through the existing importer, validator, sealed store, resolver, and native lifecycle.
- Treat newly discovered compatible packages/features as enabled unless the existing desired-state JSON explicitly disables them.
- Persist Folder Options > Extensions global/package/feature switches atomically and apply those choices at the next startup.
- Latch global Safe Mode after a caught plugin panic, abnormal callback termination, or stale durable callback marker; the following startup executes no plugin code.
- Require an explicit **Re-enable all plugins** action and restart to clear global Safe Mode while preserving individual switches.
- Install complete `.sepack` archives and remove fixed Plugin arguments from shortcuts; retain `--plugin-dll` only as a development/test compatibility override.

## Capabilities

### New Capabilities

- `plugin-directory-autoload`: Production `.sepack` discovery, validated manifest identity, default enablement, sealing, and installer/startup composition.
- `global-extension-safe-mode`: Persistent all-plugin fault latch and explicit recovery workflow.

### Modified Capabilities

- `extension-options-management`: Extensions switches must persist desired state and expose global Safe Mode recovery without rewriting individual choices.

## Impact

This affects `explorer-app` startup and Folder Options composition, `explorer-extension-host` discovery/state/fault lifecycle, `explorer-ui` Extensions presentation/actions, installer layout and shortcuts, bundled SDK fixtures, diagnostics, restart integration tests, and installed-path verification. The state formats remain versioned and per-user; no network service, new external dependency, recursive scan, file watcher, or Rust DLL hot-unload is introduced.
