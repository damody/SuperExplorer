//! Typed M1 actions, key bindings, dispatch, and privacy-safe tracing.

use std::time::Instant;

use crate::{
    focus::FocusSurface,
    layout::{LayoutTokens, LogicalPx},
    state::{AppViewState, CommandKind},
    theme::ThemeMode,
};

gpui::actions!(
    explorer,
    [
        NavigateBack,
        NavigateForward,
        NavigateUp,
        RefreshExplorer,
        RenameFocusedItem,
        FocusAddressBar,
        FocusSearchBox,
        FocusNextSurface,
        FocusPreviousSurface,
        SubmitFocusedInput,
        CancelFocusedInput,
        CancelScrollbarDrag,
        NewExplorerTab,
        CloseExplorerTab,
        NextExplorerTab,
        PreviousExplorerTab,
        ToggleExplorerTheme,
        ToggleExplorerPreview,
        CloseExplorerWindow,
        ShrinkNavigationPane,
        GrowNavigationPane,
        ResetNavigationPane,
        ShrinkSidePane,
        GrowSidePane
    ]
);

/// GPUI key bindings are registered once at the application boundary and
/// translated back into the same domain dispatcher used by pointer controls.
pub fn gpui_key_bindings() -> Vec<gpui::KeyBinding> {
    vec![
        gpui::KeyBinding::new("alt-left", NavigateBack, None),
        gpui::KeyBinding::new("alt-right", NavigateForward, None),
        gpui::KeyBinding::new("alt-up", NavigateUp, None),
        gpui::KeyBinding::new("f5", RefreshExplorer, None),
        gpui::KeyBinding::new("f2", RenameFocusedItem, None),
        gpui::KeyBinding::new("ctrl-l", FocusAddressBar, None),
        gpui::KeyBinding::new("alt-d", FocusAddressBar, None),
        gpui::KeyBinding::new("ctrl-f", FocusSearchBox, None),
        gpui::KeyBinding::new("tab", FocusNextSurface, None),
        gpui::KeyBinding::new("shift-tab", FocusPreviousSurface, None),
        gpui::KeyBinding::new("tab", FocusNextSurface, Some("EditableText")),
        gpui::KeyBinding::new("shift-tab", FocusPreviousSurface, Some("EditableText")),
        gpui::KeyBinding::new("enter", SubmitFocusedInput, Some("EditableText")),
        gpui::KeyBinding::new("escape", CancelFocusedInput, Some("EditableText")),
        gpui::KeyBinding::new("escape", CancelScrollbarDrag, None),
        gpui::KeyBinding::new("ctrl-t", NewExplorerTab, None),
        gpui::KeyBinding::new("ctrl-w", CloseExplorerTab, None),
        gpui::KeyBinding::new("ctrl-tab", NextExplorerTab, None),
        gpui::KeyBinding::new("ctrl-shift-tab", PreviousExplorerTab, None),
        gpui::KeyBinding::new("ctrl-shift-d", ToggleExplorerTheme, None),
        gpui::KeyBinding::new("alt-p", ToggleExplorerPreview, None),
        gpui::KeyBinding::new("alt-f4", CloseExplorerWindow, None),
        gpui::KeyBinding::new("ctrl-alt-left", ShrinkNavigationPane, None),
        gpui::KeyBinding::new("ctrl-alt-right", GrowNavigationPane, None),
        gpui::KeyBinding::new("ctrl-alt-home", ResetNavigationPane, None),
        gpui::KeyBinding::new("ctrl-alt-shift-left", GrowSidePane, None),
        gpui::KeyBinding::new("ctrl-alt-shift-right", ShrinkSidePane, None),
    ]
}

