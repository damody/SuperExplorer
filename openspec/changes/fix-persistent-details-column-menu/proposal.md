## Why

The new top-level Details column popup fixed clipping and visual parity, but regressed File Explorer's persistent multi-toggle behavior: every column click closes the popup before users can see or repeat the check-state change. The popup must keep its independent-window benefits while restoring immediate, repeatable column toggling without unsafe GPUI re-entry.

## What Changes

- Classify Details popup commands as terminal actions or persistent check toggles.
- Keep one popup session, HWND, position, and scroll offset alive while enabled column rows are repeatedly checked and unchecked.
- Repaint the selected check mark immediately and reconcile the requested visibility state on the foreground UI during the same popup session.
- Retain terminal dismissal for auto-size commands and ordinary dismissal for Escape, outside click, deactivation, and replacement gestures.
- Carry persistent popup events from the background message loop to GPUI without direct cross-thread UI access or re-entrant borrowing.
- Add unit, resource-lifecycle, and headful evidence for repeated on/off toggles, fixed `Name`, stable popup identity, immediate UI state, and dismissal.

Non-goals are changing filesystem context-menu command semantics, changing the set or order of available Details columns, or redesigning the popup visual style.

## Capabilities

### New Capabilities

- `persistent-details-column-popup`: Defines persistent Details-column check toggles, immediate native/UI reconciliation, stable popup-session behavior, terminal commands, dismissal, and failure recovery.

### Modified Capabilities

None.

## Impact

- `crates/explorer-shell-win`: immersive popup activation, check-state repaint, persistent event publication, and lifecycle tests.
- `crates/explorer-ui`: popup command metadata, requested-state reconciliation, foreground event pumping, and Details tests.
- `crates/explorer-app`: background popup integration and session result routing.
- `scripts/smoke_details_column_popup.ps1` and chooser smoke coverage: repeated toggling and stable native popup identity.
- No external dependency, persisted-session schema, or public extension ABI change is intended.
