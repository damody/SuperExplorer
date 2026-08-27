## Context

The bookmark model already supports stable logical folders, typed bookmark entries, durable mutations, rollback, and a dedicated `BookmarkEditorWindow`. The manager remains a `chrome.rs` overlay controlled by `bookmark_manager_open`, while Folder/File targets are deliberately rendered read-only and their payload update is ignored. The application already has a proven observer plus `cx.open_window` integration for bookmark and Folder Options child windows. The approved source design is `docs/superpowers/specs/2026-08-27-interactive-bookmark-windows-and-paths-design.md`.

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

### Evidence-driven corrections

A-level changes may refine task order, tests, or internal method names. B-level corrections within this approved behavior require synchronized design/spec/task updates and stale-evidence marking. C-level changes—including website URLs, new dependencies, weaker persistence gates, or filesystem mutation—require user approval.

## Risks / Trade-offs

- [Raw invalid paths cannot identify Folder versus File reliably] → Preserve the user's selected target kind; do not infer it from existence.
- [Child snapshot becomes stale after a mutation] → Refresh/update the window from authoritative root state after every dispatched action.
- [Multiple manager windows diverge] → Keep one application-owned handle and activate it on repeated open.
- [Legacy sessions change shape] → Use additive serde-compatible variants and round-trip fixtures for old and raw targets.
- [Shared UI files contain unrelated edits] → Inspect diffs before patching and never revert or reformat unrelated blocks.

## Migration Plan

1. Add backward-compatible raw path serialization and round-trip tests.
2. Make editor drafts round-trip editable Folder/File target text.
3. Add the manager window and application lifecycle wiring; remove overlay rendering only after the native path is connected.
4. Add toolbar context entry points and focused tests.
5. Format and run targeted model/UI/app validation. Rollback consists of reverting this scoped change; legacy serialized values remain accepted, while newly authored raw values require the new decoder.

## Open Questions

None. The user authorized implementation decisions and explicitly required arbitrary editable paths, including erroneous paths.
