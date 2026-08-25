# Bookmark adapter evidence

Command: `cargo test -p explorer-app bookmark_store --lib`

Result: exit 0; 9 passed, 0 failed. Coverage includes independent-current precedence, authoritative empty document, current/backup rotation, corrupt-current backup recovery and current repair, backup usability when repair fails, bounded oversized rejection, corrupt-both unrelated-file preservation, one-time legacy migration, and non-destructive migration failure fallback.

Implementation is in `crates/explorer-app/src/bookmark_store.rs`; it uses a schema-versioned envelope, `RoadmapLimits::max_state_payload_bytes`, same-directory pending file, flush, atomic replacement, last-known-good backup, and owned-file quarantine.
