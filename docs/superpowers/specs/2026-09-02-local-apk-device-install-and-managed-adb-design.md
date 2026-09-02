# Local APK Device Install and Managed ADB Design

## Context

SuperExplorer can browse local files and already contains an ADB provider, but a local APK cannot be installed directly from its file context menu. The existing ADB resolver checks a configured application root and `PATH`, while the product has no user-facing recovery path when ADB is absent. The feature must use an existing system ADB when available and only download Google's official Platform-Tools when no usable installation exists.

## Goals

- Show an `Install` submenu for a single `.apk` file selected in a Local location.
- List every connected Android device as a separate submenu item using its device name.
- Install the APK on the selected device with `adb install -r`.
- Use an existing system or Android SDK ADB without downloading another copy.
- When no usable ADB exists, let the user download and install Google's official Windows Platform-Tools inside SuperExplorer's private application-data area.
- Report discovery, download, installation, cancellation, timeout, and ADB failures through the existing operation-status surfaces.

## Non-goals

- Modifying the system `PATH` or replacing a system Android SDK installation.
- Automatically uninstalling an application, allowing a downgrade, or bypassing signature errors.
- Installing split APK sets such as `.apks`, `.xapk`, or multiple APK selections.
- Treating remote ADB or SFTP files as directly installable APK inputs.

## User Experience

For a single local file whose extension is `.apk` case-insensitively, the context menu includes `Install`.

When ADB is usable, opening the submenu shows one row per result from device discovery. A usable device row displays its human-friendly device name and installs to that device when selected. The command retains the unique ADB serial internally, so duplicate device names cannot target the wrong device. `offline`, `unauthorized`, and other non-installable devices remain visible as disabled rows with their state. If no devices are reported, the submenu shows a disabled `No devices detected` row and a `Refresh devices` command.

When no usable ADB is found, the submenu contains `Download and install Google ADB...`. Selecting it starts a background Platform-Tools installation. On success, SuperExplorer reruns tool resolution and device discovery and refreshes the submenu state. A download or extraction failure leaves the prior state intact and presents an actionable error.

## Architecture

### ADB tool resolution

An `AdbToolResolver` returns a validated executable plus its provenance. Resolution order is:

1. `adb.exe` available through the process `PATH`.
2. ADB under recognized existing Android SDK locations or an explicitly configured SDK root.
3. The active SuperExplorer-managed Platform-Tools installation.

Each candidate must be a regular executable file and successfully complete a bounded `adb version` probe. An invalid candidate does not prevent evaluation of the remaining candidates. Diagnostics identify rejected candidates without exposing unrelated environment values.

This ordering ensures a usable system ADB is used directly. SuperExplorer does not download, overwrite, or modify it. If a system ADB later becomes available, it takes precedence over the managed copy on the next resolution.

### Managed Platform-Tools installation

An `AdbToolInstaller` downloads the Windows Platform-Tools archive only from Google's official HTTPS endpoint. The operation has a bounded download size, cancellation, timeout, and clear progress states. It writes to an operation-specific temporary directory, validates the archive layout, rejects absolute paths, parent traversal, links, and entries outside the extraction root, and requires the expected `platform-tools/adb.exe` payload.

After extraction, the installer runs the same bounded version probe. Only a successful probe permits an atomic promotion to the managed tools directory. Failed and cancelled attempts remove their temporary content and retain any previously working managed version. The feature does not modify the system `PATH` and does not require administrator privileges.

The official URL and any pinned integrity metadata are centralized rather than embedded in UI code. Release packaging and tests must be able to substitute a local fixture without contacting the network.

### Device discovery

`AdbDeviceDiscovery` runs `adb devices -l` with a bounded command runner and parses each device into:

- stable serial used for all commands;
- human-friendly display name, preferring the reported model/device name and falling back to the serial;
- connection state and an installable flag.

Discovery produces an immutable snapshot for the menu. Refresh replaces the snapshot as a unit, preventing rows from mixing results from different probes.

### Context-menu integration

The Local file context-menu contribution is eligible only for exactly one regular `.apk` file. Device enumeration must not block the GPUI callback or Shell menu message loop. The submenu renders the most recent snapshot and exposes loading, unavailable, empty, and error states consistently with existing owned context-menu behavior.

Selecting a device captures both the canonical local APK path and device serial from that snapshot and submits a background operation. It never reconstructs a command string from the visible label.

### APK installation

The install operation invokes the resolved executable with separate process arguments equivalent to:

```text
adb -s <serial> install -r <absolute-apk-path>
```

Arguments are passed without shell interpolation, preserving spaces and Unicode. The operation uses existing bounded output capture, cancellation, timeout, and status reporting. Success is reported only when ADB exits successfully and reports a successful install. Non-zero exit, signature mismatch, downgrade rejection, authorization loss, disconnect, cancellation, and timeout preserve the relevant bounded ADB diagnostics.

The operation never automatically adds `-d`, uninstalls an existing package, or retries against a different device.

## Data Flow

1. The user opens the context menu for one local APK.
2. The menu requests the current validated ADB resolution and device snapshot.
3. If no tool exists, the menu offers the managed download command.
4. If a tool exists, device rows are keyed by serial and labeled by device name.
5. The user selects a device; the UI submits an install request containing the validated ADB path, serial, and canonical APK path.
6. The background runner executes `install -r` and publishes lifecycle events to the existing operation-status UI.
7. Terminal success or failure is shown without blocking the file surface.

## Error Handling and Safety

- Tool probing, discovery, download, extraction, and installation are cancellable and bounded by time and captured-output limits.
- Archive extraction is confined to a verified temporary root and cannot overwrite arbitrary files.
- A partial managed installation is never advertised as active.
- Device names are presentation-only; serials are used for command targeting.
- The APK path must still resolve to the originally selected local regular file when the operation begins; stale or remote inputs fail before ADB execution.
- No credentials, full environment dump, or unbounded ADB output enter logs or user-visible diagnostics.

## Testing and Verification

Focused tests cover:

- resolver precedence across `PATH`, Android SDK, managed ADB, invalid candidates, and later system-ADB appearance;
- official-download routing, cancellation, size limits, safe ZIP extraction, atomic promotion, rollback, and fixture substitution;
- `adb devices -l` parsing for multiple devices, duplicate names, fallback names, `offline`, `unauthorized`, malformed rows, and no devices;
- context-menu eligibility for Local single `.apk` selection and exclusion of directories, multiple selections, non-APK files, ADB, and SFTP locations;
- argument-safe `install -r` invocation with spaces and Unicode, exact serial targeting, success, non-zero exit, disconnect, cancellation, and timeout;
- non-blocking menu discovery and terminal operation-status delivery.

Headful verification uses a controlled fake ADB for deterministic multi-device submenu behavior and, when available, a real authorized Android device for one successful install/update cycle. Network verification confirms the production download points only to Google's official Platform-Tools endpoint; normal automated tests remain offline.

## Alternatives Considered

Installing ADB globally and modifying `PATH` would benefit other applications but introduces administrator and environment-mutation concerns and may conflict with an existing SDK. Bundling Platform-Tools in every SuperExplorer installer improves offline availability but increases package size and couples ADB updates to application releases. The selected design instead prefers any usable system ADB and keeps an isolated, user-requested managed fallback.
