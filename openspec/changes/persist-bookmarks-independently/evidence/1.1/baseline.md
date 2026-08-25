# Baseline

- Revision: `8cef852ec8514b31575597a140f86ab9a4c3d73e`.
- Pre-existing overlapping tracked edit: `crates/explorer-model/src/bookmark.rs` only. It belongs to the user and must be preserved.
- No pre-existing tracked edits were reported for `application.rs`, `session_lifecycle.rs`, `session_store.rs`, `crates/explorer-app/tests`, or `installer/SuperExplorer.nsi`.
- `cargo test -p explorer-model bookmark --lib`: exit 0; 8 passed, 0 failed.
- `cargo test -p explorer-app session_store --lib`: exit 1 before explorer-app tests ran because pre-existing `crates/explorer-remote/src/sftp.rs:219` constructs removed field `RemoteEntry::is_directory`; current model exposes `kind`. This is outside the change and is retained as a baseline blocker, not counted as an in-scope failure.
- Current ownership: `PersistedSessionEnvelope.payload.bookmarks` is loaded by `create_session_persistence`, included in every `RuntimeSessionSnapshot`, and deleted indirectly when `WindowsSessionStore::reset(Session|AllRoadmapState)` deletes all session artifacts.
- Package deletion surface: the NSIS uninstaller deletes installed binaries, plugin DLLs, shortcuts, service registration, and `$INSTDIR`; it does not currently reference or delete `%LOCALAPPDATA%\RustGpuiExplorer`.
