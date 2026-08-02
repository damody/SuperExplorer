# Context-menu second-right-click freeze design

## Scope

Fix the remaining native Shell context-menu replacement freeze. When a popup is visible and the
user right-clicks another unobscured SuperExplorer file or folder, the old popup closes and one
new popup opens for the second target without blocking the UI. Existing popup interaction,
Escape/outside dismissal, selection semantics, and third-party Shell extensions remain unchanged.

## Root cause and selected approach

The current scoped low-level mouse hook calls `EndMenu` synchronously from the hook callback after
capturing the second right-button release. This places native modal-menu teardown inside the
low-level input callback and can create re-entrant waiting between the hook, `TrackPopupMenuEx`,
and foreground/input restoration.

The selected approach keeps genuine Win32 input replay but makes cancellation asynchronous. The
hook captures and suppresses the matched second right-button gesture, stores only its final screen
point, and posts `WM_CANCELMODE` to the hidden owner of the active popup. Posting returns
immediately; the native menu thread performs teardown in its normal message loop. Only after
`TrackPopupMenuEx` returns, the hook and popup resources are destroyed, and the terminal result is
published may the existing tagged `SendInput` path replay the gesture to SuperExplorer.

Calling `EndMenu` from the hook is rejected because it is re-entrant. Reusing the old `HMENU` is
rejected because it belongs to the old immutable target. Dispatching a semantic item command is
rejected because it bypasses GPUI hit testing and can conceal selection or virtualization bugs.

## Lifecycle

1. The popup worker records its hidden owner HWND in the scoped hook state.
2. An untagged right-button down/up pair whose points resolve to the originating SuperExplorer root
   is suppressed and captured. Popup and submenu clicks pass through unchanged.
3. On the matching release, the hook posts one `WM_CANCELMODE` to the popup owner and returns
   without synchronously ending the menu.
4. `TrackPopupMenuEx` returns through normal cancellation. The worker removes the hook, destroys
   the old popup resources, publishes the correlated terminal, and only then schedules one tagged
   replay. The replayed mouse request immediately supersedes stale pending UI state; an old terminal
   that reaches the reducer later is ignored by correlation.
5. Window/point validity, physical-button release, and full `SendInput` acceptance are rechecked.
   Failure becomes bounded cancellation and never leaves a pressed button or retained request.
6. Multiple mouse replacement attempts supersede to the latest complete gesture; keyboard or
   programmatic requests remain serialized, and stale terminal events cannot reopen an older target.

## Testing

Rust unit tests cover one-shot asynchronous cancellation, incomplete/wrong-owner/tagged gestures,
latest-request promotion, stale terminal rejection, and cleanup. The headful UTIT opens an item
popup, genuinely right-clicks a different item, asserts the UI remains responsive, invokes Copy
from exactly one replacement popup, and proves the clipboard names the second item. A repeated
alternating-target loop checks that hooks, popup/menu handles, workers, and threads remain bounded.

## Acceptance criteria

- A second right-click never freezes SuperExplorer or the native popup loop.
- The old popup is fully gone before one replacement popup opens.
- The replacement selection and invoked command target the second item exactly.
- Existing menu clicks, submenus, Escape, outside dismissal, and the next ordinary right-click work.
- No synchronous native-menu teardown remains in the low-level hook callback.
- Deterministic tests, focused headful UTIT, formatting, build, and strict OpenSpec validation pass.
