# Multi-Window Session Restore Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Restore every remembered SuperExplorer window (including windows closed earlier in the previous run) on the next first ordinary launch, while layering imported Windows File Explorer windows on top as additional independent windows.

**Architecture:** Keep one process per main window (existing decision). Change the durable session schema from one window to `windows: Vec<PersistedWindow>`, each with a stable `PersistedWindowId`. Every process still owns a `PersistenceCoordinator`, but its store is wrapped by a read-modify-write merge adapter that takes a named cross-process mutex and upserts only the writer's window into the on-disk set. The first ordinary launch resolves all windows, hosts one, and spawns one process per remaining window with `SUPEREXPLORER_RESTORE_WINDOW_ID`.

**Tech Stack:** Rust 1.97.1, `serde`/`serde_json`, `windows` crate (Win32 named mutex), GPUI-CE, workspace crates `explorer-model`, `explorer-app`, `explorer-ui`, `explorer-uitest`.

**Spec:** `docs/superpowers/specs/2026-09-17-multi-window-session-restore-design.md`

## Global Constraints

- Rust toolchain pinned by `rust-toolchain.toml` (1.97.1); no new external dependencies.
- Session schema version becomes `5`; files of version `0`–`4` must still load and migrate.
- Existing bounds and validation style in `crates/explorer-model/src/session.rs` (named `SessionValidationError`, `RoadmapLimits`) are preserved.
- Window set is bounded by `MAX_PERSISTED_WINDOWS = 32`.
- Do not introduce a memory-mapped live registry.
- Every window keeps writing through `PersistenceCoordinator`; no UI-thread filesystem work.
- `#![expect(unsafe_code)]`/`unsafe_code` policy: any new Win32 call needs the same `expected` reason pattern used by `explorer_handoff.rs`.
- No comments added to code unless the file already documents that boundary.

---

### Task 1: Model schema v5 — window identity, payload, migration, validation

**Files:**
- Modify: `crates/explorer-model/src/session.rs`
- Modify: `crates/explorer-model/src/lib.rs:141-142`

**Interfaces:**
- Consumes: existing `PersistedTab`, `PersistedWindowPlacement`, `TabId`, `ExplorerWindowState`, `RoadmapLimits`.
- Produces:
  - `pub struct PersistedWindowId(u64)` with `pub const fn new(value: u64) -> Self`, `pub const fn get(self) -> u64`, `pub fn generate() -> Self`, `pub const LEGACY: Self`, `Serialize`/`Deserialize` as a transparent `u64`.
  - `pub struct PersistedWindow { pub window_id: PersistedWindowId, pub placement: PersistedWindowPlacement, pub tabs: Vec<PersistedTab>, pub active_tab_id: TabId }`
  - `PersistedSessionPayload.windows: Vec<PersistedWindow>` replacing `window`, `tabs`, `active_tab_id`.
  - `pub const MAX_PERSISTED_WINDOWS: usize = 32;`
  - `impl PersistedWindow { pub fn from_runtime(window_id, window: &ExplorerWindowState, placement: PersistedWindowPlacement) -> Result<Self, SessionValidationError>; pub fn resolve(&self, configured_start: HistoryEntry, resolve: impl FnMut(&LocationDescriptor) -> Option<HistoryEntry>) -> Result<ExplorerWindowState, crate::TabStateInvariantError>; }`
  - `PersistedSessionEnvelope::project_windows(windows: Vec<PersistedWindow>, quick_access, bookmarks, restore_enabled, locale, theme, write_generation, provenance, limits) -> Result<Self, SessionValidationError>`
  - `PersistedSessionEnvelope::project_window(window_id, window, placement, quick_access, bookmarks, restore_enabled, locale, theme, write_generation, provenance, limits) -> Result<Self, SessionValidationError>`
  - `PersistedSessionEnvelope::merge_window_set(&self, incoming: &Self, limits) -> Result<Self, SessionValidationError>`
  - `RestorePlan { windows: Vec<PersistedWindow>, quick_access, bookmarks }`
  - `RestorePlan::resolve_first_windows(...)` is not added; callers use `plan.windows[i].resolve(...)`.

