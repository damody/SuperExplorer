## ADDED Requirements

### Requirement: Explorer-like safe New menu
The application SHALL expose a New popup that contains Folder, Text Document, and each safely supported,
deduplicated current-user ShellNew registration, and SHALL omit registrations that require arbitrary
Handler or Command execution.

#### Scenario: New popup shows deterministic and registered items
- **WHEN** the user opens New in a writable filesystem folder
- **THEN** the popup shows Folder and Text Document followed by supported registered types with truthful display names

#### Scenario: Unsafe registration is present
- **WHEN** a ShellNew registration only supplies Handler or Command behavior
- **THEN** the application omits that entry without loading or invoking it in the UI process

### Requirement: Typed safe new-item creation
The application SHALL create a selected New entry through the Shell STA using an owned typed request,
collision-safe naming, and only folder, empty-file, bounded-data, or trusted-template recipes.

#### Scenario: User creates a text document
- **WHEN** the user activates Text Document in a writable folder
- **THEN** exactly one valid empty text file with an Explorer-style non-conflicting name is created and selected

#### Scenario: User creates a registered template item
- **WHEN** the user activates a supported bounded ShellNew template entry
- **THEN** exactly one item is created through the operation pipeline and completion is reported to the UI

### Requirement: Official Fluent command chrome
The application SHALL render every app-owned ExplorerIcon command glyph from vendored regular SVG assets
originating from microsoft/fluentui-system-icons and SHALL include source and MIT attribution metadata.

#### Scenario: Application runs without source tree or network
- **WHEN** a packaged executable renders its command bar offline
- **THEN** every mapped command icon loads from embedded vendored Fluent SVG bytes

#### Scenario: Source audit runs
- **WHEN** the icon provenance audit inspects ExplorerIcon mappings
- **THEN** no mapped icon uses locally redrawn PathBuilder geometry and every asset has recorded upstream provenance

### Requirement: Command popup focus and hit-test isolation
New, Sort, View, More, and Extensions popups SHALL be mutually exclusive focus surfaces that occlude
underlying file rows and support pointer hover, click activation, Up, Down, Home, End, Enter, Space, and
Escape for every enabled item.

#### Scenario: Pointer moves across popup entries
- **WHEN** the pointer hovers an enabled popup entry above a file row
- **THEN** only that popup entry is highlighted and the underlying file row receives no hover or click event

#### Scenario: Keyboard activates an entry
- **WHEN** a popup owns focus and the user navigates to an enabled entry and presses Enter
- **THEN** that entry's command executes once, the popup closes, and focus returns to the command surface

#### Scenario: User switches command popups
- **WHEN** one popup is open and the user opens a different command popup
- **THEN** the previous popup closes before the new popup receives focus

### Requirement: Confirmed permanent deletion
Shift+Delete SHALL open an occluding accessible confirmation modal for the selected items and SHALL
dispatch permanent deletion exactly once only after explicit confirmation.

#### Scenario: User confirms Shift+Delete
- **WHEN** selected temporary items receive Shift+Delete and the user confirms
- **THEN** one confirmed PermanentDelete operation removes exactly the snapshotted items without using the recycle bin

#### Scenario: User cancels Shift+Delete
- **WHEN** the permanent-delete modal is open and the user clicks Cancel or presses Escape
- **THEN** the modal closes, no delete request is dispatched, and all selected items remain on disk

#### Scenario: Destructive key repeats
- **WHEN** Shift+Delete or confirmation input repeats while confirmation or dispatch is pending
- **THEN** the application still dispatches at most one permanent-delete operation for that snapshot

### Requirement: Interaction regression evidence
The application SHALL provide deterministic and headful evidence for New creation, Fluent asset mapping,
popup focus/hit testing, and permanent-delete confirmation, including a ten-run interaction stability test.

#### Scenario: Required verification suite runs
- **WHEN** the focused test and UITEST commands are executed on a supported Windows test environment
- **THEN** unit, integration, UIA, raster, disk-effect, and ten consecutive interaction runs pass with actionable artifacts on failure

### Requirement: Selected image preview
When the preview pane is visible, the application SHALL asynchronously display the real decoded image
for exactly one selected supported image, preserve its aspect ratio, bound memory and dimensions, and
reject stale results after selection, tab, generation, or pane changes.

#### Scenario: Supported JPEG is selected
- **WHEN** one JPEG file is selected while the preview pane is visible
- **THEN** its decoded image appears contained within the pane without distortion and the UI thread remains responsive

#### Scenario: Selection changes during decode
- **WHEN** the selected item changes before an earlier preview thumbnail completes
- **THEN** the stale pixels are not displayed for the new selection and only the current item can become visible

#### Scenario: Preview cannot be decoded
- **WHEN** the selected item is unsupported, corrupt, unavailable, or exceeds resource limits
- **THEN** the pane shows truthful fallback text and remains interactive without retaining unbounded pixels
