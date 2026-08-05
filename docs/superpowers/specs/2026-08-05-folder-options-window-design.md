# Folder Options window design

## Outcome

`Folder options` opens in one dedicated, modeless GPUI window instead of an
overlay inside the Explorer window. The Explorer remains usable while the
options window is open. The options content has its own visible vertical
scrollbar and never scrolls the file view behind it.

## Window ownership and lifecycle

The application owns a single Folder Options window controller. Opening Folder
Options creates the window when none exists; opening it again activates the
existing window rather than creating another copy. The controller owns the
window handle, a draft copied from the current applied settings, the active
page, and one scroll handle per page.

The options window is modeless and shares only typed setting commands with the
Explorer window. It does not borrow the Explorer view entity or render an
overlay in that window. Closing the owning Explorer application closes the
options window safely. Closing an individual Explorer window does not leave a
controller pointing at a dead entity.

The window uses a normal Windows title bar, a sensible initial size matching the
current dialog, and a minimum size that keeps the page tabs and action buttons
usable. It may be resized. A newly opened window is centered over the active
Explorer window when Windows provides a usable owner position; otherwise normal
Windows placement is used.

## Draft, Apply, OK, and Cancel

Opening the window takes a fresh draft from the currently applied settings.
Editing controls changes only that draft.

- `Apply` validates and applies the draft without closing the window, then marks
  the resulting state as the new cancellation baseline.
- `OK` performs the same validation and apply operation, then closes the window.
- `Cancel`, Escape, and the title-bar close button discard changes made after
  the latest successful Apply and close the window.

Applying settings notifies every live Explorer window through the existing
application/state synchronization path. If application state changes outside
the options window while its draft is dirty, the draft is not silently
overwritten. A later Apply uses the typed draft as the authoritative user
choice.

## Layout and scrolling

The title/page tabs at the top and `OK`, `Cancel`, and `Apply` buttons at the
bottom are fixed. Only the middle page viewport scrolls. General, View, and
Extensions each have an independent `ScrollHandle`, so switching pages restores
the last position used on that page.

The page viewport always reserves space for a right-side vertical scrollbar.
The thumb reflects the current page's content and viewport sizes and supports
wheel, touchpad, track click, thumb drag, Page Up/Page Down, Home, and End.
When content fits, the scrollbar track remains visible in its disabled/light
state and the thumb fills the track. Wheel and pointer events stop propagation
at the options window boundary, so the Explorer navigation pane and file list
cannot move underneath it.

At 100% through common high-DPI scales, layout calculations remain in GPUI
logical pixels. Resizing clamps the content viewport without scaling pointer
coordinates a second time.

## Keyboard and focus

The first interactive control on the active page receives focus when the window
opens. Tab and Shift+Tab remain inside the options window. Escape cancels and
closes. Enter activates the focused control; it does not implicitly confirm
while a button, checkbox, link, or editable control owns focus. Switching pages
moves focus into that page without changing another page's scroll offset.

## Code boundaries

The existing folder-options page builders are extracted from the overlay shell
and reused by a dedicated Folder Options view entity. Page content remains in
`explorer-ui`; application window creation, single-instance activation, and
cross-window setting synchronization remain in `explorer-app`. Folder option
draft mutation continues to use typed actions/reducers rather than callbacks
that directly edit application state.

The old `folder-options-overlay` render path and its backdrop event capture are
removed after the new window is connected. Other overlays and the existing
About dialog are outside this change.

## Failure behavior

If window creation fails, the current Explorer stays responsive and records a
diagnostic; it does not set an `open` flag that prevents a retry. Validation or
persistence failures keep the options window open, preserve the draft, and show
an actionable inline error. A stale or already-closed window handle is cleared
before a replacement is created.

## Verification

Rust unit tests cover the single-instance controller, fresh-draft creation,
Apply baseline semantics, OK/Cancel/Escape/title-close transitions, page-local
scroll position, resize clamping, and stale-handle recovery.

UITEST opens Folder Options from a real Explorer window and verifies that it is
a distinct native window, the Explorer remains operable, reopening activates
the same window, and the right scrollbar is visible. It scrolls long General,
View, and Extensions content by wheel and thumb drag; verifies page positions
are restored; proves the underlying file and navigation views do not scroll;
and covers Apply, OK, Cancel, Escape, title-bar close, minimum-size layout, and
representative 100%, 125%, 150%, and 200% DPI runs.

## Alternatives rejected

A Win32 controls dialog would look native but would duplicate the existing GPUI
settings controls and extension UI. Keeping the in-window overlay and only
adding overflow scrolling would not satisfy the dedicated-window requirement.
Allowing one options window per Explorer window would create conflicting drafts
and ambiguous Apply order, so the application uses one modeless instance.