- [ ] Step 1: Add `PersistedWindowId` and `PersistedWindow` types; bump `SESSION_SCHEMA_VERSION` to `5`; change `PersistedSessionPayload`.
- [ ] Step 2: Replace the window/tabs validation block in `validate_without_checksum` with a per-window loop enforcing non-empty windows, unique ids, the count bound, rect/DPI checks, non-empty tabs, unique tab ids, and the active-tab invariant.
- [ ] Step 3: Add `PersistedWindow::from_runtime` (extracted from `project_with_bookmarks`), `PersistedWindow::resolve` (extracted from `RestorePlan::resolve_window`), `project_windows`, `project_window`, and `merge_window_set`.
- [ ] Step 4: Rework `decode_or_migrate`: add `LegacySessionV4` + `LegacySessionPayloadV4`; retarget v0/v1/v3 to the legacy payload; convert every legacy payload into one `PersistedWindow` (id `PersistedWindowId::LEGACY`); keep v2's explicit conversion.
- [ ] Step 5: Update `restore_plan` to return `windows`.
- [ ] Step 6: Update `crates/explorer-model/src/lib.rs` exports.
- [ ] Step 7: Run `cargo test -p explorer-model` and fix model unit tests for the new shape.

**Migration code shape:**

```rust
const fn legacy_window(placement: PersistedWindowPlacement, tabs: Vec<PersistedTab>, active_tab_id: TabId) -> PersistedWindow {
    PersistedWindow { window_id: PersistedWindowId::LEGACY, placement, tabs, active_tab_id }
}
```

### Task 2: Model merge and restore tests

**Files:**
- Modify: `crates/explorer-model/src/session.rs` (test module)

**Interfaces:**
- Consumes: Task 1 API.
- Produces: tests named `schema_v4_migrates_single_window_into_window_set`, `merge_window_set_upserts_by_id_and_preserves_siblings`, `restore_plan_resolves_each_window_independently`, `duplicate_window_id_is_rejected`, `window_count_bound_is_enforced`.

- [ ] Step 1: Write the failing tests above using `PersistedSessionEnvelope::new`, `project_window`, and `merge_window_set`.
- [ ] Step 2: Run `cargo test -p explorer-model session` and confirm failures are only the new expectations.
- [ ] Step 3: Implement any missing behavior surfaced by the tests.
- [ ] Step 4: Run `cargo test -p explorer-model` to green.

### Task 3: Runtime window id and single-window projection

**Files:**
- Modify: `crates/explorer-app/src/session_lifecycle.rs`
- Modify: `crates/explorer-app/src/application.rs` (observer wiring, later task)

**Interfaces:**
- Consumes: Task 1 `project_window`, `PersistedWindowId`.
- Produces:
  - `RuntimeSessionSnapshot` gains `pub window_id: PersistedWindowId`.
  - `PendingSnapshot::project` calls `PersistedSessionEnvelope::project_window(runtime.window_id, &runtime.window, ...)`.

- [ ] Step 1: Add the field and change `project`.
- [ ] Step 2: `cargo check -p explorer-app` and fix the observer construction in `create_session_persistence` to pass a window id (placeholder `PersistedWindowId::generate()` until Task 5 threads the real one).

### Task 4: Cross-process merge store adapter

**Files:**
- Modify: `crates/explorer-app/src/session_store.rs`

**Interfaces:**
- Consumes: Task 1 `merge_window_set`.
- Produces:
  - `pub struct MergingSessionStore { inner: WindowsSessionStore }` implementing `explorer_model::SessionStore`, plus `pub fn new(inner: WindowsSessionStore) -> Self`.
  - `save` locks `Local\SuperExplorer.SessionWrite.v1` (bounded wait 2000 ms), loads the disk envelope, merges via `merge_window_set`, and writes with the inner store.
  - `load` and `reset` delegate to the inner store.

