## Purpose

Provide a Finder-style, horizontally browsable local filesystem view in which each folder level has its own column and a selected file can be previewed alongside its containing folders.

## ADDED Requirements

### Requirement: Built-in Columns view and location eligibility
The application SHALL offer a localized, keyboard-accessible Columns view in the existing View control for local drive filesystem directories. The selected per-tab mode SHALL persist across restart. For unsupported locations, the application SHALL show a usable Details view and SHALL restore Columns when that tab returns to an eligible local directory.

#### Scenario: Choose Columns in a local folder
- **WHEN** a user chooses Columns while viewing a directory on a local drive
- **THEN** the file surface changes to adjacent folder columns without opening a new tab or changing the selected directory

#### Scenario: Navigate to an unsupported location
- **WHEN** a tab with Columns selected navigates to UNC, WSL, a Shell namespace, an archive, a virtual/cloud location, or a remote provider
- **THEN** it shows Details with an accurate mode indicator; returning to a local drive directory restores Columns

### Requirement: Folder-level column navigation
Each directory column SHALL list its immediate visible children with the existing hidden-item, sorting, icon, and filesystem-identity rules. Selecting an eligible folder SHALL select it in its parent column, show its children in the next column, update the tab's current location/address/breadcrumb and history through formal navigation, and discard any deeper columns from the previous branch.

#### Scenario: Select nested folders
- **WHEN** a user selects folder B in column A and then folder C in B's column
- **THEN** both ancestor selections remain visible, C's children appear in the next column, the address names C, and Back can return to B

#### Scenario: Choose a sibling folder
- **WHEN** a user selects a different folder in an ancestor column
- **THEN** all stale descendant columns and any file preview disappear before the new branch loads

#### Scenario: Empty or inaccessible folder
- **WHEN** the chosen folder is empty or cannot be read
- **THEN** its column shows a localized empty or error state, retains a usable parent column, and offers the existing recovery/refresh path

### Requirement: File selection and integrated preview
Selecting exactly one local file SHALL keep its containing folder as the current location and show a right-side integrated preview with the file's name and available metadata. The view SHALL reuse the existing supported image and Windows Preview Handler behavior, including loading, unsupported, offline, crash, and error fallbacks. An existing separate Preview Pane SHALL NOT be drawn in addition to the integrated preview.

The existing Preview Pane command and Alt+P SHALL toggle this integrated preview while Columns is effective. Its visibility SHALL be a per-tab Columns preference, independent of the ordinary Preview Pane preference used by other modes.

#### Scenario: Select an image
- **WHEN** a user selects a supported image in the rightmost directory column
- **THEN** its aspect-preserving preview appears at the right without blocking folder interaction

#### Scenario: Select a file in an ancestor column
- **WHEN** a user selects a file in an ancestor column
- **THEN** deeper folder columns are removed, the containing folder becomes current, and only that file is previewed

#### Scenario: Select multiple items or a folder
- **WHEN** selection contains multiple items, becomes empty, or points to a folder
- **THEN** the prior file preview is removed and a truthful neutral or folder summary appears

#### Scenario: Toggle preview in Columns
- **WHEN** a user invokes Alt+P while Columns is effective
- **THEN** the integrated preview opens or closes, the command's checked state follows it, and no second preview pane appears

### Requirement: Preview lifecycle and safety
Preview work SHALL be asynchronous, bounded by the existing preview resource policies, and scoped to the current tab, item, and selection generation. Changing selection, folder, tab, mode, or closing the window SHALL cancel or ignore stale results and release active preview resources. Automatic preview SHALL NOT force cloud placeholder hydration.

#### Scenario: Fast selection changes
- **WHEN** an earlier preview finishes after a different file is selected
- **THEN** the earlier content never appears for the current selection

#### Scenario: Handler failure
- **WHEN** a preview handler times out, crashes, or cannot load a file
- **THEN** navigation remains responsive and the integrated preview presents the existing recoverable fallback

### Requirement: Deep paths and resizable layout
The view SHALL support arbitrary folder depth through horizontal scrolling, independent vertical scrolling per directory column, and adjustable directory-column and preview widths within usable limits. The selected column and preview SHALL remain reachable at supported window sizes, DPI scales, and themes; reduced width SHALL not silently discard the path.

#### Scenario: Deep folder chain
- **WHEN** a user navigates deeper than fits in the window
- **THEN** the view scrolls horizontally to reveal the newly opened column while preserving access to its ancestors

#### Scenario: Resize a column
- **WHEN** a user drags a column divider or changes window size/DPI
- **THEN** widths remain clamped to usable bounds, content stays clipped within its own column, and focus/selection are retained

### Requirement: Keyboard, commands, and file operations
The active directory column SHALL participate in existing selection, context-menu, clipboard, drag/drop, rename, delete, refresh, and open commands using the selected item's true containing directory. Arrow keys SHALL traverse rows and levels; Enter or double-click on a file SHALL use the existing open policy. Back, Forward, Up, address entry, breadcrumbs, and F5 SHALL produce a coherent visible column chain.

#### Scenario: Keyboard exploration
- **WHEN** a user focuses a folder row and presses Right, then Left
- **THEN** Right enters/reveals its child column and Left returns focus to the parent selection without losing the path

#### Scenario: File command from an ancestor column
- **WHEN** a user invokes a file operation on a selected item in an ancestor column
- **THEN** the command targets that exact item and its real parent directory, then affected visible columns refresh

#### Scenario: Refresh after external mutation
- **WHEN** an external process removes a selected folder and the user refreshes
- **THEN** invalid descendant columns and preview are removed, surviving ancestor content updates, and the tab remains navigable

### Requirement: Session, accessibility, and responsiveness
The application SHALL restore the Columns mode and valid layout settings without persisting file preview pixels, handler objects, or stale directory snapshots. Each column, row, selected branch, divider, and preview status SHALL expose stable accessible roles/names and a predictable focus order. Large directories and rapid column changes SHALL remain responsive through bounded, cancellable work.

#### Scenario: Restart with Columns selected
- **WHEN** a session is restored after Columns was active in a local folder
- **THEN** the tab rebuilds a valid column chain from its restored location, applies saved widths, and does not display a stale file preview

#### Scenario: Screen-reader traversal
- **WHEN** a keyboard or screen-reader user traverses columns and the preview
- **THEN** the current folder level, selected item, loading/error status, and available actions are discoverable without a mouse
