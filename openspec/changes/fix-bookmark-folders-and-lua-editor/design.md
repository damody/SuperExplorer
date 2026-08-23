## Context

The existing completed bookmark change stores `Bookmarks` as an ordered flat collection. The UI creates `EditableTextState` instances for the Lua editor but renders raw `text_input` controls without the normal control colours, borders, focus treatment, or an explicit focus transition; on the affected theme this leaves the modal apparently empty and prevents a dependable editing flow. The change spans persisted model data, state mutations, navigation/toolbar projections and modal UI.

## Goals / Non-Goals

**Goals:**

- Restore an accessible and non-stalling Lua bookmark editor.
- Preserve every valid flat bookmark while introducing nested user-managed bookmark folders.
- Provide Firefox-like save/edit/remove semantics from the star and contextual add action.
- Maintain current Lua isolation, asynchronous execution, session rollback, accessibility and existing file/folder bookmark behavior.

**Non-Goals:**

- URLs, sync/import/export, filesystem directory creation/removal, or expanded Lua permissions.
- Changing `.explorer.lua` removal or adding arbitrary shell commands to bookmark folders.

## Decisions

### Versioned tree encoded in the existing session field

`Bookmarks` will become a version-tolerant collection of UUID-addressed nodes with optional parent ID and sibling ordering. Nodes are either `Folder` or a typed bookmark target. Legacy flat entries decode as root bookmark nodes in original order. This retains the existing session envelope, background persistence and rollback contract. A separate profile database was rejected because it creates a second recovery/migration lifecycle.

### Shared command and picker surface

State owns one draft with destination-folder ID and edit/remove mode. The star, contextual "Add bookmark", manager edit, and folder commands populate that draft; the chrome renders a single labelled dialog and tree picker. Direct star toggling was rejected because it cannot select a folder and differs from the requested Firefox interaction.

### Explicit editor field styling and lifecycle

The modal uses a reusable form-field style derived from UI tokens, strong entity handles until close, initial name-field focus, keyboard Escape cancel, and backdrop-safe event handling. Successful durable mutation clears input entities; failed persistence restores the mutation and leaves draft/input values intact. Raw unstyled text controls are rejected because their visual behavior is theme-dependent.

### Bookmark folders are logical, not filesystem folders

Left navigation exposes a logical favourites subtree with expand/collapse state. Folder and bookmark context commands are routed through Explorer actions rather than invoking the Windows Shell context menu. Deleting a non-empty logical folder requires a confirmation showing the number of descendants. Delete always affects bookmarks only.

## Risks / Trade-offs

- [Corrupt/mixed session tree] → Validate parent references, cycles and sibling order; recover valid orphan nodes at root and retain diagnostics.
- [Data loss from recursive deletion] → confirmation, explicit descendant count, rollback-capable mutation and durable-state failure recovery.
- [Tree UI adds focus regressions] → keyboard/UIA roles plus focused unit and headful automation coverage.
- [Toolbar becomes overcrowded] → root folders become compact menus and preserve root sibling order; keep existing overflow contract.
- [Modal remains visually broken under themes] → visual/headful checks across supported themes and token-only colours.

## Migration Plan

1. Extend deserialization to accept the current flat JSON payload and emit root nodes without changing UUIDs, labels, targets or order.
2. Persist the new tree only after a successful mutation; retain last-known-good session fallback.
3. On rollback, reinstall the exact pre-mutation tree; no filesystem targets are touched.
4. A binary rollback reads the former flat format only through its existing recovery fallback; users can retain a backup session. No automatic destructive migration occurs.

## Open Questions

None. Visual labels use Traditional Chinese consistent with the current UI. Work may be refined only as task mechanics (A); any new bookmark type, external sync, Lua authority, or destructive filesystem action is a material scope change requiring user approval.
