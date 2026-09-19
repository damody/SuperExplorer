# Multi-Window Session Restore

## Goal

When SuperExplorer opens, it must not only import the current Windows File
Explorer state; it must also restore SuperExplorer's own state from before it
last closed. "Its own state" is the **set of all SuperExplorer windows** that was
known during the previous run — including windows the user opened and later
closed during that run — each with its tabs, history, view settings, active tab,
and window placement. Imported File Explorer windows are layered on top as
additional independent windows.

## User-visible behavior

- The first ordinary launch restores every remembered SuperExplorer window, one
  top-level window per remembered window, including ones closed earlier in the
  previous run (decision **B**).
- Each restored window returns to its saved placement, tabs, per-tab
  back/forward history and view settings, and active tab.
- At the same time, currently open Windows File Explorer windows are imported as
  they are today: the first imported window becomes this process's window and
  each additional File Explorer window becomes its own SuperExplorer window.
- Imported File Explorer windows and Win+E windows are themselves remembered as
  normal SuperExplorer windows, so they are restored on the next launch.
- Opening a SuperExplorer window and then closing it **does not** remove it from
  the remembered set (decision B). The set grows only by windows that were never
  seen before; restored windows keep their existing identity and do not
  duplicate.
- The set is bounded and can be cleared through the existing session reset
  scope and the existing "restore previous session" preference; when restore is
  disabled no windows are restored.
- Because of decision B, an imported File Explorer window is remembered as a
  normal window and reappears on later launches until the set is reset.

## Non-goals

- Single-process multi-window ownership. Each main window intentionally remains
  an independently process-owned window (see
  `openspec/changes/repeated-launch-new-window`).
- A memory-mapped live session registry. A shared mapping would only add a
  fragile real-time synchronization surface; durability still has to come from
  the existing checksummed, backed-up, atomically replaced file store.
- Arbitrary `window-id` remapping or migration of tab identity across processes.
- A UI to prune the remembered window set.
- Per-window isolation of global settings (theme, locale, quick access,
  bookmarks). These stay global as they are today.
- Tab transfer between windows.

## Architecture

### Storage split

The existing single-file session store is split into two concerns:

1. **Global envelope** — `session.json` keeps the global fields only:
   `restore_enabled`, `locale`, `theme`, `quick_access`, `bookmarks`, plus
   provenance and write generation. Its window fields move into the window set.
2. **Window set** — a new `windows: Vec<PersistedWindow>` field in the same
   envelope holds every remembered window. Each `PersistedWindow` carries a
   stable `window_id`, its `PersistedWindowPlacement`, `Vec<PersistedTab>`, and
   `active_tab_id`.

Keeping the window set inside the existing `session.json` avoids a second store,
a second trait, and a second migration path; it reuses `WindowsSessionStore`'s
checksum, last-known-good backup, quarantine, and `ReplaceFileW` atomic replace.

### Cross-process merge

Every process still owns a `PersistenceCoordinator` writing the same
`session.json`. To stop windows from clobbering each other, the store is wrapped
by a read-modify-write merge adapter:

- On save, the adapter takes a named cross-process mutex
  (`Local\SuperExplorer.SessionWrite.v1`, mirroring the handoff owner-mutex
  pattern in `explorer_handoff.rs`).
- It loads the current envelope from disk (or starts from defaults), replaces
  **only the entry whose `window_id` matches this process's window**, leaves all
  other windows and the global fields from disk intact, recomputes the checksum,
  and writes atomically.
- The process's own global-field changes are written only when they differ from
  the loaded base, so normal per-window churn does not revert another window's
  theme/locale/quick-access/bookmark change. Global last-writer-wins remains
  possible only for simultaneous edits, which is unchanged from today.

The adapter sits behind the existing `SessionStore` trait, so the
`PersistenceCoordinator`, its debounce, retry, health counters, and shutdown
flush are unchanged.

### Window identity

A new `PersistedWindowId` (stable, serializable) identifies one remembered
window:

- A restored window keeps the id it was saved with.
- A genuinely new window (first run, Win+E, imported File Explorer window, or a
  manually opened window) generates a fresh id from process start time, process
  id, and a local counter, which is collision-resistant within one login
  session.
- The current process's id is resolved once at startup and threaded into both
  the store adapter and the persistence observer.

### Startup flow

Only the first ordinary process performs restore (existing
`consume_launch_import(first_ordinary_process)` gate):

1. Load `session.json`, honoring `restore_enabled`.
2. Build a restore plan over `windows`, resolving each window's saved locations
   through the existing `resolve_saved_location` logic. Windows whose locations
   are all stale are dropped; remaining windows keep their tabs, history, view
   settings, placement, and active tab.
