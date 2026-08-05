## Context

Official extension metadata already advertises two command contributions and the SDK
fixtures already define a bulk-folder form/plan and EXIF parser/rename plan. The
production Extensions popup currently turns both into generic
`InvokeExtensionCommand` actions that only close the popup. The UI crate must remain
platform-neutral, and mutation must remain host-owned.

## Goals / Non-Goals

**Goals:**

- Keep every extension label inside a bounded popup.
- Render both command forms and previews in the production command surface.
- Guarantee that preview, invalid input, outside click, and Escape do not mutate disk.
- Execute confirmed typed steps through existing host-owned operations and refresh the
  active folder afterward.
- Preserve keyboard and accessibility behavior and prove it headfully.

**Non-Goals:**

- A general arbitrary GPUI renderer supplied by third-party plugins.
- Direct filesystem access from Lua or extension callbacks.
- Recursive deletion of directories during undo.
- New persisted settings or ABI types.

## Decisions

### Use a typed native panel state in `explorer-ui`

`AppViewState` stores which extension panel is open plus its draft fields, preview,
and validation status. Opening a command replaces the command list within the same
anchored popup. This follows Explorer flyout continuity and makes Escape a deterministic
panel-to-list-to-closed sequence. Separate modal windows were rejected as unnecessarily
disruptive.

### Keep preview pure and execution host-owned

Pure helpers parse form values and produce typed preview rows or field errors. A
confirm action carries the validated request to the composition root. The host checks
the current location/selection again, builds an operation plan, submits existing file
commands, and refreshes only after accepted execution. Direct `create_dir` or rename
calls from the render tree are forbidden.

### Adapt official fixtures without changing ABI

Bulk generation follows `command_form`, `generate_names`, and `build_plan`. EXIF
preview follows the in-process parser and `build_rename_plan`; the host supplies bytes
and selected paths. Existing contribution IDs remain stable, so discovery and packages
need no migration.

### Bound row geometry explicitly

The popup receives a fixed/minimum Explorer-style width and a maximum width. Rows use
`min_w_0`, `flex_1`, no wrapping, text ellipsis, and the complete label for ARIA/help.
Panels share the same width and keep primary/cancel buttons inside it.

## Risks / Trade-offs

- **[Selection changes after preview]** → Revalidate folder, item identities, and target
  collisions on confirmation; reject stale requests without mutation.
- **[Large folder count blocks UI]** → Keep generation bounded at 100,000 and require
  second confirmation above 1,000; operation submission remains asynchronous.
- **[Missing or unsupported EXIF]** → Show a per-file preview error and disable Rename.
- **[Partial operation failure]** → Preserve existing operation result and conservative
  undo semantics; never recursively remove non-empty generated folders.
- **[Long localized labels]** → Ellipsize visually while preserving full accessible text.

## Migration Plan

No storage or ABI migration is required. Land UI state/rendering and pure validation,
then host execution, then UITEST. Rollback removes the new panel actions and restores
generic command rows; contribution IDs and packages remain valid.

## Open Questions

None. The approved design fixes the panel form fields, confirmation boundary, and
Explorer-style cancellation behavior.
