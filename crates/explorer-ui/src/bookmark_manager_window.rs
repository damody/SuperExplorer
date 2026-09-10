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
    History,
    HistoryBucket(HistoryBucket),
    RunLog,
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(crate) enum HistoryBucket {
    Today,
    Yesterday,
    Last7Days,
    ThisMonth,
    Month { year: u16, month: u8 },
    Older,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum BookmarkManagerMenu {
    Manage,
    View,
    Transfer,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum BookmarkManagerSubmenu {
    Columns,
    Sort,
    Restore,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum BookmarkManagerSortColumn {
    None,
    Name,
    Tags,
    Location,
    LastVisited,
    VisitCount,
    DateAdded,
    DateModified,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct BookmarkManagerColumns {
    pub name: bool,
    pub tags: bool,
    pub location: bool,
    pub last_visited: bool,
    pub visit_count: bool,
    pub date_added: bool,
    pub date_modified: bool,
}

impl Default for BookmarkManagerColumns {
    fn default() -> Self {
        Self {
            name: true,
            tags: true,
            location: true,
            last_visited: false,
            visit_count: false,
            date_added: true,
            date_modified: false,
        }
    }
}

impl BookmarkManagerColumns {
    fn visible_count(self) -> usize {
        [
            self.name,
            self.tags,
            self.location,
            self.last_visited,
            self.visit_count,
            self.date_added,
            self.date_modified,
        ]
        .into_iter()
        .filter(|visible| *visible)
        .count()
    }

    fn toggle_flag(flag: &mut bool, visible_count: usize) {
        if *flag && visible_count == 1 {
            return;
        }
        *flag = !*flag;
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum BookmarkManagerUiAction {
    Back,
    Forward,
    Navigate(BookmarkManagerLocation),
    ToggleFolder(explorer_model::BookmarkFolderId),
    SelectBookmark(explorer_model::BookmarkId),
    SelectHistory(usize),
    SelectRunLog(usize),
    ToggleHistoryExpanded,
    ToggleMenu(BookmarkManagerMenu),
    OpenSubmenu(BookmarkManagerSubmenu),
    Sort(BookmarkManagerSortColumn),
    SetSortDescending(bool),
    ToggleColumnName,
    ToggleColumnTags,
    ToggleColumnLocation,
    ToggleColumnLastVisited,
    ToggleColumnVisitCount,
    ToggleColumnDateAdded,
    ToggleColumnDateModified,
    DismissMenu,
    SelectAll,
    CutSelection,
    CopySelection,
    PasteSelection,
}

pub(crate) type BookmarkManagerUiCallback =
    Rc<dyn Fn(&BookmarkManagerUiAction, &mut Window, &mut App)>;

#[derive(Clone, Debug)]
pub(crate) struct BookmarkManagerUiState {
    pub location: BookmarkManagerLocation,
    pub selected_bookmark: Option<explorer_model::BookmarkId>,
    pub selected_history: Option<usize>,
    pub selected_run_log: Option<usize>,
    pub selected_folder: Option<explorer_model::BookmarkFolderId>,
    pub expanded_folders: HashSet<explorer_model::BookmarkFolderId>,
    pub history_expanded: bool,
    pub history: Vec<BookmarkManagerLocation>,
    pub history_index: usize,
    pub open_menu: Option<BookmarkManagerMenu>,
    pub submenu: Option<BookmarkManagerSubmenu>,
    pub sort_column: BookmarkManagerSortColumn,
    pub descending: bool,
    pub columns: BookmarkManagerColumns,
    pub selected_ids: HashSet<explorer_model::BookmarkId>,
}

impl Default for BookmarkManagerUiState {
    fn default() -> Self {
        Self {
            location: BookmarkManagerLocation::AllBookmarks,
            selected_bookmark: None,
            selected_history: None,
            selected_run_log: None,
            selected_folder: None,
            expanded_folders: HashSet::new(),
            history_expanded: false,
            history: vec![BookmarkManagerLocation::AllBookmarks],
            history_index: 0,
            open_menu: None,
            submenu: None,
            sort_column: BookmarkManagerSortColumn::None,
            descending: false,
            columns: BookmarkManagerColumns::default(),
            selected_ids: HashSet::new(),
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
        self.selected_history = None;
        self.selected_run_log = None;
        self.selected_folder = match location {
            BookmarkManagerLocation::Folder(id) => Some(id),
            BookmarkManagerLocation::AllBookmarks
            | BookmarkManagerLocation::Root
            | BookmarkManagerLocation::History
            | BookmarkManagerLocation::HistoryBucket(_)
            | BookmarkManagerLocation::RunLog => None,
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
                self.selected_history = None;
                self.selected_run_log = None;
                self.selected_folder = None;
                self.selected_ids = HashSet::from([id]);
                self.open_menu = None;
                self.submenu = None;
            }
            BookmarkManagerUiAction::SelectHistory(index) => {
                self.selected_history = Some(index);
                self.selected_bookmark = None;
                self.selected_run_log = None;
                self.selected_folder = None;
                self.selected_ids.clear();
                self.open_menu = None;
                self.submenu = None;
            }
            BookmarkManagerUiAction::SelectRunLog(index) => {
                self.selected_run_log = Some(index);
                self.selected_bookmark = None;
                self.selected_history = None;
                self.selected_folder = None;
                self.selected_ids.clear();
                self.open_menu = None;
                self.submenu = None;
            }
            BookmarkManagerUiAction::ToggleHistoryExpanded => {
                self.history_expanded = !self.history_expanded;
            }
            BookmarkManagerUiAction::ToggleMenu(menu) => {
                if self.open_menu == Some(menu) {
                    self.open_menu = None;
                    self.submenu = None;
                } else {
                    self.open_menu = Some(menu);
                    self.submenu = match menu {
                        BookmarkManagerMenu::View => Some(BookmarkManagerSubmenu::Sort),
                        BookmarkManagerMenu::Transfer | BookmarkManagerMenu::Manage => None,
                    };
                }
            }
            BookmarkManagerUiAction::OpenSubmenu(menu) => {
                self.submenu = Some(menu);
            }
            BookmarkManagerUiAction::Sort(column) => {
                if column == BookmarkManagerSortColumn::None {
                    self.sort_column = BookmarkManagerSortColumn::None;
                    self.descending = false;
                } else if self.sort_column == column {
                    self.descending = !self.descending;
                } else {
                    self.sort_column = column;
                    self.descending = false;
                }
                self.open_menu = None;
                self.submenu = None;
            }
            BookmarkManagerUiAction::SetSortDescending(descending) => {
                if self.sort_column != BookmarkManagerSortColumn::None {
                    self.descending = descending;
                }
                self.open_menu = None;
                self.submenu = None;
            }
            BookmarkManagerUiAction::ToggleColumnName => {
                let visible = self.columns.visible_count();
                BookmarkManagerColumns::toggle_flag(&mut self.columns.name, visible);
            }
            BookmarkManagerUiAction::ToggleColumnTags => {
                let visible = self.columns.visible_count();
                BookmarkManagerColumns::toggle_flag(&mut self.columns.tags, visible);
            }
            BookmarkManagerUiAction::ToggleColumnLocation => {
                let visible = self.columns.visible_count();
                BookmarkManagerColumns::toggle_flag(&mut self.columns.location, visible);
            }
            BookmarkManagerUiAction::ToggleColumnLastVisited => {
                let visible = self.columns.visible_count();
                BookmarkManagerColumns::toggle_flag(&mut self.columns.last_visited, visible);
            }
            BookmarkManagerUiAction::ToggleColumnVisitCount => {
                let visible = self.columns.visible_count();
                BookmarkManagerColumns::toggle_flag(&mut self.columns.visit_count, visible);
            }
            BookmarkManagerUiAction::ToggleColumnDateAdded => {
                let visible = self.columns.visible_count();
                BookmarkManagerColumns::toggle_flag(&mut self.columns.date_added, visible);
            }
            BookmarkManagerUiAction::ToggleColumnDateModified => {
                let visible = self.columns.visible_count();
                BookmarkManagerColumns::toggle_flag(&mut self.columns.date_modified, visible);
            }
            BookmarkManagerUiAction::DismissMenu => {
                self.open_menu = None;
                self.submenu = None;
            }
            BookmarkManagerUiAction::SelectAll
            | BookmarkManagerUiAction::CutSelection
            | BookmarkManagerUiAction::CopySelection
            | BookmarkManagerUiAction::PasteSelection => {}
        }
    }
}

const SECONDS_PER_DAY: u64 = 86_400;

pub(crate) fn weekday_key(epoch_seconds: u64) -> &'static str {
    const KEYS: [&str; 7] = [
        "weekday-sun",
        "weekday-mon",
        "weekday-tue",
        "weekday-wed",
        "weekday-thu",
        "weekday-fri",
        "weekday-sat",
    ];
    KEYS[((epoch_seconds / SECONDS_PER_DAY + 4) % 7) as usize]
}

pub(crate) fn history_bucket_for(visit_epoch: u64, now_epoch: u64) -> HistoryBucket {
    let (now_year, now_month, now_day) = utc_ymd(now_epoch);
    let today_start = ymd_to_epoch(now_year, now_month, now_day);
    let yesterday_start = today_start.saturating_sub(SECONDS_PER_DAY);
    let last7_start = today_start.saturating_sub(7 * SECONDS_PER_DAY);
    let this_month_start = ymd_to_epoch(now_year, now_month, 1);
    let six_months_start = subtract_months(now_year, now_month, 6);
    if visit_epoch >= today_start {
        HistoryBucket::Today
    } else if visit_epoch >= yesterday_start {
        HistoryBucket::Yesterday
    } else if visit_epoch >= last7_start {
        HistoryBucket::Last7Days
    } else if visit_epoch >= this_month_start {
        HistoryBucket::ThisMonth
    } else {
        let (year, month, _) = utc_ymd(visit_epoch);
        if ymd_to_epoch(year, month, 1) >= six_months_start {
            HistoryBucket::Month {
                year: year as u16,
                month,
            }
        } else {
            HistoryBucket::Older
        }
    }
}

pub(crate) fn history_bucket_order(now_epoch: u64) -> Vec<HistoryBucket> {
    let (year, month, _) = utc_ymd(now_epoch);
    let mut buckets = vec![
        HistoryBucket::Today,
        HistoryBucket::Yesterday,
        HistoryBucket::Last7Days,
        HistoryBucket::ThisMonth,
    ];
    let mut y = year;
    let mut m = month;
    for _ in 0..5 {
        if m == 1 {
            m = 12;
            y -= 1;
        } else {
            m -= 1;
        }
        buckets.push(HistoryBucket::Month {
            year: y as u16,
            month: m,
        });
    }
    buckets.push(HistoryBucket::Older);
    buckets
}

fn utc_ymd(epoch_seconds: u64) -> (i32, u8, u8) {
    let mut day_index = epoch_seconds / SECONDS_PER_DAY;
    let mut year = 1970i32;
    loop {
        let leap = year % 4 == 0 && (year % 100 != 0 || year % 400 == 0);
        let days_in_year = if leap { 366 } else { 365 };
        if day_index < days_in_year {
            break;
        }
        day_index -= days_in_year;
        year += 1;
        if year > 9999 {
            return (1970, 1, 1);
        }
    }
    let leap = year % 4 == 0 && (year % 100 != 0 || year % 400 == 0);
    let month_days = [
        31u64,
        if leap { 29 } else { 28 },
        31,
        30,
        31,
        30,
        31,
        31,
        30,
        31,
        30,
        31,
    ];
    let mut month = 1u8;
    for days in month_days {
        if day_index < days {
            break;
        }
        day_index -= days;
        month += 1;
    }
    (year, month, (day_index + 1) as u8)
}

fn ymd_to_epoch(year: i32, month: u8, day: u8) -> u64 {
    let mut days = 0u64;
    for y in 1970..year {
        let leap = y % 4 == 0 && (y % 100 != 0 || y % 400 == 0);
        days += if leap { 366 } else { 365 };
    }
    let leap = year % 4 == 0 && (year % 100 != 0 || year % 400 == 0);
    let month_days = [
        31u64,
        if leap { 29 } else { 28 },
        31,
        30,
        31,
        30,
        31,
        31,
        30,
        31,
        30,
        31,
    ];
    for m in 1..month {
        days += month_days[(m - 1) as usize];
    }
    days += u64::from(day.saturating_sub(1));
    days * SECONDS_PER_DAY
}

fn subtract_months(year: i32, month: u8, count: u8) -> u64 {
    let mut y = year;
    let mut m = month as i32 - i32::from(count);
    while m <= 0 {
        m += 12;
        y -= 1;
    }
    ymd_to_epoch(y, m as u8, 1)
}

pub fn bookmark_manager_window_options(cx: &App, title: impl Into<SharedString>) -> WindowOptions {
    WindowOptions {
        window_bounds: Some(WindowBounds::Windowed(Bounds::centered(
            None,
            size(px(1100.0), px(720.0)),
            cx,
        ))),
        titlebar: Some(gpui::TitlebarOptions {
            title: Some(title.into()),
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
        let live_reorder = matches!(action, ExplorerAction::MoveBookmark { .. });
        match owner.update(cx, |root, owner_window, cx| {
            if matches!(action, ExplorerAction::EditBookmark { .. }) {
                root.clear_bookmark_editor_anchor();
            }
            root.dispatch_bookmark_manager_action(action, source, owner_window, cx);
            root.bookmark_manager_window_snapshot()
        }) {
            Ok(snapshot) => {
                let bookmarks_changed =
                    snapshot.state.bookmarks() != self.snapshot.state.bookmarks();
                self.ui.reconcile(snapshot.state.bookmarks());
                self.snapshot = snapshot;
                if bookmarks_changed || !live_reorder {
                    cx.notify();
                    window.refresh();
                }
            }
            Err(error) => {
                tracing::warn!(%error, "Bookmark manager owner window is unavailable");
                window.remove_window();
            }
        }
    }

    fn apply_clipboard_action(
        &mut self,
        action: BookmarkManagerUiAction,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let ids: Vec<_> = self.ui.selected_ids.iter().copied().collect();
        let parent = self.ui.selected_folder;
        let owner = self.owner;
        if let Ok(snapshot) = owner.update(cx, |root, _, _| {
            match action {
                BookmarkManagerUiAction::CopySelection => {
                    root.state.copy_bookmarks_to_clipboard(&ids);
                }
                BookmarkManagerUiAction::CutSelection => {
                    root.state.copy_bookmarks_to_clipboard(&ids);
                    root.state.push_bookmark_undo();
                    for id in &ids {
                        let _ = root.state.remove_bookmark(*id);
                    }
                    let _ = root.notify_durable_state();
                }
                BookmarkManagerUiAction::PasteSelection => {
                    let _ = root.state.paste_bookmarked_clipboard(parent);
                    let _ = root.notify_durable_state();
                }
                _ => {}
            }
            root.bookmark_manager_window_snapshot()
        }) {
            self.ui.reconcile(snapshot.state.bookmarks());
            self.snapshot = snapshot;
            cx.notify();
            window.refresh();
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
                if let BookmarkManagerUiAction::SelectRunLog(index) = action
                    && let Some(record) = this.snapshot.state.run_log().get(*index)
                {
                    this.detail_input = cx.new(|cx| {
                        EditableTextState::new(StringStorage::from(record.display_name.clone()), cx)
                    });
                    this.detail_location_input = cx.new(|cx| {
                        EditableTextState::new(
                            StringStorage::from(record.location.editable_text()),
                            cx,
                        )
                    });
                }
                if matches!(
                    action,
                    BookmarkManagerUiAction::CopySelection
                        | BookmarkManagerUiAction::CutSelection
                        | BookmarkManagerUiAction::PasteSelection
                ) {
                    this.apply_clipboard_action(*action, window, cx);
                }
                if *action == BookmarkManagerUiAction::SelectAll {
                    this.ui.selected_ids = this
                        .snapshot
                        .state
                        .bookmarks()
                        .entries()
                        .iter()
                        .map(|bookmark| bookmark.id)
                        .collect();
                }
                let previous_submenu = this.ui.submenu;
                this.ui.apply(*action);
                if matches!(action, BookmarkManagerUiAction::OpenSubmenu(_))
                    && this.ui.submenu == previous_submenu
                {
                    return;
                }
                cx.notify();
                window.refresh();
            },
        ));
        let search_query = self.search_input.read(cx).as_str().to_owned();
        let catalog = self.snapshot.state.catalog();
        window.set_window_title(&catalog.t("dialog-bookmark-library"));
        div()
            .id("bookmark-manager-window")
            .role(gpui::Role::Dialog)
            .aria_label(catalog.t("a11y-bookmark-manager-window"))
            .size_full()
            .track_focus(&self.focus_handle)
            .capture_key_down(cx.listener(|this, event: &gpui::KeyDownEvent, window, cx| {
                let key = event.keystroke.key.as_str();
                let modifiers = &event.keystroke.modifiers;
                if key == "escape" {
                    cx.stop_propagation();
                    window.remove_window();
                } else if key == "enter" {
                    cx.stop_propagation();
                    if matches!(this.ui.location, BookmarkManagerLocation::RunLog) {
                        if let Some(index) = this.ui.selected_run_log {
                            this.dispatch(
                                ExplorerAction::LaunchRunRecord { index },
                                ActionSource::Keyboard,
                                window,
                                cx,
                            );
                        }
                    } else {
                        this.commit_detail_name(window, cx);
                    }
                } else if modifiers.control && key == "z" {
                    cx.stop_propagation();
                    this.dispatch(
                        ExplorerAction::UndoBookmarkChange,
                        ActionSource::Keyboard,
                        window,
                        cx,
                    );
                } else if modifiers.control && key == "y" {
                    cx.stop_propagation();
                    this.dispatch(
                        ExplorerAction::RedoBookmarkChange,
                        ActionSource::Keyboard,
                        window,
                        cx,
                    );
                } else if modifiers.control && key == "w" {
                    cx.stop_propagation();
                    window.remove_window();
                } else if modifiers.control && key == "a" {
                    cx.stop_propagation();
                    this.ui.apply(BookmarkManagerUiAction::SelectAll);
                    this.ui.selected_ids = this
                        .snapshot
                        .state
                        .bookmarks()
                        .entries()
                        .iter()
                        .map(|bookmark| bookmark.id)
                        .collect();
                    cx.notify();
                    window.refresh();
                } else if modifiers.control && key == "x" {
                    cx.stop_propagation();
                    this.apply_clipboard_action(BookmarkManagerUiAction::CutSelection, window, cx);
                } else if modifiers.control && key == "c" {
                    cx.stop_propagation();
                    this.apply_clipboard_action(BookmarkManagerUiAction::CopySelection, window, cx);
                } else if modifiers.control && key == "v" {
                    cx.stop_propagation();
                    this.apply_clipboard_action(
                        BookmarkManagerUiAction::PasteSelection,
                        window,
                        cx,
                    );
                } else if key == "delete" {
                    cx.stop_propagation();
                    if let Some(id) = this.ui.selected_bookmark {
                        this.dispatch(
                            ExplorerAction::RequestRemoveBookmark { id },
                            ActionSource::Keyboard,
                            window,
                            cx,
                        );
                    } else if let Some(id) = this.ui.selected_folder {
                        this.dispatch(
                            ExplorerAction::RemoveBookmarkFolder { id },
                            ActionSource::Keyboard,
                            window,
                            cx,
                        );
                    }
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
        BookmarkManagerSubmenu, BookmarkManagerUiAction, BookmarkManagerUiState, HistoryBucket,
        SECONDS_PER_DAY, history_bucket_for, history_bucket_order, weekday_key, ymd_to_epoch,
    };

    #[test]
    fn manager_uses_a_normal_dedicated_window_and_shared_reducer() {
        let source = include_str!("bookmark_manager_window.rs");
        assert!(source.contains("WindowKind::Normal"));
        assert!(source.contains("dispatch_bookmark_manager_action"));
        assert!(source.contains("live_reorder"));
        assert!(source.contains("bookmarks_changed"));
        assert!(source.contains("chrome::bookmark_manager("));
        assert!(source.contains("window.remove_window()"));
        assert!(source.contains("size(px(1100.0), px(720.0))"));
        assert!(source.contains("catalog.t(\"dialog-bookmark-library\")"));
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
    fn manager_ui_actions_toggle_menus_sort_and_selection() {
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
        state.apply(BookmarkManagerUiAction::DismissMenu);
        assert_eq!(state.open_menu, None);
        state.apply(BookmarkManagerUiAction::ToggleMenu(
            BookmarkManagerMenu::View,
        ));
        assert_eq!(state.open_menu, Some(BookmarkManagerMenu::View));
        assert_eq!(
            state.submenu,
            Some(BookmarkManagerSubmenu::Sort),
            "View must open the sort flyout so the full library sort list is visible"
        );
        assert!(state.columns.date_added, "Date column is on by default");
        state.apply(BookmarkManagerUiAction::ToggleColumnDateAdded);
        assert!(!state.columns.date_added);
        assert_eq!(
            state.open_menu,
            Some(BookmarkManagerMenu::View),
            "toggling a column must keep the View menu open"
        );
        state.apply(BookmarkManagerUiAction::ToggleColumnLastVisited);
        assert!(state.columns.last_visited);
        state.apply(BookmarkManagerUiAction::ToggleColumnName);
        assert!(!state.columns.name);
        state.apply(BookmarkManagerUiAction::Navigate(
            BookmarkManagerLocation::History,
        ));
        assert_eq!(state.location, BookmarkManagerLocation::History);
        state.apply(BookmarkManagerUiAction::ToggleHistoryExpanded);
        assert!(state.history_expanded);
        state.apply(BookmarkManagerUiAction::Navigate(
            BookmarkManagerLocation::RunLog,
        ));
        assert_eq!(state.location, BookmarkManagerLocation::RunLog);
        state.apply(BookmarkManagerUiAction::SelectRunLog(3));
        assert_eq!(state.selected_run_log, Some(3));
        let remaining = [
            state.columns.name,
            state.columns.tags,
            state.columns.location,
            state.columns.last_visited,
            state.columns.visit_count,
            state.columns.date_added,
            state.columns.date_modified,
        ]
        .into_iter()
        .filter(|visible| *visible)
        .count();
        assert!(remaining >= 1);
    }

    #[test]
    fn history_buckets_group_today_yesterday_and_older() {
        let today = ymd_to_epoch(2026, 9, 9);
        let noon = today + 12 * 3600;
        assert_eq!(history_bucket_for(noon, noon), HistoryBucket::Today);
        assert_eq!(
            history_bucket_for(today - SECONDS_PER_DAY, noon),
            HistoryBucket::Yesterday
        );
        assert_eq!(
            history_bucket_for(today - 3 * SECONDS_PER_DAY, noon),
            HistoryBucket::Last7Days
        );
        assert_eq!(
            history_bucket_for(ymd_to_epoch(2026, 9, 1), noon),
            HistoryBucket::ThisMonth
        );
        assert_eq!(
            history_bucket_for(ymd_to_epoch(2026, 4, 15), noon),
            HistoryBucket::Month {
                year: 2026,
                month: 4
            }
        );
        assert_eq!(
            history_bucket_for(ymd_to_epoch(2025, 1, 1), noon),
            HistoryBucket::Older
        );
        assert!(
            history_bucket_order(noon)
                .iter()
                .any(|bucket| *bucket == HistoryBucket::Yesterday)
        );
    }

    #[test]
    fn weekday_key_maps_unix_epoch_thursday() {
        assert_eq!(weekday_key(0), "weekday-thu");
        assert_eq!(weekday_key(SECONDS_PER_DAY), "weekday-fri");
        assert_eq!(weekday_key(3 * SECONDS_PER_DAY), "weekday-sun");
    }
}
