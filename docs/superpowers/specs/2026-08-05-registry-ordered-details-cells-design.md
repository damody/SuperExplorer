# Registry-ordered details cells

## Problem

The details header and the corresponding row cells currently use different ordering rules. The
header iterates the complete `ColumnRegistry`, whose descriptors have a stable `ColumnId` order.
Rows instead append built-in cells, the Folder size cell, and Code lines-family cells in separate
hard-coded groups. Sorting the Code lines group internally does not align that group with Folder
size or other extension columns.

This produces the observed mismatch: the header can read `Code lines`, `Folder size`, `Lock
owners`, `Main code lines`, while the row emits Folder size data, Lua line counts, lock-owner data,
and Rust main-language counts. Any extension installation, removal, enable, disable, or reload can
expose another mismatch whenever its registry position differs from its renderer group's fixed
position.

## Decision

`ColumnRegistry` is the single source of truth for details-column order. Both the header and every
row iterate the same visible descriptor projection in stable `ColumnId` order. Each row resolves
the renderer and state for the current descriptor by its exact `ColumnId`; it never infers column
identity from vector position or renderer category.

## Architecture and data flow

1. Build the visible descriptor sequence from `ColumnRegistry` and `ViewSettings`.
2. Preserve the existing registry `ColumnId` ordering without introducing a second order table.
3. Build or expose renderer/state lookups keyed by `ColumnId` for host-owned and extension-owned
   columns.
4. Render each header from one descriptor in the sequence.
5. Render each row by traversing that same sequence and dispatching the descriptor to its matching
   built-in, Folder size, Code lines, or other supported extension renderer.
6. Require the resolved renderer's descriptor ID to equal the registry descriptor ID before
   rendering the cell.

The initial implementation may keep the existing specialized renderer code, but its placement in
the row must be driven by the registry traversal rather than by a fixed chain of `.when` and
`.children` calls.

## Extension lifecycle behavior

- Enabling or installing an extension registers its descriptor and makes its cell appear at the
  corresponding registry position.
- Disabling, uninstalling, or replacing an extension removes its active descriptor projection;
  the row must not retain an empty cell or stale renderer at the old position.
- A runtime without a currently registered descriptor is ignored.
- A registered extension descriptor without a ready runtime must retain correct geometry and may
  render the existing loading/unavailable state, but it must never borrow another column's data.
- Multiple instances of the same renderer family, including Lua and Rust Code lines providers,
  remain independent because lookup is by full `ColumnId`.

## Error handling and invariants

- No cell may be selected by vector index alone.
- A descriptor/runtime ID mismatch must fail closed: render no foreign data in that column.
- Column width, visibility, sorting, accessibility identity, and cell content all use the same
  descriptor ID.
- Extension lifecycle changes must invalidate and rebuild the descriptor projection before the
  next details render.

## Verification

Add focused tests that compare the ordered header IDs with the ordered row cell IDs and cover:

- Folder size plus Lua Code lines;
- Folder size plus Rust Main code lines;
- Folder size, Lua, Rust, and Lock owners together;
- different extension registration/install orders;
- disabling each extension independently;
- removing and re-enabling an extension;
- a stale runtime whose descriptor is no longer registered;
- a descriptor whose runtime is temporarily unavailable.

Run the relevant Rust unit tests and the headful extension smoke test. The final headful scenario
must exercise extension switches, then capture a screenshot showing that every visible heading is
above its own data: Folder size shows byte sizes/bars, Code lines shows Lua numeric line counts,
Lock owners shows owner values, and Main code lines shows values such as `Rust: 1,250`.

## Scope

This change fixes details-column identity and ordering. It does not add user-controlled column
reordering, change the registry's current stable `ColumnId` ordering, or redesign extension cell
rendering beyond what is necessary to make descriptor-to-cell dispatch authoritative.
