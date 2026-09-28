## Context

See [proposal.md](proposal.md) and [local-column-view/spec.md](specs/local-column-view/spec.md). `ViewMode` and its eight values are closed built-ins in `explorer-model/src/navigation.rs`; `PersistedViewMode` and per-tab `ViewSettings` live in `session.rs`. `ExplorerWindow` currently renders one `FileViewHost` plus an optional details/preview side pane. The active tab has one navigated `DirectoryState`, while `AppViewState` also owns a bounded directory cache. Directory commands and events carry request contexts. The current preview path already supports asynchronous images and broker-hosted Windows Preview Handlers.

The user chose local files for the first release. Existing remote, Shell namespace, extension-view and file-operation changes in the working tree are unrelated and must be preserved.

## Goals / Non-Goals

**Goals:**

- Make folder-row selection a real location transition while retaining its ancestor rows and snapshots as a visible path.
- Keep auxiliary ancestor listings separate from the active tab's navigation request and preserve generation-based stale-result rejection.
- Share the existing preview scheduler/host and command dispatch rather than introducing another decoder, handler process, or file-operation implementation.
- Keep the rendering cost proportional to visible rows and visible columns, not the total number of descendants.

**Non-Goals:**

- No recursive tree scan, directory-size computation, or content indexing to populate columns.
- No plugin ABI change and no remote-provider column enumeration in this release.
- No new preview format; unsupported files retain the current honest fallback.

## Decisions

### 1. Add a built-in mode with an effective-mode gate

Add `ViewMode::Columns` and `PersistedViewMode::Columns`, update every exhaustive mode match, the View menu and keyboard indices, icon-size policy, session conversion and locale strings. Keep the selected mode in per-tab settings. Derive an **effective** view mode from selected mode plus current location. Eligible paths are local drive directories (drive-letter paths backed by local fixed/removable media), excluding UNC/mapped network drives, WSL, namespace/virtual paths, and archives. At unsupported locations, render Details and show its effective state; do not overwrite the selected Columns preference. On a valid local return, Columns appears again. This is safer than silently treating a remote or namespace provider as a local `PathBuf`.

### 2. Treat the active tab location as the rightmost selected directory

The active tab's existing directory state represents the deepest selected directory. A folder row single click requests normal navigation to that folder and records history once; another click on the already current folder does not add an entry. The column branch retains ancestors as `(directory location, selected child identity, snapshot/loading/error, vertical offset)` and drops descendants immediately on branch change. A file click in the current column only changes selection. A file click in an ancestor column navigates to its containing folder, then applies that file's identity as pending selection after the matching directory generation resolves. Back/Forward/Up/address/breadcrumb navigation rebuilds the branch from the destination path; folder rows stay selected by stable item identity where available, falling back to canonical paths. Reparse points/junctions use resolved identity on the active branch to stop cycles, while the visible label and path remain the user's chosen path.

Alternative considered: keep a private selected folder that differs from the tab address. That would make Back, Up, clipboard targets and status ambiguous. Formal navigation is chosen despite the extra history entries from single-click exploration.

### 3. Add auxiliary, generation-scoped column listings

Reuse the normal directory enumerator/shell service, but introduce an explicit `EnumerateColumn` command and typed batch/terminal events so ancestor loading never mutates the active tab's single `DirectoryState` or history. Each request carries tab ID, branch revision, column directory identity and request ID. Maintain a small bounded number of concurrent auxiliary loads; reuse cached snapshots when valid, then load visible missing ancestors first. Cancel or ignore work on branch switch, tab close, mode change or refresh. A result is applied only when all identity and revision fields still match. F5 invalidates/reloads affected visible columns and the active directory; operation completion or watcher changes invalidate the matching ancestor column, not every tab indiscriminately.

Alternative considered: issue ordinary `Navigate` for each ancestor. Existing event acceptance ties those events to one active tab directory, so extra navigations would corrupt the address and history.

### 4. Render columns as one file surface with per-column virtualization

