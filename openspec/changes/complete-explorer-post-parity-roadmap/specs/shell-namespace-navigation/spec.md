## ADDED Requirements

### Requirement: Explorer namespace roots
The application SHALL expose Explorer-like Home, Quick Access, Known Folders, This PC, drives, Libraries, ZIP folders, Recycle Bin, Network root, and compatible third-party Shell Namespace Extensions when available on the current Windows system.

#### Scenario: Navigate standard roots
- **WHEN** the user selects each available standard root from the navigation pane or address bar
- **THEN** the application SHALL resolve and enumerate the same public Shell items without requiring a filesystem path

### Requirement: Stable non-path identity
All namespace navigation, selection, history, persistence, thumbnails, properties, and commands SHALL use stable typed Shell identities with reconstructible descriptors rather than treating display names or paths as identity.

#### Scenario: Item has no filesystem path
- **WHEN** a namespace child exposes an absolute PIDL or Known Folder identity but no path
- **THEN** it SHALL remain navigable, selectable, persistable when serializable, and isolated from path-only operations

### Requirement: Capability-aware commands
Containers and items SHALL advertise capabilities that determine whether open, rename, delete, restore, empty, pin, unpin, copy, paste, drop, search, properties, and context-menu commands are enabled.

#### Scenario: Unsupported namespace operation
- **WHEN** the current item does not advertise the capability required by a command
- **THEN** the command SHALL be disabled or return a typed unavailable result with an accessible explanation and no partial mutation

### Requirement: Home and Quick Access behavior
Home SHALL aggregate reconstructible pinned and recent locations/items, and Quick Access SHALL support pin/unpin and stable ordering without presenting synthetic entries as filesystem children.

#### Scenario: Pin and reopen a location
- **WHEN** the user pins a supported location and restarts the application
- **THEN** the pin SHALL remain in Quick Access, navigate to the same identity, and expose unpin through command and context surfaces

#### Scenario: Recent item becomes unavailable
- **WHEN** a recent item no longer resolves or violates privacy/filter policy
- **THEN** Home SHALL omit or mark it unavailable without blocking other Home content

### Requirement: Explorer navigation semantics
Namespace locations SHALL participate in breadcrumbs, editable address input, Back/Forward/Up, new tabs, middle-click where applicable, refresh, focus restoration, and tab-local history using Explorer-compatible public behavior.

#### Scenario: Navigate across path and namespace locations
- **WHEN** a tab moves from a filesystem folder to This PC, into a drive, and then Back/Forward
- **THEN** history, breadcrumb identity, command availability, focus, and selection restoration SHALL follow the corresponding location generation

### Requirement: Namespace enumeration and metadata
Enumeration SHALL be incremental, cancellable, capability-aware, and able to retrieve Shell display names, icons, overlays, type, properties, and column metadata without blocking the UI thread.

#### Scenario: Slow third-party namespace
- **WHEN** a provider enumerates slowly, fails, or returns malformed metadata
- **THEN** partial valid items SHALL remain usable, the request SHALL terminate with typed status, and later navigation SHALL remain responsive

### Requirement: Archive, Library, Network, and Recycle Bin behavior
The application SHALL support public Shell browsing for ZIP folders and Libraries, basic Network discovery/navigation using Windows-owned authentication UI, and Recycle Bin browse, restore, permanent delete, and confirmed empty operations where capabilities permit.

#### Scenario: Restore from Recycle Bin
- **WHEN** the user selects a restorable Recycle Bin item and confirms Restore
- **THEN** the Shell operation SHALL restore it or report a per-item typed failure without treating the virtual item as a normal path

### Requirement: Data transfer and context interoperability
Namespace items SHALL integrate with Windows Shell `IDataObject`, drop targets, context menus, and file operations when their advertised capabilities permit, including path and non-path combinations.

#### Scenario: Copy from ZIP to filesystem folder
- **WHEN** a ZIP namespace item supplies a transferable Shell data object and the user copies it to a writable filesystem folder
- **THEN** the operation SHALL use Shell transfer semantics, progress, cancellation, and per-item outcomes

### Requirement: Namespace accessibility and evidence
Navigation roots and children SHALL expose stable UIA roles, localized names, selection/expanded state, actions, keyboard traversal, high-contrast behavior, and real-Windows capability evidence.

#### Scenario: Keyboard-only namespace navigation
- **WHEN** a user navigates Home, This PC, a Library, and Recycle Bin using only the keyboard
- **THEN** focus, expansion, selection, activation, context commands, and accessible state SHALL remain observable and deterministic
