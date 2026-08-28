## 1. Remote menu presentation model

- [x] 1.1 Define the focused remote command descriptors and grouping rules for item and background menus.
- [x] 1.2 Add focused tests for item/background command membership, paste availability, and permanent-delete danger semantics.

## 2. Windows 11-style rendering

- [x] 2.1 Implement the reusable menu surface, separators, text-command rows, and semantic interaction states.
- [x] 2.2 Implement the item-menu icon command strip with existing Fluent Cut, Copy, Rename, and Delete icons.
- [x] 2.3 Connect item and background composition to the existing remote menu state and action callback without changing provider actions.
- [x] 2.4 Update menu-size positioning constants and preserve inside-event propagation and outside/Escape dismissal behavior.

## 3. Focused validation

- [x] 3.1 Add or update focused lifecycle, positioning, theme-contract, accessibility-label, and local-Shell-isolation tests.
- [x] 3.2 Run formatting and the focused explorer-ui test/build commands after implementation is complete.
- [x] 3.3 Perform representative headful checks for ADB and SFTP item and background menus, including hover, command activation, edge placement, and dismissal; record any unavailable environment prerequisite explicitly.
- [x] 3.4 Review the final diff for unrelated changes and confirm every remote-menu requirement maps to implementation and evidence.