- [ ] Step 1: Add a private `NamedMutex` RAII wrapper using `CreateMutexW`/`WaitForSingleObject`/`ReleaseMutex` with an `#[expect(unsafe_code, reason = ...)]` module attribute matching the handoff pattern.
- [ ] Step 2: Implement `MergingSessionStore`.
- [ ] Step 3: Unit-test the merge path with two envelopes sharing the file (test in `session_store.rs` tests module).
- [ ] Step 4: `cargo test -p explorer-app --lib session_store`.

### Task 5: Startup restore-all, self selection, and spawn

**Files:**
- Modify: `crates/explorer-app/src/application.rs`
- Modify: `crates/explorer-app/src/explorer_import.rs`
- Modify: `crates/explorer-app/src/main.rs`

**Interfaces:**
- Consumes: Tasks 1, 3, 4.
- Produces:
  - `pub const RESTORE_WINDOW_ID_ENV: &str = "SUPEREXPLORER_RESTORE_WINDOW_ID";` in `explorer_import.rs`.
  - `pub fn parse_restore_window_id() -> Option<explorer_model::PersistedWindowId>` in `explorer_import.rs`.
  - `pub fn spawn_restored_window(seed: &explorer_model::PersistedWindow) -> Result<(), String>` in `explorer_import.rs`.
  - `load_session_restore` returns `Vec<(PersistedWindowId, PersistedWindowPlacement, ExplorerWindowState)>` plus globals, or a dedicated struct.
  - `ApplicationLifecycle::run_gpui_with_launch(initial_path, imported_window, this_pc, restore_window_id: Option<PersistedWindowId>)`.
  - `create_session_persistence` wraps the store in `MergingSessionStore` and takes `session_window_id: PersistedWindowId`.

- [ ] Step 1: Add env parsing/spawn helpers in `explorer_import.rs`.
- [ ] Step 2: Change `load_session_restore` to resolve the whole window set.
- [ ] Step 3: In `run_gpui_with_launch`, select self (imported window, or the restore-window id, or the first planned window), spawn the remaining planned windows, and thread `session_window_id`.
- [ ] Step 4: Update `main.rs` to pass the parsed restore window id.
- [ ] Step 5: `cargo check -p explorer-app` then `cargo test -p explorer-app --lib`.

### Task 6: Fixtures and integration tests

**Files:**
- Modify: `crates/explorer-uitest/src/bin/session_fixture.rs`
- Modify: `crates/explorer-app/tests/roadmap_combined.rs`

**Interfaces:**
- Consumes: Task 1 payload/restore API.
- Produces: fixture writes a one-window v5 session; `roadmap_combined` uses `plan.windows[0]` and `plan.windows[0].resolve(...)`.

- [ ] Step 1: Update `session_fixture.rs` to build `PersistedWindow`.
- [ ] Step 2: Update `roadmap_combined.rs`.
- [ ] Step 3: `cargo test -p explorer-app --test roadmap_combined`.

### Task 7: Workspace verification

- [ ] Step 1: `cargo fmt --all -- --check`.
- [ ] Step 2: `cargo clippy --workspace --all-targets -- -D warnings` (or the repo's configured lint command).
- [ ] Step 3: `cargo test --workspace`.
- [ ] Step 4: Record results in the plan's completion notes.

## Self-Review

- Spec coverage: schema/identity/migration (Task 1), merge (Task 4), restore-all/import layering/env (Task 5), close semantics B is implemented by never removing entries and by the append/upsert order (Tasks 1, 4), bounds/reset (Tasks 1, 4), failure handling (Tasks 1, 4, 5), tests (Tasks 2, 4, 6, 7).
- Placeholder scan: no TBD/TODO; each task names exact files, signatures, and tests.
- Type consistency: `PersistedWindowId`, `PersistedWindow`, `windows`, `merge_window_set`, `RESTORE_WINDOW_ID_ENV`, `run_gpui_with_launch` used consistently across tasks.
