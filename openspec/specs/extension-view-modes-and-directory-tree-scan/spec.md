# extension-view-modes-and-directory-tree-scan Specification

## Purpose
TBD - created by archiving change build-extensible-plugin-platform. Update Purpose after archive.
## Requirements
### Requirement: Dynamic view mode registration
Rust extensions SHALL register feature-scoped stable view IDs with localized name/icon, supported location kinds, priority, selection capability and a data-only render-plan factory. The view switcher and session state SHALL accept built-in and extension IDs; the host owns the GPUI element.

#### Scenario: Size Map feature is enabled
- **WHEN** its package and view feature are effective
- **THEN** `Size Map` appears beside built-in view modes and can be selected for the current tab

### Requirement: Public worker-safe view plan lifecycle
`ViewModeRenderPlanV1` SHALL receive only public location/refresh generations, viewport, DPI, theme facade and selection snapshot. Its synchronous ABI callback SHALL run on a bounded host worker with a per-call durable marker, return a data-only plan, and SHALL NOT receive an action sink, GPUI thread/entity/context, ExplorerState, or internal handle. The host owns lifecycle, GPUI painting, focus, navigation, refresh, suspension, and close; it accepts only the current full host-minted revision.

#### Scenario: Tab changes location
- **WHEN** navigation commits a new current location
- **THEN** the worker callback receives an owned new-generation snapshot and cannot use the old mutable tab model; GPUI rejects a returned old revision

### Requirement: Host-owned recursive tree scan
`DirectoryTreeScanServiceV1` SHALL accept an authorized location, recursive/symlink/hard-link/ignore/metadata policy, deadline, quotas, cancellation and refresh generation. It SHALL perform filesystem I/O off the GPUI thread and return bounded add/update/remove/partial/subtree-complete/scan-complete deltas.

#### Scenario: Deep tree scans incrementally
- **WHEN** Size Map requests complete recursion
- **THEN** direct children and known sizes appear first, deeper deltas aggregate later, and the view remains interactive before scan completion

### Requirement: Owned tree nodes and terminal states
Tree nodes SHALL contain opaque node/item and parent IDs, name, kind, logical bytes, optional allocated bytes, scan state and generation without arbitrary unauthorized paths or native handles. Terminal states SHALL include complete, partial, cancelled, unavailable, resource-limited and failed.

#### Scenario: Subtree is inaccessible
- **WHEN** traversal encounters a permission-denied directory
- **THEN** the scan/view retains other results and marks that subtree partial instead of failing the entire view

### Requirement: Symlink and hard-link policy
Default recursion SHALL not follow directory symlinks/junctions. If following is enabled, file identity SHALL prevent cycles. Hard links SHALL default to per-directory-entry logical size, with an optional same-volume identity-once policy exposed in view legend/tooltip.

#### Scenario: Junction cycle exists
- **WHEN** an enabled follow policy encounters a previously visited directory identity
- **THEN** traversal reports/skips the cycle and does not recurse indefinitely

### Requirement: Shared selection and formal navigation
Extension views SHALL exchange opaque selection through `ViewSelectionBridgeV1` and SHALL request open/enter/new-tab/reveal through host `NavigationRequestV1`. Single click SHALL select; double-click folder SHALL update formal address, breadcrumb and history; double-click file SHALL use the existing open policy.

#### Scenario: Folder rectangle is double-clicked
- **WHEN** a user double-clicks a Size Map folder node
- **THEN** the tab navigates into that folder and back/forward can return, rather than only zooming private plugin state

### Requirement: Refresh generation and stale layout rejection
F5 SHALL increment current location refresh generation, invalidate prior scan aggregation and start a new scan. Requests, deltas and layout commits SHALL carry generation; switching view/folder/tab or disabling the feature SHALL cancel or ignore older work.

#### Scenario: Two scans finish out of order
- **WHEN** an older F5 scan completes after a newer scan
- **THEN** its deltas/layout are rejected and cannot overwrite the current Size Map

### Requirement: View fallback and persistence
If an extension view is missing, incompatible, faulted or disabled, the host SHALL save its unknown ID and switch affected tabs to the last usable built-in view (or Details). Re-enabling SHALL restore availability without forcibly changing current view.

#### Scenario: Active Size Map is disabled
- **WHEN** the user applies feature disable while a tab uses Size Map
- **THEN** the tab safely falls back to a built-in view and no stale renderer callback continues

### Requirement: Size Map semantics and accessibility
The official Size Map SHALL use nested rectangles whose area represents logical bytes, default color represents normalized file type/extension, and hierarchy represents folders. It SHALL provide exact bytes/percentage/type/status tooltips, stable fallback colors, keyboard/UIA navigation and accessible access to visually aggregated small items.

#### Scenario: Tiny nodes are visually grouped
- **WHEN** many files fall below minimum visible area
- **THEN** they may render as `Other`, but remain represented in data, tooltip/search and keyboard/UIA access

### Requirement: Size Map scale and batching
The scan and renderer SHALL handle a synthetic tree of at least 100,000 nodes within configured memory/resource limits using delta and layout batching rather than one synchronous GPUI redraw per node.

#### Scenario: One hundred thousand nodes arrive
- **WHEN** the performance fixture streams the full synthetic tree
- **THEN** UI remains responsive, cancellation completes within the measured bound and redraw/layout count is coalesced
