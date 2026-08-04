## Why

Folder search currently falls into a slow recursive backend after a successful zero-result Everything query, and literal substring matching causes familiar queries such as `*.rs` to return nothing. These defects make backend selection misleading and produce inconsistent search semantics across providers.

## What Changes

- Treat every successful Everything SDK query, including zero results, as authoritative and complete.
- Fall back only for DLL/ABI/IPC/database/query availability failures; cancellation ends the request without fallback.
- Add bounded case-insensitive filename glob semantics for unqualified text containing `*` or `?`.
- Share one post-filter matcher across Everything candidates, LocalIndex, and filesystem traversal while preserving ordinary substring and `type:`/`ext:` behavior.
- Add regression and cross-provider contract tests for backend selection, wildcard matching, escaping, cancellation, deduplication, and exactly-one terminal delivery.

## Capabilities

### New Capabilities

- `search-backend-selection`: Defines authoritative Everything completion and the exact fallback/cancellation boundary.
- `search-glob-semantics`: Defines provider-independent filename glob parsing and matching.

### Modified Capabilities

None.

## Impact

The change affects `explorer-search` query representation/matching, `explorer-shell-win` Everything rendering and provider orchestration, related unit/integration tests, and startup diagnostics. It adds no dependency, schema migration, external installation, or public plugin ABI change.
