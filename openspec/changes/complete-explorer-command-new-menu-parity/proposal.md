## Why

The command bar still diverges from Windows File Explorer: New creates only a folder, locally
redrawn glyphs are not the requested Microsoft assets, command popups leak hover/click hit testing
to file rows and do not own focus, and Shift+Delete never reaches its pending confirmation. These
are high-frequency interactions and must be corrected before the remaining combined Explorer
parity gates can be credible.

## What Changes

- Replace the one-shot New Folder button with a focusable Explorer-style New popup backed by the
  current user's bounded ShellNew registration catalog and safe Folder/Text fallbacks.
- Add typed new-item descriptors and creation requests supporting folders, empty files, bounded
  registered data, and trusted template files through the Shell STA and `IFileOperation`.
- Replace every app-owned chrome glyph with vendored regular SVG assets from
  `microsoft/fluentui-system-icons`, including offline asset loading, source metadata, and MIT
  attribution.
- Give New, Sort, View, More, and Extensions popups exclusive hit testing, enabled-row keyboard
  navigation, native/UIA focus, hover highlighting, activation, close, and focus restoration.
- Complete Shift+Delete with a modal confirmation, keyboard/pointer Confirm and Cancel, exactly-once
  permanent deletion, and stale-state cleanup.
- Render the selected image's real decoded thumbnail in the preview pane with bounded asynchronous
  loading, aspect-ratio-preserving containment, stale-selection rejection, and truthful fallback text.
- Add deterministic, process/Shell, source-audit, headful UIA, raster, disk-effect, and ten-run
  interaction regression evidence, then update the umbrella roadmap task status truthfully.

## Capabilities

### New Capabilities

- `explorer-command-menu-parity`: Covers registered New-item creation, official Fluent chrome
  icons, popup hit testing/focus/activation, and confirmed Shift+Delete behavior.

### Modified Capabilities

None. The umbrella roadmap remains active; this focused capability supplies the detailed contract
and evidence needed to close its related combined parity tasks.

## Impact

Affected areas include `explorer-model` owned protocols, `explorer-shell-win` registry and
`IFileOperation` adapters, `explorer-ui` actions/state/chrome/assets/focus, `explorer-app`
composition, Fluent SVG third-party attribution, UITEST manifest/headful scripts, and the active
umbrella roadmap. The session schema and existing public navigation commands remain compatible.
