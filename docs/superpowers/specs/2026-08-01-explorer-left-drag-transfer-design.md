# Explorer-style left-drag file transfer

## Goal

Dragging selected filesystem items with the left mouse button must transfer them to a writable folder with Windows Explorer semantics. The interaction must work inside SuperExplorer and remain interoperable with Windows Explorer through the existing OLE drag-and-drop pipeline.

## Interaction contract

- A left-button press on a selected file or folder creates a drag candidate. Crossing the Windows drag threshold begins one OLE drag for the current selection.
- Holding `Ctrl` forces Copy when the target accepts copies.
- Holding `Shift` forces Move when the target accepts moves. `Shift` selection still occurs on mouse-down, but it must not suppress drag initiation.
- With no modifier, the native Shell preference decides the effect: same-volume destinations move and cross-volume destinations copy.
- Dropping on a writable folder row targets that folder. Dropping on writable file-view background targets the current folder. Invalid, read-only, or self/descendant destinations reject the drop.
- Escape, button release without a valid target, focus loss, navigation, tab changes, and window shutdown clear transient drag state exactly once.
- Existing conflict handling remains `Prompt`, matching Explorer rather than silently overwriting.

## Architecture

Keep the current OLE source and GPUI Windows OLE target. The UI owns selection, threshold detection, target hit testing, and visual cues. The Shell worker owns `IDataObject`, `DoDragDrop`, native effect negotiation, and the resulting file operation. No filesystem mutation runs on the UI thread.

Effect negotiation uses live OLE key state. Explicit `Ctrl` and `Shift` override the source preference. For an unmodified drag, the source must not falsely force Move; the Shell/target determines the same-volume versus cross-volume default from real source and destination paths.

## Verification

- Unit tests cover threshold crossing, Shift-start eligibility, Ctrl/Shift negotiation, default same/cross-volume resolution, invalid targets, and terminal cleanup.
- UI tests prove folder-row and background drops create the correct typed operation with `ConflictDecision::Prompt`.
- Shell tests prove the native preferred effect and performed disk result.
- UTIT registers deterministic contract coverage plus the existing headful OLE interoperability fixture for left-drag move/copy/cancel.

