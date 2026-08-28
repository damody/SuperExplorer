## Context

The bookmark model already supports stable logical folders, typed bookmark entries, durable mutations, rollback, and a dedicated `BookmarkEditorWindow`. The manager remains a `chrome.rs` overlay controlled by `bookmark_manager_open`, while Folder/File targets are deliberately rendered read-only and their payload update is ignored. The application already has a proven observer plus `cx.open_window` integration for bookmark and Folder Options child windows. The approved source designs are `docs/superpowers/specs/2026-08-27-interactive-bookmark-windows-and-paths-design.md` and `docs/superpowers/specs/2026-08-27-bookmark-action-window-design.md`.

The worktree contains unrelated active changes. Implementation MUST preserve them and restrict edits to bookmark-owned code and the minimum shared wiring.

## Goals / Non-Goals

**Goals:**

- Make the manager a real focusable native child window.
- Expose complete toolbar background/folder/bookmark context CRUD.
- Allow exact arbitrary path text to be authored and durably restored.
- Preserve the existing single reducer, mutation, persistence, and rollback authority.
- Keep legacy serialized bookmark targets compatible.

**Non-Goals:**

- Website URLs, synchronization, import/export, or browser behavior.
- Creating, deleting, or repairing filesystem targets.
- Expanding Lua authority or changing unrelated Explorer context menus.

## Decisions

### Dedicated GPUI manager window

Add a `BookmarkManagerWindow` module and application observer following `BookmarkEditorWindow` and Folder Options patterns. A manager snapshot contains a cloned render state; commands are dispatched to the owning `ExplorerRoot`, which refreshes the child snapshot after successful or rolled-back mutations. Reopening activates the existing manager window when its handle is valid.

Alternatives rejected: retaining the overlay leaves focus/event routing coupled to the file view; a separate Win32 form duplicates theme, accessibility, and reducer integration.

### Raw path target variant

Extend typed filesystem bookmark payloads with an exact user-authored path representation that is serializable without validation. Existing structured `LocationDescriptor` payloads remain decodable. Editor conversion displays either the structured location's canonical text or the stored raw text; saving a Folder/File draft records the non-empty text exactly. Activation attempts the normal descriptor/path conversion at that time and reports failure without deleting or rewriting the bookmark.

This is preferred to constructing a fake `LocationDescriptor`, which would blur provider invariants, and to existence validation, which rejects valid offline or future targets.

### Unified action routing

Add explicit actions for opening the manager, creating a logical folder under an optional parent, and creating a typed path bookmark under an optional parent. Toolbar background, folder menus, bookmark menus, and manager rows dispatch the same actions. Rename/edit/delete continue through existing drafts and rollback mutations.

### Confirmed bookmark action window

> Superseded by the later inline bookmark context-menu decision below. The native action window is no longer opened by bookmark right-click.

Bookmark-item right-click uses a singleton `BookmarkActionWindow`, not an in-surface overlay or an immediate native popup menu. Its snapshot identifies the bookmark; local window state owns the selected applicable command and delete-confirmation stage. Confirm dispatches through the owner `ExplorerRoot`; cancel, Escape, or close has no reducer mutation. Reopening for another bookmark replaces the snapshot, resets selection to Open, and activates the existing window. Edit hands off to `BookmarkEditorWindow`; Delete requires a second explicit confirmation.

Alternatives rejected: the existing overlay repeats the event-routing failure; an ordinary popup executes on click and does not meet the explicit-confirmation requirement.

### Dedicated bookmark-folder editor window

Folder creation and rename open a singleton `BookmarkFolderEditorWindow` with its own focus scope and editable text entity. Main and manager windows only start the authoritative draft and request presentation; neither renders a rename overlay. Text changes update the root draft, while Save and Cancel dispatch through the existing reducer and durable rollback path. Validation or persistence failure retains the draft and window; Escape or window close cancels it. Reopening retargets and activates the existing native window.

Alternatives rejected: keeping the overlay retains the input freeze, while unlimited editor windows conflict with the single authoritative folder draft.

### Toolbar bookmark folder drag

Reuse the manager's typed GPUI `BookmarkDrag` for toolbar bookmark entries. Logical folder buttons accept the drag and stop propagation; the toolbar background accepts it as a root destination. A typed reducer action changes `parent_id`, appends at the destination, normalizes sibling order, and uses the existing durable rollback path. Same-parent and invalid destinations are no-ops. Logical folders are not draggable in this scope.

