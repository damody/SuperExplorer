## Context

Bookmarks currently live in `PersistedSessionEnvelope.payload.bookmarks`. `WindowsSessionStore` owns `%LOCALAPPDATA%\RustGpuiExplorer\state\v1` and deletes all session artifacts for both `Session` and `AllRoadmapState` reset scopes. The NSIS installer installs binaries under Program Files and does not delete LocalAppData, so the observed loss is a data-ownership defect rather than an installer file-copy defect.

The existing bookmark model already provides version-tolerant decoding and repairs invalid tree relationships. The session lifecycle already coalesces accepted UI snapshots on one background worker, performs bounded retry, and carries the complete bookmark value. The change must preserve the legacy `RustGpuiExplorer` root because it is a tested persistence compatibility identity.

## Goals / Non-Goals

**Goals:**

- Make a dedicated bookmark document authoritative independently of windows, tabs, quick access, and view settings.
- Preserve bookmarks through install, upgrade, repair, all session reset scopes, uninstall, and reinstall.
- Migrate valid bookmarks from legacy session envelopes once without overwriting a newer or intentionally empty independent collection.
- Provide bounded reads, atomic replacement, last-known-good recovery, corruption quarantine, background writes, and privacy-safe errors.
- Keep old builds able to read the transitional session bookmark snapshot.

**Non-Goals:**

- Cloud synchronization, export/import UI, credentials, bookmark history, or a new bookmark type.
- Renaming `%LOCALAPPDATA%\RustGpuiExplorer`.
- Deleting user bookmark data during uninstall or adding a general profile-reset command.
- Changing bookmark UI semantics, Lua authority, remote address redaction, or extension ABI.

## Decisions

### Dedicated versioned document

Add an application adapter rooted at `%LOCALAPPDATA%\RustGpuiExplorer\bookmarks\v1` with `bookmarks.json`, `bookmarks.last-known-good.json`, and `bookmarks.pending.json`. The document contains a schema version and the complete `Bookmarks` value. It applies the existing maximum state-payload bound, validates JSON/model invariants, flushes the pending file, and replaces the current file while rotating the last-known-good copy.

A document is present even when the collection is empty, so absence means “not migrated yet” while an empty current document means “the user intentionally has no bookmarks.” Registry storage was rejected because bookmark trees and Lua payloads need structured, bounded, recoverable storage. Reusing `session.json` with special reset edits was rejected because whole-file loss still couples unrelated data.

### Independent load with one-time legacy migration

Application startup loads session and bookmarks separately. A valid current bookmark document wins; otherwise the valid backup wins and is repaired to current. When neither bookmark artifact exists, the adapter saves the legacy session bookmark collection and returns it. This migration is idempotent and does not delete or edit the session envelope. If bookmark storage is unavailable, startup uses the legacy collection for the current process and emits only a storage-category diagnostic without serializing paths, Lua source, or bookmark names.

Corrupt current and backup files are renamed within the owned bookmark directory with a timestamped `.corrupt` suffix. No unrelated path is traversed or removed.

### One background durability pipeline, two ownership domains

`PersistenceCoordinator` receives a bookmark-store dependency in addition to the session store. For every accepted runtime snapshot, it projects the session envelope, writes bookmarks to their dedicated store, then writes the transitional session envelope. Either failure keeps the latest snapshot dirty and schedules the existing bounded retry. This keeps filesystem and serialization work off the UI thread without introducing a second coordinator or reordering concurrent bookmark mutations.

Reset requests call only `SessionStore::reset`; they never call bookmark deletion. Existing partial resets continue rewriting the session envelope, including its compatibility bookmark copy, but the independent file remains authoritative.

### Transitional dual representation

The session schema and `payload.bookmarks` remain intact. New builds always prefer the independent document; old builds can still observe the latest session copy after downgrade. Removing the legacy field is deferred to a separately approved schema migration after downgrade support is no longer needed.

### Packaging contract

The installer and uninstaller delete only installed program files, shortcuts, service registrations, and existing explicitly owned caches. A source-level contract comment and test name the bookmark directory as preserved user data. This prevents future cleanup changes from silently expanding into the bookmark namespace.

### Evidence and change control

Every atomic task writes or references a unique evidence record under `openspec/changes/persist-bookmarks-independently/evidence/`. A-level refinements may change factoring or commands only. B-level corrections update design, specs, tasks, and invalidate dependent evidence. C-level changes—compatibility root changes, bookmark deletion on uninstall, sync, credentials, new destructive behavior, or weaker recovery gates—require user approval.

## Risks / Trade-offs

- [Two files can briefly differ after a crash] → The bookmark file is written first and is authoritative; the session copy is downgrade-only, and each file has independent atomic recovery.
- [A transient bookmark write failure delays unrelated session persistence] → Reuse bounded retry and health counters so the latest coherent snapshot is retried instead of silently losing bookmarks.
- [A corrupt independent file could mask valid legacy data] → Attempt current then backup, quarantine invalid owned artifacts, and use legacy data only when no valid independent artifact remains.
- [An intentionally empty collection could be mistaken for missing data] → Represent presence with the document itself; never infer migration state from collection length.
- [Uninstaller evolution could delete LocalAppData later] → Add an explicit preservation assertion over the NSIS source.

## Migration Plan

1. Ship the bookmark adapter and startup migration while retaining the session bookmark field.
2. On the first new-version launch, create the independent document from the valid session collection only if neither independent current nor backup exists.
3. Persist subsequent snapshots to both the independent bookmark document and transitional session envelope.
4. On rollback, an older build continues using the last dual-written session collection; the independent document remains untouched for a later upgrade.
5. No uninstall rollback step deletes the independent directory.

## Open Questions

None. User direction delegates remaining implementation choices and explicitly requires direct execution without further confirmation.
