# Explorer Tab Surface and Breadcrumb Hover Design

## Goal

Match Windows File Explorer in two narrowly scoped visual interactions:

- The active tab and the content below it form one continuous white surface. The active tab does
  not retain a blue selection fill or a divider along its bottom edge. Inactive tabs use the same
  gray surface as the surrounding tab strip.
- A breadcrumb chevron menu uses one gray highlight that follows the physical pointer from row to
  row. A stale keyboard-selected blue row must not remain highlighted while the pointer is over a
  different row.

Keyboard focus remains accessible and is distinct from the active-tab surface. Keyboard menu
navigation continues to use the same current-row identity as pointer navigation.

## Rendering

`tab_background` will map an active tab to the semantic content `surface` token and an inactive tab
to the tab-strip `subtle_surface` token. The navigation row immediately below also uses `surface`.
The active tab adds a surface-colored bottom occluder over a separately rendered strip divider, so
the tab and navigation/content surface meet without a visible line. The existing rounded top
geometry and close button behavior remain; keyboard focus uses a top-edge indicator that never
reintroduces the shared bottom line.

Breadcrumb child rows keep stable IDs and native Shell icons. Their current-row and pointer-hover
fill both use the visibly gray `selected_inactive`; pressed state continues to use
`control_pressed`. The menu no longer uses blue `selected_active` for its current row.

## Interaction

A typed `SetBreadcrumbMenuFocus { index }` action updates the existing per-tab breadcrumb menu
focus. Each row dispatches it from physical pointer movement before click activation. The reducer
validates that the menu is open and the index is in range. Keyboard movement, type-ahead, Escape,
click activation, focus restoration, and async menu generations remain unchanged.

## Verification

Rust tests cover the color-token mapping, divider occlusion contract, pointer action routing, and
bounded focus reducer. A headful UTIT creates two tabs, captures pixel evidence for active/content
continuity and inactive/strip equality, opens a breadcrumb child menu, moves the real pointer over
two rows, and proves the gray highlight moves while the previous row returns to the menu fill.
Evidence is stored as screenshots plus a JSON report and mapped into the OpenSpec coverage gate.
