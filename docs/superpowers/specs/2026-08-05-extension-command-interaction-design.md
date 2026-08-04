# Extension command interaction design

## Outcome

The Extensions popup keeps every label inside its bounds and turns the existing
`Rename from EXIF` and `Bulk folder generator` commands into host-rendered,
confirm-before-mutation workflows.

## UI and state

The command popup has a bounded Explorer-style width. Every row uses a shrinking
text region, one line, ellipsis, and the full label as its accessibility name.
Selecting either command replaces the command list with an anchored interaction
panel inside the same popup boundary.

`Rename from EXIF` shows a template choice, the selected image count, representative
source-to-target previews, validation failures, and Rename/Cancel buttons.
`Bulk folder generator` shows Prefix, Start, Count, Padding, and Suffix fields,
representative generated names, validation failures, and Create/Cancel buttons.
Escape closes the panel back to the command list; a second Escape closes Extensions.
Outside clicks cancel without filesystem mutation.

## Data and execution

The UI owns only draft values and preview presentation. It submits typed extension
command requests to the host, which validates the active folder and selection,
constructs a typed operation plan, and routes approved steps through existing file
operations. No mutation occurs while opening, editing, previewing, cancelling, or
encountering invalid input.

Bulk folder names follow the fixture contract: count 1..100000, padding 0..16,
checked numeric range, Windows-invalid/reserved-name rejection, traversal rejection,
and case-insensitive collision rejection. EXIF rename accepts image selections only,
parses metadata in-process, reports missing tokens and collisions, and preserves file
extensions. Plans above the existing confirmation threshold require the existing
second confirmation.

## Failure behavior

Field errors remain in the panel and focus stays in the workflow. Stale navigation or
selection invalidates the preview and prevents confirmation. Per-step execution
failures use the existing operation result and undo behavior; no recursive cleanup is
performed for bulk-created directories that are no longer empty.

## Verification

Unit tests cover text sizing/truncation contracts, panel state transitions, Escape,
validation boundaries, preview stability, cancellation, and typed plan creation.
Headful UITEST clicks both real commands, enters values, verifies previews, proves
Cancel causes no disk changes, confirms real rename/folder creation fixtures, and
captures popup screenshots showing that no label exceeds its bounds.

## Alternatives rejected

A separate modal wizard was rejected because it breaks command-bar continuity.
Immediate execution with defaults was rejected because it mutates files without a
preview and explicit confirmation.
