## Context

SuperExplorer currently persists one `icon_cache_memory_mb` setting and divides that budget between visible Shell textures and decoded thumbnails. The two caches have different reuse and byte-cost behavior, so a shared split neither represents user intent nor explains post-navigation memory growth. Shared/base icons and Host extension data also have independent storage, but Folder Options has no consolidated view. Shell icon and thumbnail disk caches currently share a raw-RGBA envelope implementation under separate roots; raw pixels are fast but consume substantially more disk than WebP. The MFT Service owns the complete folder aggregate index and LRU but its fixed-size query response does not expose diagnostics.

The change crosses model/session contracts, GPUI state and window lifecycle, Shell STA jobs, Host extension storage, local named-pipe IPC, UITest, and installer-produced Release binaries. All operations must remain local, bounded, path-redacted, offline-buildable, and consistent with Explorer responsiveness.

## Goals / Non-Goals

**Goals:**

- Persist and enforce independent icon and thumbnail memory budgets, defaulting to 32 MiB and 128 MiB.
- Aggregate cache telemetry in the Host and display memory, disk, and MFT Service sections every second without blocking GPUI.
- Encode icons as lossless WebP and thumbnails as lossy WebP quality 80 while preserving bounded decode and corruption recovery.
- Preserve prior sessions and lazily supersede raw-RGBA disk entries.
- Produce auditable unit, protocol, UITest, and Release-profile evidence.

**Non-Goals:**

- Reporting Windows filesystem cache, GPU driver allocations, or unregistered plugin-private storage.
- Sending paths, file names, individual MFT records, or per-user content through telemetry.
- Converting every old raw-RGBA entry during startup.
- Guaranteeing a fixed whole-process working-set number independent of Windows, GPU, extension, and view workload.

## Decisions

### Independent persisted settings

Keep `icon_cache_memory_mb` as the icon budget and add `thumbnail_cache_memory_mb`. The existing field therefore retains its session meaning. Older sessions receive the 128 MiB thumbnail default through serde migration. Each setting has its own normalization, action, Folder Options control, and immediate `set_byte_budget` call. This is preferred over a combined total with an allocation slider because it is explicit, stable, and independently enforceable.

### Host-owned immutable telemetry snapshot

Introduce bounded telemetry value types with stable cache IDs, category, availability, current bytes, optional limit, entry count, and optional hit/miss counters. `ExplorerRoot` captures UI-owned memory cache stats and combines them with the latest background disk/Host sample and MFT diagnostics. Folder Options receives an immutable snapshot rather than references to cache internals. This keeps UI composition independent of implementations and prevents plugins from choosing persistence or inventing untrusted counters.

A one-second refresh timer exists only while Folder Options is alive. Refresh is single-flight; a tick during an active sample reuses the latest snapshot. Memory values are immediate. Recursive disk accounting and MFT IPC execute off the UI thread with bounded deadlines. An unavailable source remains `Unavailable`, never a false zero.

Direct UI inspection was rejected because it couples Folder Options to every cache and risks recursive filesystem work on GPUI. A telemetry file was rejected because it creates stale disk writes and another polling protocol.

### Three presentation sections

Folder Options displays Memory, Disk, and MFT Service groups. Bounded rows use `used / limit`; unbounded registered Host rows use `used · Managed by Host`; unavailable rows say `Unavailable`. Subtotals use saturating addition over available byte values and mark partial totals when any member is unavailable. The UI does not equate Private Bytes with resident cache use.

### Versioned WebP envelope

Refactor the Shell disk cache to accept a cache kind and encoding policy. The new envelope contains magic, schema, kind, key digest, decoded width/height, encoded length, and CRC32. Entry names remain digest-based and end in `.webp`. Icons use lossless WebP; thumbnails use lossy quality 80. Decode validates envelope and encoded bounds before codec invocation, then validates decoded dimensions, stride, byte count, and maximum decoded bytes.

The codec runs on existing background/STA job paths, never GPUI. Atomic temporary-file publication and quota cleanup remain. Roots and accounting stay separate. Old `.rgba` entries are misses under the new schema and are lazily replaced; cleanup removes obsolete files. This is preferred over in-place conversion because conversion would add startup latency and accept stale pixels.

The implementation SHALL first verify that the existing locked `image` dependency provides the required WebP encode/decode behavior. Adding another codec is permitted only after license, offline lockfile, Windows release, alpha, quality, and decompression-bound gates pass.

### Fixed-size MFT diagnostics IPC

Extend the local named-pipe protocol with a distinct request discriminator and fixed-size diagnostics response containing aggregate LRU bytes/limit, volume or entry count, persisted index bytes, hits, misses, and generation. The response contains no paths or record data. It uses the existing LocalSystem plus interactive-user local-only ACL and bounded timeouts.

### Evidence and adjustment discipline

Evidence may refine task mechanics (A-level) without changing requirements. Corrections inside approved scope (B-level) require affected design/spec/tasks and evidence to be updated and dependent results marked stale. Any change to public scope, defaults, WebP policies, refresh interval, security boundary, blocking threshold, dependency class, or required evidence is C-level and requires user approval. Gates cannot be silently weakened.

## Risks / Trade-offs

- **WebP encoding consumes CPU** → keep it off GPUI, deduplicate existing jobs, benchmark Release navigation, and reject codec choices that materially regress responsiveness.
- **Lossy thumbnails can show artifacts** → use quality 80, retain lossless icons, and verify representative fixtures visually and structurally.
- **Encoded files can expand during decode** → validate dimensions and decoded byte budgets before allocation where the codec permits and reject the result before publication otherwise.
- **One-second disk scans can create I/O churn** → use a single-flight background sampler, retain the latest snapshot, and account only known cache roots.
- **Cross-process diagnostics can expose filesystem state** → return aggregate counters only over the existing local-only authenticated pipe boundary.
- **Old session fixtures can change unexpectedly** → preserve the icon field and add a serde default for the new field; run current and prior golden fixtures.
- **Telemetry accounting can drift from allocator residency** → label values as cache-owned bytes, not process working set, and use byte-cost contracts consistently.
- **Existing dirty worktree can contaminate evidence** → use scoped diffs, commands, hashes, and evidence records; never discard unrelated user edits.

## Migration Plan

1. Add model/session fields with backward-compatible defaults and tests.
2. Split runtime budget application while retaining current functionality.
3. Add telemetry contracts/reporters and background sampling before rendering the new section.
4. Add MFT diagnostics IPC and unavailable fallback.
5. Introduce the new WebP schema and switch icon/thumbnail writers and readers atomically.
6. Add Folder Options controls and one-second lifecycle.
7. Run corruption, migration, UITest, Release profiling, installer build, and visual evidence gates.

Rollback can restore the previous readers/writers and UI controls without session loss because the existing icon field remains. New WebP entries are ignored by old builds. The new thumbnail setting is an unknown serde field to older compatible readers according to the existing session validation contract; if strict old readers reject it, rollback uses the prior-session migration/export path rather than rewriting user data destructively.

## Open Questions

No product decisions remain open. Codec selection is an implementation gate constrained by the approved lossless-icon, quality-80-thumbnail, offline-build, license, and decoded-resource requirements.
