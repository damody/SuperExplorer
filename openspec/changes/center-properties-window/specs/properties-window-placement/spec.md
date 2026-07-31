## ADDED Requirements

### Requirement: Native Properties sheets use owner-relative placement
SuperExplorer SHALL position a native Windows Shell Properties sheet over the validated active
SuperExplorer owner without replacing or modifying the provider-owned property sheet.

#### Scenario: Active owner is valid
- **WHEN** a file, folder, compatible multi-selection, executable, or script Properties command
  creates its top-level native property sheet and the originating SuperExplorer HWND is valid
- **THEN** the sheet SHALL be centered over that SuperExplorer window within a bounded DPI-aware
  tolerance before activation

#### Scenario: Owner is unavailable
- **WHEN** the originating SuperExplorer owner cannot provide a valid rectangle
- **THEN** the sheet SHALL be centered within the work area of the monitor nearest the invocation
  point

### Requirement: Properties placement remains work-area safe
SuperExplorer MUST keep the positioned sheet usable within the selected monitor work area without
changing its native dimensions, focus, ownership, Z-order, pages, or lifecycle.

#### Scenario: Centered rectangle crosses a work-area edge
- **WHEN** owner-relative centering would place any fitting sheet edge outside the monitor work area
- **THEN** the position SHALL be clamped so the complete sheet rectangle remains inside that work
  area

#### Scenario: Sheet is larger than a work-area dimension
- **WHEN** the native sheet is larger than the selected work area in one dimension
- **THEN** that dimension SHALL align to the work-area origin without resizing the native sheet

#### Scenario: Placement support fails
- **WHEN** hook installation, rectangle lookup, monitor lookup, or repositioning fails
- **THEN** the native Properties command SHALL continue without leaking hook state or preventing
  subsequent context-menu commands

### Requirement: Properties placement has result-based regression coverage
The Properties headful UTIT MUST validate real native sheets and their placement across the
supported target classes.

#### Scenario: Representative native sheets are invoked
- **WHEN** UTIT invokes Properties for a file, folder, compatible multi-selection, executable, and
  script through genuine pointer context menus
- **THEN** every sheet SHALL expose native filesystem property controls, target-appropriate title,
  owner-relative center evidence, work-area containment evidence, Escape dismissal, and usable
  subsequent context-menu behavior

