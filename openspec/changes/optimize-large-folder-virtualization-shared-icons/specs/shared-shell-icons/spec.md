## ADDED Requirements

### Requirement: Shared base icon classification
The system SHALL classify normal folders into a shared folder base and normal associated files by normalized lowercase extension, with size bucket, DPI, theme, and association epoch included in the base cache identity.

#### Scenario: Normal folders share a base
- **WHEN** multiple normal folders request an icon at the same size, DPI, theme, and association epoch
- **THEN** the system issues one shared base-icon load and all matching rows reuse its texture

#### Scenario: Extension comparison is case-insensitive
- **WHEN** `.JPG` and `.jpg` files request icons with otherwise identical base dimensions
- **THEN** both requests resolve to the same shared association base key

#### Scenario: Navigation preserves association cache
- **WHEN** the user navigates between directories without an association, DPI, or theme change
- **THEN** normal folder and extension base icons remain reusable across navigation generations

### Requirement: Identity-specific icon classes
The system MUST use stable item or Shell identity for executables, libraries, icons, shortcuts, control-panel items, drives, known folders, Shell namespace items, and any other class whose base icon can differ per item.

#### Scenario: Executables remain distinct
- **WHEN** two executable files expose different embedded icons
- **THEN** the system does not merge them into one extension base texture

#### Scenario: Shell namespace identity
- **WHEN** two special Shell namespace items request icons
- **THEN** each request retains its stable Shell identity and cannot collide by display name or extension

### Requirement: Visible per-item overrides
The system SHALL render a shared base immediately and SHALL request overlay, custom-folder, or other item-specific Shell results only for visible and near-visible items.

#### Scenario: Overlay replaces shared base
- **WHEN** a realized file has a OneDrive, TortoiseGit, or equivalent Shell overlay
- **THEN** the row first displays its shared base and then displays the item-specific composed result when available

#### Scenario: Custom folder icon
- **WHEN** a realized normal folder has a `desktop.ini` custom icon
- **THEN** the visible item result replaces the generic folder base without changing the base used by other folders

#### Scenario: Offscreen item has no consumer
- **WHEN** an item leaves the visible and near-visible scheduling range before its override completes
- **THEN** the scheduler removes its consumer and permits cancellation without restoring stale UI state

### Requirement: Negative visible-result caching
The system SHALL cache that a visible item has no distinct override for the current overlay epoch so repeated realization does not repeatedly query the Shell.

#### Scenario: Item without overlay re-enters viewport
- **WHEN** an item with a cached negative result leaves and re-enters the viewport during the same overlay epoch
- **THEN** the system reuses the shared base without submitting another item-specific Shell request

### Requirement: Independent icon cache domains
The system SHALL maintain separate byte- and entry-bounded LRU caches for shared bases, visible item results, and thumbnails, and matching rows SHALL share the same `Arc<RenderImage>` allocation.

#### Scenario: Base cache remains bounded
- **WHEN** unique association classes exceed the configured base cache budgets
- **THEN** least-recently-used base entries are evicted until both entry and decoded-byte limits are satisfied

#### Scenario: Overlay eviction does not discard base
- **WHEN** a visible item result is evicted
- **THEN** the corresponding shared extension or folder base remains independently reusable

### Requirement: Independent invalidation epochs
The system SHALL invalidate association bases only for relevant association, DPI, or theme changes and SHALL invalidate per-item results only for relevant overlay, watcher, or Shell state changes.

#### Scenario: Overlay changes independently
- **WHEN** a watcher or Shell notification changes one item's overlay epoch
- **THEN** the system invalidates that item result without invalidating unrelated shared extension bases

#### Scenario: Association changes globally
- **WHEN** Windows reports a file-association change
- **THEN** affected extension base keys advance association epoch and reload on demand

### Requirement: Thumbnail separation
Content thumbnails MUST remain generation-safe, independently cached, and scheduled only from the virtual visible range; a thumbnail MUST NOT become a shared extension base.

#### Scenario: Image thumbnail arrives
- **WHEN** a realized image thumbnail completes for the current source generation
- **THEN** it replaces the association base only for that item and does not affect other files of the same extension
