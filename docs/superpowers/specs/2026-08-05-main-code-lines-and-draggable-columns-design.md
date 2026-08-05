# Main code lines semantics and draggable Details columns

## Goals

- `Code lines` reports the sum of code lines across every recognized language.
- `Main code lines` reports only the greatest per-language aggregate and renders `Language: N`.
- Mixed-language folders visibly demonstrate the two values differ whenever non-main languages
  contribute code; equality remains valid for genuinely single-language content.
- Details columns except `Name` can be reordered by horizontal header drag, matching File
  Explorer insertion behavior. `Name` is permanently first.
- Order survives extension enable/disable, re-enable, and app restart.

## Decisions

### Separate aggregation contracts

Lua Code lines retains one numeric total across all tokei languages. Rust Main code lines retains
per-language aggregates, selects greatest code count with ascending language-name tie break, and
uses a distinct cache schema/version. Host directory fast paths and plugin paths must implement
the same respective contracts; neither may reuse the other's cached payload.

### OrderedColumnLayout owns presentation order

`ColumnRegistry` owns descriptor availability and stable identity. `OrderedColumnLayout` owns
user presentation order, visibility, and widths. Header, row cells, menus, accessibility order,
and hit testing project `details_layout.visible_registered(registry)`. Extension registration adds
unknown descriptors without resetting retained order.

### Name invariant

Every restore, reorder, drag, and descriptor insertion path canonicalizes `Name` to index zero.
Dragging Name is rejected. Dropping another column before Name resolves to immediately after Name.

### Drag interaction

A left-button movement beyond the existing logical drag threshold starts a column-header drag;
simple click still sorts. The insertion target is selected from header midpoints like File
Explorer. A visible insertion cue follows the pointer. Drop commits one `move_before` mutation and
persists through the existing session pipeline. Escape or pointer cancellation leaves order
unchanged.

## Testing

- Unit tests for all-language total versus largest-language aggregate, ties, cache separation,
  and single-language equality.
- Model tests for move left/right, Name rejection/canonicalization, hidden/unknown extensions,
  disable/re-enable, and persistence round trip.
- UI tests for click-versus-drag separation and header/row shared order.
- UITEST headful case with a deterministic mixed-language fixture: assert exact unequal values,
  drag `Main code lines` left of `Folder size`, reject dragging Name, restart, assert retained
  order, and capture screenshots.

## Non-goals

- Vertical grouping, frozen columns other than Name, multi-column drag, or changing sorting
  semantics.
