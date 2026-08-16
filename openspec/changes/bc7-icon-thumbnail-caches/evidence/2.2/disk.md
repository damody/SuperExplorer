# G-DISK evidence

Icon and thumbnail stores use independent roots/budgets and `.bc7cache` entries. Writes use a same-directory unique temporary file, flush/sync, close, atomic rename/replace, and best-effort temporary cleanup. Reads reject symlinks, stale identity, invalid schema/layout/checksum, and trailing data before cache admission.

`cargo test -p explorer-shell-win --lib --locked --offline icon_disk_cache::tests -- --nocapture --test-threads=1` passed 14/14. Coverage includes concurrent duplicate writers, injected interruption before atomic publication with temporary cleanup, simultaneous readers observing only miss or complete hit, explicit icon/thumbnail quota and root isolation, schema mismatch, corruption removal, lazy legacy `.rgba` miss, scoped obsolete-data cleanup, quota eviction, independent build/key digests, and icon/thumbnail round trips.

Source hash: `crates/explorer-shell-win/src/icon_disk_cache.rs` `C4203D98CD6E3742D0E31CB7149725D1A5C8B14F6717C13955ED7C13D64B3AAF`.

Task 2.2.7 is intentionally open: the cache-specific WebP representation is absent from `explorer-shell-win`, but repository-wide `image-webp` remains reachable through generic user-file image decoding. Removing that unrelated decoder would break supported `.webp` files and is not authorized by this cache migration.
