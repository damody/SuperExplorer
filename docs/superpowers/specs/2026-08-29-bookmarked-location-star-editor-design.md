# Bookmarked Location Star and Editor Design

## Goal

Make the current-location bookmark control communicate its state immediately and open a compact, Firefox-inspired editor for an existing bookmark.

## Interaction

- A bookmarkable location that is not yet stored shows an outline star.
- A location whose exact typed bookmark target is already stored shows a solid star in the theme focus blue.
- Clicking the solid star edits that existing bookmark; it does not remove it immediately.
- The editor is a normal, independent window. It remains the single route for editing the name, arbitrary path text, and bookmark-folder destination, and for saving or removing the bookmark.

## Presentation

The editor uses a centered GPUI window whose initial width is 80% of the primary display, with a 640px minimum, and a 560px initial height. The user may resize it. It has no native system titlebar, so Windows does not add minimize, maximize, or close buttons; Cancel and Escape are the explicit close paths. Its content has a clear title, labeled name and path fields, a bounded destination list, and a right-aligned action row containing Remove Bookmark, Cancel, and Save. Remove deletes an existing bookmark through persistence; for an unsaved add draft it cancels creation. Existing theme tokens provide surface, focus, accent, and danger colors.

## Data Flow and Errors

The toolbar derives the filled state from `current_folder_bookmark_target_and_id`. Clicking dispatches the existing `ToggleCurrentFolderBookmark` action, which starts an update draft when an ID exists and presents the dedicated editor window. Save continues through the existing typed reducer and persistence notice. Arbitrary non-empty path text is retained without filesystem validation; an empty target keeps the editor open.

## Alternatives

1. Reuse the existing editor window and restyle it (selected): preserves one state and persistence path.
2. Add a second star-specific editor: rejected because save, remove, validation, and rollback behavior would be duplicated.
3. Show an in-window overlay: rejected because the requested interaction is an independent window and overlays previously caused blocked interaction.

## Verification

Add source-contract coverage for the focus-blue solid star, existing-bookmark editor dispatch, responsive 80%-wide normal-window bounds, and retained editor controls. Run focused `explorer-ui` tests, `cargo check -p explorer-app`, formatting checks, and strict OpenSpec validation.
