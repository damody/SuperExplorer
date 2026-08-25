# Independent bookmark storage contract

- Root: `%LOCALAPPDATA%\RustGpuiExplorer\bookmarks\v1`.
- Current: `bookmarks.json`.
- Last-known-good: `bookmarks.last-known-good.json`.
- Pending: `bookmarks.pending.json`.
- Schema: object containing `schema_version: 1` and the complete `Bookmarks` value.
- Bounds: read and encoded writes must not exceed `RoadmapLimits::max_state_payload_bytes`.
- Authority: valid independent current, then valid independent backup. A present empty document is authoritative.
- Migration: only complete absence of independent current and backup permits a one-time copy from the valid legacy session collection.
- Recovery: invalid owned files are quarantined inside the bookmark directory; unrelated paths are never traversed or deleted.
- Reset: all `SessionResetScope` values affect `SessionStore` only.
- Packaging: installation, upgrade, repair, uninstall, and reinstall preserve the entire bookmark root.
- Diagnostics: report operation and error category only; never bookmark names, target paths, remote secrets, or Lua source.

Call path inventory:

1. `create_session_persistence` loads `WindowsSessionStore`, derives bookmarks, and creates `PersistenceCoordinator`.
2. UI bookmark mutations publish a complete `RuntimeSessionSnapshot` through `DurableStateObserver`.
3. `PersistenceCoordinator::worker_loop` projects the envelope, writes it through `SessionStore::save`, and retries the latest pending snapshot after failures.
4. Reset observers enqueue `SessionResetScope`; the worker calls `SessionStore::reset`, and Session/AllRoadmapState currently delete the session current, backup, and pending files.
5. The new bookmark adapter inserts independent load/migration at step 1 and independent save before session save at step 3; step 4 remains session-only.