### Firefox-style folder content menu

Left-click folder panels render only immediate child folders and bookmarks in stored order. Child folders show a folder icon and disclosure arrow and switch the panel to that folder. Mutation commands are absent from this panel and remain in the existing right-click context menu, including for nested folder rows. A single drill-in panel is used instead of a multi-column hover cascade to preserve the current stable popup lifecycle.

### Provider-aware bookmark icons

One chrome helper classifies bookmark targets and supplies icons to every bookmark projection. Local/unknown targets use `🔖`, ADB uses the phone icon `📱`, SFTP uses the remote-computer icon `🖥`, and Lua uses Lua.org's unchanged official logo; logical folders retain `📁`. Structured targets use `file_system_kind`, while arbitrary raw paths use only case-insensitive scheme prefixes and remain unvalidated. The official GIF is embedded offline, scaled with preserved proportions, and attributed in source under Lua.org's published logo permission.

### Bookmark browse-menu dismissal

The `ActivateBookmark` reducer synchronously clears both browse-only popups—the active folder menu and overflow menu—before bookmark lookup or provider activation. Consequently stale IDs, invalid paths, unavailable providers, file-launch errors, and Lua failures cannot leave a menu visible. Child-folder drill-in remains open because it uses the separate folder-toggle action.

### Inline bookmark context menu

Bookmark right-click stores a validated bookmark ID and pointer coordinates in root view state and renders a compact inline menu using the same geometry, surface, border, radius, spacing, hover, shadow, and danger colors as the logical-folder context menu. Open, optional Open in New Tab, and Edit dispatch directly after dismissal. Delete dismisses the menu and presents the existing dedicated confirmation window. The former native action window is no longer part of the right-click route.

### Bookmarked-location star editor

The toolbar continues to derive current-location state from the exact typed target lookup. An existing match renders a solid star in the theme focus blue; a bookmarkable unmatched location renders an outline star. Both states use the existing toggle action, but an existing ID starts an update draft and presents the singleton `BookmarkEditorWindow` rather than removing immediately. The editor remains a normal independent window whose initial width is 80% of the primary display with a 640px minimum; it is resizable and retains editable name and arbitrary path fields, destination selection, Remove, Cancel, and Save. Browser-only controls without a SuperExplorer behavior are intentionally omitted.

The editor suppresses the native titlebar so Windows does not add minimize, maximize, and close controls. Its in-content action row always exposes Remove Bookmark, Cancel, and Save. Remove uses the durable deletion path for an existing draft ID; for a not-yet-persisted add draft it cancels creation, producing the same visible result without a fake deletion.

### Evidence-driven corrections

A-level changes may refine task order, tests, or internal method names. B-level corrections within this approved behavior require synchronized design/spec/task updates and stale-evidence marking. C-level changes—including website URLs, new dependencies, weaker persistence gates, or filesystem mutation—require user approval.

## Risks / Trade-offs

- [Raw invalid paths cannot identify Folder versus File reliably] → Preserve the user's selected target kind; do not infer it from existence.
- [Child snapshot becomes stale after a mutation] → Refresh/update the window from authoritative root state after every dispatched action.
- [Multiple manager windows diverge] → Keep one application-owned handle and activate it on repeated open.
- [Legacy sessions change shape] → Use additive serde-compatible variants and round-trip fixtures for old and raw targets.
- [Shared UI files contain unrelated edits] → Inspect diffs before patching and never revert or reformat unrelated blocks.
- [A bookmark disappears while its action window is open] → Recheck stable ID on confirmation; close without dispatch if it is stale.

## Migration Plan

1. Add backward-compatible raw path serialization and round-trip tests.
2. Make editor drafts round-trip editable Folder/File target text.
3. Add the manager window and application lifecycle wiring; remove overlay rendering only after the native path is connected.
4. Add toolbar context entry points and focused tests.
5. Format and run targeted model/UI/app validation. Rollback consists of reverting this scoped change; legacy serialized values remain accepted, while newly authored raw values require the new decoder.

## Open Questions

None. The user authorized implementation decisions and explicitly required arbitrary editable paths, including erroneous paths.
