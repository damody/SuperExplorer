## Why

Local APK files currently have no direct installation workflow in SuperExplorer, even though the application already integrates with ADB. Users also reach a dead end when ADB is missing; the application should use an existing system installation when possible and provide a safe, official managed fallback when it is not.

## What Changes

- Add an `Install` submenu for exactly one local `.apk` file, populated with connected Android device names and keyed internally by device serial.
- Install the selected APK with argument-safe `adb -s <serial> install -r <path>` execution and existing background operation status/error delivery.
- Resolve and validate ADB with system `PATH` first, recognized Android SDK locations second, and SuperExplorer-managed Platform-Tools last.
- Represent unavailable, offline, unauthorized, empty, loading, stale, and refresh device-list states without blocking the file-surface UI.
- When no usable ADB exists, expose a user-initiated download of Google's official Windows Platform-Tools into a private per-user location with bounded transfer, safe ZIP extraction, validation, atomic activation, and rollback.
- Add deterministic offline fixtures plus focused and headful verification for resolver precedence, archive safety, device discovery, menu behavior, and APK installation.
- Do not modify system `PATH`, replace an SDK, auto-uninstall applications, permit downgrades, support split APK sets, or install remote files.

## Capabilities

### New Capabilities

- `local-apk-device-install`: Local APK context-menu eligibility, per-device submenu behavior, exact-device installation, operation lifecycle, and failure recovery.
- `managed-adb-tooling`: Existing ADB discovery/validation precedence and the user-initiated Google Platform-Tools download, safe installation, activation, and rollback contract.

### Modified Capabilities

None.

## Impact

The change affects the ADB provider/runner in `crates/explorer-remote`, application service composition in `crates/explorer-app`, owned local context-menu construction and operation status integration in `crates/explorer-ui`, localization strings, test fixtures, and UITest coverage. It introduces an HTTPS archive-download implementation and ZIP extraction dependency or reuses an existing workspace facility if one is already suitable. There is no public extension ABI change, persistent user-data migration, system environment mutation, administrator requirement, or automatic network access before the user selects the download command.
