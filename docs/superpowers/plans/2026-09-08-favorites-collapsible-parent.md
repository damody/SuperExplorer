# Favorites Collapsible Parent Implementation Plan

> **For agentic workers:** Execute inline in this session. Do not pause for confirmation.

**Goal:** Left navigation Favorites is a collapsible parent like Quick Access: clicking it opens a virtual Favorites folder in the file view; clicking a child folder does not open the bookmark toolbar dropdown.

**Architecture:** Add `SyntheticRoot::Favorites` for the root listing and `super-explorer:favorites/{uuid}` parsing names for nested bookmark folders. Split toolbar `ToggleBookmarkFolderMenu` from left-tree expansion. Left-nav parent uses `ActivateNavigationItem` + `ToggleNavigationNode`; left-nav folders use `ActivateNavigationItem` + new `ToggleBookmarkFolderExpanded`.

**Tech Stack:** Rust crates `explorer-model`, `explorer-ui`; existing bookmark tree and synthetic-root navigation.

**Spec:** Conversation 2026-09-08. Screenshot: clicking left Favorites must not pop the bookmark-bar folder menu; Favorites must collapse into a parent node.

## Global Constraints

- Bookmark toolbar folder buttons keep `ToggleBookmarkFolderMenu` and still open the overlay dropdown.
- `toggle_bookmark_folder_menu` must not mutate `expanded_bookmark_folders`.
- Left-nav favorite folder left-click must never dispatch `ToggleBookmarkFolderMenu`.
- Favorites parent defaults expanded (`favorites_nav_collapsed: false`).
- Nested bookmark folders default collapsed until the user expands the chevron or navigates into them.
- Virtual Favorites listings are read-only (`can_write: false`).
- Reuse `nav-favorites` for the parent label and root title. Nested titles are folder names.
- Keep element ids `favorites-tree-heading`, `favorite-folder-nav`, `favorite-bookmark-nav`.
- Right-click parent still adds a root bookmark folder. Right-click left-nav folder opens `OpenBookmarkToolbarContextMenu`, not the toolbar dropdown.
- Leaf bookmarks still activate via `ActivateBookmark`.
- Do not change Linux/This PC/Phones always-visible static children.

---

### Task 1: Favorites location descriptors

**Files:**
- Modify: `crates/explorer-model/src/domain.rs`
- Test: same file `#[cfg(test)]` module

**Interfaces:**
- Produces: `SyntheticRoot::Favorites`
- Produces: `LocationDescriptor::favorites_folder(id: Uuid) -> Self`
- Produces: `LocationDescriptor::favorites_folder_id(&self) -> Option<Uuid>`
- Produces: `LocationDescriptor::lua_bookmark(id: Uuid) -> Self`
- Produces: `LocationDescriptor::lua_bookmark_id(&self) -> Option<Uuid>`
- Produces: `FAVORITES_NAME = "super-explorer:favorites"`
- Produces: folder prefix `super-explorer:favorites/`
- Produces: lua prefix `super-explorer:bookmark/`

- [ ] **Step 1: Write the failing test** in `domain.rs` next to `location_boundaries_validate_synthetic_empty_oversized_and_unknown_data`.

```rust
#[test]
fn favorites_and_lua_bookmark_parsing_names_round_trip() {
    let root = LocationDescriptor::synthetic(SyntheticRoot::Favorites);
    assert_eq!(root.synthetic_root(), Some(SyntheticRoot::Favorites));
    assert_eq!(root.editable_text(), "super-explorer:favorites");
    assert_eq!(root.favorites_folder_id(), None);

    let folder_id = uuid::Uuid::parse_str("11111111-1111-1111-1111-111111111111").unwrap();
    let folder = LocationDescriptor::favorites_folder(folder_id);
    assert_eq!(folder.synthetic_root(), None);
    assert_eq!(folder.favorites_folder_id(), Some(folder_id));
    assert_eq!(
        folder.editable_text(),
        "super-explorer:favorites/11111111-1111-1111-1111-111111111111"
    );

    let bookmark_id = uuid::Uuid::parse_str("22222222-2222-2222-2222-222222222222").unwrap();
    let lua = LocationDescriptor::lua_bookmark(bookmark_id);
    assert_eq!(lua.lua_bookmark_id(), Some(bookmark_id));
    assert_eq!(root.favorites_folder_id(), None);
    assert!(lua.favorites_folder_id().is_none());
}
```

