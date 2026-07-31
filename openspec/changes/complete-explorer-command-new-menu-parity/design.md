## Context

The command bar currently draws its own glyph geometry, wires New directly to Create Folder, and
renders popups without an independent focus surface or mouse occlusion. Shift+Delete reaches an
in-memory pending confirmation but no modal consumes it. The application already owns a Shell STA,
typed file-operation requests, native focus synchronization, and headful UITEST hooks, so the change
extends those boundaries instead of introducing a second UI or operation stack.

The implementation must coexist with concurrent work in the shared tree, remain offline after build,
avoid loading third-party Shell code in the UI process, and keep arbitrary registry data bounded.

## Goals / Non-Goals

**Goals:**

- Provide an Explorer-like New menu derived from safe current-user ShellNew registrations plus
  deterministic Folder and Text Document fallbacks.
- Create folders, empty files, bounded registered data files, and trusted registered templates through
  the existing Shell STA and IFileOperation path.
- Render app-owned chrome with official regular Fluent System Icons SVG assets and attribution.
- Make command popups true focus and hit-test surfaces with complete enabled-item activation.
- Finish Shift+Delete with an accessible modal and exactly-once permanent deletion.
- Show a real bounded image preview for one selected image using the existing thumbnail pipeline.
- Add deterministic, integration, headful, raster, disk-effect, and ten-run regression evidence.

**Non-Goals:**

- Loading arbitrary ShellNew handlers, commands, or preview extensions in the UI process.
- Reproducing undocumented Explorer ordering pixel-for-pixel across every Windows build.
- Claiming unsupported registered types as creatable; unsafe registrations are omitted truthfully.
- Replacing file-type thumbnails or content icons with Fluent chrome icons.

## Decisions

### Use a bounded owned ShellNew catalog

The Shell worker enumerates the per-user Explorer ShellNew class list and merged class registrations,
normalizes each supported registration into an owned descriptor, and returns only Folder, NullFile,
bounded Data, or trusted FileName templates. Handler and Command registrations are excluded. Folder
and Text Document are deterministic fallbacks so the menu is always useful.

This is preferred over a fixed list, which becomes inaccurate when applications register types, and
over hosting Explorer's native context menu, which would surrender focus, accessibility, timeout, and
test control to arbitrary extensions.

### Extend the typed file-operation protocol

A CreateItem request carries parent, collision-safe requested name, and an owned safe creation recipe.
The Shell STA performs creation via IFileOperation and applies bounded initial data only after a
successful item creation. Results use the existing operation progress and completion pipeline.

### Vendor a minimal official SVG set

Only icons referenced by ExplorerIcon are vendored from microsoft/fluentui-system-icons regular SVG
assets. An AssetSource serves embedded bytes so release builds do not depend on source-tree files or
the network. A source manifest and upstream MIT license preserve provenance. File thumbnails and Shell
icons keep their existing providers.

### Treat command menus as one exclusive popup state machine

New, Sort, View, More, and Extensions share mutual exclusion, active-row navigation, dedicated native
focus, pointer hover selection, enabled-row activation, Escape dismissal, and focus restoration. Popup
roots occlude pointer hit testing so file rows cannot hover or activate through them. Disabled commands
remain visibly disabled and are never focus targets.

### Render permanent delete as an application modal

RequestPermanentDelete snapshots selected items and opens an occluding modal. Confirm consumes the
snapshot before dispatching one confirmed PermanentDelete request; Cancel and Escape discard it.
Navigation, tab close, and shutdown clear stale pending state.

### Use a bounded trusted raster fast path behind the asynchronous preview boundary

When the preview pane is visible and exactly one filesystem image is selected, the root schedules a
larger ActiveVisible request keyed by item identity, selection generation, DPI, and requested size.
Common raster formats use the app's memory/dimension-bounded pure Rust decoder on a dedicated one-slot
background lane; this avoids cold Shell handler startup and prevents background thumbnail traffic from
starving the selected preview. EXIF orientation is applied before bounded scaling. Unsupported formats
continue through the isolated broker/worker thumbnail path, and offline/cache-only requests retain the
existing broker policy. Ready pixels are converted at the renderer boundary and passed to the pure
chrome view model. The pane uses ObjectFit::Contain and never decodes or reads the file on the UI thread.
Changed selection, tab, generation, size, or pane visibility invalidates the presentation and stale
completions are ignored.

## Risks / Trade-offs

- [Registry registrations can be malformed or huge] -> Bound strings and data, canonicalize template
  paths, allow only supported recipes, deduplicate extensions, and fail individual entries closed.
- [Some Explorer New entries rely on undocumented handlers] -> Omit unsafe entries rather than create
  corrupt files; add dedicated safe adapters only when behavior is testable.
- [SVG asset names can drift upstream] -> Vendor pinned bytes and record the upstream commit/source;
  source audits fail if a mapped asset is absent.
- [Popup focus changes can regress keyboard navigation] -> Centralize navigation and add unit plus
  headful UIA coverage for open, move, activate, dismiss, and restoration.
- [Permanent deletion is destructive] -> Require explicit modal confirmation, snapshot exact paths,
  consume state before dispatch, and cover cancel/no-side-effect on a temporary test directory.
- [Large or corrupt images can exhaust resources] -> Enforce decoder dimension/input/output limits,
  one selected-preview lane, scheduler cancellation, and cache budgets; show fallback text on
  unsupported or failed content.

## Migration Plan

1. Add protocol/catalog types and tests without changing the command bar.
2. Vendor assets and switch ExplorerIcon rendering behind the same semantic button API.
3. Introduce shared popup state and move each existing popup onto it.
4. Switch New to the catalog popup and enable safe creation requests.
5. Add the permanent-delete modal and disk-effect tests.
6. Run focused tests, workspace tests, source audits, headful UITEST, and the ten-run interaction soak.

Rollback is source-level: retain typed request compatibility and revert each UI integration layer. No
persistent user-data migration or settings schema change is required.

## Open Questions

No blocking questions remain. Unsupported ShellNew handler/command registrations will be omitted until
a separately specified isolated adapter is available.
