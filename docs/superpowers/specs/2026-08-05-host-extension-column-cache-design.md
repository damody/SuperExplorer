# Host-enforced extension column cache

## Decision

The host, not an extension, owns caching policy for every extension-provided data column. Before dispatching a provider, the host computes a cache key from the loaded contribution identity, a host-attested stable filesystem identity, and the source modification timestamp. A matching entry is rebound to the current tab and generation and published without calling plugin code. A changed or unavailable modification timestamp is a cache miss.

## Scope

The first production implementation covers every extension data-column runtime currently wired into SuperExplorer: Folder size, Rust/Lua Code lines, and Lock owners. Folder size caches only exact terminal totals. Batch columns cache copied typed terminal values or stable unsupported outcomes. Cache entries are bounded, process-global for the loaded contribution, never contain ABI objects or callbacks, and cannot publish across a stale UI generation.

Provider-local caches may remain as a restart optimization, but they are not authoritative and do not control whether the host invokes a provider.

## Data flow

1. The host canonicalizes the item path and reads its metadata modification timestamp.
2. It looks up the contribution-scoped cache before queueing work.
3. A hit is copied into a result carrying the current request context and item ID.
4. A miss invokes the provider. The host rechecks the same metadata key after completion and stores only when it is unchanged.
5. Navigation, tab switching, and generation changes affect publication context but do not invalidate otherwise matching cached data. Metadata changes force a miss.

## Failure behavior and validation

Canonicalization or metadata failure disables reuse for that request and never fabricates a value. Corrupt or partial provider results are not cached as exact values. Minimal local tests prove same-modification-time reuse, changed-time invalidation, no provider invocation on a hit, and stale-generation rejection. One completed example smoke may then verify folder A to B to A reuse. CI is never used.