Also extend the existing synthetic-root loop:

```rust
for root in [SyntheticRoot::Home, SyntheticRoot::QuickAccess, SyntheticRoot::Favorites]
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p explorer-model favorites_and_lua_bookmark_parsing_names_round_trip -- --nocapture`

Expected: compile error (`no variant Favorites` / missing methods).

- [ ] **Step 3: Write minimal implementation**

```rust
pub enum SyntheticRoot {
    Home,
    QuickAccess,
    Favorites,
}

impl SyntheticRoot {
    const FAVORITES_NAME: &'static str = "super-explorer:favorites";
    // parsing_name / from_parsing_name include Favorites -> FAVORITES_NAME
}

impl LocationDescriptor {
    pub const FAVORITES_FOLDER_PREFIX: &'static str = "super-explorer:favorites/";
    pub const LUA_BOOKMARK_PREFIX: &'static str = "super-explorer:bookmark/";

    pub fn favorites_folder(id: Uuid) -> Self {
        Self::ParsingName(format!("{}{id}", Self::FAVORITES_FOLDER_PREFIX))
    }
    pub fn favorites_folder_id(&self) -> Option<Uuid> { /* strip prefix, parse */ }
    pub fn lua_bookmark(id: Uuid) -> Self {
        Self::ParsingName(format!("{}{id}", Self::LUA_BOOKMARK_PREFIX))
    }
    pub fn lua_bookmark_id(&self) -> Option<Uuid> { /* strip prefix, parse */ }
}
```

- [ ] **Step 4: Run tests**

Run: `cargo test -p explorer-model --lib`

Expected: PASS

- [ ] **Step 5: Commit** (skip unless the session is committing; keep working)

---

### Task 2: State — expansion split, listing, Up, reveal

**Files:**
- Modify: `crates/explorer-ui/src/state.rs`
- Modify: `crates/explorer-ui/src/actions.rs`
- Modify: `crates/explorer-ui/src/lib.rs` (action handler for the new toggle)

**Interfaces:**
- Consumes: Task 1 location helpers
- Produces: `favorites_nav_collapsed: bool` default `false`
- Produces: `fn favorites_nav_expanded(&self) -> bool`
- Produces: `fn toggle_bookmark_folder_expanded(&mut self, id) -> bool`
- Produces: `fn reveal_favorites_navigation(&mut self, location: &LocationDescriptor)`
- Produces: `synthetic_root_entries` lists Favorites root children; new `favorites_entries(parent: Option<BookmarkFolderId>) -> Vec<FileEntry>`
- Produces: `begin_up_navigation` walks bookmark-folder parent to Favorites root
- Produces: `ExplorerAction::ToggleBookmarkFolderExpanded { id }`

Listing rules for a parent (`None` = root):
1. Child folders first, sorted by `order`, each `FileEntry`:
   - `id` from `b"favorites-folder:" + uuid bytes`
   - `location` = `LocationDescriptor::favorites_folder(folder.id)`
   - `is_container` = true
   - `type_display` = `"Bookmark folder"`
2. Child bookmarks next, sorted by `order`:
   - Folder/File location targets use that location
   - FolderPath/FilePath use `resolve_bookmark_path` logic duplicated as a small helper in `state.rs` (same rules as `lib.rs::resolve_bookmark_path`)
   - Lua uses `LocationDescriptor::lua_bookmark(bookmark.id)`, `is_container` = false, `type_display` = `"Lua bookmark"`
   - `is_container` = `target.is_folder()`
   - `id` from bookmark uuid bytes

`toggle_bookmark_folder_menu` becomes only:

```rust
self.bookmark_folder_menu = (self.bookmark_folder_menu != Some(id)).then_some(id);
```

