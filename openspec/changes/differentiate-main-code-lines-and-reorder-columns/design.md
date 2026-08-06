## Context

The code-line surfaces have separate labels but insufficient cross-provider verification. Details
already persists an `OrderedColumnLayout`, yet header and rows currently project registry order and
have resize/sort interactions only. The change spans provider semantics, model invariants, GPUI
pointer interaction, session persistence, and headful automation.

## Goals / Non-Goals

**Goals:** distinct aggregation contracts; Name-fixed drag reorder; shared header/row order;
extension-safe persistence; blocking UITEST screenshots.

**Non-Goals:** ABI changes, multi-column drag, frozen columns beyond Name, or sort redesign.

## Decisions

- Lua Code lines sums `code` for all recognized languages. Rust Main code lines aggregates by
  language then selects greatest code count with ascending-name tie break. Cache versions/keys
  cannot cross semantics.
- Registry owns availability; `OrderedColumnLayout` owns order. All visible Details projections
  traverse `visible_registered`.
- Model methods canonicalize Name at index zero. Name drag is rejected; drop before Name becomes
  immediately after Name.
- Header pointer movement past the logical drag threshold starts reorder; click without drag sorts.
  Midpoint insertion, cue, Escape cancellation, one atomic drop mutation, and current session save
  follow File Explorer behavior.
- UITEST uses mixed Rust plus non-Rust source whose exact totals force Code lines > Main code lines.

## Risks / Trade-offs

- Pointer gesture may conflict with sorting or resize → separate header body drag from splitter and
  require movement threshold.
- Hidden/disabled extensions may lose relative position → retain unknown layout entries and filter
  only at projection.
- Existing sessions may place Name elsewhere → canonicalize on restore without discarding widths.
- Provider cache may mask fixes → bump semantic cache version and test cold/warm paths.

## Migration Plan

Canonicalize existing layouts on load, preserve every stable ID/width/visibility, and invalidate
only Main code lines semantic caches. Rollback leaves compatible session data.

## Adjustment policy

A-level task mechanics may change. In-scope B-level corrections update design/spec/tasks and rerun
dependent evidence. ABI, persistence schema, scope, permissions, or blocking UITEST changes are
C-level and require user authority; blocking gates are never weakened silently.

## Open Questions

None; File Explorer parity resolves gesture decisions.
