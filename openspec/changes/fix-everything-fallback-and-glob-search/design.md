## Context

The approved source design is `docs/superpowers/specs/2026-08-04-everything-zero-result-and-glob-search-design.md`. Production search already loads the official adjacent Everything SDK, probes current-session IPC, publishes bounded batches, and falls back to LocalIndex/filesystem traversal. The orchestration currently treats a successful zero-result query as a reason to fall through, while the shared post-filter treats `*` and `?` literally.

## Goals / Non-Goals

**Goals:**

- Make successful Everything completion authoritative at every result count.
- Keep cancellation distinct from availability failure.
- Provide identical bounded filename glob semantics in all production providers.
- Preserve existing request generation, batching, deduplication, scope, privacy, and terminal-event contracts.

**Non-Goals:**

- Installing, starting, or configuring Everything.
- Changing LocalIndex storage, search UI, plugin ABI, or advanced filter syntax.
- Replacing substring matching for queries without wildcard metacharacters.

## Decisions

### Return on every successful Everything result

`run_everything_bounded` success means the SDK query and result extraction completed. The orchestrator publishes `Complete`, emits `Finished`, and returns regardless of count. Only typed availability/query failures enter LocalIndex; cancellation publishes `Cancelled` and returns. This fixes the defect at the selection boundary rather than inventing a sentinel result.

Alternative: fall back on zero results to compensate for index lag. Rejected because it makes ordinary misses recursively scan large trees and makes provider status untruthful.

### Represent wildcard intent in parsed text expressions

The parser records whether unqualified text contains an unescaped `*` or `?`. Plain text retains substring behavior. A single matcher in `explorer-search` applies case-insensitive full-filename glob semantics to wildcard expressions and is reused by LocalIndex, filesystem traversal, and Everything post-filtering.

The matcher uses a bounded iterative wildcard algorithm rather than regex compilation. `*` matches zero or more Unicode scalar values and `?` exactly one. Backslash escapes `*`, `?`, and backslash; existing quoted phrase escaping remains intact.

Alternative: translate `*.rs` to `type:rs` only. Rejected because it does not cover prefix, infix, `?`, or general Windows-style filename patterns.

### Treat Everything as a candidate generator

The Everything renderer emits an escaped filename expression that preserves wildcard operators, but every returned entry still passes through the shared matcher. Canonical folder scope remains a separate mandatory query clause. This keeps visible semantics independent of Everything version while retaining indexed candidate generation.

### Preserve failover delivery contracts

Mid-query SDK/IPC failure may fall back after emitting batches. Existing stable identity/path deduplication remains authoritative and exactly one terminal event is emitted. No fallback is started after cancellation.

## Risks / Trade-offs

- [Unicode case folding can expand characters] → Compare lowercased scalar sequences consistently across all providers and cover representative Unicode fixtures.
- [A broad `*` query can match many entries] → Preserve existing pagination, result, traversal, and cancellation bounds.
- [Escaping can accidentally turn wildcard into syntax] → Separate glob parsing from Everything rendering and test metacharacters, quotes, slashes, and scoped paths.
- [Changing `Expr` affects exhaustive matches] → Update all constructors/tests through compiler-guided changes and keep serialization out of scope because `Expr` is request-local.

## Migration Plan

No persistent migration is required. Land parser/matcher changes first, update provider consumers, then fix orchestration and diagnostics. Rollback restores the prior matcher and count check; no stored data changes are involved.

## Open Questions

None. Technical refinements that preserve the approved semantics and gates may be made during implementation; material scope or public-contract changes require renewed approval.