`toggle_bookmark_folder_expanded`:

```rust
if !self.expanded_bookmark_folders.remove(&id) {
    self.expanded_bookmark_folders.insert(id);
    true
} else {
    false
}
```

`navigation_node_expanded` / `toggle_navigation_node`: if location is Favorites root, toggle `favorites_nav_collapsed` instead of the HashSet (absent HashSet would default-collapse; we need default expanded).

`reveal_favorites_navigation`:
- Favorites root: nothing required beyond selection
- Favorites folder: `favorites_nav_collapsed = false`, insert folder id and ancestor folder ids into `expanded_bookmark_folders`
- Call from `begin_active_navigation` when the destination is a favorites location

`begin_up_navigation` extra parent:

```rust
.or_else(|| {
    if location.synthetic_root() == Some(SyntheticRoot::Favorites) {
        return None;
    }
    let id = location.favorites_folder_id()?;
    let parent = self.bookmarks.folder(id)?.parent_id;
    Some(parent.map(LocationDescriptor::favorites_folder)
        .unwrap_or_else(|| LocationDescriptor::synthetic(SyntheticRoot::Favorites)))
})
```

`open_row_command`: if `entry.location.lua_bookmark_id()` is `Some`, return `None` (ExplorerRoot intercepts OpenItem/OpenFocused and dispatches `ActivateBookmark`). Add `fn lua_bookmark_id_for_row(&self, row_index) -> Option<BookmarkId>`.

- [ ] **Step 1: Write failing tests** in `state.rs` near `quick_access_toggle_persists_...`:

```rust
#[test]
fn favorites_parent_defaults_expanded_and_collapses_without_opening_folder_menu() {
    let mut state = AppViewState::default();
    let root = LocationDescriptor::synthetic(explorer_model::SyntheticRoot::Favorites);
    assert!(state.favorites_nav_expanded());
    assert!(state.toggle_navigation_node(root.clone()));
    // wait: toggle currently returns true when expanding. For favorites default expanded,
    // first toggle collapses and should return false.
    assert!(!state.favorites_nav_expanded());
    assert!(state.bookmark_folder_menu().is_none());
}

#[test]
fn toolbar_folder_menu_does_not_expand_left_nav_folder() {
    let mut state = AppViewState::default();
    let _ = state.add_bookmark_folder("super".into(), None);
    let id = state.bookmarks().folders()[0].id;
    state.toggle_bookmark_folder_menu(id);
    assert_eq!(state.bookmark_folder_menu(), Some(id));
    assert!(!state.bookmark_folder_expanded(id));
}

#[test]
fn favorites_listing_and_up_from_nested_folder() {
    let mut state = AppViewState::default();
    let _ = state.add_bookmark_folder("super".into(), None);
    let folder_id = state.bookmarks().folders()[0].id;
    let _ = state.bookmarks_mut_for_test().begin_add_to(
        "portable".into(),
        BookmarkTarget::FolderPath { path: r"C:\portable".into() },
        None,
    );
    let entries = state.favorites_entries(None);
    assert_eq!(entries[0].display_name, "super");
    assert_eq!(entries[0].location, LocationDescriptor::favorites_folder(folder_id));
    assert!(entries[0].is_container);
    assert_eq!(entries[1].display_name, "portable");

    let nested = LocationDescriptor::favorites_folder(folder_id);
    let _ = state.begin_active_navigation(nested.clone(), false);
    assert!(state.bookmark_folder_expanded(folder_id));
    assert!(state.favorites_nav_expanded());

    let up = state.begin_up_navigation().expect("up from nested favorites");
    match up {
        ExplorerCommand::Navigate { location, .. } => {
            assert_eq!(location, LocationDescriptor::synthetic(SyntheticRoot::Favorites));
        }
        other => panic!("{other:?}"),
    }
}
```

If `bookmarks_mut_for_test` does not exist, add bookmarks via existing `add_bookmark` / `begin_add` on `configure_bookmarks`. Prefer building a `Bookmarks` value and `configure_bookmarks`.

- [ ] **Step 2: Run tests, confirm they fail**

