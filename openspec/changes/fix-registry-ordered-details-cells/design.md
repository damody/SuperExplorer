## Context

The details header projects visible descriptors by iterating `ColumnRegistry`, a `BTreeMap` keyed
by stable `ColumnId`. The row renderer independently emits built-in cells in a fixed chain,
Folder size next, and a separately sorted Code lines vector last. Consequently the geometry can
match in total width while descriptor identity does not match by horizontal position. The current
failure places Folder size bars beneath `Code lines` and Lua counts beneath `Folder size`.

Production extension combinations make a one-off reorder unsafe. Registration, disable,
replacement, and stale-runtime conditions must all preserve identity without changing the public
extension ABI, persisted settings, or current registry order.

## Goals / Non-Goals

**Goals:**

- Use one ordered visible-descriptor projection for header and row placement.
- Resolve every extension cell by exact `ColumnId` and fail closed on missing/mismatched state.
- Preserve independent Lua, Rust, Folder size, and Lock owners behavior across lifecycle changes.
- Verify identity alignment in unit tests and the headful production-like extension path.

**Non-Goals:**

- Add user-controlled column ordering.
- Change `ColumnRegistry` ordering, extension ABI, manifests, or saved width/visibility semantics.
- Generalize unrelated extension render-plan behavior.

## Decisions

### Registry traversal owns placement

The row SHALL traverse the same visible registry descriptors used by the header. A descriptor is
dispatched to the matching built-in or extension renderer, and the produced cell uses that
descriptor's ID for width, visibility, sorting identity, accessibility identity, and content.

Alternative: sort each renderer family and concatenate them. Rejected because inter-family order
still diverges when stable IDs interleave. Alternative: maintain a synchronized renderer order
table. Rejected because it duplicates registry state and creates another lifecycle consistency
boundary.

### Renderer state is keyed by exact descriptor identity

Existing specialized rendering code may remain, but its inputs are projected into ID-keyed
lookups before row construction. A runtime/visual pair is usable only when both descriptors agree
with the current registry descriptor. Stale runtimes are ignored. A current descriptor without a
ready runtime retains its own width and renders the established loading/unavailable presentation;
it never consumes a neighbor's value.

### Lifecycle projection is rebuilt, not patched by position

Registry generation and the current runtime/visual collections determine the projection for each
render snapshot. Enable, disable, uninstall, replacement, and re-enable operations therefore add
or remove cells by ID. No empty slot survives descriptor removal and no old vector index is reused.

### Evidence is blocking

The change is complete only when focused identity/order tests pass and a headful run exercises
extension switches, validates semantic cell content, and records a screenshot. Pixel position
alone is insufficient: automation must associate visible header and cell accessibility identities
or exact values, including Folder size byte formatting/bar behavior and `Rust: 1,250`.

## Risks / Trade-offs

- **Risk: Large render-function refactor introduces visual regressions.** → Extract the smallest
  descriptor-dispatch projection and retain existing specialized cell bodies.
- **Risk: A missing runtime collapses geometry and shifts later columns.** → Dispatch from visible
  descriptors first and emit only that descriptor's defined unavailable/loading cell contract.
- **Risk: stale runtime data leaks into a newly registered descriptor.** → Match full `ColumnId`
  and current registry membership before rendering.
- **Risk: tests verify only one install order.** → Exercise different registration orders and
  independent enable/disable sequences.
- **Trade-off: per-render ID lookup adds small map construction/lookup cost.** → Visible column
  counts are bounded and correctness dominates; avoid new persistent synchronization state.

## Migration Plan

No data migration is required. Land the row-order correction and tests together, build the app,
run focused tests, then run the headful extension switch scenario. Rollback is the code revert;
persisted column IDs, widths, and visibility remain compatible.

## Planning and evidence adjustments

- **A — task refinement:** task order, split, command, or evidence filename may change without
  altering scope, requirements, gates, or public contracts.
- **B — design/spec correction:** a discovered in-scope identity/lifecycle flaw pauses affected
  work; design, spec, tasks, and evidence are updated and strictly revalidated. Dependent evidence
  is marked stale and rerun.
- **C — material change:** any ABI, registry ordering, persistence, scope, required evidence, gate,
  permission, or external/destructive operation change requires user approval.

Blocking gates and required screenshot/lifecycle evidence SHALL NOT be weakened through an A- or
B-level adjustment.

## Open Questions

None. Stable registry `ColumnId` ordering and the required extension combinations were approved.