pub fn gpui_text_input_bindings() -> Vec<gpui::KeyBinding> {
    gpui_elements::editable_text::actions::default_bindings()
        .as_keybindings(Some(
            gpui_elements::editable_text::actions::DEFAULT_INPUT_CONTEXT,
        ))
        // Single-line Explorer editors own these chords at the window dispatcher. Keeping the
        // generic EditableText Enter/Escape/Tab actions would consume them at the child element
        // before address, search, rename, or focus traversal can observe the command.
        .filter(|binding| {
            !binding
                .keystrokes()
                .iter()
                .any(|keystroke| matches!(keystroke.key(), "enter" | "escape" | "tab"))
        })
        .collect()
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FolderOptionsPage {
    General,
    View,
    Extensions,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum NavigationHistoryDirection {
    Back,
    Forward,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BookmarkPathKind {
    Folder,
    File,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PermanentDeleteDialogTarget {
    Cancel,
    Delete,
}

#[derive(Clone, Debug, PartialEq)]
pub enum ExplorerAction {
    Back,
    Forward,
    OpenNavigationHistory {
        direction: NavigationHistoryDirection,
    },
    CloseNavigationHistory,
    MoveNavigationHistoryFocus {
        direction: i8,
    },
    SetNavigationHistoryFocus {
        index: usize,
    },
    ActivateNavigationHistory {
        direction: NavigationHistoryDirection,
        steps: usize,
    },
    Up,
    Refresh,
    FocusAddress,
    EnterAddressEdit,
    UpdateAddressDraft(String),
    SubmitAddress(String),
    CancelAddressEdit,
    ActivateBreadcrumbSegment {
        location: explorer_model::LocationDescriptor,
    },
    OpenBreadcrumbChildren {
        segment_id: explorer_model::BreadcrumbSegmentId,
    },
    RetryBreadcrumbChildren {
        segment_id: explorer_model::BreadcrumbSegmentId,
    },
    ToggleBreadcrumbOverflow,
    CloseBreadcrumbMenu,
    MoveBreadcrumbSegmentFocus {
        direction: i8,
    },
    MoveBreadcrumbMenuFocus {
        movement: explorer_model::MenuFocusMovement,
    },
    SetBreadcrumbMenuFocus {
        index: usize,
    },
    TypeAheadBreadcrumbMenu {
        text: String,
    },
    ActivateBreadcrumbChild {
        location: explorer_model::LocationDescriptor,
    },
    ActivateNavigationItem {
        location: explorer_model::LocationDescriptor,
    },
    ToggleNavigationNode {
        location: explorer_model::LocationDescriptor,
    },
    FocusSearch,
    ClearSearch,
    FocusNext,
    FocusPrevious,
    SubmitFocusedInput,
    CancelFocusedInput,
    RestorePreviousFocus,
    NewTab,
    CloseActiveTab,
    ActivateTab {
        tab_id: explorer_model::TabId,
    },
    CloseTab {
        tab_id: explorer_model::TabId,
    },
    ReorderTab {
        tab_id: explorer_model::TabId,
        destination_index: usize,
    },
    NextTab,
    PreviousTab,
    OpenItem {
        row_index: usize,
        new_tab: bool,
    },
    OpenExtensionViewItem {
        item_id: explorer_model::ShellItemId,
        location: explorer_model::LocationDescriptor,
        is_container: bool,
        new_tab: bool,
    },
    OpenFocused,
    SelectItem {
        row_index: usize,
    },
    SelectAdditionalItem {
        row_index: usize,
    },
    SelectRange {
        row_index: usize,
        additive: bool,
    },
    FocusItem {
        row_index: usize,
    },
    SelectAllItems,
    InvertSelection,
    ClearSelection,
    TypeAheadFileView {
        text: String,
    },
    ClearFileViewTypeAhead,
    BeginRenameFocused,
    CommitInlineRename,
    CancelInlineRename,
    RequestPermanentDelete,
    ConfirmPermanentDelete,
    CancelPermanentDelete,
    MovePermanentDeleteDialogFocus {
        direction: i8,
    },
    SetPermanentDeleteDialogFocus {
        target: PermanentDeleteDialogTarget,
    },
    CloseLockOwnersAndRetry,
    RetryLockedDelete,
    CancelLockedDeleteRecovery,
    MoveLockedDeleteDialogFocus {
        direction: i8,
    },
    BeginMarquee {
        x: f32,
        y: f32,
        additive: bool,
    },
    UpdateMarquee {
        x: f32,
        y: f32,
        scroll_y: f32,
        viewport_width: f32,
    },
    EndMarquee,
    CreateFolder,
    CreateRemoteSymlink,
    CreateRemoteSymlinkToFolder {
        row_index: usize,
    },
    ShowRemoteBackgroundProperties,
    ToggleNewMenu,
    CloseNewMenu,
    MoveNewMenuFocus {
        direction: i8,
    },
    CreateNewItem {
        index: usize,
    },
    RecycleDeleteSelected,
    CreateShortcutSelected,
    CopySelected,
    CutSelected,
    Paste,
    DownloadSelectedToDownloads,
    ShareSelected,
    PinSelectedToStart,
    ShowPropertiesSelected,
    CloseRemoteProperties,
    ToggleRemotePermission {
        mask: u32,
    },
    ApplyRemoteProperties,
    RestoreSelected,
    EmptyRecycleBin,
    UndoCurrentFolder,
    CompressSelectedToZip,
    AddSelectedToFavorites,
    AddSelectedToBookmarks,
    ToggleCurrentFolderBookmark {
        screen_x: i32,
        screen_y: i32,
    },
    ActivateBookmark {
        id: explorer_model::BookmarkId,
    },
    OpenBookmarkInNewTab {
        id: explorer_model::BookmarkId,
    },
    OpenBookmarkContextMenu {
        id: explorer_model::BookmarkId,
        x: f32,
        y: f32,
    },
    CloseBookmarkContextMenu,
    RequestRemoveBookmark {
        id: explorer_model::BookmarkId,
    },
    OpenBookmarkToolbarContextMenu {
        parent_id: Option<explorer_model::BookmarkFolderId>,
        x: f32,
        y: f32,
    },
    CloseBookmarkToolbarContextMenu,
    AddPathBookmark {
        parent_id: Option<explorer_model::BookmarkFolderId>,
        kind: BookmarkPathKind,
    },
    CloseRemoteContextMenu,
    AddLuaBookmark,
    EditBookmark {
        id: explorer_model::BookmarkId,
    },
    SaveBookmarkEditor,
    CancelBookmarkEditor,
    SelectBookmarkDestination {
        parent_id: Option<explorer_model::BookmarkFolderId>,
    },
    AddBookmarkFolder {
        parent_id: Option<explorer_model::BookmarkFolderId>,
    },
    EditBookmarkFolder {
        id: explorer_model::BookmarkFolderId,
    },
    SaveBookmarkFolderEditor,
    CancelBookmarkFolderEditor,
    RemoveBookmarkFolder {
        id: explorer_model::BookmarkFolderId,
    },
    ConfirmRemoveBookmarkFolder,
    CancelRemoveBookmarkFolder,
    RemoveEditingBookmark,
    ToggleBookmarkManager,
    ImportBookmarksFromClipboard,
    BackupBookmarksToClipboard,
    ToggleBookmarkOverflow,
    ToggleBookmarkFolderMenu {
        id: explorer_model::BookmarkFolderId,
    },
    RemoveBookmark {
        id: explorer_model::BookmarkId,
    },
    MoveBookmark {
        id: explorer_model::BookmarkId,
        destination: usize,
    },
    MoveBookmarkToFolder {
        id: explorer_model::BookmarkId,
        parent_id: Option<explorer_model::BookmarkFolderId>,
    },
    CopySelectedPaths,
    OpenAboutDialog,
    CloseAboutDialog,
    OpenFolderOptions,
    CloseFolderOptions,
    SetFolderOptionsPage(FolderOptionsPage),
    ToggleFolderOptionExtension {
        index: usize,
    },
    OpenExtensionAuthorWebsite {
        index: usize,
    },
    OpenExtensionCommunityWebsite {
        index: usize,
    },
    InvokeExtensionCommand {
        contribution_id: String,
    },
    CloseExtensionCommandPanel,
    RunBulkFolderPreset {
        count: u32,
    },
    RunExifRenamePreset {
        preset: crate::extension_commands::ExifRenamePreset,
    },
    ToggleFolderOptionItemCheckBoxes,
    ToggleFolderOptionFileNameExtensions,
    ToggleFolderOptionHiddenItems,
    ToggleFolderOptionCompactView,
    ToggleFolderOptionAlwaysShowIcons,
    SetFolderOptionIconCacheMemoryMb(u16),
    SetFolderOptionThumbnailCacheMemoryMb(u16),
    SetFolderOptionMftCacheMemoryMb(u16),
    SetFolderOptionCacheBudgets(explorer_model::CacheBudgetSettingsV1),
    ClearThumbnailCache,
    ToggleFolderOptionDetailsPane,
    ToggleFolderOptionPreviewPane,
    ToggleRestorePreviousSession,
    ResetSavedSession,
    ResetSavedViewSettings,
    ResetAllSavedExplorerState,
    ConfirmSavedStateReset,
    CancelSavedStateReset,
    RetrySavedStateReset,
    RetryExtensionBroker,
    ResetFolderOptions,
    ApplyFolderOptions,
    ConfirmFolderOptions,
    BeginFileDrag {
        x: f32,
        y: f32,
        button: explorer_model::DragButton,
    },
    BeginContextItemGesture {
        item_id: explorer_model::ShellItemId,
        x: f32,
        y: f32,
        extended_verbs: bool,
    },
    UpdateFileDrag {
        x: f32,
        y: f32,
    },
    CancelFileDrag,
    DropExternal {
        paths: Vec<std::path::PathBuf>,
        destination_row: Option<usize>,
        effect: explorer_model::DragEffect,
        right_button: bool,
        allowed: explorer_model::TransferEffects,
    },
    UpdateExternalDrag {
        destination_row: Option<usize>,
        target: explorer_model::DropTargetKind,
        pointer_y: f32,
        top: f32,
        bottom: f32,
        effect: explorer_model::DragEffect,
    },
    ClearExternalDrag,
    ResolveRightDrop {
        effect: explorer_model::DragEffect,
    },
    ShowContextMenu {
        item_id: Option<explorer_model::ShellItemId>,
        owner_window: u64,
        x: i32,
        y: i32,
        client_x: f32,
        client_y: f32,
        keyboard_invoked: bool,
        extended_verbs: bool,
    },
    CancelOperation {
        request_id: explorer_common::RequestId,
    },
    ToggleSortMenu,
    CloseSortMenu,
    MoveSortMenuFocus {
        direction: i8,
    },
    SetSortMenuFocus {
        index: usize,
    },
    ToggleMoreMenu,
    CloseMoreMenu,
    MoveMoreMenuFocus {
        direction: i8,
    },
    SetMoreMenuFocus {
        index: usize,
    },
    ToggleExtensionsMenu,
    CloseExtensionsMenu,
    RefreshTortoiseGitStatus,
    ToggleViewMenu,
    CloseViewMenu,
    MoveViewMenuFocus {
        direction: i8,
    },
    SetViewMenuFocus {
        index: usize,
    },
    ToggleViewShowSubmenu,
    SetViewMode(explorer_model::ViewMode),
    SetExtensionView {
        view_id: String,
    },
    ZoomView {
        direction: i8,
    },
    SetColumnId(explorer_model::ColumnId),
    SetSortDirection(explorer_model::SortDirection),
    SetDetailsColumnWidth {
        column: explorer_model::ColumnId,
        width: u16,
    },
    MoveDetailsColumn {
        column: explorer_model::ColumnId,
        before: Option<explorer_model::ColumnId>,
    },
    UpdateDetailsColumnDragPreview {
        column: explorer_model::ColumnId,
        target: explorer_model::ColumnId,
        pointer_x: f32,
        target_left: f32,
        target_right: f32,
    },
    CommitDetailsColumnDrag,
    CancelDetailsColumnDrag,
    AutoSizeDetailsColumn {
        column: explorer_model::ColumnId,
    },
    OpenDetailsColumnMenu {
        column: explorer_model::ColumnId,
    },
    CloseDetailsColumnMenu,
    OpenDetailsFilterMenu {
        column: explorer_model::ColumnId,
    },
    CloseDetailsFilterMenu,
    ToggleDetailsFilter {
        column: explorer_model::ColumnId,
        key: String,
    },
    ClearDetailsFilter {
        column: explorer_model::ColumnId,
    },
    ToggleDetailsColumn(explorer_model::ColumnId),
    ToggleFolderSizeProportionalBar,
    ToggleCodeLinesDetail,
    AutoSizeAllDetailsColumns,
    BeginDetailsColumnResize {
        column: explorer_model::ColumnId,
        pointer_x: f32,
    },
    UpdateDetailsColumnResize {
        pointer_x: f32,
    },
    EndDetailsColumnResize,
    BeginSidePaneResize {
        pointer_x: f32,
    },
    UpdateSidePaneResize {
        pointer_x: f32,
    },
    EndSidePaneResize,
    ResetSidePaneWidth,
    AdjustSidePaneWidth {
        direction: i8,
    },
    UpdatePreviewHostBoundary {
        parent_window: u64,
        left_physical: i32,
        top_physical: i32,
        width_physical: u32,
        height_physical: u32,
        dpi: u32,
    },
    BeginScrollbarDrag {
        kind: crate::interaction::ScrollbarKind,
        grab_offset_y: f32,
    },
    UpdateScrollbarDrag {
        pointer_y: f32,
    },
    EndScrollbarDrag {
        reason: crate::interaction::ScrollbarTerminal,
    },
    ToggleDetailsPane,
    TogglePreviewPane,
    ToggleItemCheckBoxes,
    ToggleFileNameExtensions,
    ToggleHiddenItems,
    ToggleCompactView,
    ToggleTheme,
    CloseWindow,
    ResizeNavigationPane {
        width: LogicalPx,
    },
    BeginNavigationPaneResize {
        pointer_x: f32,
    },
    UpdateNavigationPaneResize {
        pointer_x: f32,
    },
    EndNavigationPaneResize,
    ResetNavigationPaneWidth,
    AdjustNavigationPaneWidth {
        direction: i8,
    },
}

impl ExplorerAction {
    pub fn name(&self) -> &'static str {
        match self {
            Self::Back => "Back",
            Self::Forward => "Forward",
            Self::OpenNavigationHistory { .. } => "OpenNavigationHistory",
            Self::CloseNavigationHistory => "CloseNavigationHistory",
            Self::MoveNavigationHistoryFocus { .. } => "MoveNavigationHistoryFocus",
            Self::SetNavigationHistoryFocus { .. } => "SetNavigationHistoryFocus",
            Self::ActivateNavigationHistory { .. } => "ActivateNavigationHistory",
            Self::Up => "Up",
            Self::Refresh => "Refresh",
            Self::FocusAddress => "FocusAddress",
            Self::EnterAddressEdit => "EnterAddressEdit",
            Self::UpdateAddressDraft(_) => "UpdateAddressDraft",
            Self::SubmitAddress(_) => "SubmitAddress",
            Self::CancelAddressEdit => "CancelAddressEdit",
            Self::ActivateBreadcrumbSegment { .. } => "ActivateBreadcrumbSegment",
            Self::OpenBreadcrumbChildren { .. } => "OpenBreadcrumbChildren",
            Self::RetryBreadcrumbChildren { .. } => "RetryBreadcrumbChildren",
            Self::ToggleBreadcrumbOverflow => "ToggleBreadcrumbOverflow",
            Self::CloseBreadcrumbMenu => "CloseBreadcrumbMenu",
            Self::MoveBreadcrumbSegmentFocus { .. } => "MoveBreadcrumbSegmentFocus",
            Self::MoveBreadcrumbMenuFocus { .. } => "MoveBreadcrumbMenuFocus",
            Self::SetBreadcrumbMenuFocus { .. } => "SetBreadcrumbMenuFocus",
            Self::TypeAheadBreadcrumbMenu { .. } => "TypeAheadBreadcrumbMenu",
            Self::ActivateBreadcrumbChild { .. } => "ActivateBreadcrumbChild",
            Self::ActivateNavigationItem { .. } => "ActivateNavigationItem",
            Self::ToggleNavigationNode { .. } => "ToggleNavigationNode",
            Self::FocusSearch => "FocusSearch",
            Self::ClearSearch => "ClearSearch",
            Self::FocusNext => "FocusNext",
            Self::FocusPrevious => "FocusPrevious",
            Self::SubmitFocusedInput => "SubmitFocusedInput",
            Self::CancelFocusedInput => "CancelFocusedInput",
            Self::RestorePreviousFocus => "RestorePreviousFocus",
            Self::NewTab => "NewTab",
            Self::CloseActiveTab => "CloseActiveTab",
            Self::ActivateTab { .. } => "ActivateTab",
            Self::CloseTab { .. } => "CloseTab",
            Self::ReorderTab { .. } => "ReorderTab",
            Self::NextTab => "NextTab",
            Self::PreviousTab => "PreviousTab",
            Self::OpenItem { .. } => "OpenItem",
            Self::OpenExtensionViewItem { .. } => "OpenExtensionViewItem",
            Self::OpenFocused => "OpenFocused",
            Self::SelectItem { .. } => "SelectItem",
            Self::SelectAdditionalItem { .. } => "SelectAdditionalItem",
            Self::SelectRange { .. } => "SelectRange",
            Self::FocusItem { .. } => "FocusItem",
            Self::SelectAllItems => "SelectAllItems",
            Self::InvertSelection => "InvertSelection",
            Self::ClearSelection => "ClearSelection",
            Self::TypeAheadFileView { .. } => "TypeAheadFileView",
            Self::ClearFileViewTypeAhead => "ClearFileViewTypeAhead",
            Self::BeginRenameFocused => "BeginRenameFocused",
            Self::CommitInlineRename => "CommitInlineRename",
            Self::CancelInlineRename => "CancelInlineRename",
            Self::RequestPermanentDelete => "RequestPermanentDelete",
            Self::ConfirmPermanentDelete => "ConfirmPermanentDelete",
            Self::CancelPermanentDelete => "CancelPermanentDelete",
            Self::MovePermanentDeleteDialogFocus { .. } => "MovePermanentDeleteDialogFocus",
            Self::SetPermanentDeleteDialogFocus { .. } => "SetPermanentDeleteDialogFocus",
            Self::CloseLockOwnersAndRetry => "CloseLockOwnersAndRetry",
            Self::RetryLockedDelete => "RetryLockedDelete",
            Self::CancelLockedDeleteRecovery => "CancelLockedDeleteRecovery",
            Self::MoveLockedDeleteDialogFocus { .. } => "MoveLockedDeleteDialogFocus",
            Self::BeginMarquee { .. } => "BeginMarquee",
            Self::UpdateMarquee { .. } => "UpdateMarquee",
            Self::EndMarquee => "EndMarquee",
            Self::CreateFolder => "CreateFolder",
            Self::CreateRemoteSymlink => "CreateRemoteSymlink",
            Self::CreateRemoteSymlinkToFolder { .. } => "CreateRemoteSymlinkToFolder",
            Self::ShowRemoteBackgroundProperties => "ShowRemoteBackgroundProperties",
            Self::ToggleNewMenu => "ToggleNewMenu",
            Self::CloseNewMenu => "CloseNewMenu",
            Self::MoveNewMenuFocus { .. } => "MoveNewMenuFocus",
            Self::CreateNewItem { .. } => "CreateNewItem",
            Self::RecycleDeleteSelected => "RecycleDeleteSelected",
            Self::CreateShortcutSelected => "CreateShortcutSelected",
            Self::CopySelected => "CopySelected",
            Self::CutSelected => "CutSelected",
            Self::Paste => "Paste",
            Self::DownloadSelectedToDownloads => "DownloadSelectedToDownloads",
            Self::ShareSelected => "ShareSelected",
            Self::PinSelectedToStart => "PinSelectedToStart",
            Self::ShowPropertiesSelected => "ShowPropertiesSelected",
            Self::CloseRemoteProperties => "CloseRemoteProperties",
            Self::ToggleRemotePermission { .. } => "ToggleRemotePermission",
            Self::ApplyRemoteProperties => "ApplyRemoteProperties",
            Self::RestoreSelected => "RestoreSelected",
            Self::EmptyRecycleBin => "EmptyRecycleBin",
            Self::UndoCurrentFolder => "UndoCurrentFolder",
            Self::CompressSelectedToZip => "CompressSelectedToZip",
            Self::AddSelectedToFavorites => "AddSelectedToFavorites",
            Self::AddSelectedToBookmarks => "AddSelectedToBookmarks",
            Self::ToggleCurrentFolderBookmark { .. } => "ToggleCurrentFolderBookmark",
            Self::ActivateBookmark { .. } => "ActivateBookmark",
            Self::OpenBookmarkInNewTab { .. } => "OpenBookmarkInNewTab",
            Self::OpenBookmarkContextMenu { .. } => "OpenBookmarkContextMenu",
            Self::CloseBookmarkContextMenu => "CloseBookmarkContextMenu",
            Self::RequestRemoveBookmark { .. } => "RequestRemoveBookmark",
            Self::OpenBookmarkToolbarContextMenu { .. } => "OpenBookmarkToolbarContextMenu",
            Self::CloseBookmarkToolbarContextMenu => "CloseBookmarkToolbarContextMenu",
            Self::AddPathBookmark { .. } => "AddPathBookmark",
            Self::CloseRemoteContextMenu => "CloseRemoteContextMenu",
            Self::AddLuaBookmark => "AddLuaBookmark",
            Self::EditBookmark { .. } => "EditBookmark",
            Self::SaveBookmarkEditor => "SaveBookmarkEditor",
            Self::CancelBookmarkEditor => "CancelBookmarkEditor",
            Self::SelectBookmarkDestination { .. } => "SelectBookmarkDestination",
            Self::AddBookmarkFolder { .. } => "AddBookmarkFolder",
            Self::EditBookmarkFolder { .. } => "EditBookmarkFolder",
            Self::SaveBookmarkFolderEditor => "SaveBookmarkFolderEditor",
            Self::CancelBookmarkFolderEditor => "CancelBookmarkFolderEditor",
            Self::RemoveBookmarkFolder { .. } => "RemoveBookmarkFolder",
            Self::ConfirmRemoveBookmarkFolder => "ConfirmRemoveBookmarkFolder",
            Self::CancelRemoveBookmarkFolder => "CancelRemoveBookmarkFolder",
            Self::RemoveEditingBookmark => "RemoveEditingBookmark",
            Self::ToggleBookmarkManager => "ToggleBookmarkManager",
            Self::ImportBookmarksFromClipboard => "ImportBookmarksFromClipboard",
            Self::BackupBookmarksToClipboard => "BackupBookmarksToClipboard",
            Self::ToggleBookmarkOverflow => "ToggleBookmarkOverflow",
            Self::ToggleBookmarkFolderMenu { .. } => "ToggleBookmarkFolderMenu",
            Self::RemoveBookmark { .. } => "RemoveBookmark",
            Self::MoveBookmark { .. } => "MoveBookmark",
            Self::MoveBookmarkToFolder { .. } => "MoveBookmarkToFolder",
            Self::CopySelectedPaths => "CopySelectedPaths",
            Self::OpenAboutDialog => "OpenAboutDialog",
            Self::CloseAboutDialog => "CloseAboutDialog",
            Self::OpenFolderOptions => "OpenFolderOptions",
            Self::CloseFolderOptions => "CloseFolderOptions",
            Self::SetFolderOptionsPage(_) => "SetFolderOptionsPage",
            Self::ToggleFolderOptionExtension { .. } => "ToggleFolderOptionExtension",
            Self::OpenExtensionAuthorWebsite { .. } => "OpenExtensionAuthorWebsite",
            Self::OpenExtensionCommunityWebsite { .. } => "OpenExtensionCommunityWebsite",
            Self::InvokeExtensionCommand { .. } => "InvokeExtensionCommand",
            Self::CloseExtensionCommandPanel => "CloseExtensionCommandPanel",
            Self::RunBulkFolderPreset { .. } => "RunBulkFolderPreset",
            Self::RunExifRenamePreset { .. } => "RunExifRenamePreset",
            Self::ToggleFolderOptionItemCheckBoxes => "ToggleFolderOptionItemCheckBoxes",
            Self::ToggleFolderOptionFileNameExtensions => "ToggleFolderOptionFileNameExtensions",
            Self::ToggleFolderOptionHiddenItems => "ToggleFolderOptionHiddenItems",
            Self::ToggleFolderOptionCompactView => "ToggleFolderOptionCompactView",
            Self::ToggleFolderOptionAlwaysShowIcons => "ToggleFolderOptionAlwaysShowIcons",
            Self::SetFolderOptionIconCacheMemoryMb(_) => "SetFolderOptionIconCacheMemoryMb",
            Self::SetFolderOptionThumbnailCacheMemoryMb(_) => {
                "SetFolderOptionThumbnailCacheMemoryMb"
            }
            Self::SetFolderOptionMftCacheMemoryMb(_) => "SetFolderOptionMftCacheMemoryMb",
            Self::SetFolderOptionCacheBudgets(_) => "SetFolderOptionCacheBudgets",
            Self::ClearThumbnailCache => "ClearThumbnailCache",
            Self::ToggleFolderOptionDetailsPane => "ToggleFolderOptionDetailsPane",
            Self::ToggleFolderOptionPreviewPane => "ToggleFolderOptionPreviewPane",
            Self::ToggleRestorePreviousSession => "ToggleRestorePreviousSession",
            Self::ResetSavedSession => "ResetSavedSession",
            Self::ResetSavedViewSettings => "ResetSavedViewSettings",
            Self::ResetAllSavedExplorerState => "ResetAllSavedExplorerState",
            Self::ConfirmSavedStateReset => "ConfirmSavedStateReset",
            Self::CancelSavedStateReset => "CancelSavedStateReset",
            Self::RetrySavedStateReset => "RetrySavedStateReset",
            Self::RetryExtensionBroker => "RetryExtensionBroker",
            Self::ResetFolderOptions => "ResetFolderOptions",
            Self::ApplyFolderOptions => "ApplyFolderOptions",
            Self::ConfirmFolderOptions => "ConfirmFolderOptions",
            Self::BeginFileDrag { .. } => "BeginFileDrag",
            Self::BeginContextItemGesture { .. } => "BeginContextItemGesture",
            Self::UpdateFileDrag { .. } => "UpdateFileDrag",
            Self::CancelFileDrag => "CancelFileDrag",
            Self::DropExternal { .. } => "DropExternal",
            Self::UpdateExternalDrag { .. } => "UpdateExternalDrag",
            Self::ClearExternalDrag => "ClearExternalDrag",
            Self::ResolveRightDrop { .. } => "ResolveRightDrop",
            Self::ShowContextMenu { .. } => "ShowContextMenu",
            Self::CancelOperation { .. } => "CancelOperation",
            Self::ToggleSortMenu => "ToggleSortMenu",
            Self::CloseSortMenu => "CloseSortMenu",
            Self::MoveSortMenuFocus { .. } => "MoveSortMenuFocus",
            Self::SetSortMenuFocus { .. } => "SetSortMenuFocus",
            Self::ToggleMoreMenu => "ToggleMoreMenu",
            Self::CloseMoreMenu => "CloseMoreMenu",
            Self::MoveMoreMenuFocus { .. } => "MoveMoreMenuFocus",
            Self::SetMoreMenuFocus { .. } => "SetMoreMenuFocus",
            Self::ToggleExtensionsMenu => "ToggleExtensionsMenu",
            Self::CloseExtensionsMenu => "CloseExtensionsMenu",
            Self::RefreshTortoiseGitStatus => "RefreshTortoiseGitStatus",
            Self::ToggleViewMenu => "ToggleViewMenu",
            Self::CloseViewMenu => "CloseViewMenu",
            Self::MoveViewMenuFocus { .. } => "MoveViewMenuFocus",
            Self::SetViewMenuFocus { .. } => "SetViewMenuFocus",
            Self::ToggleViewShowSubmenu => "ToggleViewShowSubmenu",
            Self::SetViewMode(_) => "SetViewMode",
            Self::SetExtensionView { .. } => "SetExtensionView",
            Self::ZoomView { .. } => "ZoomView",
            Self::SetColumnId(_) => "SetColumnId",
            Self::SetSortDirection(_) => "SetSortDirection",
            Self::SetDetailsColumnWidth { .. } => "SetDetailsColumnWidth",
            Self::MoveDetailsColumn { .. } => "MoveDetailsColumn",
            Self::UpdateDetailsColumnDragPreview { .. } => "UpdateDetailsColumnDragPreview",
            Self::CommitDetailsColumnDrag => "CommitDetailsColumnDrag",
            Self::CancelDetailsColumnDrag => "CancelDetailsColumnDrag",
            Self::AutoSizeDetailsColumn { .. } => "AutoSizeDetailsColumn",
            Self::OpenDetailsColumnMenu { .. } => "OpenDetailsColumnMenu",
            Self::CloseDetailsColumnMenu => "CloseDetailsColumnMenu",
            Self::OpenDetailsFilterMenu { .. } => "OpenDetailsFilterMenu",
            Self::CloseDetailsFilterMenu => "CloseDetailsFilterMenu",
            Self::ToggleDetailsFilter { .. } => "ToggleDetailsFilter",
            Self::ClearDetailsFilter { .. } => "ClearDetailsFilter",
            Self::ToggleDetailsColumn(_) => "ToggleDetailsColumn",
            Self::ToggleFolderSizeProportionalBar => "ToggleFolderSizeProportionalBar",
            Self::ToggleCodeLinesDetail => "ToggleCodeLinesDetail",
            Self::AutoSizeAllDetailsColumns => "AutoSizeAllDetailsColumns",
            Self::BeginDetailsColumnResize { .. } => "BeginDetailsColumnResize",
            Self::UpdateDetailsColumnResize { .. } => "UpdateDetailsColumnResize",
            Self::EndDetailsColumnResize => "EndDetailsColumnResize",
            Self::BeginSidePaneResize { .. } => "BeginSidePaneResize",
            Self::UpdateSidePaneResize { .. } => "UpdateSidePaneResize",
            Self::EndSidePaneResize => "EndSidePaneResize",
            Self::ResetSidePaneWidth => "ResetSidePaneWidth",
            Self::AdjustSidePaneWidth { .. } => "AdjustSidePaneWidth",
            Self::UpdatePreviewHostBoundary { .. } => "UpdatePreviewHostBoundary",
            Self::BeginScrollbarDrag { .. } => "BeginScrollbarDrag",
            Self::UpdateScrollbarDrag { .. } => "UpdateScrollbarDrag",
            Self::EndScrollbarDrag { .. } => "EndScrollbarDrag",
            Self::ToggleDetailsPane => "ToggleDetailsPane",
            Self::TogglePreviewPane => "TogglePreviewPane",
            Self::ToggleItemCheckBoxes => "ToggleItemCheckBoxes",
            Self::ToggleFileNameExtensions => "ToggleFileNameExtensions",
            Self::ToggleHiddenItems => "ToggleHiddenItems",
            Self::ToggleCompactView => "ToggleCompactView",
            Self::ToggleTheme => "ToggleTheme",
            Self::CloseWindow => "CloseWindow",
            Self::ResizeNavigationPane { .. } => "ResizeNavigationPane",
            Self::BeginNavigationPaneResize { .. } => "BeginNavigationPaneResize",
            Self::UpdateNavigationPaneResize { .. } => "UpdateNavigationPaneResize",
            Self::EndNavigationPaneResize => "EndNavigationPaneResize",
            Self::ResetNavigationPaneWidth => "ResetNavigationPaneWidth",
            Self::AdjustNavigationPaneWidth { .. } => "AdjustNavigationPaneWidth",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum BindingScope {
    Window,
    TextInput,
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum KeyCode {
    Left,
    Right,
    Up,
    L,
    F,
    Escape,
    D,
    F4,
    F5,
    T,
    W,
    Tab,
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct KeyChord {
    pub key: KeyCode,
    pub control: bool,
    pub shift: bool,
    pub alt: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct KeyBinding {
    pub scope: BindingScope,
    pub chord: KeyChord,
    pub action: ExplorerAction,
}

pub const DEFAULT_BINDINGS: [KeyBinding; 16] = [
    binding(
        BindingScope::Window,
        KeyCode::Left,
        false,
        false,
        true,
        ExplorerAction::Back,
    ),
    binding(
        BindingScope::Window,
        KeyCode::Right,
        false,
        false,
        true,
        ExplorerAction::Forward,
    ),
    binding(
        BindingScope::Window,
        KeyCode::Up,
        false,
        false,
        true,
        ExplorerAction::Up,
    ),
    binding(
        BindingScope::Window,
        KeyCode::F5,
        false,
        false,
        false,
        ExplorerAction::Refresh,
    ),
    binding(
        BindingScope::Window,
        KeyCode::L,
        true,
        false,
        false,
        ExplorerAction::FocusAddress,
    ),
    binding(
        BindingScope::Window,
        KeyCode::D,
        false,
        false,
        true,
        ExplorerAction::FocusAddress,
    ),
    binding(
        BindingScope::Window,
        KeyCode::F,
        true,
        false,
        false,
        ExplorerAction::FocusSearch,
    ),
    binding(
        BindingScope::TextInput,
        KeyCode::Escape,
        false,
        false,
        false,
        ExplorerAction::RestorePreviousFocus,
    ),
    binding(
        BindingScope::Window,
        KeyCode::Tab,
        false,
        false,
        false,
        ExplorerAction::FocusNext,
    ),
    binding(
        BindingScope::Window,
        KeyCode::Tab,
        false,
        true,
        false,
        ExplorerAction::FocusPrevious,
    ),
    binding(
        BindingScope::Window,
        KeyCode::D,
        true,
        true,
        false,
        ExplorerAction::ToggleTheme,
    ),
    binding(
        BindingScope::Window,
        KeyCode::T,
        true,
        false,
        false,
        ExplorerAction::NewTab,
    ),
    binding(
        BindingScope::Window,
        KeyCode::W,
        true,
        false,
        false,
        ExplorerAction::CloseActiveTab,
    ),
    binding(
        BindingScope::Window,
        KeyCode::Tab,
        true,
        false,
        false,
        ExplorerAction::NextTab,
    ),
    binding(
        BindingScope::Window,
        KeyCode::Tab,
        true,
        true,
        false,
        ExplorerAction::PreviousTab,
    ),
    binding(
        BindingScope::Window,
        KeyCode::F4,
        false,
        false,
        true,
        ExplorerAction::CloseWindow,
    ),
];

const fn binding(
    scope: BindingScope,
    key: KeyCode,
    control: bool,
    shift: bool,
    alt: bool,
    action: ExplorerAction,
) -> KeyBinding {
    KeyBinding {
        scope,
        chord: KeyChord {
            key,
            control,
            shift,
            alt,
        },
        action,
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BindingConflict {
    pub first_index: usize,
    pub second_index: usize,
}

/// Rejects duplicate chords within the same routing scope.
///
/// # Errors
///
/// Returns the indexes of the first conflicting binding pair.
pub fn validate_bindings(bindings: &[KeyBinding]) -> Result<(), BindingConflict> {
    for (first_index, first) in bindings.iter().enumerate() {
        for (second_index, second) in bindings.iter().enumerate().skip(first_index + 1) {
            if first.scope == second.scope && first.chord == second.chord {
                return Err(BindingConflict {
                    first_index,
                    second_index,
                });
            }
        }
    }
    Ok(())
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ActionSource {
    Mouse,
    Keyboard,
    Accessibility,
    Programmatic,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ActionOutcome {
    Handled,
    Disabled,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ActionTrace {
    pub action_name: &'static str,
    pub source: ActionSource,
    pub handled_surface: FocusSurface,
    pub outcome: ActionOutcome,
}

fn action_dispatches_at_info(action: &ExplorerAction) -> bool {
    !matches!(action, ExplorerAction::UpdateFileDrag { .. })
}

pub fn dispatch_action(
    state: &mut AppViewState,
    action: ExplorerAction,
    source: ActionSource,
) -> ActionTrace {
    let action_name = action.name();
    let dispatches_at_info = action_dispatches_at_info(&action);
    let available = action_available(state, &action);
    let synchronize_command_popup_focus = matches!(
        &action,
        ExplorerAction::ToggleNewMenu
            | ExplorerAction::CloseNewMenu
            | ExplorerAction::MoveNewMenuFocus { .. }
            | ExplorerAction::CreateNewItem { .. }
            | ExplorerAction::ToggleSortMenu
            | ExplorerAction::CloseSortMenu
            | ExplorerAction::MoveSortMenuFocus { .. }
            | ExplorerAction::SetSortMenuFocus { .. }
            | ExplorerAction::SetColumnId(_)
            | ExplorerAction::SetSortDirection(_)
            | ExplorerAction::ToggleViewMenu
            | ExplorerAction::CloseViewMenu
            | ExplorerAction::MoveViewMenuFocus { .. }
            | ExplorerAction::SetViewMenuFocus { .. }
            | ExplorerAction::ToggleViewShowSubmenu
            | ExplorerAction::SetViewMode(_)
            | ExplorerAction::SetExtensionView { .. }
            | ExplorerAction::ZoomView { .. }
            | ExplorerAction::ToggleMoreMenu
            | ExplorerAction::CloseMoreMenu
            | ExplorerAction::MoveMoreMenuFocus { .. }
            | ExplorerAction::SetMoreMenuFocus { .. }
            | ExplorerAction::ToggleExtensionsMenu
            | ExplorerAction::CloseExtensionsMenu
            | ExplorerAction::OpenNavigationHistory { .. }
            | ExplorerAction::CloseNavigationHistory
            | ExplorerAction::MoveNavigationHistoryFocus { .. }
            | ExplorerAction::ActivateNavigationHistory { .. }
    );
    let preserve_more_menu = matches!(
        &action,
        ExplorerAction::ToggleMoreMenu
            | ExplorerAction::MoveMoreMenuFocus { .. }
            | ExplorerAction::SetMoreMenuFocus { .. }
    );
    let preserve_new_menu = matches!(
        &action,
        ExplorerAction::ToggleNewMenu | ExplorerAction::MoveNewMenuFocus { .. }
    );
    let preserve_sort_menu = matches!(
        &action,
        ExplorerAction::ToggleSortMenu
            | ExplorerAction::MoveSortMenuFocus { .. }
            | ExplorerAction::SetSortMenuFocus { .. }
    );
    let preserve_view_menu = matches!(
        &action,
        ExplorerAction::ToggleViewMenu
            | ExplorerAction::MoveViewMenuFocus { .. }
            | ExplorerAction::SetViewMenuFocus { .. }
            | ExplorerAction::ToggleViewShowSubmenu
    );
    let preserve_extensions_menu = matches!(
        &action,
        ExplorerAction::ToggleExtensionsMenu
            | ExplorerAction::InvokeExtensionCommand { .. }
            | ExplorerAction::CloseExtensionCommandPanel
    );
    let preserve_details_column_menu = matches!(
        &action,
        ExplorerAction::OpenDetailsColumnMenu { .. }
            | ExplorerAction::ToggleDetailsColumn(_)
            | ExplorerAction::AutoSizeDetailsColumn { .. }
            | ExplorerAction::AutoSizeAllDetailsColumns
    );
    let preserve_details_filter_menu = matches!(
        &action,
        ExplorerAction::OpenDetailsFilterMenu { .. }
            | ExplorerAction::ToggleDetailsFilter { .. }
            | ExplorerAction::ClearDetailsFilter { .. }
    );
    let preserve_navigation_history = matches!(
        &action,
        ExplorerAction::OpenNavigationHistory { .. }
            | ExplorerAction::MoveNavigationHistoryFocus { .. }
            | ExplorerAction::SetNavigationHistoryFocus { .. }
    );
    let handled_surface = if available {
        let handled_surface = apply_action(state, action);
        // Keep the reducer's focus model synchronized with the surface that handled the action.
        // Native focus synchronization consumes this state after dispatch; returning the surface
        // only in the trace left keyboard-opened command popups focused on the file view.
        if synchronize_command_popup_focus {
            state.focus(handled_surface);
        }
        handled_surface
    } else {
        state.focused_surface()
    };
    let outcome = if available {
        if !preserve_more_menu {
            state.close_more_menu();
        }
        if !preserve_new_menu {
            state.close_new_menu();
        }
        if !preserve_sort_menu {
            state.close_sort_menu();
        }
        if !preserve_view_menu {
            state.close_view_menu();
        }
        if !preserve_extensions_menu {
            state.close_extensions_menu();
        }
        if !preserve_details_column_menu {
            state.close_details_column_menu();
        }
        if !preserve_details_filter_menu {
            state.close_details_filter_menu();
        }
        if !preserve_navigation_history {
            state.close_navigation_history_menu();
        }
        ActionOutcome::Handled
    } else {
        ActionOutcome::Disabled
    };
    let trace = ActionTrace {
        action_name,
        source,
        handled_surface,
        outcome,
    };
    if dispatches_at_info {
        tracing::info!(
            action = trace.action_name,
            source = ?trace.source,
            handled_surface = ?trace.handled_surface,
            outcome = ?trace.outcome,
            "Explorer action dispatched"
        );
    } else {
        tracing::trace!(
            action = trace.action_name,
            source = ?trace.source,
            handled_surface = ?trace.handled_surface,
            outcome = ?trace.outcome,
            "Explorer high-frequency action dispatched"
        );
    }
    trace
}

#[allow(
    clippy::match_same_arms,
    reason = "action families stay visibly grouped so availability remains auditable against Explorer command surfaces"
)]
fn action_available(state: &AppViewState, action: &ExplorerAction) -> bool {
    let availability = state.command_availability();
    match action {
        ExplorerAction::Back => availability.is_enabled(CommandKind::Back),
        ExplorerAction::Forward => availability.is_enabled(CommandKind::Forward),
        ExplorerAction::OpenNavigationHistory { direction } => {
            state.navigation_history_len(*direction) > 0
        }
        ExplorerAction::CloseNavigationHistory
        | ExplorerAction::MoveNavigationHistoryFocus { .. }
        | ExplorerAction::SetNavigationHistoryFocus { .. } => {
            state.navigation_history_menu_direction().is_some()
        }
        ExplorerAction::ActivateNavigationHistory { direction, steps } => {
            state.navigation_history_menu_direction() == Some(*direction)
                && *steps > 0
                && *steps <= state.navigation_history_len(*direction)
        }
        ExplorerAction::Up => availability.is_enabled(CommandKind::Up),
        ExplorerAction::Refresh => availability.is_enabled(CommandKind::Refresh),
        ExplorerAction::FocusAddress => availability.is_enabled(CommandKind::FocusAddress),
        ExplorerAction::EnterAddressEdit
        | ExplorerAction::UpdateAddressDraft(_)
        | ExplorerAction::SubmitAddress(_)
        | ExplorerAction::CancelAddressEdit
        | ExplorerAction::ActivateBreadcrumbSegment { .. }
        | ExplorerAction::OpenBreadcrumbChildren { .. }
        | ExplorerAction::RetryBreadcrumbChildren { .. }
        | ExplorerAction::ToggleBreadcrumbOverflow
        | ExplorerAction::CloseBreadcrumbMenu
        | ExplorerAction::MoveBreadcrumbSegmentFocus { .. }
        | ExplorerAction::MoveBreadcrumbMenuFocus { .. }
        | ExplorerAction::SetBreadcrumbMenuFocus { .. }
        | ExplorerAction::TypeAheadBreadcrumbMenu { .. }
        | ExplorerAction::ActivateBreadcrumbChild { .. } => true,
        ExplorerAction::ActivateNavigationItem { .. }
        | ExplorerAction::ToggleNavigationNode { .. } => true,
        ExplorerAction::FocusSearch => availability.is_enabled(CommandKind::FocusSearch),
        ExplorerAction::ClearSearch => !matches!(
            state.tabs().active_tab().search,
            explorer_model::TabSearchState::Idle
        ),
        ExplorerAction::RestorePreviousFocus => state.previous_focus().is_some(),
        ExplorerAction::NewTab => availability.is_enabled(CommandKind::NewTab),
        ExplorerAction::CloseActiveTab => availability.is_enabled(CommandKind::CloseTab),
        ExplorerAction::CloseTab { tab_id } => {
            availability.is_enabled(CommandKind::CloseTab)
                && state.tabs().tabs().iter().any(|tab| tab.id == *tab_id)
        }
        ExplorerAction::ActivateTab { tab_id } => {
            state.tabs().tabs().iter().any(|tab| tab.id == *tab_id)
        }
        ExplorerAction::ReorderTab {
            tab_id,
            destination_index,
        } => {
            *destination_index < state.tabs().tabs().len()
                && state.tabs().tabs().iter().any(|tab| tab.id == *tab_id)
        }
        ExplorerAction::NextTab => availability.is_enabled(CommandKind::NextTab),
        ExplorerAction::PreviousTab => availability.is_enabled(CommandKind::PreviousTab),
        ExplorerAction::OpenItem { row_index, .. } => {
            state.row_namespace_command_enabled(*row_index, explorer_model::NamespaceCommand::Open)
        }
        ExplorerAction::OpenExtensionViewItem { .. } => true,
        ExplorerAction::OpenFocused => state.focused_row_index().is_some_and(|row_index| {
            state.row_namespace_command_enabled(row_index, explorer_model::NamespaceCommand::Open)
        }),
        ExplorerAction::SelectItem { row_index }
        | ExplorerAction::SelectAdditionalItem { row_index }
        | ExplorerAction::SelectRange { row_index, .. }
        | ExplorerAction::FocusItem { row_index } => *row_index < state.visible_row_count(),
        ExplorerAction::SelectAllItems
        | ExplorerAction::InvertSelection
        | ExplorerAction::ClearSelection
        | ExplorerAction::TypeAheadFileView { .. }
        | ExplorerAction::ClearFileViewTypeAhead
        | ExplorerAction::CommitInlineRename
        | ExplorerAction::CancelInlineRename
        | ExplorerAction::BeginMarquee { .. }
        | ExplorerAction::UpdateMarquee { .. }
        | ExplorerAction::EndMarquee => true,
        ExplorerAction::BeginRenameFocused => state
            .focused_row_index()
            .or_else(|| (state.visible_row_count() > 0).then_some(0))
            .is_some_and(|row_index| {
                state.row_namespace_command_enabled(
                    row_index,
                    explorer_model::NamespaceCommand::Rename,
                )
            }),
        ExplorerAction::RequestPermanentDelete => {
            state.selected_namespace_command_enabled(explorer_model::NamespaceCommand::Delete)
        }
        ExplorerAction::ConfirmPermanentDelete
        | ExplorerAction::CancelPermanentDelete
        | ExplorerAction::MovePermanentDeleteDialogFocus { .. }
        | ExplorerAction::SetPermanentDeleteDialogFocus { .. } => {
            state.permanent_delete_confirmation_count().is_some()
        }
        ExplorerAction::CloseLockOwnersAndRetry => state
            .lock_recovery()
            .is_some_and(crate::state::LockRecoveryUiState::can_close),
        ExplorerAction::RetryLockedDelete => state
            .lock_recovery()
            .is_some_and(crate::state::LockRecoveryUiState::can_retry),
        ExplorerAction::CancelLockedDeleteRecovery => state.lock_recovery().is_some(),
        ExplorerAction::MoveLockedDeleteDialogFocus { .. } => state.lock_recovery().is_some(),
        ExplorerAction::CreateFolder
        | ExplorerAction::CreateRemoteSymlink
        | ExplorerAction::CreateRemoteSymlinkToFolder { .. }
        | ExplorerAction::ToggleNewMenu => state.active_presentation().can_write,
        ExplorerAction::ShowRemoteBackgroundProperties => true,
        ExplorerAction::CloseNewMenu | ExplorerAction::MoveNewMenuFocus { .. } => true,
        ExplorerAction::CreateNewItem { index } => {
            state.active_presentation().can_write && *index < state.new_items().len()
        }
        ExplorerAction::RecycleDeleteSelected => {
            state.selected_namespace_command_enabled(explorer_model::NamespaceCommand::Delete)
        }
        ExplorerAction::CreateShortcutSelected => {
            !state.tabs().active_tab().selection.is_empty() && state.active_presentation().can_write
        }
        ExplorerAction::BeginContextItemGesture { .. } => true,
        ExplorerAction::BeginFileDrag { .. }
        | ExplorerAction::UpdateFileDrag { .. }
        | ExplorerAction::CancelFileDrag => !state.tabs().active_tab().selection.is_empty(),
        ExplorerAction::CopySelected => {
            state.selected_namespace_command_enabled(explorer_model::NamespaceCommand::Copy)
        }
        ExplorerAction::CutSelected => {
            state.selected_namespace_command_enabled(explorer_model::NamespaceCommand::Copy)
                && state
                    .selected_namespace_command_enabled(explorer_model::NamespaceCommand::Delete)
        }
        ExplorerAction::ShareSelected
        | ExplorerAction::PinSelectedToStart
        | ExplorerAction::CompressSelectedToZip => {
            state.selected_namespace_command_enabled(explorer_model::NamespaceCommand::ContextMenu)
        }
        ExplorerAction::ShowPropertiesSelected => {
            state.selected_namespace_command_enabled(explorer_model::NamespaceCommand::Properties)
        }
        ExplorerAction::CloseRemoteProperties
        | ExplorerAction::ToggleRemotePermission { .. }
        | ExplorerAction::ApplyRemoteProperties => state.remote_properties().is_some(),
        ExplorerAction::RestoreSelected => {
            state.selected_namespace_command_enabled(explorer_model::NamespaceCommand::Restore)
        }
        ExplorerAction::EmptyRecycleBin => state.active_is_recycle_bin(),
        ExplorerAction::AddSelectedToFavorites | ExplorerAction::AddSelectedToBookmarks => {
            state.selected_namespace_command_enabled(explorer_model::NamespaceCommand::Pin)
        }
        ExplorerAction::ToggleCurrentFolderBookmark { .. } => {
            state.current_folder_bookmark_target_and_id().is_some()
        }
        ExplorerAction::CopySelectedPaths => !state.tabs().active_tab().selection.is_empty(),
        ExplorerAction::Paste => {
            !matches!(
                state.clipboard(),
                explorer_model::ClipboardState::None { .. }
                    | explorer_model::ClipboardState::Unsupported { .. }
            ) && state.active_presentation().can_write
        }
        ExplorerAction::DownloadSelectedToDownloads => state.selected_items_include_remote(),
        ExplorerAction::DropExternal {
            destination_row, ..
        } => destination_row.is_none_or(|row| state.presentation_row_is_container(row)),
        ExplorerAction::FocusNext
        | ExplorerAction::FocusPrevious
        | ExplorerAction::SubmitFocusedInput
        | ExplorerAction::CancelFocusedInput
        | ExplorerAction::UpdateExternalDrag { .. }
        | ExplorerAction::ClearExternalDrag
        | ExplorerAction::ResolveRightDrop { .. } => true,
        ExplorerAction::ShowContextMenu { item_id, .. } => item_id.as_ref().is_none_or(|item_id| {
            state.item_namespace_command_enabled(
                item_id,
                explorer_model::NamespaceCommand::ContextMenu,
            )
        }),
        ExplorerAction::CancelOperation { request_id } => state
            .operation_center()
            .get(*request_id)
            .is_some_and(|record| !record.phase.is_terminal()),
        ExplorerAction::ToggleDetailsColumn(explorer_model::ColumnId::Name) => false,
        ExplorerAction::EndDetailsColumnResize => state.details_column_resize_active(),
        ExplorerAction::SetColumnId(column) => state.sort_column_supported(column.clone()),
        ExplorerAction::ToggleSortMenu
        | ExplorerAction::CloseSortMenu
        | ExplorerAction::MoveSortMenuFocus { .. }
        | ExplorerAction::SetSortMenuFocus { .. }
        | ExplorerAction::ToggleMoreMenu
        | ExplorerAction::CloseMoreMenu
        | ExplorerAction::MoveMoreMenuFocus { .. }
        | ExplorerAction::SetMoreMenuFocus { .. }
        | ExplorerAction::ToggleExtensionsMenu
        | ExplorerAction::CloseExtensionsMenu
        | ExplorerAction::ToggleViewMenu
        | ExplorerAction::CloseViewMenu
        | ExplorerAction::MoveViewMenuFocus { .. }
        | ExplorerAction::SetViewMenuFocus { .. }
        | ExplorerAction::ToggleViewShowSubmenu
        | ExplorerAction::SetViewMode(_)
        | ExplorerAction::SetExtensionView { .. }
        | ExplorerAction::ZoomView { .. }
        | ExplorerAction::SetSortDirection(_)
        | ExplorerAction::SetDetailsColumnWidth { .. }
        | ExplorerAction::MoveDetailsColumn { .. }
        | ExplorerAction::UpdateDetailsColumnDragPreview { .. }
        | ExplorerAction::CommitDetailsColumnDrag
        | ExplorerAction::CancelDetailsColumnDrag
        | ExplorerAction::AutoSizeDetailsColumn { .. }
        | ExplorerAction::OpenDetailsColumnMenu { .. }
        | ExplorerAction::CloseDetailsColumnMenu
        | ExplorerAction::OpenDetailsFilterMenu { .. }
        | ExplorerAction::CloseDetailsFilterMenu
        | ExplorerAction::ToggleDetailsFilter { .. }
        | ExplorerAction::ClearDetailsFilter { .. }
        | ExplorerAction::ToggleDetailsColumn(_)
        | ExplorerAction::ToggleFolderSizeProportionalBar
        | ExplorerAction::ToggleCodeLinesDetail
        | ExplorerAction::AutoSizeAllDetailsColumns
        | ExplorerAction::BeginDetailsColumnResize { .. }
        | ExplorerAction::UpdateDetailsColumnResize { .. }
        | ExplorerAction::UndoCurrentFolder
        | ExplorerAction::OpenFolderOptions
        | ExplorerAction::OpenAboutDialog
        | ExplorerAction::CloseAboutDialog
        | ExplorerAction::CloseFolderOptions
        | ExplorerAction::SetFolderOptionsPage(_)
        | ExplorerAction::ToggleFolderOptionExtension { .. }
        | ExplorerAction::OpenExtensionAuthorWebsite { .. }
        | ExplorerAction::OpenExtensionCommunityWebsite { .. }
        | ExplorerAction::InvokeExtensionCommand { .. }
        | ExplorerAction::CloseExtensionCommandPanel
        | ExplorerAction::RunBulkFolderPreset { .. }
        | ExplorerAction::RunExifRenamePreset { .. }
        | ExplorerAction::ToggleFolderOptionItemCheckBoxes
        | ExplorerAction::ToggleFolderOptionFileNameExtensions
        | ExplorerAction::ToggleFolderOptionHiddenItems
        | ExplorerAction::ToggleFolderOptionCompactView
        | ExplorerAction::ToggleFolderOptionAlwaysShowIcons
        | ExplorerAction::SetFolderOptionIconCacheMemoryMb(_)
        | ExplorerAction::SetFolderOptionThumbnailCacheMemoryMb(_)
        | ExplorerAction::SetFolderOptionMftCacheMemoryMb(_)
        | ExplorerAction::SetFolderOptionCacheBudgets(_)
        | ExplorerAction::ClearThumbnailCache
        | ExplorerAction::ToggleFolderOptionDetailsPane
        | ExplorerAction::ToggleFolderOptionPreviewPane
        | ExplorerAction::ToggleRestorePreviousSession
        | ExplorerAction::ResetSavedSession
        | ExplorerAction::ResetSavedViewSettings
        | ExplorerAction::ResetAllSavedExplorerState
        | ExplorerAction::ConfirmSavedStateReset
        | ExplorerAction::CancelSavedStateReset
        | ExplorerAction::RetrySavedStateReset
        | ExplorerAction::RetryExtensionBroker
        | ExplorerAction::ResetFolderOptions
        | ExplorerAction::ApplyFolderOptions
        | ExplorerAction::ConfirmFolderOptions
        | ExplorerAction::BeginSidePaneResize { .. }
        | ExplorerAction::UpdateSidePaneResize { .. }
        | ExplorerAction::EndSidePaneResize
        | ExplorerAction::ResetSidePaneWidth
        | ExplorerAction::AdjustSidePaneWidth { .. }
        | ExplorerAction::UpdatePreviewHostBoundary { .. }
        | ExplorerAction::BeginScrollbarDrag { .. }
        | ExplorerAction::ToggleDetailsPane
        | ExplorerAction::TogglePreviewPane
        | ExplorerAction::ToggleItemCheckBoxes
        | ExplorerAction::ToggleFileNameExtensions
        | ExplorerAction::ToggleHiddenItems
        | ExplorerAction::ToggleCompactView => true,
        ExplorerAction::RefreshTortoiseGitStatus => state.tortoise_git_available(),
        ExplorerAction::UpdateScrollbarDrag { .. } | ExplorerAction::EndScrollbarDrag { .. } => {
            state.scrollbar_drag_session().is_some()
        }
        ExplorerAction::ToggleTheme => availability.is_enabled(CommandKind::ToggleTheme),
        ExplorerAction::ActivateBookmark { .. }
        | ExplorerAction::OpenBookmarkInNewTab { .. }
        | ExplorerAction::OpenBookmarkContextMenu { .. }
        | ExplorerAction::CloseBookmarkContextMenu
        | ExplorerAction::RequestRemoveBookmark { .. }
        | ExplorerAction::OpenBookmarkToolbarContextMenu { .. }
        | ExplorerAction::CloseBookmarkToolbarContextMenu
        | ExplorerAction::AddPathBookmark { .. }
        | ExplorerAction::CloseRemoteContextMenu
        | ExplorerAction::AddLuaBookmark
        | ExplorerAction::EditBookmark { .. }
        | ExplorerAction::SaveBookmarkEditor
        | ExplorerAction::CancelBookmarkEditor
        | ExplorerAction::SelectBookmarkDestination { .. }
        | ExplorerAction::AddBookmarkFolder { .. }
        | ExplorerAction::EditBookmarkFolder { .. }
        | ExplorerAction::SaveBookmarkFolderEditor
        | ExplorerAction::CancelBookmarkFolderEditor
        | ExplorerAction::RemoveBookmarkFolder { .. }
        | ExplorerAction::ConfirmRemoveBookmarkFolder
        | ExplorerAction::CancelRemoveBookmarkFolder
        | ExplorerAction::RemoveEditingBookmark
        | ExplorerAction::ToggleBookmarkManager
        | ExplorerAction::ImportBookmarksFromClipboard
        | ExplorerAction::BackupBookmarksToClipboard
        | ExplorerAction::ToggleBookmarkOverflow
        | ExplorerAction::ToggleBookmarkFolderMenu { .. }
        | ExplorerAction::RemoveBookmark { .. }
        | ExplorerAction::MoveBookmark { .. }
        | ExplorerAction::MoveBookmarkToFolder { .. } => true,
        ExplorerAction::CloseWindow => availability.is_enabled(CommandKind::CloseWindow),
        ExplorerAction::ResizeNavigationPane { .. }
        | ExplorerAction::BeginNavigationPaneResize { .. }
        | ExplorerAction::UpdateNavigationPaneResize { .. }
        | ExplorerAction::EndNavigationPaneResize
        | ExplorerAction::ResetNavigationPaneWidth
        | ExplorerAction::AdjustNavigationPaneWidth { .. } => {
            availability.is_enabled(CommandKind::ResizeNavigationPane)
        }
    }
}

#[allow(
    clippy::match_same_arms,
    clippy::too_many_lines,
    reason = "one exhaustive typed action transition keeps keyboard and pointer dispatch identical while preserving feature-family grouping"
)]
fn apply_action(state: &mut AppViewState, action: ExplorerAction) -> FocusSurface {
    match action {
        ExplorerAction::Back
        | ExplorerAction::Forward
        | ExplorerAction::ActivateNavigationHistory { .. }
        | ExplorerAction::Up
        | ExplorerAction::Refresh => state.focused_surface(),
        ExplorerAction::OpenNavigationHistory { direction } => {
            let _ = state.open_navigation_history_menu(direction);
            state.focus(FocusSurface::AddressBar);
            FocusSurface::AddressBar
        }
        ExplorerAction::CloseNavigationHistory => {
            state.close_navigation_history_menu();
            FocusSurface::AddressBar
        }
        ExplorerAction::MoveNavigationHistoryFocus { direction } => {
            let _ = state.move_navigation_history_focus(direction);
            state.focus(FocusSurface::AddressBar);
            FocusSurface::AddressBar
        }
        ExplorerAction::SetNavigationHistoryFocus { index } => {
            let _ = state.set_navigation_history_focus(index);
            state.focus(FocusSurface::AddressBar);
            FocusSurface::AddressBar
        }
        ExplorerAction::FocusAddress | ExplorerAction::EnterAddressEdit => {
            state.enter_address_edit();
            state.focus(FocusSurface::AddressBar);
            FocusSurface::AddressBar
        }
        ExplorerAction::UpdateAddressDraft(value) => {
            let _ = state.update_address_edit_input(value);
            FocusSurface::AddressBar
        }
        ExplorerAction::SubmitAddress(_) => FocusSurface::AddressBar,
        ExplorerAction::CancelAddressEdit => {
            state.cancel_address_edit();
            let _ = state.restore_previous_focus();
            state.focused_surface()
        }
        ExplorerAction::ActivateBreadcrumbSegment { .. }
        | ExplorerAction::ActivateBreadcrumbChild { .. } => {
            state.close_address_menu();
            state.focus(FocusSurface::FileView);
            FocusSurface::FileView
        }
        ExplorerAction::ActivateNavigationItem { location } => {
            state.set_navigation_focus(location);
            state.focus(FocusSurface::NavigationPane);
            FocusSurface::NavigationPane
        }
        ExplorerAction::ToggleNavigationNode { location } => {
            state.set_navigation_focus(location.clone());
            let _ = state.toggle_navigation_node(location);
            state.focus(FocusSurface::NavigationPane);
            FocusSurface::NavigationPane
        }
        ExplorerAction::OpenBreadcrumbChildren { segment_id } => {
            let _ = state.open_address_menu(segment_id);
            FocusSurface::AddressBar
        }
        ExplorerAction::RetryBreadcrumbChildren { segment_id } => {
            state.close_address_menu();
            let _ = state.open_address_menu(segment_id);
            FocusSurface::AddressBar
        }
        ExplorerAction::ToggleBreadcrumbOverflow => {
            let _ = state.toggle_address_overflow();
            FocusSurface::AddressBar
        }
        ExplorerAction::CloseBreadcrumbMenu => {
            state.close_address_menu();
            FocusSurface::AddressBar
        }
        ExplorerAction::MoveBreadcrumbSegmentFocus { direction } => {
            let _ = state.move_breadcrumb_segment_focus(direction);
            state.focus(FocusSurface::AddressBar);
            FocusSurface::AddressBar
        }
        ExplorerAction::MoveBreadcrumbMenuFocus { movement } => {
            let _ = state.move_breadcrumb_menu_focus(movement);
            state.focus(FocusSurface::AddressBar);
            FocusSurface::AddressBar
        }
        ExplorerAction::SetBreadcrumbMenuFocus { index } => {
            let _ = state.set_breadcrumb_menu_focus(index);
            state.focus(FocusSurface::AddressBar);
            FocusSurface::AddressBar
        }
        ExplorerAction::TypeAheadBreadcrumbMenu { text } => {
            let _ = state.typeahead_breadcrumb_menu(&text);
            state.focus(FocusSurface::AddressBar);
            FocusSurface::AddressBar
        }
        ExplorerAction::FocusSearch => {
            state.begin_search_editing();
            state.focus(FocusSurface::Search);
            FocusSurface::Search
        }
        ExplorerAction::ClearSearch => {
            state.leave_active_search();
            state.begin_search_editing();
            state.focus(FocusSurface::Search);
            FocusSurface::Search
        }
        ExplorerAction::FocusNext => {
            if state.permanent_delete_confirmation_count().is_some() {
                let _ = state.move_permanent_delete_confirmation_focus(1);
            } else if state.lock_recovery().is_some() {
                let _ = state.move_lock_recovery_focus(1);
            } else {
                let _ = state.traverse_focus(crate::focus::FocusDirection::Forward);
            }
            state.focused_surface()
        }
        ExplorerAction::FocusPrevious => {
            if state.permanent_delete_confirmation_count().is_some() {
                let _ = state.move_permanent_delete_confirmation_focus(-1);
            } else if state.lock_recovery().is_some() {
                let _ = state.move_lock_recovery_focus(-1);
            } else {
                let _ = state.traverse_focus(crate::focus::FocusDirection::Backward);
            }
            state.focused_surface()
        }
        ExplorerAction::SubmitFocusedInput => state.focused_surface(),
        ExplorerAction::CancelFocusedInput => {
            if state.focused_surface() == FocusSurface::AddressBar {
                state.cancel_address_edit();
            } else if state.focused_surface() == FocusSurface::Search {
                state.leave_active_search();
            }
            let _ = state.restore_previous_focus();
            state.focused_surface()
        }
        ExplorerAction::RestorePreviousFocus => {
            let _ = state.restore_previous_focus();
            state.focused_surface()
        }
        ExplorerAction::NewTab => {
            let _ = state.new_tab();
            FocusSurface::TabStrip
        }
        ExplorerAction::CloseActiveTab => {
            let id = state.tabs().active_tab_id();
            let _ = state.close_tab(id);
            FocusSurface::TabStrip
        }
        ExplorerAction::ActivateTab { tab_id } => {
            let _ = state.activate_tab(tab_id);
            FocusSurface::TabStrip
        }
        ExplorerAction::CloseTab { tab_id } => {
            let _ = state.close_tab(tab_id);
            FocusSurface::TabStrip
        }
        ExplorerAction::ReorderTab {
            tab_id,
            destination_index,
        } => {
            let _ = state.reorder_tab(tab_id, destination_index);
            FocusSurface::TabStrip
        }
        ExplorerAction::NextTab => {
            let _ = state.cycle_tab(1);
            FocusSurface::TabStrip
        }
        ExplorerAction::PreviousTab => {
            let _ = state.cycle_tab(-1);
            FocusSurface::TabStrip
        }
        ExplorerAction::OpenItem { .. }
        | ExplorerAction::OpenExtensionViewItem { .. }
        | ExplorerAction::OpenFocused
        | ExplorerAction::CreateFolder
        | ExplorerAction::CreateRemoteSymlink
        | ExplorerAction::CreateRemoteSymlinkToFolder { .. }
        | ExplorerAction::ShowRemoteBackgroundProperties
        | ExplorerAction::CreateNewItem { .. }
        | ExplorerAction::RecycleDeleteSelected
        | ExplorerAction::CreateShortcutSelected
        | ExplorerAction::CloseLockOwnersAndRetry
        | ExplorerAction::RetryLockedDelete
        | ExplorerAction::CopySelected
        | ExplorerAction::CutSelected
        | ExplorerAction::Paste
        | ExplorerAction::DownloadSelectedToDownloads
        | ExplorerAction::ShareSelected
        | ExplorerAction::PinSelectedToStart
        | ExplorerAction::ShowPropertiesSelected
        | ExplorerAction::CloseRemoteProperties
        | ExplorerAction::ToggleRemotePermission { .. }
        | ExplorerAction::ApplyRemoteProperties
        | ExplorerAction::UndoCurrentFolder
        | ExplorerAction::CompressSelectedToZip
        | ExplorerAction::AddSelectedToFavorites
        | ExplorerAction::AddSelectedToBookmarks
        | ExplorerAction::ToggleCurrentFolderBookmark { .. }
        | ExplorerAction::CopySelectedPaths
        | ExplorerAction::CancelOperation { .. } => FocusSurface::FileView,
        ExplorerAction::BeginFileDrag { x, y, button } => {
            let _ = state.begin_drag_candidate(x, y, button);
            FocusSurface::FileView
        }
        ExplorerAction::BeginContextItemGesture {
            item_id,
            x,
            y,
            extended_verbs,
        } => {
            let _ = state.begin_context_item_gesture(item_id, x, y, extended_verbs);
            state.focus(FocusSurface::FileView);
            FocusSurface::FileView
        }
        ExplorerAction::UpdateFileDrag { x, y } => {
            let _ = state.update_drag_pointer(x, y);
            FocusSurface::FileView
        }
        ExplorerAction::CancelFileDrag => {
            let _ = state.cancel_drag();
            FocusSurface::FileView
        }
        ExplorerAction::DropExternal {
            paths,
            destination_row,
            effect,
            right_button,
            allowed,
        } => {
            state.queue_external_drop(paths, destination_row, effect, right_button, allowed);
            FocusSurface::FileView
        }
        ExplorerAction::UpdateExternalDrag {
            destination_row,
            target,
            pointer_y,
            top,
            bottom,
            effect,
        } => {
            state.update_external_drag_target(
                destination_row,
                target,
                pointer_y,
                top,
                bottom,
                effect,
            );
            FocusSurface::FileView
        }
        ExplorerAction::ClearExternalDrag => {
            state.clear_external_drag();
            FocusSurface::FileView
        }
        ExplorerAction::ResolveRightDrop { effect } => {
            state.resolve_right_drop(effect);
            FocusSurface::FileView
        }
        ExplorerAction::ShowContextMenu {
            item_id,
            keyboard_invoked,
            ..
        } => {
            // Pointer item selection was committed from the stable mouse-down identity.
            // Reapplying a mouse-up closure identity here can select a stale/first row after
            // GPUI replaces the hit element during the selection re-render.
            if keyboard_invoked || item_id.is_none() {
                state.prepare_context_selection(item_id.as_ref());
            }
            state.focus(FocusSurface::FileView);
            FocusSurface::FileView
        }
        ExplorerAction::SelectItem { row_index } => {
            let _ = state.select_row(row_index);
            state.focus(FocusSurface::FileView);
            FocusSurface::FileView
        }
        ExplorerAction::SelectAdditionalItem { row_index } => {
            let _ = state.toggle_row(row_index);
            state.focus(FocusSurface::FileView);
            FocusSurface::FileView
        }
        ExplorerAction::SelectRange {
            row_index,
            additive,
        } => {
            let _ = state.select_row_range(row_index, additive);
            state.focus(FocusSurface::FileView);
            FocusSurface::FileView
        }
        ExplorerAction::FocusItem { row_index } => {
            let _ = state.focus_row(row_index);
            state.focus(FocusSurface::FileView);
            FocusSurface::FileView
        }
        ExplorerAction::SelectAllItems => {
            state.select_all_rows();
            state.focus(FocusSurface::FileView);
            FocusSurface::FileView
        }
        ExplorerAction::InvertSelection => {
            state.invert_selection();
            state.focus(FocusSurface::FileView);
            FocusSurface::FileView
        }
        ExplorerAction::ClearSelection => {
            state.clear_selection();
            state.focus(FocusSurface::FileView);
            FocusSurface::FileView
        }
        ExplorerAction::TypeAheadFileView { text } => {
            let _ = state.typeahead_file_view(&text, Instant::now());
            state.focus(FocusSurface::FileView);
            FocusSurface::FileView
        }
        ExplorerAction::ClearFileViewTypeAhead => {
            state.clear_file_view_typeahead();
            FocusSurface::FileView
        }
        ExplorerAction::BeginRenameFocused => {
            let _ = state.begin_focused_inline_rename();
            state.focus(FocusSurface::FileView);
            FocusSurface::FileView
        }
        ExplorerAction::CommitInlineRename => FocusSurface::FileView,
        ExplorerAction::CancelInlineRename => {
            state.cancel_inline_rename();
            FocusSurface::FileView
        }
        ExplorerAction::RequestPermanentDelete => FocusSurface::FileView,
        ExplorerAction::ConfirmPermanentDelete => FocusSurface::FileView,
        ExplorerAction::CancelPermanentDelete => {
            let _ = state.cancel_permanent_delete_confirmation();
            FocusSurface::FileView
        }
        ExplorerAction::MovePermanentDeleteDialogFocus { direction } => {
            let _ = state.move_permanent_delete_confirmation_focus(direction);
            FocusSurface::FileView
        }
        ExplorerAction::SetPermanentDeleteDialogFocus { target } => {
            let _ = state.set_permanent_delete_confirmation_focus(target);
            FocusSurface::FileView
        }
        ExplorerAction::CancelLockedDeleteRecovery => {
            let _ = state.cancel_lock_recovery();
            FocusSurface::FileView
        }
        ExplorerAction::MoveLockedDeleteDialogFocus { direction } => {
            let _ = state.move_lock_recovery_focus(direction);
            FocusSurface::FileView
        }
        ExplorerAction::RestoreSelected => FocusSurface::FileView,
        ExplorerAction::EmptyRecycleBin => FocusSurface::FileView,
        ExplorerAction::BeginMarquee { x, y, additive } => {
            let _ = state.begin_marquee(x, y, additive);
            state.focus(FocusSurface::FileView);
            FocusSurface::FileView
        }
        ExplorerAction::UpdateMarquee {
            x,
            y,
            scroll_y,
            viewport_width,
        } => {
            let _ = state.update_marquee(x, y, scroll_y, viewport_width, LayoutTokens::WINDOWS_11);
            FocusSurface::FileView
        }
        ExplorerAction::EndMarquee => {
            let _ = state.end_marquee();
            FocusSurface::FileView
        }
        ExplorerAction::ToggleNewMenu => {
            state.toggle_new_menu();
            FocusSurface::CommandBar
        }
        ExplorerAction::CloseNewMenu => {
            state.close_new_menu();
            FocusSurface::CommandBar
        }
        ExplorerAction::MoveNewMenuFocus { direction } => {
            state.move_new_menu_focus(direction);
            FocusSurface::CommandBar
        }
        ExplorerAction::ToggleViewMenu => {
            state.toggle_view_menu();
            FocusSurface::CommandBar
        }
        ExplorerAction::ToggleSortMenu => {
            state.toggle_sort_menu();
            FocusSurface::CommandBar
        }
        ExplorerAction::CloseSortMenu => {
            state.close_sort_menu();
            FocusSurface::CommandBar
        }
        ExplorerAction::MoveSortMenuFocus { direction } => {
            state.move_sort_menu_focus(direction);
            FocusSurface::CommandBar
        }
        ExplorerAction::SetSortMenuFocus { index } => {
            let _ = state.set_sort_menu_focus(index);
            FocusSurface::CommandBar
        }
        ExplorerAction::ToggleMoreMenu => {
            state.toggle_more_menu();
            FocusSurface::CommandBar
        }
        ExplorerAction::CloseMoreMenu => {
            state.close_more_menu();
            FocusSurface::CommandBar
        }
        ExplorerAction::MoveMoreMenuFocus { direction } => {
            state.move_more_menu_focus(direction);
            FocusSurface::CommandBar
        }
        ExplorerAction::SetMoreMenuFocus { index } => {
            let _ = state.set_more_menu_focus(index);
            FocusSurface::CommandBar
        }
        ExplorerAction::ToggleExtensionsMenu => {
            state.toggle_extensions_menu();
            FocusSurface::CommandBar
        }
        ExplorerAction::CloseExtensionsMenu => {
            state.close_extensions_menu();
            FocusSurface::CommandBar
        }
        ExplorerAction::RefreshTortoiseGitStatus => FocusSurface::CommandBar,
        ExplorerAction::OpenFolderOptions => {
            state.open_folder_options();
            FocusSurface::CommandBar
        }
        ExplorerAction::ActivateBookmark { .. }
        | ExplorerAction::OpenBookmarkInNewTab { .. }
        | ExplorerAction::OpenBookmarkContextMenu { .. }
        | ExplorerAction::CloseBookmarkContextMenu
        | ExplorerAction::RequestRemoveBookmark { .. }
        | ExplorerAction::OpenBookmarkToolbarContextMenu { .. }
        | ExplorerAction::CloseBookmarkToolbarContextMenu
        | ExplorerAction::AddPathBookmark { .. }
        | ExplorerAction::CloseRemoteContextMenu
        | ExplorerAction::AddLuaBookmark
        | ExplorerAction::EditBookmark { .. }
        | ExplorerAction::SaveBookmarkEditor
        | ExplorerAction::CancelBookmarkEditor
        | ExplorerAction::SelectBookmarkDestination { .. }
        | ExplorerAction::AddBookmarkFolder { .. }
        | ExplorerAction::EditBookmarkFolder { .. }
        | ExplorerAction::SaveBookmarkFolderEditor
        | ExplorerAction::CancelBookmarkFolderEditor
        | ExplorerAction::RemoveBookmarkFolder { .. }
        | ExplorerAction::ConfirmRemoveBookmarkFolder
        | ExplorerAction::CancelRemoveBookmarkFolder
        | ExplorerAction::RemoveEditingBookmark
        | ExplorerAction::ToggleBookmarkManager
        | ExplorerAction::ImportBookmarksFromClipboard
        | ExplorerAction::BackupBookmarksToClipboard
        | ExplorerAction::ToggleBookmarkOverflow
        | ExplorerAction::ToggleBookmarkFolderMenu { .. }
        | ExplorerAction::RemoveBookmark { .. }
        | ExplorerAction::MoveBookmark { .. }
        | ExplorerAction::MoveBookmarkToFolder { .. } => FocusSurface::CommandBar,
        ExplorerAction::OpenAboutDialog => {
            state.open_about_dialog();
            FocusSurface::CommandBar
        }
        ExplorerAction::CloseAboutDialog => {
            state.close_about_dialog();
            FocusSurface::CommandBar
        }
        ExplorerAction::CloseFolderOptions => {
            state.close_folder_options();
            FocusSurface::CommandBar
        }
        ExplorerAction::SetFolderOptionsPage(page) => {
            state.set_folder_options_page(page);
            FocusSurface::CommandBar
        }
        ExplorerAction::ToggleFolderOptionExtension { index } => {
            state.toggle_folder_option_extension(index);
            FocusSurface::CommandBar
        }
        ExplorerAction::OpenExtensionAuthorWebsite { .. } => FocusSurface::CommandBar,
        ExplorerAction::OpenExtensionCommunityWebsite { .. } => FocusSurface::CommandBar,
        ExplorerAction::InvokeExtensionCommand { contribution_id } => {
            state.open_extension_command_panel(&contribution_id);
            FocusSurface::CommandBar
        }
        ExplorerAction::CloseExtensionCommandPanel => {
            state.close_extension_command_panel();
            FocusSurface::CommandBar
        }
        ExplorerAction::RunBulkFolderPreset { .. } | ExplorerAction::RunExifRenamePreset { .. } => {
            state.close_extensions_menu();
            FocusSurface::CommandBar
        }
        ExplorerAction::ToggleFolderOptionItemCheckBoxes => {
            state.update_folder_options(|settings| {
                settings.item_check_boxes = !settings.item_check_boxes;
            });
            FocusSurface::CommandBar
        }
        ExplorerAction::ToggleFolderOptionFileNameExtensions => {
            state.update_folder_options(|settings| {
                settings.file_name_extensions = !settings.file_name_extensions;
            });
            FocusSurface::CommandBar
        }
        ExplorerAction::ToggleFolderOptionHiddenItems => {
            state.update_folder_options(|settings| {
                settings.hidden_items = !settings.hidden_items;
            });
            FocusSurface::CommandBar
        }
        ExplorerAction::ToggleFolderOptionCompactView => {
            state.update_folder_options(|settings| {
                settings.compact_view = !settings.compact_view;
            });
            FocusSurface::CommandBar
        }
        ExplorerAction::ToggleFolderOptionAlwaysShowIcons => {
            state.update_folder_options(|settings| {
                settings.always_show_icons = !settings.always_show_icons;
            });
            FocusSurface::CommandBar
        }
        ExplorerAction::SetFolderOptionIconCacheMemoryMb(value) => {
            state.update_folder_options(|settings| {
                settings.icon_cache_memory_mb =
                    explorer_model::normalized_icon_cache_memory_mb(value);
                settings.cache_budgets.icon_memory_mb = u32::from(value);
            });
            FocusSurface::CommandBar
        }
        ExplorerAction::SetFolderOptionThumbnailCacheMemoryMb(value) => {
            state.update_folder_options(|settings| {
                settings.thumbnail_cache_memory_mb =
                    explorer_model::normalized_thumbnail_cache_memory_mb(value);
                settings.cache_budgets.thumbnail_memory_mb = u32::from(value);
            });
            FocusSurface::CommandBar
        }
        ExplorerAction::SetFolderOptionMftCacheMemoryMb(value) => {
            state.update_folder_options(|settings| {
                settings.mft_folder_cache_memory_mb =
                    explorer_model::normalized_mft_folder_cache_memory_mb(value);
                settings.cache_budgets.mft_lru_mb = u32::from(value);
            });
            FocusSurface::CommandBar
        }
        ExplorerAction::SetFolderOptionCacheBudgets(budgets) => {
            state.update_folder_options(|settings| {
                settings.cache_budgets = budgets.normalized();
                settings.icon_cache_memory_mb = explorer_model::normalized_icon_cache_memory_mb(
                    settings
                        .cache_budgets
                        .icon_memory_mb
                        .min(u32::from(u16::MAX)) as u16,
                );
                settings.thumbnail_cache_memory_mb =
                    explorer_model::normalized_thumbnail_cache_memory_mb(
                        settings
                            .cache_budgets
                            .thumbnail_memory_mb
                            .min(u32::from(u16::MAX)) as u16,
                    );
                settings.mft_folder_cache_memory_mb =
                    explorer_model::normalized_mft_folder_cache_memory_mb(
                        settings.cache_budgets.mft_lru_mb.min(u32::from(u16::MAX)) as u16,
                    );
            });
            FocusSurface::CommandBar
        }
        ExplorerAction::ClearThumbnailCache => FocusSurface::CommandBar,
        ExplorerAction::ToggleFolderOptionDetailsPane => {
            state.update_folder_options(|settings| {
                settings.details_pane = !settings.details_pane;
                if settings.details_pane {
                    settings.preview_pane = false;
                }
            });
            FocusSurface::CommandBar
        }
        ExplorerAction::ToggleFolderOptionPreviewPane => {
            state.update_folder_options(|settings| {
                settings.preview_pane = !settings.preview_pane;
                if settings.preview_pane {
                    settings.details_pane = false;
                }
            });
            FocusSurface::CommandBar
        }
        ExplorerAction::ToggleRestorePreviousSession => {
            state.toggle_restore_previous_session();
            FocusSurface::CommandBar
        }
        ExplorerAction::ResetSavedSession => {
            state.begin_session_reset_confirmation(explorer_model::SessionResetScope::Session);
            FocusSurface::CommandBar
        }
        ExplorerAction::ResetSavedViewSettings => {
            state.begin_session_reset_confirmation(explorer_model::SessionResetScope::ViewSettings);
            FocusSurface::CommandBar
        }
        ExplorerAction::ResetAllSavedExplorerState => {
            state.begin_session_reset_confirmation(
                explorer_model::SessionResetScope::AllRoadmapState,
            );
            FocusSurface::CommandBar
        }
        ExplorerAction::ConfirmSavedStateReset => {
            state.confirm_session_reset();
            FocusSurface::CommandBar
        }
        ExplorerAction::CancelSavedStateReset => {
            state.cancel_session_reset_confirmation();
            FocusSurface::CommandBar
        }
        ExplorerAction::RetrySavedStateReset => {
            state.retry_session_reset();
            FocusSurface::CommandBar
        }
        ExplorerAction::RetryExtensionBroker => {
            state.set_broker_health(crate::state::BrokerUiHealth::Retrying);
            FocusSurface::CommandBar
        }
        ExplorerAction::ResetFolderOptions => {
            state.reset_folder_options();
            FocusSurface::CommandBar
        }
        ExplorerAction::ApplyFolderOptions => {
            state.apply_folder_options();
            FocusSurface::CommandBar
        }
        ExplorerAction::ConfirmFolderOptions => {
            state.confirm_folder_options();
            FocusSurface::FileView
        }
        ExplorerAction::CloseViewMenu => {
            state.close_view_menu();
            FocusSurface::CommandBar
        }
        ExplorerAction::MoveViewMenuFocus { direction } => {
            state.move_view_menu_focus(direction);
            FocusSurface::CommandBar
        }
        ExplorerAction::SetViewMenuFocus { index } => {
            let _ = state.set_view_menu_focus(index);
            FocusSurface::CommandBar
        }
        ExplorerAction::ToggleViewShowSubmenu => {
            state.toggle_view_show_submenu();
            FocusSurface::CommandBar
        }
        ExplorerAction::SetViewMode(mode) => {
            state.set_view_mode(mode);
            FocusSurface::FileView
        }
        ExplorerAction::SetExtensionView { view_id } => {
            state.set_extension_view(view_id);
            FocusSurface::FileView
        }
        ExplorerAction::ZoomView { direction } => {
            state.zoom_view(direction);
            FocusSurface::FileView
        }
        ExplorerAction::SetColumnId(column) => {
            state.set_sort_column(column);
            FocusSurface::FileView
        }
        ExplorerAction::SetSortDirection(direction) => {
            state.set_sort_direction(direction);
            FocusSurface::FileView
        }
        ExplorerAction::SetDetailsColumnWidth { column, width } => {
            state.set_details_column_width(column, width);
            FocusSurface::FileView
        }
        ExplorerAction::MoveDetailsColumn { column, before } => {
            state.move_details_column_before(column, before);
            FocusSurface::FileView
        }
        ExplorerAction::UpdateDetailsColumnDragPreview {
            column,
            target,
            pointer_x,
            target_left,
            target_right,
        } => {
            state.update_details_column_drag_preview(
                column,
                target,
                pointer_x,
                target_left,
                target_right,
            );
            FocusSurface::FileView
        }
        ExplorerAction::CommitDetailsColumnDrag => {
            state.commit_details_column_drag();
            FocusSurface::FileView
        }
        ExplorerAction::CancelDetailsColumnDrag => {
            state.cancel_details_column_drag();
            FocusSurface::FileView
        }
        ExplorerAction::AutoSizeDetailsColumn { column } => {
            state.auto_size_details_column(column);
            FocusSurface::FileView
        }
        ExplorerAction::OpenDetailsColumnMenu { column } => {
            state.open_details_column_menu(column);
            FocusSurface::FileView
        }
        ExplorerAction::CloseDetailsColumnMenu => {
            state.close_details_column_menu();
            FocusSurface::FileView
        }
        ExplorerAction::OpenDetailsFilterMenu { column } => {
            state.open_details_filter_menu(column);
            FocusSurface::FileView
        }
        ExplorerAction::CloseDetailsFilterMenu => {
            state.close_details_filter_menu();
            FocusSurface::FileView
        }
        ExplorerAction::ToggleDetailsFilter { column, key } => {
            state.toggle_details_filter(column, key);
            FocusSurface::FileView
        }
        ExplorerAction::ClearDetailsFilter { column } => {
            state.clear_details_filter(column);
            FocusSurface::FileView
        }
        ExplorerAction::ToggleDetailsColumn(column) => {
            state.toggle_details_column(column);
            FocusSurface::FileView
        }
        ExplorerAction::ToggleFolderSizeProportionalBar => FocusSurface::FileView,
        ExplorerAction::ToggleCodeLinesDetail => FocusSurface::FileView,
        ExplorerAction::AutoSizeAllDetailsColumns => {
            state.auto_size_all_details_columns();
            FocusSurface::FileView
        }
        ExplorerAction::BeginDetailsColumnResize { column, pointer_x } => {
            state.begin_details_column_resize(column, pointer_x);
            FocusSurface::FileView
        }
        ExplorerAction::UpdateDetailsColumnResize { pointer_x } => {
            state.update_details_column_resize(pointer_x);
            FocusSurface::FileView
        }
        ExplorerAction::EndDetailsColumnResize => {
            state.end_details_column_resize();
            FocusSurface::FileView
        }
        ExplorerAction::BeginSidePaneResize { pointer_x } => {
            let _ = state.begin_side_pane_resize(pointer_x);
            FocusSurface::FileView
        }
        ExplorerAction::UpdateSidePaneResize { pointer_x } => {
            let _ = state.update_side_pane_resize(pointer_x);
            FocusSurface::FileView
        }
        ExplorerAction::EndSidePaneResize => {
            state.end_side_pane_resize();
            FocusSurface::FileView
        }
        ExplorerAction::UpdatePreviewHostBoundary { .. } => state.focused_surface(),
        ExplorerAction::ResetSidePaneWidth => {
            state.reset_side_pane_width();
            FocusSurface::FileView
        }
        ExplorerAction::AdjustSidePaneWidth { direction } => {
            state.adjust_side_pane_width(direction);
            FocusSurface::FileView
        }
        ExplorerAction::BeginScrollbarDrag {
            kind,
            grab_offset_y,
        } => {
            let _ = state.begin_scrollbar_drag(kind, grab_offset_y);
            FocusSurface::FileView
        }
        ExplorerAction::UpdateScrollbarDrag { .. } => FocusSurface::FileView,
        ExplorerAction::EndScrollbarDrag { reason } => {
            let _ = state.end_scrollbar_drag(reason);
            FocusSurface::FileView
        }
        ExplorerAction::ToggleDetailsPane => {
            state.toggle_details_pane();
            state.close_view_menu();
            FocusSurface::CommandBar
        }
        ExplorerAction::TogglePreviewPane => {
            state.toggle_preview_pane();
            state.close_view_menu();
            FocusSurface::CommandBar
        }
        ExplorerAction::ToggleItemCheckBoxes => {
            state.toggle_item_check_boxes();
            state.close_view_menu();
            FocusSurface::CommandBar
        }
        ExplorerAction::ToggleFileNameExtensions => {
            state.toggle_file_name_extensions();
            state.close_view_menu();
            FocusSurface::CommandBar
        }
        ExplorerAction::ToggleHiddenItems => {
            state.toggle_hidden_items();
            state.close_view_menu();
            FocusSurface::CommandBar
        }
        ExplorerAction::ToggleCompactView => {
            state.toggle_compact_view();
            state.close_view_menu();
            FocusSurface::CommandBar
        }
        ExplorerAction::ToggleTheme => {
            let next = match state.current_theme() {
                ThemeMode::Light => ThemeMode::Dark,
                ThemeMode::Dark => ThemeMode::Light,
            };
            state.set_theme(next);
            state.focused_surface()
        }
        ExplorerAction::CloseWindow => {
            state.request_close();
            FocusSurface::WindowChrome
        }
        ExplorerAction::ResizeNavigationPane { width } => {
            let layout = LayoutTokens::WINDOWS_11;
            let clamped = width.value().clamp(
                layout.navigation_pane_min_width.value(),
                layout.navigation_pane_max_width.value(),
            );
            state.set_navigation_pane_width(LogicalPx::new(clamped));
            FocusSurface::NavigationPane
        }
        ExplorerAction::BeginNavigationPaneResize { pointer_x } => {
            let _ = state.begin_divider_drag(pointer_x);
            FocusSurface::NavigationPane
        }
        ExplorerAction::UpdateNavigationPaneResize { pointer_x } => {
            let _ = state.update_divider_drag(pointer_x);
            FocusSurface::NavigationPane
        }
        ExplorerAction::EndNavigationPaneResize => {
            let _ = state.finish_divider_drag();
            FocusSurface::NavigationPane
        }
        ExplorerAction::ResetNavigationPaneWidth => {
            state.reset_divider();
            FocusSurface::NavigationPane
        }
        ExplorerAction::AdjustNavigationPaneWidth { direction } => {
            state.adjust_divider(direction);
            FocusSurface::NavigationPane
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{
        ActionOutcome, ActionSource, BindingConflict, DEFAULT_BINDINGS, ExplorerAction, KeyBinding,
        action_dispatches_at_info, dispatch_action, validate_bindings,
    };
    use crate::{
        focus::FocusSurface,
        layout::{LayoutTokens, LogicalPx},
        state::AppViewState,
        theme::ThemeMode,
    };

    #[test]
    fn only_file_drag_pointer_updates_bypass_info_dispatch_logging() {
        let pointer_update = ExplorerAction::UpdateFileDrag { x: 1.0, y: 2.0 };
        assert!(!action_dispatches_at_info(&pointer_update));
        assert!(action_dispatches_at_info(&ExplorerAction::BeginFileDrag {
            x: 1.0,
            y: 2.0,
            button: explorer_model::DragButton::Left,
        }));
        assert!(action_dispatches_at_info(&ExplorerAction::CancelFileDrag));
        assert!(action_dispatches_at_info(&ExplorerAction::Refresh));

        let mut state = writable_state();
        let trace = dispatch_action(&mut state, pointer_update, ActionSource::Mouse);
        assert_eq!(trace.outcome, ActionOutcome::Disabled);
    }

    fn writable_state() -> AppViewState {
        let mut state = AppViewState::with_initial_location(explorer_model::HistoryEntry::new(
            explorer_model::LocationDescriptor::file_system(r"C:\fixture"),
            "fixture",
        ));
        let command = state
            .begin_active_location_load()
            .expect("load writable state");
        let context = command.context().expect("load context").clone();
        let _ = state.apply_service_event(explorer_model::ExplorerEvent::LocationResolved {
            context: context.clone(),
            metadata: explorer_model::LocationMetadata {
                descriptor: explorer_model::LocationDescriptor::file_system(r"C:\fixture"),
                display_title: "fixture".to_owned(),
                can_go_up: true,
                can_write: true,
            },
        });
        let _ =
            state.apply_service_event(explorer_model::ExplorerEvent::DirectoryFinished { context });
        state
    }

    #[test]
    fn command_popups_are_exclusive_focusable_and_activation_closes_once() {
        let mut state = writable_state();
        state.focus(FocusSurface::FileView);

        let opened = dispatch_action(
            &mut state,
            ExplorerAction::ToggleNewMenu,
            ActionSource::Keyboard,
        );
        assert_eq!(opened.outcome, ActionOutcome::Handled);
        assert!(state.new_menu_open());
        assert_eq!(state.focused_surface(), FocusSurface::CommandBar);
        assert_eq!(state.previous_focus(), Some(FocusSurface::FileView));

        dispatch_action(
            &mut state,
            ExplorerAction::ToggleSortMenu,
            ActionSource::Mouse,
        );
        assert!(!state.new_menu_open());
        assert!(state.sort_menu_open());
        assert_eq!(state.focused_surface(), FocusSurface::CommandBar);

        let first = dispatch_action(
            &mut state,
            ExplorerAction::SetSortDirection(explorer_model::SortDirection::Descending),
            ActionSource::Keyboard,
        );
        assert_eq!(first.outcome, ActionOutcome::Handled);
        assert!(!state.sort_menu_open());
        assert_eq!(
            state.view_settings().sort.direction,
            explorer_model::SortDirection::Descending
        );
        assert_eq!(state.focused_surface(), FocusSurface::FileView);

        let second = dispatch_action(
            &mut state,
            ExplorerAction::SetSortDirection(explorer_model::SortDirection::Descending),
            ActionSource::Keyboard,
        );
        assert_eq!(second.outcome, ActionOutcome::Handled);
        assert!(!state.sort_menu_open());
        assert_eq!(
            state.view_settings().sort.direction,
            explorer_model::SortDirection::Descending
        );
    }

    #[test]
    fn details_column_chooser_repeats_toggles_without_losing_file_view_ownership() {
        let mut state = writable_state();
        state.focus(FocusSurface::FileView);

        let opened = dispatch_action(
            &mut state,
            ExplorerAction::OpenDetailsColumnMenu {
                column: explorer_model::ColumnId::Size,
            },
            ActionSource::Mouse,
        );
        assert_eq!(opened.handled_surface, FocusSurface::FileView);
        assert_eq!(state.focused_surface(), FocusSurface::FileView);
        assert_eq!(
            state.details_column_menu(),
            Some(explorer_model::ColumnId::Size)
        );

        let stray_resize_end = dispatch_action(
            &mut state,
            ExplorerAction::EndDetailsColumnResize,
            ActionSource::Mouse,
        );
        assert_eq!(stray_resize_end.outcome, ActionOutcome::Disabled);
        assert_eq!(
            state.details_column_menu(),
            Some(explorer_model::ColumnId::Size),
            "an inactive separator mouse-up must not dismiss the chooser"
        );

        let initially_visible = state.details_column_visible(explorer_model::ColumnId::Size);
        for expected in [
            !initially_visible,
            initially_visible,
            !initially_visible,
            initially_visible,
        ] {
            let toggled = dispatch_action(
                &mut state,
                ExplorerAction::ToggleDetailsColumn(explorer_model::ColumnId::Size),
                ActionSource::Mouse,
            );
            assert_eq!(toggled.handled_surface, FocusSurface::FileView);
            assert_eq!(
                state.details_column_visible(explorer_model::ColumnId::Size),
                expected
            );
            assert_eq!(
                state.details_column_menu(),
                Some(explorer_model::ColumnId::Size),
                "hiding the originating header must not dismiss its chooser"
            );
        }

        let name = dispatch_action(
            &mut state,
            ExplorerAction::ToggleDetailsColumn(explorer_model::ColumnId::Name),
            ActionSource::Mouse,
        );
        assert_eq!(name.outcome, ActionOutcome::Disabled);
        assert!(state.details_column_visible(explorer_model::ColumnId::Name));
        assert!(state.details_column_menu().is_some());

        dispatch_action(
            &mut state,
            ExplorerAction::CloseDetailsColumnMenu,
            ActionSource::Keyboard,
        );
        assert!(state.details_column_menu().is_none());
    }

    #[test]
    fn details_filter_selection_survives_global_pointer_terminal_then_closes_on_focus_change() {
        let mut state = writable_state();
        state.open_details_filter_menu(explorer_model::ColumnId::Name);

        dispatch_action(
            &mut state,
            ExplorerAction::EndDetailsColumnResize,
            ActionSource::Mouse,
        );
        assert!(state.details_filter_menu().is_none());

        dispatch_action(
            &mut state,
            ExplorerAction::ToggleDetailsFilter {
                column: explorer_model::ColumnId::Name,
                key: "name:a-h".to_owned(),
            },
            ActionSource::Mouse,
        );
        assert_eq!(
            state.details_filter_menu(),
            Some(explorer_model::ColumnId::Name)
        );

        dispatch_action(
            &mut state,
            ExplorerAction::FocusAddress,
            ActionSource::Mouse,
        );
        assert!(state.details_filter_menu().is_none());
    }

    #[test]
    fn view_menu_actions_mutate_real_per_tab_settings() {
        let mut state = AppViewState::default();
        assert!(!state.view_menu_open());
        dispatch_action(
            &mut state,
            ExplorerAction::ToggleViewMenu,
            ActionSource::Mouse,
        );
        assert!(state.view_menu_open());
        dispatch_action(
            &mut state,
            ExplorerAction::SetViewMode(explorer_model::ViewMode::ExtraLargeIcons),
            ActionSource::Mouse,
        );
        assert_eq!(
            state.view_settings().mode,
            explorer_model::ViewMode::ExtraLargeIcons
        );
        assert!(!state.view_menu_open());
        dispatch_action(
            &mut state,
            ExplorerAction::ToggleHiddenItems,
            ActionSource::Mouse,
        );
        assert!(!state.view_menu_open());
        dispatch_action(
            &mut state,
            ExplorerAction::ToggleViewMenu,
            ActionSource::Mouse,
        );
        dispatch_action(
            &mut state,
            ExplorerAction::ToggleCompactView,
            ActionSource::Mouse,
        );
        assert!(!state.view_menu_open());
        assert!(state.view_settings().hidden_items);
        assert!(state.view_settings().compact_view);
    }

    #[test]
    fn more_menu_is_mutually_exclusive_and_commands_close_it() {
        let mut state = AppViewState::default();
        dispatch_action(
            &mut state,
            ExplorerAction::ToggleMoreMenu,
            ActionSource::Mouse,
        );
        assert!(state.more_menu_open());
        dispatch_action(
            &mut state,
            ExplorerAction::ToggleViewMenu,
            ActionSource::Mouse,
        );
        assert!(!state.more_menu_open());
        assert!(state.view_menu_open());
        dispatch_action(
            &mut state,
            ExplorerAction::ToggleMoreMenu,
            ActionSource::Mouse,
        );
        assert!(state.more_menu_open());
        assert!(!state.view_menu_open());
        dispatch_action(
            &mut state,
            ExplorerAction::MoveMoreMenuFocus { direction: i8::MAX },
            ActionSource::Keyboard,
        );
        assert_eq!(state.more_menu_index(), 9);
        assert!(state.more_menu_open());
        dispatch_action(
            &mut state,
            ExplorerAction::MoveMoreMenuFocus { direction: -1 },
            ActionSource::Keyboard,
        );
        assert_eq!(state.more_menu_index(), 8);
        dispatch_action(
            &mut state,
            ExplorerAction::ToggleTheme,
            ActionSource::Keyboard,
        );
        assert!(!state.more_menu_open());
    }

    #[test]
    fn extensions_menu_is_mutually_exclusive_and_refresh_requires_tortoise_git() {
        let mut state = AppViewState::default();
        assert_eq!(
            dispatch_action(
                &mut state,
                ExplorerAction::RefreshTortoiseGitStatus,
                ActionSource::Mouse,
            )
            .outcome,
            ActionOutcome::Disabled
        );

        dispatch_action(
            &mut state,
            ExplorerAction::ToggleMoreMenu,
            ActionSource::Mouse,
        );
        dispatch_action(
            &mut state,
            ExplorerAction::ToggleExtensionsMenu,
            ActionSource::Mouse,
        );
        assert!(state.extensions_menu_open());
        assert!(!state.more_menu_open());

        state.set_tortoise_git_available(true);
        assert_eq!(
            dispatch_action(
                &mut state,
                ExplorerAction::RefreshTortoiseGitStatus,
                ActionSource::Keyboard,
            )
            .outcome,
            ActionOutcome::Handled
        );
        assert!(!state.extensions_menu_open());

        dispatch_action(
            &mut state,
            ExplorerAction::ToggleExtensionsMenu,
            ActionSource::Mouse,
        );
        dispatch_action(
            &mut state,
            ExplorerAction::ToggleViewMenu,
            ActionSource::Mouse,
        );
        assert!(!state.extensions_menu_open());
        assert!(state.view_menu_open());
    }

    #[test]
    fn extension_commands_open_a_panel_and_cancel_before_execution() {
        let mut state = AppViewState::default();
        dispatch_action(
            &mut state,
            ExplorerAction::ToggleExtensionsMenu,
            ActionSource::Mouse,
        );

        dispatch_action(
            &mut state,
            ExplorerAction::InvokeExtensionCommand {
                contribution_id: "lua-bulk-folder:button".to_owned(),
            },
            ActionSource::Mouse,
        );
        assert!(state.extensions_menu_open());
        assert_eq!(
            state.extension_command_panel(),
            Some(crate::extension_commands::ExtensionCommandPanel::BulkFolder)
        );

        dispatch_action(
            &mut state,
            ExplorerAction::CloseExtensionCommandPanel,
            ActionSource::Keyboard,
        );
        assert!(state.extensions_menu_open());
        assert_eq!(state.extension_command_panel(), None);

        dispatch_action(
            &mut state,
            ExplorerAction::InvokeExtensionCommand {
                contribution_id: "rust-exif-rename:button".to_owned(),
            },
            ActionSource::Mouse,
        );
        assert_eq!(
            state.extension_command_panel(),
            Some(crate::extension_commands::ExtensionCommandPanel::ExifRename)
        );
        dispatch_action(
            &mut state,
            ExplorerAction::RunExifRenamePreset {
                preset: crate::extension_commands::ExifRenamePreset::DateTime,
            },
            ActionSource::Mouse,
        );
        assert!(!state.extensions_menu_open());
        assert_eq!(state.extension_command_panel(), None);
    }

    #[test]
    fn breadcrumb_retry_replaces_the_failed_menu_session() {
        let mut state = AppViewState::with_initial_location(explorer_model::HistoryEntry::new(
            explorer_model::LocationDescriptor::file_system(r"D:\fixture"),
            "fixture",
        ));
        let segment_id = explorer_model::BreadcrumbSegmentId(0);
        dispatch_action(
            &mut state,
            ExplorerAction::OpenBreadcrumbChildren { segment_id },
            ActionSource::Mouse,
        );
        assert!(matches!(
            state.tabs().active_tab().view.address.mode,
            explorer_model::AddressBarMode::EnumeratingMenu { generation: 1, .. }
        ));
        dispatch_action(
            &mut state,
            ExplorerAction::RetryBreadcrumbChildren { segment_id },
            ActionSource::Keyboard,
        );
        assert!(matches!(
            state.tabs().active_tab().view.address.mode,
            explorer_model::AddressBarMode::EnumeratingMenu { generation: 2, .. }
        ));
        assert!(state.tabs().active_tab().view.address.menu_loading);
    }

    #[test]
    fn breadcrumb_overflow_action_toggles_without_starting_shell_enumeration() {
        let mut state = AppViewState::default();
        for expected in [true, false] {
            dispatch_action(
                &mut state,
                ExplorerAction::ToggleBreadcrumbOverflow,
                ActionSource::Keyboard,
            );
            let address = &state.tabs().active_tab().view.address;
            assert_eq!(address.overflow_open, expected);
            assert!(matches!(
                address.mode,
                explorer_model::AddressBarMode::Browsing
            ));
        }
    }

    #[test]
    fn breadcrumb_keyboard_actions_roam_segments_and_loaded_menu_without_editing() {
        let mut state = AppViewState::with_initial_location(explorer_model::HistoryEntry::new(
            explorer_model::LocationDescriptor::file_system(r"D:\fixture\nested"),
            "nested",
        ));
        state.focus(FocusSurface::AddressBar);
        dispatch_action(
            &mut state,
            ExplorerAction::MoveBreadcrumbSegmentFocus { direction: -127 },
            ActionSource::Keyboard,
        );
        assert_eq!(
            state
                .tabs()
                .active_tab()
                .view
                .address
                .keyboard_segment_index,
            Some(0)
        );
        let segment_id = explorer_model::BreadcrumbSegmentId(0);
        dispatch_action(
            &mut state,
            ExplorerAction::OpenBreadcrumbChildren { segment_id },
            ActionSource::Keyboard,
        );
        let explorer_model::AddressBarMode::EnumeratingMenu { generation, .. } =
            state.tabs().active_tab().view.address.mode
        else {
            panic!("menu did not open");
        };
        let command = state
            .begin_child_container_request()
            .expect("child request");
        let context = command.context().expect("context").clone();
        assert_eq!(
            state.apply_service_event(explorer_model::ExplorerEvent::ChildContainersBatch {
                context: context.clone(),
                segment_id,
                menu_generation: generation,
                children: vec![
                    explorer_model::BreadcrumbMenuItem {
                        display_name: "Alpha".to_owned(),
                        location: explorer_model::LocationDescriptor::file_system(r"D:\Alpha"),
                    },
                    explorer_model::BreadcrumbMenuItem {
                        display_name: "Beta".to_owned(),
                        location: explorer_model::LocationDescriptor::file_system(r"D:\Beta"),
                    },
                ],
            }),
            explorer_model::WindowEventOutcome::Applied
        );
        assert_eq!(
            state.apply_service_event(explorer_model::ExplorerEvent::ChildContainersFinished {
                context,
                segment_id,
                menu_generation: generation,
                outcome: explorer_model::BreadcrumbTerminal::Finished,
            }),
            explorer_model::WindowEventOutcome::Applied
        );
        dispatch_action(
            &mut state,
            ExplorerAction::MoveBreadcrumbMenuFocus {
                movement: explorer_model::MenuFocusMovement::Next,
            },
            ActionSource::Keyboard,
        );
        assert_eq!(
            state.focused_breadcrumb_menu_location(),
            Some(explorer_model::LocationDescriptor::file_system(r"D:\Beta"))
        );
        dispatch_action(
            &mut state,
            ExplorerAction::SetBreadcrumbMenuFocus { index: 0 },
            ActionSource::Mouse,
        );
        assert_eq!(
            state.focused_breadcrumb_menu_location(),
            Some(explorer_model::LocationDescriptor::file_system(r"D:\Alpha"))
        );
        dispatch_action(
            &mut state,
            ExplorerAction::SetBreadcrumbMenuFocus { index: 99 },
            ActionSource::Mouse,
        );
        assert_eq!(
            state.focused_breadcrumb_menu_location(),
            Some(explorer_model::LocationDescriptor::file_system(r"D:\Alpha"))
        );
        dispatch_action(
            &mut state,
            ExplorerAction::TypeAheadBreadcrumbMenu {
                text: "a".to_owned(),
            },
            ActionSource::Keyboard,
        );
        assert_eq!(
            state.focused_breadcrumb_menu_location(),
            Some(explorer_model::LocationDescriptor::file_system(r"D:\Alpha"))
        );
        dispatch_action(
            &mut state,
            ExplorerAction::CloseBreadcrumbMenu,
            ActionSource::Mouse,
        );
        dispatch_action(
            &mut state,
            ExplorerAction::SetBreadcrumbMenuFocus { index: 1 },
            ActionSource::Mouse,
        );
        assert_eq!(state.focused_breadcrumb_menu_location(), None);
    }

    #[test]
    fn default_bindings_have_no_same_scope_conflicts() {
        validate_bindings(&DEFAULT_BINDINGS).expect("unique bindings");
        let duplicate: [KeyBinding; 2] = [DEFAULT_BINDINGS[0].clone(), DEFAULT_BINDINGS[0].clone()];
        assert_eq!(
            validate_bindings(&duplicate),
            Err(BindingConflict {
                first_index: 0,
                second_index: 1
            })
        );
    }

    #[test]
    fn gpui_registration_has_one_binding_per_window_chord() {
        assert_eq!(super::gpui_key_bindings().len(), 27);
        assert!(!super::gpui_text_input_bindings().is_empty());
    }

    #[test]
    fn editable_text_bindings_are_scoped_and_window_submit_cancel_are_explicit() {
        let text_bindings = super::gpui_text_input_bindings();
        assert!(
            text_bindings
                .iter()
                .all(|binding| binding.predicate().is_some())
        );
        assert!(text_bindings.iter().all(|binding| {
            binding
                .keystrokes()
                .iter()
                .all(|keystroke| !matches!(keystroke.key(), "enter" | "escape" | "tab"))
        }));

        let window_bindings = super::gpui_key_bindings();
        assert!(window_bindings.iter().any(|binding| {
            binding.keystrokes().len() == 1
                && binding.keystrokes()[0].key() == "f2"
                && binding.action().name() == "explorer::RenameFocusedItem"
        }));
        assert!(window_bindings.iter().any(|binding| {
            binding.keystrokes().len() == 1
                && binding.keystrokes()[0].key() == "t"
                && binding.keystrokes()[0].modifiers().control
                && binding.action().name() == "explorer::NewExplorerTab"
                && binding.predicate().is_none()
        }));
        for (key, action_name) in [
            ("enter", "explorer::SubmitFocusedInput"),
            ("escape", "explorer::CancelFocusedInput"),
        ] {
            assert!(window_bindings.iter().any(|binding| {
                binding.keystrokes().len() == 1
                    && binding.keystrokes()[0].key() == key
                    && binding.action().name() == action_name
                    && binding.predicate().is_some()
            }));
        }
    }

    #[cfg(not(target_os = "macos"))]
    #[test]
    fn editable_text_bindings_include_windows_shift_selection_chords() {
        let text_bindings = super::gpui_text_input_bindings();
        for (key, action_suffix) in [
            ("home", "::SelectLineStart"),
            ("end", "::SelectLineEnd"),
            ("left", "::SelectLeft"),
            ("right", "::SelectRight"),
        ] {
            assert!(
                text_bindings.iter().any(|binding| {
                    binding.keystrokes().len() == 1
                        && binding.keystrokes()[0].key() == key
                        && binding.keystrokes()[0].modifiers().shift
                        && !binding.keystrokes()[0].modifiers().control
                        && !binding.keystrokes()[0].modifiers().alt
                        && !binding.keystrokes()[0].modifiers().platform
                        && binding.action().name().ends_with(action_suffix)
                        && binding.predicate().is_some()
                }),
                "missing scoped Shift+{key} binding for {action_suffix}"
            );
        }

        let window_bindings = super::gpui_key_bindings();
        for key in ["home", "end", "left", "right"] {
            assert!(
                !window_bindings.iter().any(|binding| {
                    binding.keystrokes().len() == 1
                        && binding.keystrokes()[0].key() == key
                        && binding.keystrokes()[0].modifiers().shift
                        && !binding.keystrokes()[0].modifiers().control
                        && !binding.keystrokes()[0].modifiers().alt
                        && !binding.keystrokes()[0].modifiers().platform
                }),
                "window binding must not consume Shift+{key} from a focused editor"
            );
        }
    }

    #[test]
    fn scrollbar_drag_actions_are_typed_exclusive_and_idempotent() {
        use crate::interaction::{ScrollbarKind, ScrollbarTerminal};

        let mut state = AppViewState::default();
        let begin = dispatch_action(
            &mut state,
            ExplorerAction::BeginScrollbarDrag {
                kind: ScrollbarKind::FileView,
                grab_offset_y: 7.0,
            },
            ActionSource::Mouse,
        );
        assert_eq!(begin.outcome, ActionOutcome::Handled);
        assert_eq!(
            state.scrollbar_drag_session(),
            Some(crate::interaction::ScrollbarDragSession {
                kind: ScrollbarKind::FileView,
                grab_offset_y: 7.0,
            })
        );
        assert_eq!(
            dispatch_action(
                &mut state,
                ExplorerAction::UpdateScrollbarDrag { pointer_y: 400.0 },
                ActionSource::Mouse,
            )
            .outcome,
            ActionOutcome::Handled
        );
        dispatch_action(
            &mut state,
            ExplorerAction::BeginScrollbarDrag {
                kind: ScrollbarKind::Navigation,
                grab_offset_y: 3.0,
            },
            ActionSource::Mouse,
        );
        assert_eq!(
            state.scrollbar_drag_session().map(|session| session.kind),
            Some(ScrollbarKind::Navigation)
        );
        assert_eq!(
            dispatch_action(
                &mut state,
                ExplorerAction::EndScrollbarDrag {
                    reason: ScrollbarTerminal::PointerUpOutside,
                },
                ActionSource::Mouse,
            )
            .outcome,
            ActionOutcome::Handled
        );
        assert_eq!(
            dispatch_action(
                &mut state,
                ExplorerAction::EndScrollbarDrag {
                    reason: ScrollbarTerminal::PointerUpOutside,
                },
                ActionSource::Mouse,
            )
            .outcome,
            ActionOutcome::Disabled
        );
    }

    #[test]
    fn disabled_navigation_is_traced_once_without_changing_state() {
        let mut state = AppViewState::default();
        let before_theme = state.current_theme();
        let before_tab = state.tabs().active_tab_id();
        let trace = dispatch_action(&mut state, ExplorerAction::Back, ActionSource::Keyboard);
        assert_eq!(trace.outcome, ActionOutcome::Disabled);
        assert_eq!(state.current_theme(), before_theme);
        assert_eq!(state.tabs().active_tab_id(), before_tab);
    }

    #[test]
    fn focus_theme_close_and_resize_actions_update_only_owned_state() {
        let mut state = AppViewState::default();
        let trace = dispatch_action(
            &mut state,
            ExplorerAction::FocusSearch,
            ActionSource::Keyboard,
        );
        assert_eq!(trace.handled_surface, FocusSurface::Search);
        assert_eq!(state.previous_focus(), Some(FocusSurface::FileView));

        dispatch_action(
            &mut state,
            ExplorerAction::RestorePreviousFocus,
            ActionSource::Keyboard,
        );
        assert_eq!(state.focused_surface(), FocusSurface::FileView);

        dispatch_action(&mut state, ExplorerAction::ToggleTheme, ActionSource::Mouse);
        assert_eq!(state.current_theme(), ThemeMode::Dark);

        dispatch_action(
            &mut state,
            ExplorerAction::ResizeNavigationPane {
                width: LogicalPx::new(999.0),
            },
            ActionSource::Mouse,
        );
        assert!(
            (state.navigation_pane_width().value()
                - LayoutTokens::WINDOWS_11.navigation_pane_max_width.value())
            .abs()
                < f32::EPSILON
        );

        dispatch_action(
            &mut state,
            ExplorerAction::CloseWindow,
            ActionSource::Accessibility,
        );
        assert!(state.close_requested());
    }

    #[test]
    fn file_view_pointer_selection_restores_focus_after_navigation_pane() {
        let mut state = AppViewState::default();
        dispatch_action(
            &mut state,
            ExplorerAction::ActivateNavigationItem {
                location: explorer_model::LocationDescriptor::file_system(r"D:\"),
            },
            ActionSource::Mouse,
        );
        assert_eq!(state.focused_surface(), FocusSurface::NavigationPane);

        let trace = dispatch_action(
            &mut state,
            ExplorerAction::ClearSelection,
            ActionSource::Mouse,
        );
        assert_eq!(trace.handled_surface, FocusSurface::FileView);
        assert_eq!(state.focused_surface(), FocusSurface::FileView);
    }

    #[test]
    fn tab_and_shift_tab_traverse_every_focus_surface_in_both_directions() {
        let mut state = AppViewState::default();
        dispatch_action(
            &mut state,
            ExplorerAction::TogglePreviewPane,
            ActionSource::Keyboard,
        );
        let mut forward = Vec::new();
        for _ in 0..FocusSurface::ORDER.len() {
            let trace = dispatch_action(
                &mut state,
                ExplorerAction::FocusNext,
                ActionSource::Keyboard,
            );
            assert_eq!(trace.outcome, ActionOutcome::Handled);
            forward.push(trace.handled_surface);
        }
        assert_eq!(
            forward,
            vec![
                FocusSurface::PreviewPane,
                FocusSurface::StatusBar,
                FocusSurface::WindowChrome,
                FocusSurface::TabStrip,
                FocusSurface::CommandBar,
                FocusSurface::AddressBar,
                FocusSurface::Search,
                FocusSurface::NavigationPane,
                FocusSurface::FileView,
            ]
        );

        let mut backward = Vec::new();
        for _ in 0..FocusSurface::ORDER.len() {
            backward.push(
                dispatch_action(
                    &mut state,
                    ExplorerAction::FocusPrevious,
                    ActionSource::Keyboard,
                )
                .handled_surface,
            );
        }
        assert_eq!(
            backward,
            vec![
                FocusSurface::NavigationPane,
                FocusSurface::Search,
                FocusSurface::AddressBar,
                FocusSurface::CommandBar,
                FocusSurface::TabStrip,
                FocusSurface::WindowChrome,
                FocusSurface::StatusBar,
                FocusSurface::PreviewPane,
                FocusSurface::FileView,
            ]
        );
    }

    #[test]
    fn tab_actions_create_cycle_close_and_apply_last_tab_rule() {
        let mut state = AppViewState::default();
        let first = state.tabs().active_tab_id();
        assert_eq!(
            dispatch_action(&mut state, ExplorerAction::NewTab, ActionSource::Keyboard).outcome,
            ActionOutcome::Handled
        );
        let second = state.tabs().active_tab_id();
        assert_ne!(first, second);
        assert_eq!(state.tabs().tabs().len(), 2);
        dispatch_action(
            &mut state,
            ExplorerAction::ReorderTab {
                tab_id: second,
                destination_index: 0,
            },
            ActionSource::Mouse,
        );
        assert_eq!(state.tabs().tabs()[0].id, second);
        assert_eq!(state.tabs().active_tab_id(), second);
        dispatch_action(
            &mut state,
            ExplorerAction::PreviousTab,
            ActionSource::Keyboard,
        );
        assert_eq!(state.tabs().active_tab_id(), first);
        dispatch_action(&mut state, ExplorerAction::NextTab, ActionSource::Keyboard);
        assert_eq!(state.tabs().active_tab_id(), second);
        dispatch_action(
            &mut state,
            ExplorerAction::CloseActiveTab,
            ActionSource::Keyboard,
        );
        assert_eq!(state.tabs().tabs().len(), 1);
        assert!(!state.close_requested());
        dispatch_action(
            &mut state,
            ExplorerAction::CloseActiveTab,
            ActionSource::Keyboard,
        );
        assert!(state.close_requested());
    }

    #[test]
    fn tab_shortcuts_are_unique_window_actions_not_text_input_bindings() {
        for key in [super::KeyCode::T, super::KeyCode::W, super::KeyCode::Tab] {
            assert!(DEFAULT_BINDINGS.iter().any(|binding| {
                binding.scope == super::BindingScope::Window
                    && binding.chord.control
                    && binding.chord.key == key
            }));
            assert!(!DEFAULT_BINDINGS.iter().any(|binding| {
                binding.scope == super::BindingScope::TextInput && binding.chord.key == key
            }));
        }
    }

    #[test]
    fn switching_tabs_restores_each_tabs_last_focus_surface() {
        let mut state = AppViewState::default();
        let first = state.tabs().active_tab_id();
        dispatch_action(
            &mut state,
            ExplorerAction::FocusSearch,
            ActionSource::Keyboard,
        );
        let second = state.new_tab();
        dispatch_action(
            &mut state,
            ExplorerAction::FocusAddress,
            ActionSource::Keyboard,
        );
        dispatch_action(
            &mut state,
            ExplorerAction::ActivateTab { tab_id: first },
            ActionSource::Keyboard,
        );
        assert_eq!(state.focused_surface(), FocusSurface::Search);
        dispatch_action(
            &mut state,
            ExplorerAction::ActivateTab { tab_id: second },
            ActionSource::Keyboard,
        );
        assert_eq!(state.focused_surface(), FocusSurface::AddressBar);
    }
}