Run: `cargo test -p explorer-ui --lib favorites_parent_defaults_expanded -- --nocapture`

- [ ] **Step 3: Implement state + action variant + exhaustive match arms** (`name`, `is_enabled`, `apply_action`). `apply_action` for `ToggleBookmarkFolderExpanded` focuses `FocusSurface::NavigationPane`.

- [ ] **Step 4: Tests pass**

Run: `cargo test -p explorer-ui --lib favorites_ -- --nocapture`

---

### Task 3: Left navigation rendering

**Files:**
- Modify: `crates/explorer-ui/src/chrome.rs` (`bookmark_navigation_rows`)
- Modify: `crates/explorer-ui/src/icons.rs`
- Modify: `crates/explorer-ui/src/navigation_pane.rs` (`NavigationIcon::Favorites`)
- Modify: `crates/explorer-ui/src/lib.rs` (handle `ToggleBookmarkFolderExpanded`)

**Interfaces:**
- Consumes: `favorites_nav_expanded`, `ToggleBookmarkFolderExpanded`, Favorites locations
- Parent row id stays `favorites-tree-heading`
- Folder rows stay `favorite-folder-nav`
- Bookmark rows stay `favorite-bookmark-nav`

Parent row:
- Same height / chevron / icon / label layout as `navigation_item_row`
- Icon `NavigationIcon::Favorites` (draw with Home/QuickAccess art)
- Chevron: `ToggleNavigationNode { location: Favorites root }`, `cx.stop_propagation()`
- Row click: `ActivateNavigationItem { location: Favorites root }`
- Right-click: `AddBookmarkFolder { parent_id: None }` (unchanged)
- Children rendered only when `state.favorites_nav_expanded()`

Folder row:
- Chevron: `ToggleBookmarkFolderExpanded { id }`, `stop_propagation`
- Row click: `ActivateNavigationItem { location: favorites_folder(id) }`
- Right-click: `OpenBookmarkToolbarContextMenu { parent_id: Some(id), x, y }`
- Nested children only when `bookmark_folder_expanded(id)`

Bookmark row:
- Click: `ActivateBookmark { id }` (unchanged)
- Right-click: `OpenBookmarkContextMenu` (unchanged)

`NavigationPane` flattening: parent chevron state `item.expanded = state.favorites_nav_expanded()` if we convert parent to a `NavigationItem`. Prefer keeping `bookmark_navigation_rows` so leaf bookmarks can keep `ActivateBookmark`.

Source-scan test updates in `chrome.rs`:
- Assert `favorite-folder-nav` click uses `ActivateNavigationItem` / `ToggleBookmarkFolderExpanded`
- Assert it does **not** use `ToggleBookmarkFolderMenu` inside `bookmark_navigation_rows`

- [ ] **Step 1: Failing source-scan + icon exhaustiveness tests**

```rust
#[test]
fn left_nav_favorite_folder_does_not_toggle_toolbar_folder_menu() {
    let production = include_str!("chrome.rs")
        .split("fn bookmark_navigation_rows")
        .nth(1)
        .expect("bookmark_navigation_rows");
    let visit = production.split("fn navigation_item_shell_texture").next().unwrap();
    assert!(visit.contains("ToggleBookmarkFolderExpanded"));
    assert!(visit.contains("ActivateNavigationItem"));
    assert!(!visit.contains("ToggleBookmarkFolderMenu"));
    assert!(visit.contains("favorites_nav_expanded"));
}
```

Add `NavigationIcon::Favorites` to the Linux-style exhaustiveness source scan if one exists (`icons.rs` match + `has_chevron` does not need Favorites because parent is custom).

- [ ] **Step 2: Run, confirm fail**
- [ ] **Step 3: Implement rendering**
- [ ] **Step 4: Run** `cargo test -p explorer-ui --lib left_nav_favorite_folder -- --nocapture`

---

### Task 4: Service-independent Favorites navigation

**Files:**
- Modify: `crates/explorer-ui/src/lib.rs` (`submit_command`, `OpenItem`/`OpenFocused`)

