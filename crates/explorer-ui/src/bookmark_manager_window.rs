//! Dedicated interactive bookmark manager window.

use std::{collections::HashSet, rc::Rc};

use crate::{
    ExplorerRoot, UiTokens,
    actions::{ActionSource, ExplorerAction},
    chrome::{self, ActionCallback},
    state::AppViewState,
};
use gpui::{
    App, Bounds, Context, FocusHandle, Focusable, IntoElement, Render, SharedString, Window,
    WindowBounds, WindowHandle, WindowOptions, div, prelude::*, px, size,
};
use gpui_elements::editable_text::{EditableTextState, StringStorage};

#[derive(Clone)]
pub struct BookmarkManagerWindowSnapshotV1 {
    pub state: AppViewState,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum BookmarkManagerLocation {
    AllBookmarks,
    Root,
    Folder(explorer_model::BookmarkFolderId),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum BookmarkManagerMenu {
    Manage,
    View,
    Transfer,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum BookmarkManagerSortColumn {
    Name,
    Tags,
    Location,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum BookmarkManagerUiAction {
    Back,
    Forward,
    Navigate(BookmarkManagerLocation),
    ToggleFolder(explorer_model::BookmarkFolderId),
    SelectBookmark(explorer_model::BookmarkId),
    ToggleMenu(BookmarkManagerMenu),
    Sort(BookmarkManagerSortColumn),
    ToggleDensity,
    DismissMenu,
}

pub(crate) type BookmarkManagerUiCallback =
    Rc<dyn Fn(&BookmarkManagerUiAction, &mut Window, &mut App)>;

#[derive(Clone, Debug)]
pub(crate) struct BookmarkManagerUiState {
    pub location: BookmarkManagerLocation,
    pub selected_bookmark: Option<explorer_model::BookmarkId>,
    pub selected_folder: Option<explorer_model::BookmarkFolderId>,
    pub expanded_folders: HashSet<explorer_model::BookmarkFolderId>,
    pub history: Vec<BookmarkManagerLocation>,
    pub history_index: usize,
    pub open_menu: Option<BookmarkManagerMenu>,
    pub sort_column: BookmarkManagerSortColumn,
    pub descending: bool,
    pub compact: bool,
}

impl Default for BookmarkManagerUiState {
    fn default() -> Self {
        Self {
            location: BookmarkManagerLocation::AllBookmarks,
            selected_bookmark: None,
            selected_folder: None,
            expanded_folders: HashSet::new(),
            history: vec![BookmarkManagerLocation::AllBookmarks],
            history_index: 0,
            open_menu: None,
            sort_column: BookmarkManagerSortColumn::Name,
            descending: false,
            compact: false,
        }
    }
}

impl BookmarkManagerUiState {
    fn reconcile(&mut self, bookmarks: &explorer_model::Bookmarks) {
        if self
            .selected_bookmark
            .is_some_and(|id| !bookmarks.entries().iter().any(|entry| entry.id == id))
        {
            self.selected_bookmark = None;
        }
        if self
            .selected_folder
            .is_some_and(|id| bookmarks.folder(id).is_none())
        {
            self.selected_folder = None;
        }
        if matches!(self.location, BookmarkManagerLocation::Folder(id) if bookmarks.folder(id).is_none())
        {
            self.navigate(BookmarkManagerLocation::AllBookmarks);
        }
        self.expanded_folders
            .retain(|id| bookmarks.folder(*id).is_some());
    }

    pub fn navigate(&mut self, location: BookmarkManagerLocation) {
        if self.location == location {
            return;
        }
        self.history.truncate(self.history_index + 1);
        self.history.push(location);
        self.history_index = self.history.len() - 1;
        self.location = location;
        self.selected_bookmark = None;
        self.selected_folder = match location {
            BookmarkManagerLocation::Folder(id) => Some(id),
            BookmarkManagerLocation::AllBookmarks | BookmarkManagerLocation::Root => None,
        };
    }

    pub fn go_back(&mut self) -> bool {
        if self.history_index == 0 {
            return false;
        }
        self.history_index -= 1;
        self.location = self.history[self.history_index];
        true
    }

    pub fn go_forward(&mut self) -> bool {
        if self.history_index + 1 >= self.history.len() {
            return false;
        }
        self.history_index += 1;
        self.location = self.history[self.history_index];
        true
    }

    fn apply(&mut self, action: BookmarkManagerUiAction) {
        match action {
            BookmarkManagerUiAction::Back => {
                self.go_back();
            }
            BookmarkManagerUiAction::Forward => {
                self.go_forward();
            }
            BookmarkManagerUiAction::Navigate(location) => self.navigate(location),
            BookmarkManagerUiAction::ToggleFolder(id) => {
                if !self.expanded_folders.remove(&id) {
                    self.expanded_folders.insert(id);
                }
            }
            BookmarkManagerUiAction::SelectBookmark(id) => {
                self.selected_bookmark = Some(id);
                self.selected_folder = None;
                self.open_menu = None;
            }
            BookmarkManagerUiAction::ToggleMenu(menu) => {
                self.open_menu = (self.open_menu != Some(menu)).then_some(menu);
            }
            BookmarkManagerUiAction::Sort(column) => {
                if self.sort_column == column {
                    self.descending = !self.descending;
                } else {
                    self.sort_column = column;
                    self.descending = false;
                }
            }
            BookmarkManagerUiAction::ToggleDensity => self.compact = !self.compact,
            BookmarkManagerUiAction::DismissMenu => self.open_menu = None,
        }
    }
}

pub fn bookmark_manager_window_options(cx: &App) -> WindowOptions {
    WindowOptions {
        window_bounds: Some(WindowBounds::Windowed(Bounds::centered(
            None,
            size(px(1100.0), px(720.0)),
            cx,
        ))),
        titlebar: Some(gpui::TitlebarOptions {
            title: Some(SharedString::from("收藏庫")),
            ..Default::default()
        }),
        kind: gpui::WindowKind::Normal,
        is_resizable: true,
        window_min_size: Some(size(px(760.0), px(520.0))),
        ..Default::default()
    }
}

pub struct BookmarkManagerWindow {
    tokens: UiTokens,
    owner: WindowHandle<ExplorerRoot>,
    snapshot: BookmarkManagerWindowSnapshotV1,
    search_input: gpui::Entity<EditableTextState>,
    detail_input: gpui::Entity<EditableTextState>,
    detail_location_input: gpui::Entity<EditableTextState>,
    ui: BookmarkManagerUiState,
    focus_handle: FocusHandle,
    was_active: bool,
}

impl BookmarkManagerWindow {
    fn commit_detail_name(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(id) = self.ui.selected_bookmark else {
            return;
        };
        let name = self.detail_input.read(cx).as_str().trim().to_owned();
        let payload = self
            .detail_location_input
            .read(cx)
            .as_str()
            .trim()
            .to_owned();
        if name.is_empty() {
            return;
        }
        let owner = self.owner;
        if let Ok(snapshot) = owner.update(cx, |root, _, _| {
            root.update_bookmark_from_manager(id, name, payload);
            root.bookmark_manager_window_snapshot()
        }) {
            self.ui.reconcile(snapshot.state.bookmarks());
            self.snapshot = snapshot;
            cx.notify();
            window.refresh();
        }
    }

    pub fn new(
        tokens: UiTokens,
        owner: WindowHandle<ExplorerRoot>,
        snapshot: BookmarkManagerWindowSnapshotV1,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let owner_for_close = owner;
        window.on_window_should_close(cx, move |_, _| {
            let _ = owner_for_close;
            true
        });
        let search_input =
            cx.new(|cx| EditableTextState::new(StringStorage::from(String::new()), cx));
        let detail_input =
            cx.new(|cx| EditableTextState::new(StringStorage::from(String::new()), cx));
        let detail_location_input =
            cx.new(|cx| EditableTextState::new(StringStorage::from(String::new()), cx));
        cx.observe(&search_input, |_, _, cx| cx.notify()).detach();
        cx.observe(&detail_input, |_, _, cx| cx.notify()).detach();
        cx.observe(&detail_location_input, |_, _, cx| cx.notify())
            .detach();
        Self {
            tokens,
            owner,
            snapshot,
            search_input,
            detail_input,
            detail_location_input,
            ui: BookmarkManagerUiState::default(),
            focus_handle: cx.focus_handle(),
            was_active: false,
        }
    }

    pub fn replace_snapshot(
        &mut self,
        snapshot: BookmarkManagerWindowSnapshotV1,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.ui.reconcile(snapshot.state.bookmarks());
        self.snapshot = snapshot;
        cx.notify();
        window.refresh();
    }

    fn dispatch(
        &mut self,
        action: ExplorerAction,
        source: ActionSource,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.ui.open_menu = None;
        if action == ExplorerAction::ToggleBookmarkManager {
            window.remove_window();
            return;
        }
        let owner = self.owner;
        match owner.update(cx, |root, owner_window, cx| {
            if matches!(action, ExplorerAction::EditBookmark { .. }) {
                root.clear_bookmark_editor_anchor();
            }
            root.dispatch_bookmark_manager_action(action, source, owner_window, cx);
            root.bookmark_manager_window_snapshot()
        }) {
            Ok(snapshot) => {
                self.ui.reconcile(snapshot.state.bookmarks());
                self.snapshot = snapshot;
                cx.notify();
                window.refresh();
            }
            Err(error) => {
                tracing::warn!(%error, "Bookmark manager owner window is unavailable");
                window.remove_window();
            }
        }
    }
}

impl Focusable for BookmarkManagerWindow {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Render for BookmarkManagerWindow {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let active = window.is_window_active();
        if active && !self.was_active {
            if let Ok(snapshot) = self
                .owner
                .update(cx, |root, _, _| root.bookmark_manager_window_snapshot())
            {
                self.ui.reconcile(snapshot.state.bookmarks());
                self.snapshot = snapshot;
            }
        }
        self.was_active = active;
        let on_action: ActionCallback =
            Rc::new(cx.listener(|this, action: &ExplorerAction, window, cx| {
                this.dispatch(action.clone(), ActionSource::Mouse, window, cx);
            }));
        let on_ui_action: BookmarkManagerUiCallback = Rc::new(cx.listener(
            |this, action: &BookmarkManagerUiAction, window, cx| {
                if let BookmarkManagerUiAction::SelectBookmark(id) = action
                    && let Some(bookmark) = this
                        .snapshot
                        .state
                        .bookmarks()
                        .entries()
                        .iter()
                        .find(|bookmark| bookmark.id == *id)
                {
                    this.detail_input = cx.new(|cx| {
                        EditableTextState::new(StringStorage::from(bookmark.name.clone()), cx)
                    });
                    this.detail_location_input = cx.new(|cx| {
                        EditableTextState::new(
                            StringStorage::from(bookmark.target.editable_payload()),
                            cx,
                        )
                    });
                }
                this.ui.apply(*action);
                cx.notify();
                window.refresh();
            },
        ));
        let search_query = self.search_input.read(cx).as_str().to_owned();
        div()
            .id("bookmark-manager-window")
            .role(gpui::Role::Dialog)
            .aria_label("Bookmark manager window")
            .size_full()
            .track_focus(&self.focus_handle)
            .capture_key_down(cx.listener(|this, event: &gpui::KeyDownEvent, window, cx| {
                if event.keystroke.key == "escape" {
                    cx.stop_propagation();
                    window.remove_window();
                } else if event.keystroke.key == "enter" {
                    cx.stop_propagation();
                    this.commit_detail_name(window, cx);
                }
            }))
            .child(chrome::bookmark_manager(
                self.tokens,
                &self.snapshot.state,
                &self.ui,
                Some(gpui::Entity::downgrade(&self.search_input)),
                Some(gpui::Entity::downgrade(&self.detail_input)),
                Some(gpui::Entity::downgrade(&self.detail_location_input)),
                &search_query,
                Some(on_action),
                Some(on_ui_action),
            ))
    }
}

#[cfg(test)]
mod tests {
    use super::{
        BookmarkManagerLocation, BookmarkManagerMenu, BookmarkManagerSortColumn,
        BookmarkManagerUiAction, BookmarkManagerUiState,
    };

    #[test]
    fn manager_uses_a_normal_dedicated_window_and_shared_reducer() {
        let source = include_str!("bookmark_manager_window.rs");
        assert!(source.contains("WindowKind::Normal"));
        assert!(source.contains("dispatch_bookmark_manager_action"));
        assert!(source.contains("chrome::bookmark_manager("));
        assert!(source.contains("window.remove_window()"));
        assert!(source.contains("size(px(1100.0), px(720.0))"));
        assert!(source.contains("SharedString::from(\"收藏庫\")"));
        assert!(source.contains("search_input"));
        assert!(source.contains("root.clear_bookmark_editor_anchor()"));
    }

    #[test]
    fn manager_history_truncates_forward_branch_and_has_bounded_navigation() {
        let mut state = BookmarkManagerUiState::default();
        state.navigate(BookmarkManagerLocation::Root);
        assert!(state.go_back());
        assert!(!state.go_back());
        assert!(state.go_forward());
        assert!(!state.go_forward());
        assert!(state.go_back());
        state.navigate(BookmarkManagerLocation::Root);
        assert!(!state.go_forward());
        assert_eq!(state.history.len(), 2);
    }

    #[test]
    fn manager_reconciles_deleted_selection_and_location() {
        let mut bookmarks = explorer_model::Bookmarks::default();
        assert!(bookmarks.begin_add_folder("Folder".into(), None).changed());
        let folder_id = bookmarks.folders()[0].id;
        let mut state = BookmarkManagerUiState::default();
        state.navigate(BookmarkManagerLocation::Folder(folder_id));
        state.expanded_folders.insert(folder_id);

        state.reconcile(&explorer_model::Bookmarks::default());

        assert_eq!(state.location, BookmarkManagerLocation::AllBookmarks);
        assert_eq!(state.selected_folder, None);
        assert!(state.expanded_folders.is_empty());
    }

    #[test]
    fn manager_ui_actions_toggle_menus_sort_density_and_selection() {
        let mut state = BookmarkManagerUiState::default();
        state.apply(BookmarkManagerUiAction::ToggleMenu(
            BookmarkManagerMenu::Manage,
        ));
        assert_eq!(state.open_menu, Some(BookmarkManagerMenu::Manage));
        state.apply(BookmarkManagerUiAction::Sort(
            BookmarkManagerSortColumn::Location,
        ));
        assert_eq!(state.sort_column, BookmarkManagerSortColumn::Location);
        assert!(!state.descending);
        state.apply(BookmarkManagerUiAction::Sort(
            BookmarkManagerSortColumn::Location,
        ));
        assert!(state.descending);
        state.apply(BookmarkManagerUiAction::ToggleDensity);
        assert!(state.compact);
        state.apply(BookmarkManagerUiAction::DismissMenu);
        assert_eq!(state.open_menu, None);
    }
}
