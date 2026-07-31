## 1. Generic Shell Icon Contract

- [x] 1.1 Add a canonical generic breadcrumb folder key using the existing synthetic folder location, current association generation, DPI, theme, and zero overlay generation.
- [x] 1.2 Include the generic key exactly once in initialization and navigation-icon submission, with deduplication and invalidation tests.
- [x] 1.3 Add Shell-path coverage proving the generic key uses `SHGFI_USEFILEATTRIBUTES` with directory attributes and stays distinct from concrete location keys.

## 2. Breadcrumb Rendering

- [x] 2.1 Add a shared Shell-only breadcrumb icon renderer with concrete-over-generic precedence and a fixed-size empty final fallback.
- [x] 2.2 Apply the renderer to This PC root, visible ancestry segments, overflow items, and child-menu items.
- [x] 2.3 Remove application-drawn `NavigationIcon` fallback calls from the breadcrumb path and add structural tests for every surface.

## 3. Cache and Event Behavior

- [x] 3.1 Include the generic texture in breadcrumb icon snapshots while preserving bounded cache and pending-key behavior.
- [x] 3.2 Add deterministic tests for late concrete replacement, concrete failure retaining generic fallback, and theme/DPI/association key changes.

## 4. UTIT

- [x] 4.1 Extend or add a headful breadcrumb case that navigates a multi-level fixture and captures root and segment icon slots.
- [x] 4.2 Verify the application remains pointer/keyboard responsive while Shell icons load and declare the new evidence in the UTIT manifest.

## 5. Verification

- [x] 5.1 Run formatting and focused explorer-ui/explorer-shell-win unit and structural tests.
- [x] 5.2 Run the breadcrumb headful UTIT cases and inspect screenshots and reports.
- [x] 5.3 Run scoped Clippy and strict OpenSpec validation.
- [x] 5.4 Mark all tasks complete and commit only intended code, tests, submodule pointer if needed, and OpenSpec artifacts.

## 6. Navigation Pane Shell Icons

- [x] 6.1 Resolve drive textures by exact key first and then the newest compatible same-location cache key so This PC epoch replacement cannot regress the icon.
- [x] 6.2 Render ordinary navigation-tree folders with the generic Shell folder texture and reserve an empty fixed slot instead of using application-drawn drive/folder fallbacks.
- [x] 6.3 Add deterministic tests for drive epoch replacement, generic folder selection, and first-frame empty slots.

## 7. Navigation Pane UTIT and Verification

- [x] 7.1 Add a headful UTIT case that captures drive icons before and after opening This PC and verifies expanded folders have non-empty Shell icon pixels.
- [x] 7.2 Run formatting, focused tests, scoped Clippy, UTIT coverage validation, strict OpenSpec validation, and commit only intended changes.
