# MFT Batch Folder Query Stream Design

## Problem and outcome

The Details view currently submits visible folders to four client workers, but every worker performs an independent synchronous named-pipe request. During volume recovery these calls wait separately, and after recovery they still pay one IPC round trip per folder. The new path submits one bounded batch, shares one exact-volume recovery, computes independent folder aggregates concurrently, and streams each terminal result as soon as it completes. The UI never waits for the slowest member before publishing faster rows.

The accepted default is visible-range priority followed by bounded background prefetch for the remaining directory children. Results remain exact-only; partial values are rejected as `Unavailable` with the service reason preserved in client diagnostics.

## Considered approaches

1. **Recommended: one duplex batch pipe with completion-order frames.** One request carries a batch ID and request-ID/path/reference records. The service sends one response frame per completed item and a final batch frame. This minimizes connection overhead, naturally supports completion-order delivery, and gives one cancellation boundary.
2. **Parallel legacy single-item pipes.** Increasing the existing four workers is simpler but retains repeated connection/recovery coordination and creates unbounded service pressure in large directories.
3. **One ordered request/response array.** This reduces connection overhead but delays every visible result until the slowest folder finishes, contradicting the requested behavior.

## Architecture and data flow

- `application.rs` partitions uncached current-generation requests into bounded visible-first batches. A batch dispatcher owns cancellation and publishes individual completions to the existing UI result channel.
- `mft_query.rs` adds a versioned batch request kind while preserving the legacy single-folder frame. Every item has a client-generated request ID, canonical path bytes, volume/reference identity, and cache limit. The response stream carries the same ID, terminal status, exact aggregate or bounded UTF-8 error, followed by an explicit end frame.
- `mft_service.rs` validates the whole envelope before scheduling work. Items are grouped by volume. Each volume group joins or starts one exact-recovery flight; after exactness, a bounded worker pool queries distinct folder keys concurrently. Existing generation-bound single-flight coalesces duplicates across batches and processes.
- Completed work is sent through a per-connection multi-producer channel to one writer, avoiding interleaved pipe frames. The writer emits results in completion order. Disconnect/cancellation stops publishing but does not corrupt shared recovery or cache state.
- The service result LRU remains the sole long-lived aggregate owner. Exact hits can complete immediately; misses use live aggregate, current SQLite, or bounded build order. Partial and stale results are never cached or sent as numeric facts.

## Bounds and scheduling

- Maximum 256 items per IPC batch and maximum 4 concurrent aggregate computations per volume; repeated folder keys share one flight.
- Visible requests precede prefetch requests. The client holds at most one active batch per view generation and cancels it when navigation/refresh changes the generation.
- Recovery is not repeated per item. Foreground query demand skips slow whole-SQLite materialization, performs the bounded NTFS recovery once, publishes the exact in-memory index before SQLite persistence, and lets persistence continue afterward.
- Every item retains its ten-second terminal deadline. One timeout does not cancel completed siblings or force the entire batch to fail.

## Failure handling and compatibility

- Malformed batch counts, lengths, duplicate request IDs, unsupported versions, oversized error payloads, and cross-volume identity mismatches are rejected before work begins.
- Per-item failures carry detailed service diagnostics and render `Unavailable`; a connection-level failure terminalizes all unfinished current-generation items with one client diagnostic per item.
- Legacy clients and the `--query-folder` diagnostic command continue to use the existing request/response format.
- Stale-generation completions are discarded by both the service publication check and the client view-generation check.

## Verification

- Protocol tests cover round trips, malformed bounds, duplicate IDs, completion-order framing, legacy compatibility, and mid-stream disconnect.
- Service concurrency tests prove one recovery per volume, same-key single-flight, different-key parallel progress, fast-before-slow delivery, exact-only LRU insertion, and stale rejection.
- Client tests prove visible-first batching, per-item publication, navigation cancellation, unfinished-item terminalization, and unchanged Code Lines behavior.
- Installed focused acceptance visits `D:\`, `D:\SuperExplorer`, and `D:\UE_5.7`, waits ten seconds at each, and requires at least one visible child folder to show an exact size. No complete workspace test suite is required.

## Self-review

The design has no placeholders. Batch bounds, concurrency, ordering, cancellation, legacy compatibility, exactness, diagnostics, persistence sequencing, and installed acceptance are explicit. It stays within the existing Windows MFT-service capability and introduces no filesystem-content mutation, public extension ABI change, or network endpoint.
