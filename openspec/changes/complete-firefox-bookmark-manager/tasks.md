## 1. Manager state and projection

- [x] 1.1 Add manager-owned location, selection, expansion, history, menu, sort, density, and search state.
- [x] 1.2 Project tree locations and filtered/sorted table rows with stale-selection repair.

## 2. Complete interactions

- [x] 2.1 Implement Back/Forward and all tree selection/expand controls.
- [x] 2.2 Implement row selection, double-click editing, drag reorder, and context commands.
- [x] 2.3 Implement editable details fields with persistence rollback and error display.
- [x] 2.4 Implement dismissible Manage and View menus with truthful enablement.
- [x] 2.5 Implement complete clipboard import/backup and truthful failure notices.

## 3. Window lifecycle

- [x] 3.1 Center manager-launched bookmark and folder editors; preserve star anchoring.
- [x] 3.2 Restore focus and selection after child editor completion or cancellation.

## 4. Verification

- [x] 4.1 Add state tests for history, filtering, sorting, selection repair, and mutations.
- [x] 4.2 Add source/render contracts proving every enabled control has a handler.
- [x] 4.3 Run formatting, focused tests, all-target compilation, and diff checks.
