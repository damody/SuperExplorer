## Context

The host currently contains code-line-specific shortcuts that track one active descriptor and one set of value/render/cache maps. That model conflicts with two independently packaged providers contributing similar columns. Concurrent working-tree changes already rename portions of the Rust and Lua surfaces and partially alter Rust directory selection; implementation must inspect and integrate those edits without reverting unrelated work.

The approved source design is `docs/superpowers/specs/2026-08-05-distinct-lua-rust-code-lines-columns-design.md`. The public extension ABI remains fixed, input remains bounded and host-attested, and validation must remain local/offline. The final gate is headful and requires visible screenshot evidence rather than source-shape inference.

## Goals / Non-Goals

**Goals:**

- Give Lua and Rust exact, distinct display names and stable contribution identities.
- Route both providers independently through registration, scheduling, caching, rendering, refresh, and sorting.
- Compute the Rust directory result from per-language aggregates and render `Language: N` with comma grouping.
- Prove both columns are simultaneously visible and populated in the real Details view.

**Non-Goals:**

- Change the public ABI schema, add dependencies, or redesign all dynamic-column storage.
- Change Lua provider statistics or label behavior beyond the requested display name.
- Display secondary languages or change the bounded directory input format.

## Decisions

### Stable identity is the routing key

Host state will key values, sort values, render plans, runtime ownership, and visible-column selection by the complete stable column identity. Code-line-specific helpers may recognize supported identities but must not collapse them into a single active slot. This preserves the general dynamic-column contract and avoids special-casing provider runtime as an identity.

Alternative: keep one active code-line column and choose a preferred provider. Rejected because it cannot display both columns concurrently and makes enabling order observable.

### Rust aggregates before selecting

The Rust directory classifier will maintain a deterministic map from language name to accumulated `CodeStats`. After the entire valid bounded pack is parsed, it selects the greatest aggregate `code` count, breaking ties by ascending language name. It returns only that language and its aggregate statistics. Unsupported entries are omitted; malformed pack structure fails the item rather than returning a partial misleading value.

Alternative: select the single file with the most code. Rejected because multiple smaller files of one language can collectively be the main language.

### Formatting is isolated from sorting

The Rust renderer will format only the visible code count with comma grouping and concatenate it as `Language: count`. The provider continues to return the raw selected `code` count as `StableSortValueV1::U64`. No locale or new formatting dependency is introduced, making output deterministic for tests.

Alternative: format in the provider payload. Rejected because presentation would contaminate typed data and risk lexicographic sorting.

### Evidence is local and iterative

Automated tests cover identities, coexistence state, aggregation, tie-breaking, formatting, unsupported input, and numeric sorting. The existing headful tokei smoke path will be adapted to enable both fixtures together, construct a mixed-language directory with a count large enough to demonstrate grouping, and capture a screenshot showing both headers and populated cells. Screenshot inspection is a blocking gate; defects found there reopen the relevant implementation and automated checks.

## Failure handling and observability

- A malformed directory pack or directory with no supported source remains `Unsupported` and must not show zero.
- Results and render plans with mismatched column identity or stale generation are discarded and must not overwrite the sibling column.
- Test output and screenshot evidence are retained under the change evidence directory with command, exit status, timestamp, and SHA-256 inventory recorded in an evidence index.

## Risks / Trade-offs

- **[Risk] Existing uncommitted code-line edits overlap this change.** → Inspect diffs before every edit, preserve unrelated hunks, and limit commits to owned files.
- **[Risk] Per-language accumulation changes cached Rust payload semantics without changing its schema number.** → Invalidate/bump the fixture cache schema so older directory aggregates cannot appear current.
- **[Risk] A narrow window clips one of the two headers or formatted values.** → Set deterministic column widths/window dimensions in the headful fixture and inspect the resulting screenshot.
- **[Risk] Host tests pass while runtime registration still collapses providers.** → Require a real packaged dual-provider headful run as the final blocking gate.

## Migration Plan

1. Update descriptors/metadata and host identity routing while retaining existing saved unknown-column entries.
2. Update Rust aggregation, cache version, renderer, and fixture tests.
3. Update packaging and dual-provider headful automation.
4. Run focused tests, package both fixtures, then run and visually inspect the real-app screenshot loop.

Rollback is source-level: revert this change's focused commits. Existing stable identities remain unchanged, so no persisted-layout migration or destructive data operation is required.

## Plan adjustment policy

- **A — task refinement:** commands, ordering, or leaf split may change without changing requirements or gates.
- **B — design/spec correction:** evidence that an in-scope assumption is wrong pauses affected work; design, delta specs, tasks, and stale evidence are updated together.
- **C — material change:** any scope, public contract, platform, permission, blocking gate, or required-evidence change requires user approval. Blocking gates and thresholds are never silently weakened.

## Open Questions

None. The approved design fixes names, format, tie rule, coexistence, and screenshot acceptance criteria.