**Interfaces:**
- Consumes: `synthetic_root_entries` + `favorites_entries`
- Navigate/Refresh to Favorites root or `favorites_folder_id` is handled locally like Home/QuickAccess
- Root title: `catalog.t("nav-favorites")`
- Nested title: folder name, fallback `nav-favorites`
- `can_go_up`: false at root, true in a folder
- `can_write`: false
- OpenItem/OpenFocused: if `lua_bookmark_id_for_row` is Some, dispatch `ActivateBookmark` instead of `OpenItem` command

```rust
if let Navigate { context, location } | Refresh { context, location } = &command {
    if location.synthetic_root().is_some() || location.favorites_folder_id().is_some() {
        // build entries:
        //   Favorites root / Home / QuickAccess via existing synthetic_root_entries
        //   favorites folder via favorites_entries(Some(id))
        // title + can_go_up as above
        return true;
    }
}
```

Change `synthetic_root_entries` match to include `Favorites => self.favorites_entries(None)`.

- [ ] **Step 1: Failing test** next to `synthetic_home_navigation_is_service_independent_and_bounded`:

```rust
#[test]
fn synthetic_favorites_navigation_lists_folders_then_bookmarks() {
    let mut root = ExplorerRoot::new(UiTokens::default());
    let mut bookmarks = explorer_model::Bookmarks::default();
    let _ = bookmarks.begin_add_folder("super".into(), None);
    let _ = bookmarks.begin_add(
        "portable".into(),
        explorer_model::BookmarkTarget::FolderPath { path: r"C:\portable".into() },
    );
    root.state.configure_bookmarks(bookmarks);
    let command = root
        .state
        .begin_active_navigation(
            explorer_model::LocationDescriptor::synthetic(
                explorer_model::SyntheticRoot::Favorites,
            ),
            false,
        )
        .expect("favorites navigation");
    assert!(root.submit_command(command));
    let tab = root.state.tabs().active_tab();
    assert_eq!(
        tab.history.current().map(|entry| entry.location.clone()),
        Some(explorer_model::LocationDescriptor::synthetic(
            explorer_model::SyntheticRoot::Favorites
        ))
    );
    let names: Vec<_> = tab
        .visible_snapshot()
        .expect("snapshot")
        .entries()
        .iter()
        .map(|entry| entry.display_name.as_str())
        .collect();
    assert_eq!(names, ["super", "portable"]);
}
```

`Bookmarks::begin_add` is public. `begin_add_folder` is public. `configure_bookmarks` is `pub(crate)` — `ExplorerRoot` tests in `lib.rs` can call it.

- [ ] **Step 2: Run fail**
- [ ] **Step 3: Implement submit_command + open intercept**
- [ ] **Step 4: Tests pass**

Run: `cargo test -p explorer-ui --lib synthetic_favorites_navigation -- --nocapture`

Also fix every `match` on `SyntheticRoot` that the compiler flags (`lib.rs` title match, session tests if any).

---

### Task 5: Verification

Run:

```
cargo test -p explorer-model --lib
cargo test -p explorer-ui --lib favorites_ left_nav_favorite_folder synthetic_favorites_navigation synthetic_home_navigation bookmark_folder_and_destination bookmark_activation_dismisses
cargo test -p explorer-i18n --test catalog_complete
```

User-perspective checklist (must all hold in code + tests):
1. Click left `我的最愛` row → file view is Favorites listing; no toolbar dropdown (`bookmark_folder_menu` stays None).
2. Click left `我的最愛` chevron → children hide/show; file view unchanged if already elsewhere.
3. Click left child folder `super` → file view lists that folder; toolbar dropdown stays closed.
4. Click left leaf bookmark → existing `ActivateBookmark` path.
5. Click toolbar `super ▾` → dropdown still opens.
6. Collapse parent → entire favorites tree children disappear; parent row remains.
7. Nested folder Up → Favorites root.
8. Lua bookmark in the virtual folder opens via `ActivateBookmark`, not the Shell OpenItem broker.

If any check fails, fix and re-run until it passes.