3. Choose this process's window:
   - If File Explorer windows are being imported, this process hosts the first
     imported window (preserving today's "import 照舊" behavior) and generates a
     new window id.
   - Otherwise it hosts the first planned window and adopts its id.
4. For every remaining planned window, spawn one SuperExplorer process with
   `SUPEREXPLORER_RESTORE_WINDOW_ID=<id>`. That child restores only the matching
   window, does not enumerate the set, and does not import File Explorer
   windows.
5. Spawn any additional imported File Explorer windows as today.

`SUPEREXPLORER_RESTORE_WINDOW_ID` is validated and bounded in the same way as
the existing `SUPEREXPLORER_INITIAL_TABS*` payloads; an unknown id falls back to
a normal single `C:\` window and records a diagnostic.

### Import layering

`explorer_import::consume_launch_import` is unchanged for enumerating and
closing File Explorer windows. The application composes the final window set as
"planned restored windows" plus "imported windows"; the import path only decides
which window this process hosts and which extra processes to spawn. Restored
windows and imported windows are never merged into one tab strip.

### Close semantics (decision B)

- A window writes its own entry on every durable transition as today.
- Closing a window never removes its entry. The entry's last-known state is what
  gets restored next launch.
- The set therefore only grows by genuinely new window ids. It is bounded by a
  model constant (for example `MAX_PERSISTED_WINDOWS = 32`). An upsert moves the
  merged window to the end of the stored order, so the vector is ordered from
  oldest-written to most-recently-written; on overflow the adapter drops entries
  from the front (oldest) and records a diagnostic.
- The existing session reset scope clears the whole set.

## Data model changes

- Bump `SESSION_SCHEMA_VERSION` from 4 to 5.
- Add `PersistedWindow { window_id, placement, tabs, active_tab_id }`.
- `PersistedSessionPayload` replaces `window` / `tabs` / `active_tab_id` with
  `windows: Vec<PersistedWindow>`.
- Add `LegacySessionV4` (and version-specific legacy payload structs for the
  v0–v3 single-window shapes) so prior files still decode. Migration v4→v5 (and
  earlier→v5) wraps the existing single window and tabs into a one-element
  `windows` vector.
- `RestorePlan` becomes `{ windows: Vec<PersistedWindow>, quick_access,
  bookmarks, ... }`; `resolve_window` becomes a per-window resolver so each
  spawned process resolves only its own entry.
- Validation extends to: at least one window in any persisted envelope (an empty
  set is represented only by the absence of `session.json` or a reset), unique
  `window_id`s, per-window tab invariants (reusing the existing tab validation),
  and the window-count bound.
- The persistence worker keeps projecting a single runtime window. A projection
  helper builds that one `PersistedWindow`; the merge adapter combines it with
  the on-disk `windows` vector, appending the merged entry to the end.

## Components

- `crates/explorer-model/src/session.rs` — schema v5, `PersistedWindow`,
  `PersistedWindowId`, multi-window `RestorePlan`, per-window resolver,
  migration from v0–v4, extended validation.
- `crates/explorer-app/src/session_store.rs` — merge adapter behind
  `SessionStore` using the named write mutex.
- `crates/explorer-app/src/session_lifecycle.rs` — single-window runtime
  projection and the window id threaded into the coordinator.
- `crates/explorer-app/src/application.rs` — `load_session_restore` returns the
  planned window set; startup selects this process's window and spawns the rest;
  import layering.
- `crates/explorer-app/src/explorer_import.rs` — `SUPEREXPLORER_RESTORE_WINDOW_ID`
  parsing alongside the existing launch payloads, and spawning restored windows.

## Failure handling

- A corrupt or unsupported `session.json` is quarantined and restore falls back
  to defaults, as today.
- A single invalid window entry is dropped without discarding its siblings; the
  drop is recorded. If every entry is invalid, restore falls back to defaults.
- The cross-process write mutex is bounded-wait; on timeout the write is
  retried by the existing coordinator, and the failure is recorded in
  `PersistenceHealth`.
- A failed child spawn records a diagnostic and does not block the remaining
  restored windows or the current window.
- Window ids that cannot be resolved at startup (already closed, or a stale
  spawn) are ignored.

## Testing

- **Model**: v0/v1/v2/v3/v4→v5 migration produces one window with the original
  tabs and active tab; multi-window round-trip; duplicate/missing `window_id`
  rejection; window-count bound; per-window tab bound.
- **Store**: merge adapter preserves sibling windows and global fields under
  concurrent merges; corrupt entry quarantine; atomic replacement and backup
  behavior retained.
- **App**: first-process restore selection (import path vs restore path);
  `SUPEREXPLORER_RESTORE_WINDOW_ID` restores exactly one window and skips
  enumeration/import; unknown id falls back safely.
- **Windows smoke**: open several SuperExplorer windows, close them (including
  closing some while others remain), launch again, and verify every remembered
  window and tab reappears; verify an imported File Explorer window is layered
  on top without merging tab strips.

## Risks

- **Set growth** under decision B: a new window opened in every run accumulates.
  Mitigated by the bound plus the existing reset scope.
- **Global last-writer** for simultaneous global-setting edits is unchanged from
  today and explicitly out of scope to fully serialize.
- **Startup fan-out**: restoring N windows spawns N processes; startup is
  bounded by the window-count cap and each spawn is independent.
