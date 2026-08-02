## Why

SuperExplorer currently starts an incomplete left-button drag path: Shift suppresses drag initiation and unmodified native drags falsely prefer Move regardless of the destination volume. Users therefore cannot reliably move or copy files between folders with the Windows Explorer gestures they already expect.

## What Changes

- Make left-button dragging work for selected files and folders after the native Windows drag threshold.
- Make `Ctrl` force Copy and `Shift` force Move throughout the native OLE drag.
- Let an unmodified drag use Explorer's same-volume Move and cross-volume Copy default.
- Accept drops on writable folder rows and current-folder backgrounds while rejecting invalid self, descendant, and read-only targets.
- Preserve asynchronous Shell file operations, conflict prompting, cancellation, and bounded transient state.
- Add deterministic unit, integration, and UTIT regression coverage.

## Capabilities

### New Capabilities

- `explorer-left-drag-transfer`: Explorer-compatible left-button file transfer, modifier negotiation, destination validation, and lifecycle behavior.

### Modified Capabilities

None.

## Impact

- `explorer-model`: drag-effect negotiation and destination semantics.
- `explorer-ui`: file-row drag initiation, target routing, and visual/transient state.
- `explorer-shell-win`: OLE source preference and real file-operation dispatch.
- `explorer-uitest` manifest and existing Windows OLE smoke automation.

