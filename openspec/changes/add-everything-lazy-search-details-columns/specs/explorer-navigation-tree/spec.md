## ADDED Requirements

### Requirement: Interactive lazy navigation tree
The navigation pane SHALL expose Explorer-like expandable roots and folders without enumerating child directories on the UI thread.

#### Scenario: User expands a collapsed node
- **WHEN** the user activates its chevron or presses Right while it is focused
- **THEN** the node expands, child containers are enumerated asynchronously once, and a loading state is shown until results arrive

#### Scenario: User collapses a node
- **WHEN** the user activates an expanded node's chevron or presses Left
- **THEN** descendants are hidden immediately and any pending child request for that node is cancelled

#### Scenario: User activates a folder row
- **WHEN** the user clicks the label or presses Enter or Space on a folder node
- **THEN** that location opens in the active tab without also toggling the node twice

### Requirement: Active path synchronization
The tree SHALL automatically reveal and select the active filesystem location independently for every tab.

#### Scenario: Navigation completes
- **WHEN** the active tab navigates to a filesystem descendant
- **THEN** its drive and ancestor folder nodes expand, the exact current node is selected, and the pane can scroll it into view

#### Scenario: Tabs have different tree state
- **WHEN** two tabs expand different nodes or navigate to different locations
- **THEN** switching tabs restores each tab's expanded nodes, cached children, focus, and selected path without cross-contamination

### Requirement: Bounded and cancellable child loading
Navigation-tree enumeration SHALL reuse the service boundary, reject stale generations, exclude non-container children, and bound cached nodes.

#### Scenario: Expansion request becomes stale
- **WHEN** a node collapses, navigation generation changes, the tab closes, or shutdown begins
- **THEN** its cancellation token is set and late batches cannot mutate the visible tree

#### Scenario: Child enumeration fails
- **WHEN** a node is unavailable or enumeration fails
- **THEN** the node remains visible with a retryable error state and the application remains responsive

### Requirement: Navigation cache mutation reconciliation
Expanded filesystem nodes SHALL invalidate and asynchronously reconcile their child cache after relevant watcher notifications and successful file operations.

#### Scenario: Expanded child folder is deleted
- **WHEN** a child folder is deleted externally or by a successful Delete or Shift+Delete operation while its parent is expanded
- **THEN** the stale navigation row SHALL disappear without manual refresh, the parent SHALL remain expanded, and unaffected sibling rows SHALL remain available
