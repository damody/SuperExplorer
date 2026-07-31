## Context

`WindowChrome` currently maps an active tab to the blue `selected_active` token while the chrome
container draws a bottom divider. `breadcrumb_child_menu` has a hover style, but its separately
tracked keyboard index remains blue and pointer movement does not update that index. The result is
two competing visual targets.

## Goals / Non-Goals

**Goals:**

- Make active tab and content visually continuous using semantic theme tokens.
- Keep inactive tabs visually part of the gray tab strip.
- Maintain one gray current row that follows real pointer movement in breadcrumb child menus.
- Preserve keyboard navigation, accessibility, stable menu identity, and click behavior.
- Produce deterministic unit and headful pixel/pointer evidence.

**Non-Goals:**

- Redesign tab sizing, tab overflow, caption controls, or general application theming.
- Change breadcrumb enumeration, Shell icons, navigation generations, or menu anchoring.
- Replace native pointer input with test-only state mutation.

## Decisions

The tab renderer will use `surface` for active and `subtle_surface` for inactive, and the navigation
row immediately below will use `surface`. A separately rendered strip divider is painted before the
tabs, and a bottom-edge surface occluder on the active tab covers only the divider under that tab.
This is preferred to deleting the strip divider globally because it must remain visible beneath
inactive/empty chrome. Keyboard focus uses a top-edge indicator so it cannot recreate the shared
bottom line.

The breadcrumb menu will add a typed `SetBreadcrumbMenuFocus { index }` action. Physical row mouse
movement dispatches this action through the existing callback/reducer path. This is preferred to a
view-local hover index because state, UIA selection, keyboard activation, and pointer activation
then share one target identity. The reducer rejects closed-menu and out-of-range updates.

Both pointer hover and keyboard-current fills use the visibly gray `selected_inactive`; pressed remains
`control_pressed`. This is preferred to keeping a blue keyboard selection because Windows menus
present the current command as a neutral hover/focus row, not as file selection.

The focused-tab accessibility outline remains independently visible. It must not reintroduce a
persistent active-selection fill or bottom divider when keyboard focus is elsewhere.

## Risks / Trade-offs

- [Parent/child paint ordering leaves one divider pixel visible] → Verify actual pixels and size the
  active-tab occluder from the semantic focus-stroke token.
- [Mouse movement dispatch causes excess redraws] → Reducer returns no change when the same index is
  already focused.
- [UIA provider does not expose menu selection] → Use physical pointer screenshots and pixel
  transitions as the result oracle while retaining existing UIA keyboard tests.
- [Theme/DPI differences make absolute RGB brittle] → Compare sampled regions to each other and
  record coordinates/colors instead of hard-coding one Windows palette.
