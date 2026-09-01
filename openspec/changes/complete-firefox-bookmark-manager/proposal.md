## Why

The bookmark manager currently resembles Firefox visually but most navigation, selection, editing, menu, and backup controls are static or incomplete. This change makes every visible interactive control truthful and completes the Firefox-style library workflow.

## What Changes

- Add manager-owned navigation, folder expansion, selection, sorting, density, and menu state.
- Make the tree filter the bookmark table and make table selection drive an editable details pane.
- Implement management, view, import, and backup commands with durable rollback and visible errors.
- Center editors launched by the manager while retaining star-anchored quick editing.
- Add behavioral tests for every visible control and failure path.

## Capabilities

### New Capabilities
- `firefox-bookmark-manager`: Complete interactive Firefox-style bookmark library behavior.

### Modified Capabilities

## Impact

Touches bookmark manager/editor windows, chrome rendering, typed actions, bookmark state and persistence tests. No bookmark storage format break is intended.
