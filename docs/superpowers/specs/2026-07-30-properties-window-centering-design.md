# Properties Window Centering Design

## Goal

Windows Shell Properties sheets opened by SuperExplorer currently appear at the top-left of the
desktop. A Properties sheet must instead open centered over the active SuperExplorer window. If
that window is unavailable, the sheet must center in the pointer monitor's work area. The final
rectangle must remain fully usable within the selected work area.

## Scope

This change applies only to the host-owned `properties` canonical verb executed on the persistent
Shell STA. It covers file, folder, compatible multi-selection, executable, and script Properties
sheets. It does not replace the Shell property sheet, change its pages, alter third-party handlers,
or create an application-owned imitation dialog.

## Considered Approaches

### Process-scoped one-shot WinEvent hook (selected)

Install a process-scoped `EVENT_OBJECT_SHOW` WinEvent hook immediately around the Properties
`IContextMenu::InvokeCommand` call. Shell handlers can create the sheet on a helper thread, so a
thread-only CBT hook is insufficient. On the first eligible top-level dialog show, read its actual
dimensions, compute the owner-relative centered rectangle, clamp it to the monitor work area, and
reposition it with `SetWindowPos`. This avoids polling and the visible top-left-to-center jump.

### Post-creation polling

Enumerate owned dialogs after invocation and move the matching property sheet. This is simpler but
can visibly flash at the original position and introduces timing-dependent polling and cleanup.

### Hidden-owner placement

Move the hidden Shell owner and rely on each handler's default placement. This is less code but is
not consistently honored by in-box and third-party property sheet implementations.

## Architecture

`PropertiesCenteringHook` is a small RAII component owned by the synchronous Properties invocation
on the persistent STA. Its process-scoped, mutex-protected one-shot state contains only the
validated SuperExplorer owner and invocation point. Installation and removal are balanced even
when invocation fails. Because some Shell handlers return before showing their same-process helper
thread window, ownership of the hook is transferred to a bounded two-second asynchronous lease
after invocation. The persistent STA returns immediately to its message loop. A 2 ms bounded
same-process dialog enumeration fallback covers handlers whose show event is not delivered to the
hook; it uses the same one-shot claim state and exits immediately after placement.

The WinEvent callback, or bounded same-process enumeration fallback, handles the first eligible
visible top-level dialog in the app process. It ignores child windows, stale handles, and unrelated
non-dialog windows. It uses the active SuperExplorer rectangle when valid. Otherwise it chooses the
monitor nearest the invocation point. The target work area comes from `GetMonitorInfoW`.

The positioning calculation is a pure function:

1. Center the actual dialog width and height on the anchor rectangle.
2. If the dialog fits, clamp its left/top so its entire rectangle stays inside the work area.
3. If it is larger than a work-area dimension, align that dimension to the work-area origin so the
   title bar and maximum usable content remain reachable.

`SetWindowPos` uses `SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE`; Shell retains ownership, focus,
Z-order, sizing, DPI behavior, and modal/modeless lifecycle.

## Failure Handling

Failure to install the hook, query the owner/monitor/dialog rectangle, or reposition the dialog
must not prevent Properties from opening. The normal Shell invocation continues and emits a
privacy-safe diagnostic. Hook and thread-local state are always cleared before the invocation
returns.

## Testing

Rust unit tests cover center calculation, each clamp edge, oversized dialogs, and fallback anchor
selection inputs without creating UI.

The existing `host-built-in-context-command-headful` UTIT is extended for file, folder,
multi-selection, executable, and script property sheets. For each real Shell sheet it records the
app and dialog rectangles and asserts:

- the dialog center is within a small DPI-tolerant distance of the expected owner-relative center;
- the dialog remains within the selected monitor work area;
- the sheet keeps native filesystem property controls and the exact target title;
- Escape closes it and a subsequent genuine right-click command remains usable.

The result report and manifest coverage expose the centering assertions so a sheet that merely
opens at `(0, 0)` cannot pass.
