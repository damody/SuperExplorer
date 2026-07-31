## 1. Baseline and Protocol

- [x] 1.1 Record the current focused test baseline and preserve concurrent shared-tree changes
- [x] 1.2 Add owned ShellNew descriptor and safe creation-recipe model types with size/path invariants
- [x] 1.3 Extend file-operation requests, validation, conflicts, progress labels, and completion handling for CreateItem
- [x] 1.4 Add model tests for valid recipes, rejected unsafe/oversized data, and owned serialization boundaries

## 2. ShellNew Discovery and Creation

- [x] 2.1 Implement bounded per-user ShellNew class enumeration and merged registration resolution
- [x] 2.2 Resolve display names, extensions, FileName templates, NullFile, and bounded Data registrations
- [x] 2.3 Exclude Handler/Command-only and malformed registrations and add deterministic Folder/Text fallbacks
- [x] 2.4 Route catalog discovery through the Shell worker/STA without loading third-party handlers in the UI process
- [x] 2.5 Implement folder, empty-file, bounded-data, and trusted-template CreateItem operations through IFileOperation
- [x] 2.6 Add registry-fixture and temporary-directory integration tests for catalog filtering and disk effects

## 3. Official Fluent Assets

- [x] 3.1 Pin and vendor the minimal official regular SVG set needed by every ExplorerIcon variant
- [x] 3.2 Add upstream source manifest, commit metadata, MIT license, and application attribution notice
- [x] 3.3 Implement an embedded Explorer AssetSource and register it during application startup
- [x] 3.4 Replace locally redrawn chrome PathBuilder geometry with exhaustive Fluent SVG mappings
- [x] 3.5 Add offline asset-load, exhaustive mapping, and no-redrawn-geometry source-audit tests

## 4. Shared Command Popup Behavior

- [x] 4.1 Add mutually exclusive New/Sort/View/More/Extensions popup state and enabled active-row selection
- [x] 4.2 Add a dedicated command-menu FocusHandle and synchronize native/UIA focus on open and close
- [x] 4.3 Add pointer occlusion and stop propagation so underlying file rows cannot hover or activate
- [x] 4.4 Implement hover selection plus Up/Down/Home/End/Enter/Space/Escape for every enabled popup item
- [x] 4.5 Wire every actionable Sort, View, More, and Extensions row to one typed action and truthful disabled state
- [x] 4.6 Add state/action/render tests for exclusivity, focus restoration, hit testing, and exactly-once activation

## 5. Explorer-like New Menu

- [x] 5.1 Replace direct New Folder activation with a semantic focusable New popup trigger
- [x] 5.2 Render safe catalog rows with type labels/icons and responsive bounded scrolling
- [x] 5.3 Wire New row pointer/keyboard activation to collision-safe CreateItem requests and item selection
- [x] 5.4 Refresh the directory and surface actionable failure text after creation completion
- [x] 5.5 Add unit and headful tests for menu population, activation, naming collisions, and disk effects

## 6. Shift+Delete Confirmation

- [x] 6.1 Add explicit ConfirmPermanentDelete and CancelPermanentDelete actions and availability rules
- [x] 6.2 Render an accessible occluding modal with item count, warning, Confirm, and Cancel controls
- [x] 6.3 Route Enter/Space/Escape and pointer activation while preventing background commands
- [x] 6.4 Consume the pending snapshot before exactly one confirmed PermanentDelete dispatch
- [x] 6.5 Clear stale confirmation on cancel, navigation, tab close, completion, and shutdown
- [x] 6.6 Add action/state/headful temporary-directory tests for confirm, cancel, repeat suppression, and no recycle fallback

## 7. Verification and Roadmap Integration

- [x] 7.1 Run formatting, focused crate tests, workspace build/tests, and source audits
- [x] 7.2 Run headful UIA/raster evidence for New, every command popup, focus/highlight isolation, and Shift+Delete
- [x] 7.3 Run the requested ten consecutive interaction cycles across multiple folders/tabs and retain failure artifacts
- [x] 7.4 Update UITEST manifest/evidence index and mark only truthfully proven umbrella roadmap tasks complete
- [x] 7.5 Recount remaining umbrella tasks and continue the next independent incomplete implementation slice

## 8. Selected Image Preview

- [x] 8.1 Schedule a bounded larger thumbnail for exactly one selected image when the preview pane is visible
- [x] 8.2 Reject stale preview completions across selection, tab, generation, size, and pane changes
- [x] 8.3 Pass ready preview pixels through the renderer boundary and render them with aspect-ratio containment
- [x] 8.4 Add loading, unsupported, corrupt, multiple-selection, cache, and responsive-selection tests
- [x] 8.5 Verify the supplied E:\\av_out\\326KJN-003.mp4.jpg case in the headful preview pane
