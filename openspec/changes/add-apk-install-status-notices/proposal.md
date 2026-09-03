## Why

Selecting an APK device currently starts a background ADB installation with no visible running state and collapses success into a generic context-menu completion, leaving users unsure whether anything is happening or finished. Super Explorer needs truthful in-app lifecycle notices even though ADB cannot provide a reliable install percentage.

## What Changes

- Publish a correlated APK-install started event before ADB spawn and exactly one success, failure, cancellation, or timeout terminal.
- Show an immediate in-app indeterminate `安裝中` notice containing the APK name and friendly device name, without fabricated percentage or byte progress.
- Replace each running notice with a clear terminal message and apply bounded success/failure retention and fade behavior.
- Keep concurrent installations independent by request ID and reject duplicate, late, stale, or terminal-without-start events.
- Preserve system-first ADB resolution, managed Google Platform-Tools fallback, canonical APK/serial revalidation, and non-blocking native menu behavior.
- Add controlled-worker, UI-state, integration, headful, supplied-APK, and final user-perspective evidence.

Non-goals are system notifications, installing split or remote APKs, changing ADB resolution precedence, or inventing percentage progress.

## Capabilities

### New Capabilities

- `apk-install-status-notices`: Defines correlated in-app APK installation started/terminal status, concurrent isolation, truthful wording, retention, failure handling, and user-visible verification.

### Modified Capabilities

None.

## Impact

- `explorer-model`: typed APK status/session events and terminal classification.
- `explorer-app`: context-menu worker event sequencing and ADB outcome mapping.
- `explorer-ui`: bounded notice state, formatting, rendering, and fade lifecycle.
- Tests/scripts/OpenSpec evidence for fake ADB, `qq9.3.55.apk`, and headful lifecycle verification.
- No persisted-session schema, public extension ABI, external dependency, system PATH, or notification permission change.
