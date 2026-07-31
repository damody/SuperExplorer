# Explorer Command Menu and New Item Parity Design

## Scope

This change completes four related Explorer command-surface gaps:

1. `新增` opens an Explorer-style menu containing Folder, Shortcut when safely available, and the
   current user's registered ShellNew file types instead of immediately creating only a folder.
2. Every application-owned chrome glyph uses unmodified geometry from Microsoft's
   `fluentui-system-icons` regular SVG set, with an offline asset source and license attribution.
3. Sort, View, New, More, and extension popups own pointer hit testing and keyboard/accessibility
   focus; every enabled row dispatches one typed action and hover never reaches the file view.
4. Shift+Delete opens an explicit permanent-delete confirmation and Confirm dispatches the existing
   confirmed `IFileOperation` request while Cancel has no filesystem effect.

This does not replace file-type icons with Fluent glyphs. Shell-owned file-type icons in the New
menu remain association-derived, like Explorer. Fluent assets cover application chrome controls.

## Selected Architecture

### ShellNew catalog

`explorer-shell-win` reads the bounded per-user Explorer ShellNew class list and the merged
`HKEY_CLASSES_ROOT` registration view on the Shell STA. It resolves extension, localized friendly
name, template mode, default new-item name, and an opaque icon request location into owned model
types. The catalog always supplies Folder and Text Document fallbacks and recognizes the empty ZIP
template. Registrations with unsupported or unsafe handler/command contracts are omitted rather
than creating corrupt files or executing registry command text.

The model exposes a bounded `ShellNewItemDescriptor` and `ShellNewItemTemplate` enum. UI receives
the catalog asynchronously through normal `ExplorerCommand`/`ExplorerEvent` routing, caches it per
association generation, and displays only actionable rows. Opening New triggers a catalog request
when absent or stale. Selecting an item produces a typed `FileOperationKind::CreateItem` request.

Blank and registered-template files use `IFileOperation::NewItem`; bounded initial-data templates
use a controlled create-new path on the Shell STA with the same name validation and KeepBoth
policy. Successful creation starts inline rename when the refreshed item becomes observable.

### Popup interaction boundary

Command popups share one explicit menu state: active menu, focused enabled row, submenu state, and
previous focus surface. Opening one closes every other popup and moves native focus to a dedicated
menu `FocusHandle`. Arrow/Home/End move across enabled rows, Enter/Space activates, Escape closes
and restores the invoking command button surface, and Tab follows the documented close/restore
rule. Pointer hover paints only the hovered row; pointer down/click is stopped at the popup, and
the complete popup uses GPUI mouse occlusion so file rows underneath cannot hover, select, drag, or
activate. Disabled capability-dependent items remain visible where Explorer does, but never claim
to be clickable.

### Fluent icon assets

The project vendors only the selected 20/24 px regular SVG files from
`microsoft/fluentui-system-icons`, retaining their filenames, upstream source URL, commit/release
metadata, MIT license, and NOTICE. `ExplorerIcon` maps each semantic icon to one asset path; no
locally redrawn line approximation remains. A small embedded `AssetSource` makes icons available
offline to GPUI's SVG renderer, and theme color continues to flow through the monochrome SVG
element. Tests require every icon enum variant to resolve to a bundled upstream asset.

### Permanent deletion

Shift+Delete remains routed through the same typed reducer as pointer actions. The request first
opens an app-owned modal with selected item count and a destructive warning. The overlay occludes
the underlying window and owns keyboard focus. Confirm consumes the pending selection exactly
once, sets `confirmed: true` and `require_confirmation: true`, then submits the existing permanent
delete operation. Escape/Cancel clears the pending request, restores file-view focus, and submits
nothing. Stale confirmation state is cleared on navigation, tab close, or window shutdown.

## Alternatives Considered

- Reusing the native background context menu would provide exact Shell handler behavior, but it
  cannot satisfy the app-owned popup focus, hover, automation, and icon requirements.
- A static list of common extensions would be easy to test but would omit installed Office,
  archive, and third-party registrations and diverge between machines.
- Keeping hand-drawn vector approximations avoids assets but directly contradicts the requested
  Fluent UI System Icons source and makes upstream provenance unverifiable.

## Error Handling and Limits

- Registry enumeration, strings, template data, and menu rows are bounded. Invalid UTF-16,
  oversized data, missing template files, unsafe commands, duplicate extensions, and malformed
  registrations are skipped with privacy-safe diagnostics.
- A missing catalog leaves Folder and Text Document available. A failed creation produces a typed
  operation result and leaves the menu closed without inventing success.
- New items require a writable filesystem-backed or provider-capable destination. Unsupported
  namespaces disable New truthfully.
- Permanent delete cannot bypass the explicit pending-confirmation token.

## Verification

- Model and Shell tests cover registry/catalog normalization, bounds, templates, unique KeepBoth
  names, folder/text/ZIP creation, missing registrations, and failure results.
- UI reducer tests cover mutual exclusion, enabled-row navigation, focus restore, activation, hover
  state, and Shift+Delete confirmation/cancel/confirm exactly once.
- Render/source audits prove every chrome icon maps to a vendored upstream Fluent regular SVG and
  popup roots occlude mouse hit testing.
- Headful UITEST opens each popup, traverses and activates every enabled row, verifies UIA focus and
  hover raster state, proves rows underneath do not highlight, creates multiple registered types,
  and validates Shift+Delete cancel and confirm against disk state.
- Formatting, locked checks/tests, warnings-as-errors Clippy, OpenSpec strict, quick/full/interop/
  visual cases, and a 10-run interaction soak are required before closing umbrella tasks.

## Rollback

The new catalog and popup state are additive. Rollback can restore the single CreateFolder command
and previous vector renderer without changing persisted session data. Permanent delete remains
fail-closed because no filesystem request is emitted without explicit confirmation.
