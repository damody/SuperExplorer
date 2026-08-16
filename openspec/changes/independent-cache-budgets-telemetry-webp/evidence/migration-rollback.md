# Migration, rollback, and user notes

- Settings remain independently persisted and normalized; rolling the cache representation forward or back does not merge icon and thumbnail budgets.
- Cache files are derived data. Clearing icon, thumbnail, extension, or MFT caches remains scoped to the selected registered root and does not delete sibling/session/log data.
- Current BC7 readers do not decode obsolete `.rgba` or legacy WebP as hits and perform no recursive startup conversion. Provider paths regenerate current entries lazily.
- Disabling BC7 uses provider-backed RGBA fallback. Existing BC7 files remain harmless derived data eligible for bounded cleanup.
- Folder Options exposes independent limits and available/pending/unavailable telemetry without paths or record payloads.
- A rollback that predates newly persisted fields must use the existing session migration/export path; user session data must not be destructively rewritten.
