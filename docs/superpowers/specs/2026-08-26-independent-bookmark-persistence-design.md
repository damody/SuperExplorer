# Independent bookmark persistence design

## Goal

Keep every valid SuperExplorer bookmark across install, upgrade, repair, session reset, full Explorer-state reset, uninstall, and reinstall. A bookmark disappears only after an explicit bookmark deletion by the user or an unrecoverable corruption of both the current and last-known-good bookmark files.

## Storage boundary

Bookmarks become an independently owned, versioned user-data document under `%LOCALAPPDATA%\RustGpuiExplorer\bookmarks\v1`. The compatibility root remains `RustGpuiExplorer`; changing it would make existing profiles appear empty. The directory contains current, last-known-good, and same-directory pending files and uses bounded reads, validation, flush, and atomic replacement.

The existing session envelope keeps its bookmark field for backward decoding and rollback compatibility, but it is no longer authoritative after the independent document exists. Session reset scopes operate only on session-owned state and never remove or empty the bookmark document. The NSIS installer and uninstaller continue deleting application binaries only and explicitly document the bookmark directory as preserved user data.

## Startup and migration

Startup loads the session and bookmark stores independently. A valid current or backup bookmark document is authoritative, including an intentionally empty collection. If neither independent artifact exists, the application copies valid bookmarks from the legacy session envelope into the new store before using them. This is an idempotent one-time migration: an existing independent document is never replaced by legacy session contents.

If independent bookmark storage is unavailable, startup falls back to the legacy session bookmark snapshot for that run and reports a privacy-safe diagnostic. Corrupt current files are quarantined and the last-known-good file is attempted. Failure of both bookmark artifacts does not delete either the legacy session snapshot or unrelated application state.

## Writes and failure behavior

Accepted durable UI snapshots continue to coalesce on the background persistence worker. Each snapshot writes the independent bookmark document and the session document off the UI thread. Bookmark persistence failure is a durable-write failure and follows the existing bounded retry path; session reset requests address only the session store. Keeping bookmarks in the legacy session snapshot for the transition period provides downgrade compatibility, but all new launches prefer the independent store.

The bookmark document preserves the complete `Bookmarks` value, including folders, stable IDs, order, file/folder targets, remote public authorities, and Lua source. It does not add credentials or filesystem contents.

## Alternatives

- Adjust only installer deletion rules: rejected because the installer does not currently delete the LocalAppData session root and bookmarks would remain coupled to session reset and recovery.
- Keep bookmarks in `session.json` but exempt fields during resets: rejected because full-file loss, replacement, and test-profile workflows would still erase bookmarks.
- Store bookmarks in the Windows Registry: rejected because structured bookmark trees and Lua payloads need bounded, inspectable, recoverable document storage.

## Verification

Tests cover new-store round trips, current-to-backup recovery, corrupt-file quarantine, bounded payload rejection, legacy migration, independent empty-store authority, session and all-state reset isolation, retry behavior, and installer/uninstaller preservation contracts. Relevant Rust tests, formatting, strict OpenSpec validation, and installer contract tests must pass before completion.

## Change control

- A — task refinement may adjust task order, test commands, or internal factoring without changing persistence behavior.
- B — an in-scope correction to schema or migration mechanics requires updating design, specs, tasks, and stale evidence before continuing.
- C — changing the compatibility root, deleting bookmark user data from uninstall, adding cloud synchronization, storing credentials, or weakening recovery gates requires new user approval.