Render a horizontal strip of fixed-width directory columns with dividers and independent vertical scroll handles. Initial width should be approximately 240 logical px, clamped to a usable range (proposed 180–480); persisted per-tab default/overrides are normalized on restore. The active/new column scrolls into view after navigation. Keep columns in state for the complete path, but realize rows only within each visible column's viewport using the existing `FilePresentation` sorting/filtering and virtualization helpers. Do not enumerate grandchildren until their parent is selected. Narrow windows keep horizontal scrolling and a reachable preview; they do not truncate the branch. Resize coordinates are in logical pixels and recomputed on DPI changes.

Alternative considered: instantiate an independent full `FileViewHost` per level. That would duplicate selection, drag, thumbnail, context menu and focus state and scale poorly with depth.

### 5. Bridge column selection to existing commands

The active column owns a selected `ItemDescriptor` and its containing location, not just a row index. Reuse existing open, context menu, rename, clipboard, drag/drop, delete, and refresh actions with that descriptor. Keep one active command-selection set; Ctrl/Shift multi-selection is confined to one directory column. Changing active column clears incompatible selection. Column Left/Right moves focus across levels; Up/Down moves within a column. Enter/double-click on a file follows the existing file-open policy; Enter on a folder reveals/enters it. UIA names announce level, directory and row state. Branch rows, dividers and loading/error states get stable IDs.

### 6. Embed the existing preview service once

In effective Columns mode, suppress the ordinary side-pane rendering and use a right-end preview slot bound to the active column selection. The slot uses the existing image thumbnail key/coordinator and brokered handler host; selection/tab/mode revisions drive cancellation and unload. Add `column_preview_visible` (default true) and `column_preview_width` to per-tab settings; Alt+P and the Preview Pane command target this setting only while Columns is effective. The ordinary `preview_pane` bit is retained for the other modes. Multi-selection, folders, unsupported items and failures show the current localized fallback semantics. The preview must never automatically hydrate an offline placeholder. The embedded host rect and focus routing follow the column slot bounds.

Alternative considered: show the current Preview Pane beside the column strip. That duplicates the view's preview region and makes the screenshot's integrated interaction awkward.

### 7. Persist preferences, reconstruct transient state

Persist selected Columns mode, normalized widths and integrated-preview visibility in the existing versioned session format with defaults for old sessions. Do not persist branch snapshots, request IDs, selection generations, handler objects or preview pixels. Rebuild the branch from the restored current local path, loading nearest visible ancestors first. A corrupt width or unavailable restored location falls back to safe defaults/Details without losing the saved mode. Maintain compatibility with older sessions and existing extension view IDs.

## Risks / Trade-offs

- **[Single-click navigation produces more history entries]** → Deduplicate current-folder clicks and test Back/Forward across sibling/ancestor changes; document this deliberate Finder-style behavior.
- **[Several columns can issue expensive filesystem loads]** → Cap concurrent auxiliary loads, prioritize visible columns, virtualize rows and measure interaction responsiveness with large directories.
- **[Stale preview or ancestor data after fast switching]** → Check tab, branch and request generation before every event/texture/host commit; unload the prior handler exactly once.
- **[Junction cycles and inaccessible parents]** → Detect repeated resolved identities, stop extension of the branch, and show an actionable column error while preserving navigation to surviving ancestors.
- **[Existing code has many exhaustive eight-mode matches and fixed menu indices]** → Compile all workspace crates and test menu keyboard focus, session round trips, icon-size normalization and extension fallback explicitly.
- **[Preexisting working-tree changes may overlap implementation files]** → Start from current contents, avoid broad resets/reformats, and review the diff against the local baseline.

## Migration Plan

1. Add persisted enum/fields with serde defaults and round-trip tests before exposing the View command. Verify old sessions load with unchanged settings.
2. Add branch model and auxiliary listing events, then render Columns behind the mode gate. Keep the existing mode as the effective fallback at unsupported locations.
3. Wire preview, commands and UIA; run focused and headful tests. Rollback can remove the View entry and use the effective Details fallback while preserving the persisted `columns` value for a future compatible build.

No external storage migration, installer change or third-party dependency is required.
