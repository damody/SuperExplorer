//! Pure window presentation state consumed by the GPUI root.

use std::{
    collections::{HashMap, HashSet},
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

use explorer_model::{
    DeleteLockKind, DirectorySnapshot, DirectoryState, ExplorerCommand, ExplorerEvent,
    ExplorerWindowState, FileOperationKind, FileOperationRequest, HistoryEntry, ItemDescriptor,
    LocationDescriptor, LockOwner, LockOwnerCloseOutcome, LockOwnerCloseRequest,
    LockOwnerCloseTerminal, LockOwnerDiscoveryRequest, LockOwnerDiscoveryTerminal, OpenDisposition,
    OperationCenterState, OperationRecord, OperationTerminal, PersistedQuickAccessPin,
    QuickAccessPins, RecentItems, RequestContext, ShellIdentity, ShellItemId, TabCloseOutcome,
    TabId, TabPresentationSnapshot, WindowEventOutcome,
};

const DIRECTORY_CACHE_MAX_LOCATIONS: usize = 64;
const DIRECTORY_CACHE_MAX_ROWS: usize = 100_000;

fn unique_remote_folder_symlink_name(base: &str, existing: &HashSet<&str>) -> String {
    for ordinal in 1_u64.. {
        let candidate = if ordinal == 1 {
            format!("{base} - 捷徑")
        } else {
            format!("{base} - 捷徑 ({ordinal})")
        };
        if !existing.contains(candidate.as_str()) {
            return candidate;
        }
    }
    unreachable!("an unbounded suffix must eventually produce a free child name")
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
enum DirectoryCacheKey {
    Local(String),
    Virtual {
        provider: String,
        authority: String,
        components: Vec<String>,
    },
}

impl DirectoryCacheKey {
    fn for_location(location: &LocationDescriptor) -> Option<Self> {
        match location {
            LocationDescriptor::FileSystem(path) => {
                let mut normalized = path
                    .to_string_lossy()
                    .replace('/', "\\")
                    .to_ascii_lowercase();
                while normalized.ends_with('\\')
                    && !(normalized.len() == 3 && normalized.as_bytes().get(1) == Some(&b':'))
                {
                    normalized.pop();
                }
                Some(Self::Local(normalized))
            }
            LocationDescriptor::Virtual(location)
                if matches!(location.provider_id.as_str(), "adb" | "sftp") =>
            {
                let provider = location.provider_id.to_ascii_lowercase();
                let authority = location.public_authority.as_ref()?.clone();
                Some(Self::Virtual {
                    authority: if provider == "sftp" {
                        authority.to_ascii_lowercase()
                    } else {
                        authority
                    },
                    provider,
                    components: location.components.clone(),
                })
            }
            _ => None,
        }
    }
}

#[derive(Clone, Debug)]
struct DirectoryCacheEntry {
    snapshot: DirectorySnapshot,
    last_used: u64,
}

#[derive(Clone, Debug)]
struct DirectorySnapshotCache {
    entries: HashMap<DirectoryCacheKey, DirectoryCacheEntry>,
    total_rows: usize,
    access_sequence: u64,
    max_locations: usize,
    max_rows: usize,
}

impl Default for DirectorySnapshotCache {
    fn default() -> Self {
        Self {
            entries: HashMap::new(),
            total_rows: 0,
            access_sequence: 0,
            max_locations: DIRECTORY_CACHE_MAX_LOCATIONS,
            max_rows: DIRECTORY_CACHE_MAX_ROWS,
        }
    }
}

impl DirectorySnapshotCache {
    #[cfg(test)]
    fn with_limits(max_locations: usize, max_rows: usize) -> Self {
        Self {
            max_locations,
            max_rows,
            ..Self::default()
        }
    }

    fn get(&mut self, location: &LocationDescriptor) -> Option<DirectorySnapshot> {
        let key = DirectoryCacheKey::for_location(location)?;
        let sequence = self.next_sequence();
        let entry = self.entries.get_mut(&key)?;
        entry.last_used = sequence;
        Some(entry.snapshot.clone())
    }

    fn insert(&mut self, location: &LocationDescriptor, snapshot: DirectorySnapshot) -> bool {
        let Some(key) = DirectoryCacheKey::for_location(location) else {
            return false;
        };
        let rows = snapshot.entries().len();
        if rows > self.max_rows {
            return false;
        }
        if let Some(previous) = self.entries.remove(&key) {
            self.total_rows = self
                .total_rows
                .saturating_sub(previous.snapshot.entries().len());
        }
        let last_used = self.next_sequence();
        self.total_rows = self.total_rows.saturating_add(rows);
        self.entries.insert(
            key,
            DirectoryCacheEntry {
                snapshot,
                last_used,
            },
        );
        self.evict_to_limits();
        true
    }

    fn next_sequence(&mut self) -> u64 {
        if self.access_sequence == u64::MAX {
            let mut order = self
                .entries
                .iter()
                .map(|(key, entry)| (key.clone(), entry.last_used))
                .collect::<Vec<_>>();
            order.sort_by_key(|(_, sequence)| *sequence);
            for (index, (key, _)) in order.into_iter().enumerate() {
                if let Some(entry) = self.entries.get_mut(&key) {
                    entry.last_used = index as u64 + 1;
                }
            }
            self.access_sequence = self.entries.len() as u64;
        }
        self.access_sequence += 1;
        self.access_sequence
    }

    fn evict_to_limits(&mut self) {
        while self.entries.len() > self.max_locations || self.total_rows > self.max_rows {
            let Some(oldest) = self
                .entries
                .iter()
                .min_by_key(|(_, entry)| entry.last_used)
                .map(|(key, _)| key.clone())
            else {
                break;
            };
            if let Some(removed) = self.entries.remove(&oldest) {
                self.total_rows = self
                    .total_rows
                    .saturating_sub(removed.snapshot.entries().len());
            }
        }
    }
}

use crate::{
    actions::{FolderOptionsPage, NavigationHistoryDirection, PermanentDeleteDialogTarget},
    extension_commands::ExtensionCommandPanel,
    focus::{FocusCoordinator, FocusDirection, FocusSurface},
    interaction::{DividerInteraction, ScrollbarDragSession, ScrollbarKind, ScrollbarTerminal},
    layout::{LayoutTokens, LogicalPx},
    theme::ThemeMode,
};

fn bookmark_target_for_current_location(
    location: &LocationDescriptor,
) -> Option<explorer_model::BookmarkTarget> {
    match location {
        LocationDescriptor::FileSystem(path) if path.is_dir() => {
            Some(explorer_model::BookmarkTarget::Folder {
                location: location.clone(),
            })
        }
        LocationDescriptor::Virtual(remote)
            if matches!(remote.provider_id.as_str(), "adb" | "sftp")
                && remote.public_authority.is_some() =>
        {
            Some(explorer_model::BookmarkTarget::Folder {
                location: location.clone(),
            })
        }
        _ => None,
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct NavigationHistoryMenuState {
    direction: NavigationHistoryDirection,
    focused_index: usize,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FolderOptionsDraft {
    pub page: FolderOptionsPage,
    pub settings: explorer_model::ViewSettings,
    pub restore_previous_session: bool,
    pub extension_enabled: Vec<bool>,
    pub applied_baseline: FolderOptionsAppliedSnapshotV1,
    pub applied_revision: u64,
    pub apply_error: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FolderOptionsAppliedSnapshotV1 {
    pub settings: explorer_model::ViewSettings,
    pub restore_previous_session: bool,
    pub extension_enabled: Vec<bool>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FolderOptionsApplyResultV1 {
    Applied { revision: u64 },
    Rejected { reason: FolderOptionsApplyFailureV1 },
    NoDraft,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FolderOptionsApplyFailureV1 {
    Validation,
    Persistence,
}

impl FolderOptionsDraft {
    pub fn applied_snapshot(&self) -> FolderOptionsAppliedSnapshotV1 {
        FolderOptionsAppliedSnapshotV1 {
            settings: self.settings.clone(),
            restore_previous_session: self.restore_previous_session,
            extension_enabled: self.extension_enabled.clone(),
        }
    }

    pub fn is_dirty(&self) -> bool {
        self.applied_snapshot() != self.applied_baseline
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ExtensionOptionV1 {
    pub package_id: &'static str,
    pub display_name: &'static str,
    pub author_name: &'static str,
    pub author_bio: &'static str,
    pub author_website: &'static str,
    pub purpose: &'static str,
    pub community_website: &'static str,
    pub release_date: &'static str,
    pub command_contribution: Option<&'static str>,
    pub enabled: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AboutInfoV1 {
    pub version: String,
    pub build_date: String,
    pub git_hash: String,
    pub author: String,
}

fn official_extensions_v1() -> Vec<ExtensionOptionV1> {
    vec![
        ExtensionOptionV1 {
            package_id: "rust-folder-size-visual-column",
            display_name: "Folder size column",
            author_name: "Damody",
            author_bio: "SuperExplorer 與官方範例擴充功能作者",
            author_website: "https://github.com/damody/SuperExplorer",
            purpose: "顯示檔案大小並在背景遞迴計算資料夾總大小。",
            community_website: "https://github.com/damody/SuperExplorer/discussions",
            release_date: "2026-08-04",
            command_contribution: None,
            enabled: true,
        },
        ExtensionOptionV1 {
            package_id: "rust-folder-size-map-view",
            display_name: "Size Map",
            author_name: "Damody",
            author_bio: "SuperExplorer 與官方範例擴充功能作者",
            author_website: "https://github.com/damody/SuperExplorer",
            purpose: "以面積圖呈現目前資料夾內各項目的空間占用。",
            community_website: "https://github.com/damody/SuperExplorer/discussions",
            release_date: "2026-08-04",
            command_contribution: None,
            enabled: true,
        },
        ExtensionOptionV1 {
            package_id: "rust-tokei-code-lines-column",
            display_name: "Main code lines",
            author_name: "Damody",
            author_bio: "SuperExplorer 與官方範例擴充功能作者",
            author_website: "https://github.com/damody/SuperExplorer",
            purpose: "使用 Rust 與 tokei 統計檔案或資料夾中的程式碼行數。",
            community_website: "https://github.com/damody/SuperExplorer/discussions",
            release_date: "2026-08-04",
            command_contribution: None,
            enabled: true,
        },
        ExtensionOptionV1 {
            package_id: "lua-tokei-code-lines-column",
            display_name: "Code lines",
            author_name: "Damody",
            author_bio: "SuperExplorer 與官方範例擴充功能作者",
            author_website: "https://github.com/damody/SuperExplorer",
            purpose: "示範以 Lua 擴充功能統計程式碼行數。",
            community_website: "https://github.com/damody/SuperExplorer/discussions",
            release_date: "2026-08-04",
            command_contribution: None,
            enabled: true,
        },
        ExtensionOptionV1 {
            package_id: "rust-lock-owner-column",
            display_name: "Lock owner",
            author_name: "Damody",
            author_bio: "SuperExplorer 與官方範例擴充功能作者",
            author_website: "https://github.com/damody/SuperExplorer",
            purpose: "顯示目前鎖定檔案的程式或服務擁有者。",
            community_website: "https://github.com/damody/SuperExplorer/discussions",
            release_date: "2026-08-04",
            command_contribution: None,
            enabled: true,
        },
        ExtensionOptionV1 {
            package_id: "rust-exif-rename-command",
            display_name: "Rename from EXIF",
            author_name: "Damody",
            author_bio: "SuperExplorer 與官方範例擴充功能作者",
            author_website: "https://github.com/damody/SuperExplorer",
            purpose: "依相片 EXIF 拍攝資訊批次產生重新命名建議。",
            community_website: "https://github.com/damody/SuperExplorer/discussions",
            release_date: "2026-08-04",
            command_contribution: Some("rust-exif-rename:button"),
            enabled: true,
        },
        ExtensionOptionV1 {
            package_id: "rust-7z-virtual-folder",
            display_name: "7-Zip virtual folder",
            author_name: "Damody",
            author_bio: "SuperExplorer 與官方範例擴充功能作者",
            author_website: "https://github.com/damody/SuperExplorer",
            purpose: "將 7-Zip 壓縮檔以可瀏覽的虛擬資料夾呈現。",
            community_website: "https://github.com/damody/SuperExplorer/discussions",
            release_date: "2026-08-04",
            command_contribution: None,
            enabled: true,
        },
        ExtensionOptionV1 {
            package_id: "lua-bulk-folder-generator",
            display_name: "Bulk folder generator",
            author_name: "Damody",
            author_bio: "SuperExplorer 與官方範例擴充功能作者",
            author_website: "https://github.com/damody/SuperExplorer",
            purpose: "依使用者指定的樣板一次建立多個資料夾。",
            community_website: "https://github.com/damody/SuperExplorer/discussions",
            release_date: "2026-08-04",
            command_contribution: Some("lua-bulk-folder:button"),
            enabled: true,
        },
    ]
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BrokerUiHealth {
    Healthy,
    Retrying,
    Unavailable,
    VersionMismatch,
    Crash,
    Timeout,
    Quarantined,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum LockRecoveryPhase {
    Discovering,
    Ready,
    Closing,
    Partial,
    Unavailable,
    Retrying,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum LockRecoveryFocusTarget {
    CloseAndRetry,
    Retry,
    Cancel,
}

#[derive(Clone, Debug)]
pub(crate) struct LockRecoveryUiState {
    pub phase: LockRecoveryPhase,
    pub owners: Vec<LockOwner>,
    pub close_outcomes: Vec<LockOwnerCloseOutcome>,
    pub status: String,
    pub item_count: usize,
    original_request: FileOperationRequest,
    request_context: RequestContext,
    retry_operation_id: Option<explorer_common::RequestId>,
    retry_count: usize,
    focus_index: usize,
}

impl LockRecoveryUiState {
    pub fn can_close(&self) -> bool {
        self.phase == LockRecoveryPhase::Ready && self.owners.iter().any(LockOwner::can_close)
    }

    pub const fn can_retry(&self) -> bool {
        matches!(
            self.phase,
            LockRecoveryPhase::Ready | LockRecoveryPhase::Partial | LockRecoveryPhase::Unavailable
        )
    }

    fn focus_targets(&self) -> Vec<LockRecoveryFocusTarget> {
        let mut targets = Vec::with_capacity(3);
        if self.can_close() {
            targets.push(LockRecoveryFocusTarget::CloseAndRetry);
        }
        if self.can_retry() {
            targets.push(LockRecoveryFocusTarget::Retry);
        }
        targets.push(LockRecoveryFocusTarget::Cancel);
        targets
    }

    pub fn focused_target(&self) -> LockRecoveryFocusTarget {
        let targets = self.focus_targets();
        targets[self.focus_index.min(targets.len().saturating_sub(1))]
    }

    fn move_focus(&mut self, direction: i8) {
        let count = self.focus_targets().len();
        if count == 0 {
            return;
        }
        self.focus_index = if direction < 0 {
            self.focus_index.checked_sub(1).unwrap_or(count - 1)
        } else {
            (self.focus_index + 1) % count
        };
    }
}

impl BrokerUiHealth {
    pub const fn message(self) -> Option<&'static str> {
        match self {
            Self::Healthy => None,
            Self::Retrying => Some("Retrying the extension service…"),
            Self::Unavailable => Some("The extension service is unavailable."),
            Self::VersionMismatch => Some("The extension service must be updated."),
            Self::Crash => Some("The extension service stopped unexpectedly."),
            Self::Timeout => Some("The extension service did not respond."),
            Self::Quarantined => Some("This extension is temporarily paused for safety."),
        }
    }
}

#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "the finite value is rounded and clamped to the complete u16 range before conversion"
)]
fn clamped_u16(value: f32, minimum: u16, maximum: u16) -> u16 {
    value.round().clamp(f32::from(minimum), f32::from(maximum)) as u16
}

fn navigation_segment_id(location: &LocationDescriptor) -> explorer_model::BreadcrumbSegmentId {
    use std::hash::{Hash as _, Hasher as _};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    location.hash(&mut hasher);
    explorer_model::BreadcrumbSegmentId(hasher.finish())
}

fn is_delete_request(request: &FileOperationRequest) -> bool {
    matches!(
        request.kind,
        FileOperationKind::RecycleDelete { .. } | FileOperationKind::PermanentDelete { .. }
    )
}

fn delete_resources(request: &FileOperationRequest) -> Vec<LocationDescriptor> {
    let limits = explorer_common::RoadmapLimits::default();
    let (FileOperationKind::RecycleDelete { items }
    | FileOperationKind::PermanentDelete { items, .. }) = &request.kind
    else {
        return Vec::new();
    };
    items
        .iter()
        .filter(|item| item.location.path().is_some())
        .take(limits.lock_recovery_max_resources)
        .map(|item| item.location.clone())
        .collect()
}

fn locked_delete_resources(
    request: &FileOperationRequest,
    outcome: &OperationTerminal,
) -> Vec<LocationDescriptor> {
    let limits = explorer_common::RoadmapLimits::default();
    if let OperationTerminal::Partial { outcomes } = outcome {
        let resources = outcomes
            .iter()
            .filter_map(|outcome| {
                let explorer_model::OperationItemResult::Failed(error) = &outcome.result else {
                    return None;
                };
                error
                    .native_code
                    .and_then(DeleteLockKind::from_native_code)?;
                outcome.item.as_ref().and_then(|item| {
                    item.location
                        .path()
                        .is_some()
                        .then(|| item.location.clone())
                })
            })
            .take(limits.lock_recovery_max_resources)
            .collect::<Vec<_>>();
        if !resources.is_empty() {
            return resources;
        }
    }
    delete_resources(request)
}

fn terminal_has_lock_error(outcome: &OperationTerminal) -> bool {
    let is_lock = |error: &explorer_common::ExplorerError| {
        error
            .native_code
            .and_then(DeleteLockKind::from_native_code)
            .is_some()
    };
    match outcome {
        OperationTerminal::Failed(error) => is_lock(error),
        OperationTerminal::Partial { outcomes } => outcomes.iter().any(|outcome| {
            matches!(&outcome.result, explorer_model::OperationItemResult::Failed(error) if is_lock(error))
        }),
        OperationTerminal::Finished | OperationTerminal::Cancelled => false,
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CommandAvailability {
    enabled: [bool; CommandKind::COUNT],
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CommandKind {
    Back,
    Forward,
    Up,
    Refresh,
    NewTab,
    CloseTab,
    NextTab,
    PreviousTab,
    FocusAddress,
    FocusSearch,
    ToggleTheme,
    CloseWindow,
    ResizeNavigationPane,
}

impl CommandKind {
    const COUNT: usize = 13;

    const fn index(self) -> usize {
        self as usize
    }
}

impl CommandAvailability {
    fn from_tabs(tabs: &ExplorerWindowState) -> Self {
        let active = tabs.active_presentation();
        let multiple_tabs = tabs.tabs().len() > 1;
        Self {
            enabled: [
                active.can_go_back,
                active.can_go_forward,
                active.can_go_up,
                true,
                true,
                true,
                multiple_tabs,
                multiple_tabs,
                true,
                true,
                true,
                true,
                true,
            ],
        }
    }

    pub const fn is_enabled(self, command: CommandKind) -> bool {
        self.enabled[command.index()]
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct BookmarkEditorDraft {
    pub(crate) id: Option<explorer_model::BookmarkId>,
    pub(crate) name: String,
    pub(crate) target: explorer_model::BookmarkTarget,
    pub(crate) parent_id: Option<explorer_model::BookmarkFolderId>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct BookmarkFolderEditorDraft {
    pub(crate) id: explorer_model::BookmarkFolderId,
    pub(crate) name: String,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct BookmarkToolbarContextMenuState {
    pub(crate) parent_id: Option<explorer_model::BookmarkFolderId>,
    pub(crate) x: f32,
    pub(crate) y: f32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct BookmarkContextMenuState {
    pub(crate) id: explorer_model::BookmarkId,
    pub(crate) x: f32,
    pub(crate) y: f32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct RemoteContextMenuState {
    pub(crate) x: f32,
    pub(crate) y: f32,
    pub(crate) background: bool,
    pub(crate) paste_available: bool,
    pub(crate) item_row_index: Option<usize>,
    pub(crate) item_is_container: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct RemotePropertiesState {
    pub(crate) entry: explorer_model::FileEntry,
    pub(crate) original_mode: u32,
    pub(crate) mode: u32,
}

#[derive(Clone, Debug)]
struct PermanentDeleteConfirmation {
    items: Vec<ItemDescriptor>,
    tab_id: TabId,
    generation: explorer_model::Generation,
    nonce: explorer_common::RequestId,
}

#[derive(Clone, Debug)]
struct PendingNewFolderRename {
    tab_id: TabId,
    generation: explorer_model::Generation,
    parent: LocationDescriptor,
    item_id: ShellItemId,
    use_create_item: bool,
}

#[derive(Clone, Debug)]
#[allow(
    clippy::struct_excessive_bools,
    reason = "these are independent Explorer overlay, pane, focus, and command-surface states rather than one disguised enum"
)]
pub struct AppViewState {
    current_theme: ThemeMode,
    navigation_pane_width: LogicalPx,
    focus: FocusCoordinator,
    tabs: ExplorerWindowState,
    directory_cache: DirectorySnapshotCache,
    tab_focus: HashMap<TabId, FocusSurface>,
    navigation_focus: HashMap<TabId, LocationDescriptor>,
    close_requested: bool,
    divider: DividerInteraction,
    operation_center: OperationCenterState,
    operation_terminal_notice: Option<(explorer_common::RequestId, Instant)>,
    rename_editor: Option<explorer_model::RenameEditorState>,
    pending_new_folder_rename: Option<PendingNewFolderRename>,
    permanent_delete_confirmation: Option<PermanentDeleteConfirmation>,
    permanent_delete_confirmation_focus: PermanentDeleteDialogTarget,
    lock_recovery: Option<LockRecoveryUiState>,
    pending_lock_recovery_command: Option<ExplorerCommand>,
    clipboard: explorer_model::ClipboardState,
    drag_session: explorer_model::DragSession,
    pending_drag_command: Option<ExplorerCommand>,
    pending_new_tab_command: Option<ExplorerCommand>,
    drop_target_row: Option<usize>,
    pending_right_drop: Option<PendingRightDrop>,
    context_menu_error: Option<explorer_common::ExplorerError>,
    thumbnail_cache_notice: Option<String>,
    quick_access: QuickAccessPins,
    bookmarks: explorer_model::Bookmarks,
    recent_items: RecentItems,
    quick_access_notice: Option<String>,
    bookmark_notice: Option<String>,
    bookmark_overflow_open: bool,
    bookmark_folder_menu: Option<explorer_model::BookmarkFolderId>,
    bookmark_toolbar_context_menu: Option<BookmarkToolbarContextMenuState>,
    bookmark_context_menu: Option<BookmarkContextMenuState>,
    remote_context_menu: Option<RemoteContextMenuState>,
    remote_properties: Option<RemotePropertiesState>,
    expanded_bookmark_folders: HashSet<explorer_model::BookmarkFolderId>,
    bookmark_folder_delete_confirmation: Option<(explorer_model::BookmarkFolderId, usize)>,
    bookmark_editor: Option<BookmarkEditorDraft>,
    bookmark_folder_editor: Option<BookmarkFolderEditorDraft>,
    broker_health: BrokerUiHealth,
    session_reset_confirmation: Option<explorer_model::SessionResetScope>,
    confirmed_session_reset: Option<explorer_model::SessionResetScope>,
    last_session_reset: Option<explorer_model::SessionResetScope>,
    session_reset_notice: Option<String>,
    pending_context_menu: Option<RequestContext>,
    pending_context_hit: Option<ShellItemId>,
    pending_context_extended_verbs: bool,
    queued_context_menu: Option<(RequestContext, explorer_model::ContextMenuRequest)>,
    pending_context_menu_command: Option<ExplorerCommand>,
    ancestry_requests: HashMap<TabId, RequestContext>,
    breadcrumb_menu_requests: HashMap<TabId, PendingBreadcrumbMenu>,
    navigation_history_menu: Option<NavigationHistoryMenuState>,
    navigation_trees: HashMap<TabId, NavigationTreeState>,
    sort_menu_open: bool,
    sort_menu_index: usize,
    new_menu_open: bool,
    new_menu_index: usize,
    new_items: Vec<explorer_model::ShellNewItemDescriptor>,
    view_menu_open: bool,
    view_menu_index: usize,
    more_menu_open: bool,
    more_menu_index: usize,
    about_dialog_open: bool,
    about_info: AboutInfoV1,
    extensions_menu_open: bool,
    extension_command_panel: Option<ExtensionCommandPanel>,
    tortoise_git_available: bool,
    loaded_extension_summary: Option<String>,
    folder_options: Option<FolderOptionsDraft>,
    folder_options_applied_revision: u64,
    extensions: Vec<ExtensionOptionV1>,
    restore_previous_session: bool,
    view_show_submenu_open: bool,
    /// Host-owned descriptor snapshot. Extension-host contribution validation will replace
    /// package descriptors in task 5.2; UI only reads this registry.
    column_registry: explorer_model::ColumnRegistry,
    details_column_resize: Option<DetailsColumnResizeSession>,
    details_column_drag: Option<DetailsColumnDragPreviewSession>,
    details_column_menu: Option<explorer_model::ColumnId>,
    details_filter_menu: Option<explorer_model::ColumnId>,
    details_filters: HashMap<TabId, crate::file_view::DetailsFilters>,
    side_pane_resize: Option<SidePaneResizeSession>,
    scrollbar_drag: Option<ScrollbarDragSession>,
    marquee: Option<MarqueeSelectionSession>,
    file_view_typeahead: Option<FileViewTypeAhead>,
    /// Exact values for the one P0 runtime column. Keeping these in view state
    /// makes its sorted presentation authoritative for every row action.
    folder_size_sort_values: HashMap<ShellItemId, Option<u64>>,
    builtin_count_sort_values: HashMap<explorer_model::ColumnId, HashMap<ShellItemId, Option<u64>>>,
    code_lines_sort_values: HashMap<explorer_model::ColumnId, HashMap<ShellItemId, Option<u64>>>,
    presentation_cache: Arc<Mutex<crate::file_view::DirectoryPresentationCache>>,
}

const FILE_VIEW_TYPEAHEAD_TIMEOUT: Duration = Duration::from_secs(1);

#[derive(Clone, Debug)]
struct FileViewTypeAhead {
    tab_id: TabId,
    generation: explorer_model::Generation,
    prefix: String,
    last_input: Instant,
}

#[derive(Clone, Debug)]
pub(crate) struct MarqueeSelectionSession {
    pub origin_x: f32,
    pub origin_y: f32,
    pub current_x: f32,
    pub current_y: f32,
    base_selection: explorer_model::SelectionModel,
}

#[derive(Clone, Debug)]
struct DetailsColumnResizeSession {
    tab_id: TabId,
    column: explorer_model::ColumnId,
    pointer_x: f32,
    width: u16,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct DetailsColumnDragPreviewSession {
    tab_id: TabId,
    column: explorer_model::ColumnId,
    original_order: Vec<explorer_model::ColumnId>,
    preview_order: Vec<explorer_model::ColumnId>,
    before: Option<explorer_model::ColumnId>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct DetailsColumnInsertionPreview {
    pub(crate) order: Vec<explorer_model::ColumnId>,
    pub(crate) before: Option<explorer_model::ColumnId>,
}

pub(crate) fn resolve_details_column_insertion(
    order: &[explorer_model::ColumnId],
    dragged: &explorer_model::ColumnId,
    target: &explorer_model::ColumnId,
    pointer_x: f32,
    target_left: f32,
    target_right: f32,
) -> Option<DetailsColumnInsertionPreview> {
    if !pointer_x.is_finite()
        || !target_left.is_finite()
        || !target_right.is_finite()
        || target_left >= target_right
        || *dragged == explorer_model::ColumnId::Name
        || !order.iter().any(|column| column == dragged)
        || !order.iter().any(|column| column == target)
    {
        return None;
    }

    if dragged == target {
        let source = order.iter().position(|column| column == dragged)?;
        return Some(DetailsColumnInsertionPreview {
            order: order.to_vec(),
            before: order.get(source.saturating_add(1)).cloned(),
        });
    }

    let mut prospective = order
        .iter()
        .filter(|column| *column != dragged)
        .cloned()
        .collect::<Vec<_>>();
    let target_index = prospective.iter().position(|column| column == target)?;
    let midpoint = target_left + (target_right - target_left) / 2.0;
    let insertion = if pointer_x < midpoint {
        target_index
    } else {
        target_index.saturating_add(1)
    }
    .max(1)
    .min(prospective.len());
    let before = prospective.get(insertion).cloned();
    prospective.insert(insertion, dragged.clone());
    Some(DetailsColumnInsertionPreview {
        order: prospective,
        before,
    })
}

#[derive(Clone, Copy, Debug)]
struct SidePaneResizeSession {
    tab_id: TabId,
    pointer_x: f32,
    width: u16,
    details: bool,
}

#[derive(Clone, Debug)]
struct PendingBreadcrumbMenu {
    context: RequestContext,
    segment_id: explorer_model::BreadcrumbSegmentId,
    menu_generation: u64,
}

const NAVIGATION_TREE_NODE_LIMIT: usize = 4096;

#[derive(Clone, Debug, Default)]
struct NavigationTreeNode {
    children: Vec<explorer_model::BreadcrumbMenuItem>,
    loaded: bool,
    loading: bool,
    error: Option<String>,
    request: Option<RequestContext>,
    request_generation: u64,
}

#[derive(Clone, Debug, Default)]
struct NavigationTreeState {
    expanded: HashSet<LocationDescriptor>,
    nodes: HashMap<LocationDescriptor, NavigationTreeNode>,
    next_request_generation: u64,
}

#[derive(Clone, Debug)]
pub(crate) struct PendingRightDrop {
    paths: Vec<std::path::PathBuf>,
    destination: LocationDescriptor,
    pub allowed: explorer_model::TransferEffects,
    tab_id: TabId,
    generation: explorer_model::Generation,
}

impl Default for AppViewState {
    fn default() -> Self {
        Self::with_initial_location(HistoryEntry::new(
            LocationDescriptor::file_system(r"C:\"),
            "This PC",
        ))
    }
}

impl AppViewState {
    pub fn with_initial_location(initial: HistoryEntry) -> Self {
        Self::with_initial_location_and_drag_threshold(initial, (4.0, 4.0))
    }

    pub fn with_initial_location_and_drag_threshold(
        initial: HistoryEntry,
        drag_threshold: (f32, f32),
    ) -> Self {
        let tabs = ExplorerWindowState::new(initial);
        let initial_tab_id = tabs.active_tab_id();
        let tab_focus = HashMap::from([(initial_tab_id, FocusSurface::FileView)]);
        Self {
            current_theme: ThemeMode::Light,
            navigation_pane_width: LayoutTokens::WINDOWS_11.navigation_pane_default_width,
            focus: FocusCoordinator::default(),
            tabs,
            directory_cache: DirectorySnapshotCache::default(),
            tab_focus,
            navigation_focus: HashMap::new(),
            close_requested: false,
            divider: DividerInteraction::default(),
            operation_center: OperationCenterState::default(),
            operation_terminal_notice: None,
            rename_editor: None,
            pending_new_folder_rename: None,
            permanent_delete_confirmation: None,
            permanent_delete_confirmation_focus: PermanentDeleteDialogTarget::Delete,
            lock_recovery: None,
            pending_lock_recovery_command: None,
            clipboard: explorer_model::ClipboardState::default(),
            drag_session: explorer_model::DragSession::new(drag_threshold.0, drag_threshold.1),
            pending_drag_command: None,
            pending_new_tab_command: None,
            drop_target_row: None,
            pending_right_drop: None,
            context_menu_error: None,
            thumbnail_cache_notice: None,
            quick_access: QuickAccessPins::default(),
            bookmarks: explorer_model::Bookmarks::default(),
            recent_items: RecentItems::new(64, 30 * 24 * 60 * 60, Vec::new()),
            quick_access_notice: None,
            bookmark_notice: None,
            bookmark_overflow_open: false,
            bookmark_folder_menu: None,
            bookmark_toolbar_context_menu: None,
            bookmark_context_menu: None,
            remote_context_menu: None,
            remote_properties: None,
            expanded_bookmark_folders: HashSet::new(),
            bookmark_folder_delete_confirmation: None,
            bookmark_editor: None,
            bookmark_folder_editor: None,
            broker_health: BrokerUiHealth::Healthy,
            session_reset_confirmation: None,
            confirmed_session_reset: None,
            last_session_reset: None,
            session_reset_notice: None,
            pending_context_menu: None,
            pending_context_hit: None,
            pending_context_extended_verbs: false,
            queued_context_menu: None,
            pending_context_menu_command: None,
            ancestry_requests: HashMap::new(),
            breadcrumb_menu_requests: HashMap::new(),
            navigation_history_menu: None,
            navigation_trees: HashMap::from([(initial_tab_id, NavigationTreeState::default())]),
            sort_menu_open: false,
            sort_menu_index: 0,
            new_menu_open: false,
            new_menu_index: 0,
            new_items: default_new_items(),
            view_menu_open: false,
            view_menu_index: 0,
            more_menu_open: false,
            more_menu_index: 0,
            about_dialog_open: false,
            about_info: AboutInfoV1 {
                version: "unknown".to_owned(),
                build_date: "unknown".to_owned(),
                git_hash: "unknown".to_owned(),
                author: "unknown".to_owned(),
            },
            extensions_menu_open: false,
            extension_command_panel: None,
            tortoise_git_available: false,
            loaded_extension_summary: None,
            folder_options: None,
            folder_options_applied_revision: 0,
            extensions: official_extensions_v1(),
            restore_previous_session: true,
            view_show_submenu_open: false,
            column_registry: explorer_model::ColumnRegistry::built_ins(),
            details_column_resize: None,
            details_column_drag: None,
            details_column_menu: None,
            details_filter_menu: None,
            details_filters: HashMap::from([(
                initial_tab_id,
                crate::file_view::DetailsFilters::default(),
            )]),
            side_pane_resize: None,
            scrollbar_drag: None,
            marquee: None,
            file_view_typeahead: None,
            folder_size_sort_values: HashMap::new(),
            builtin_count_sort_values: HashMap::new(),
            code_lines_sort_values: HashMap::new(),
            presentation_cache: Arc::new(Mutex::new(
                crate::file_view::DirectoryPresentationCache::default(),
            )),
        }
    }

    pub const fn broker_health(&self) -> BrokerUiHealth {
        self.broker_health
    }

    pub fn column_registry(&self) -> &explorer_model::ColumnRegistry {
        &self.column_registry
    }

    /// Installs the one UI-owned runtime column without giving the UI a
    /// general extension-host dependency. The current tab receives an entry
    /// once; subsequent runtime refreshes preserve its width and visibility.
    pub(crate) fn install_visual_column_descriptor(
        &mut self,
        descriptor: explorer_model::ColumnDescriptor,
    ) -> bool {
        if !crate::folder_size_column::is_supported_folder_size_descriptor(&descriptor) {
            return false;
        }
        if self
            .column_registry
            .replace_package(
                crate::folder_size_column::FOLDER_SIZE_COLUMN_PACKAGE_ID,
                [descriptor.clone()],
            )
            .is_err()
        {
            return false;
        }
        self.tabs
            .active_tab_mut()
            .view
            .settings
            .details_layout
            .ensure_descriptor(&descriptor, true)
    }

    pub(crate) fn install_code_lines_column_descriptor(
        &mut self,
        descriptor: explorer_model::ColumnDescriptor,
    ) -> bool {
        if !crate::code_lines_column::is_supported_code_lines_descriptor(&descriptor) {
            return false;
        }
        let explorer_model::ColumnId::Extension { package_id, .. } = &descriptor.id else {
            return false;
        };
        if self
            .column_registry
            .replace_package(package_id, [descriptor.clone()])
            .is_err()
        {
            return false;
        }
        self.tabs
            .active_tab_mut()
            .view
            .settings
            .details_layout
            .ensure_descriptor(&descriptor, true)
    }

    pub(crate) fn set_folder_size_sort_values(
        &mut self,
        values: HashMap<ShellItemId, Option<u64>>,
    ) -> bool {
        if self.folder_size_sort_values == values {
            return false;
        }
        self.folder_size_sort_values = values;
        self.presentation_cache
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clear();
        true
    }

    pub(crate) fn set_code_lines_sort_values(
        &mut self,
        column_id: explorer_model::ColumnId,
        values: HashMap<ShellItemId, Option<u64>>,
    ) -> bool {
        if self.code_lines_sort_values.get(&column_id) == Some(&values) {
            return false;
        }
        self.code_lines_sort_values.insert(column_id, values);
        self.presentation_cache
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clear();
        true
    }

    pub(crate) fn set_builtin_count_sort_values(
        &mut self,
        column_id: explorer_model::ColumnId,
        values: HashMap<ShellItemId, Option<u64>>,
    ) -> bool {
        if self.builtin_count_sort_values.get(&column_id) == Some(&values) {
            return false;
        }
        self.builtin_count_sort_values.insert(column_id, values);
        self.presentation_cache
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clear();
        true
    }

    pub fn set_broker_health(&mut self, health: BrokerUiHealth) {
        self.broker_health = health;
    }

    /// Rehydrates ordered application-owned pins without treating a path as Shell identity.
    pub(crate) fn configure_quick_access(&mut self, pins: Vec<PersistedQuickAccessPin>) {
        let mut hydrated = QuickAccessPins::default();
        for (index, pin) in pins.into_iter().enumerate() {
            let mut runtime_id = b"quick-access-runtime:".to_vec();
            runtime_id.extend_from_slice(&u64::try_from(index).unwrap_or(u64::MAX).to_le_bytes());
            let Some(stable_id) = ShellItemId::from_provider_bytes(runtime_id) else {
                continue;
            };
            let _ = hydrated.pin(ShellIdentity {
                stable_id,
                descriptor: pin.location.clone(),
                display_name: pin.display_name,
                parsing_name: match pin.location {
                    LocationDescriptor::ParsingName(value) => Some(value),
                    _ => None,
                },
                serializable: true,
                nonserializable_reason: None,
            });
        }
        self.quick_access = hydrated;
    }

    pub(crate) fn persisted_quick_access(&self) -> Vec<PersistedQuickAccessPin> {
        self.quick_access
            .entries()
            .iter()
            .map(|pin| PersistedQuickAccessPin {
                location: pin.identity.descriptor.clone(),
                display_name: pin.identity.display_name.clone(),
                order: pin.order,
            })
            .collect()
    }

    pub(crate) fn configure_bookmarks(&mut self, bookmarks: explorer_model::Bookmarks) {
        self.bookmarks = bookmarks;
    }

    pub(crate) const fn bookmarks(&self) -> &explorer_model::Bookmarks {
        &self.bookmarks
    }

    pub(crate) fn add_bookmark_folder(
        &mut self,
        name: String,
        parent_id: Option<explorer_model::BookmarkFolderId>,
    ) -> explorer_model::BookmarkMutation {
        self.bookmarks.begin_add_folder(name, parent_id)
    }

    pub(crate) fn remove_bookmark_folder(
        &mut self,
        id: explorer_model::BookmarkFolderId,
        allow_non_empty: bool,
    ) -> explorer_model::BookmarkMutation {
        self.bookmarks.begin_remove_folder(id, allow_non_empty)
    }

    pub(crate) fn remove_bookmark(
        &mut self,
        id: explorer_model::BookmarkId,
    ) -> explorer_model::BookmarkMutation {
        self.bookmarks.begin_remove(id)
    }

    pub(crate) fn reorder_bookmark(
        &mut self,
        id: explorer_model::BookmarkId,
        destination: usize,
    ) -> explorer_model::BookmarkMutation {
        self.bookmarks.begin_reorder(id, destination)
    }

    pub(crate) fn move_bookmark_to_folder(
        &mut self,
        id: explorer_model::BookmarkId,
        parent_id: Option<explorer_model::BookmarkFolderId>,
    ) -> explorer_model::BookmarkMutation {
        self.bookmarks.begin_move_to_folder(id, parent_id)
    }

    pub(crate) fn rollback_bookmark(&mut self, mutation: explorer_model::BookmarkMutation) {
        self.bookmarks.rollback(mutation);
    }

    pub(crate) fn quick_access_navigation_pins(&self) -> Vec<(String, LocationDescriptor)> {
        self.quick_access
            .entries()
            .iter()
            .map(|pin| {
                (
                    pin.identity.display_name.clone(),
                    pin.identity.descriptor.clone(),
                )
            })
            .collect()
    }

    pub(crate) fn navigation_node_expanded(&self, location: &LocationDescriptor) -> bool {
        self.navigation_trees
            .get(&self.tabs.active_tab_id())
            .is_some_and(|tree| tree.expanded.contains(location))
    }

    pub(crate) fn focused_navigation_location(&self) -> Option<&LocationDescriptor> {
        self.navigation_focus.get(&self.tabs.active_tab_id())
    }

    pub(crate) fn set_navigation_focus(&mut self, location: LocationDescriptor) {
        self.navigation_focus
            .insert(self.tabs.active_tab_id(), location);
    }

    pub(crate) fn navigation_node_children(
        &self,
        location: &LocationDescriptor,
    ) -> &[explorer_model::BreadcrumbMenuItem] {
        self.navigation_trees
            .get(&self.tabs.active_tab_id())
            .and_then(|tree| tree.nodes.get(location))
            .map_or(&[], |node| node.children.as_slice())
    }

    /// Returns every dynamic navigation-tree location whose Shell icon may be visible.
    /// Static roots are collected by the presentation builder; this covers expanded drives,
    /// folders, and namespace providers discovered after startup.
    pub(crate) fn navigation_icon_locations(&self) -> Vec<LocationDescriptor> {
        let Some(tree) = self.navigation_trees.get(&self.tabs.active_tab_id()) else {
            return Vec::new();
        };
        let mut locations = HashSet::new();
        for (parent, node) in &tree.nodes {
            if tree.expanded.contains(parent) {
                locations.insert(parent.clone());
                locations.extend(node.children.iter().map(|child| child.location.clone()));
            }
        }
        locations.into_iter().collect()
    }

    pub(crate) fn navigation_node_loading(&self, location: &LocationDescriptor) -> bool {
        self.navigation_trees
            .get(&self.tabs.active_tab_id())
            .and_then(|tree| tree.nodes.get(location))
            .is_some_and(|node| node.loading)
    }

    pub(crate) fn navigation_node_error(&self, location: &LocationDescriptor) -> Option<&str> {
        self.navigation_trees
            .get(&self.tabs.active_tab_id())
            .and_then(|tree| tree.nodes.get(location))
            .and_then(|node| node.error.as_deref())
    }

    pub(crate) fn toggle_navigation_node(&mut self, location: LocationDescriptor) -> bool {
        let tab_id = self.tabs.active_tab_id();
        let tree = self.navigation_trees.entry(tab_id).or_default();
        if tree.expanded.remove(&location) {
            if let Some(node) = tree.nodes.get_mut(&location) {
                if let Some(request) = node.request.take() {
                    request.cancellation.cancel();
                }
                node.loading = false;
            }
            false
        } else {
            tree.expanded.insert(location);
            true
        }
    }

    pub(crate) fn begin_navigation_node_request(
        &mut self,
        parent: LocationDescriptor,
    ) -> Option<ExplorerCommand> {
        self.begin_navigation_node_request_for_tab(self.tabs.active_tab_id(), parent)
    }

    fn begin_navigation_node_request_for_tab(
        &mut self,
        tab_id: TabId,
        parent: LocationDescriptor,
    ) -> Option<ExplorerCommand> {
        let generation = self
            .tabs
            .tabs()
            .iter()
            .find(|tab| tab.id == tab_id)?
            .generation;
        let tree = self.navigation_trees.entry(tab_id).or_default();
        let node = tree.nodes.entry(parent.clone()).or_default();
        if node.loaded || node.loading || !tree.expanded.contains(&parent) {
            return None;
        }
        tree.next_request_generation = tree.next_request_generation.saturating_add(1);
        let request_generation = tree.next_request_generation;
        let context = RequestContext::new(tab_id, generation);
        node.loading = true;
        node.error = None;
        node.request = Some(context.clone());
        node.request_generation = request_generation;
        Some(ExplorerCommand::EnumerateChildContainers {
            context,
            segment_id: navigation_segment_id(&parent),
            menu_generation: request_generation,
            parent,
        })
    }

    fn invalidate_navigation_node(&mut self, tab_id: TabId, location: &LocationDescriptor) {
        let tree = self.navigation_trees.entry(tab_id).or_default();
        let node = tree.nodes.entry(location.clone()).or_default();
        if let Some(request) = node.request.take() {
            request.cancellation.cancel();
        }
        node.children.clear();
        node.loaded = false;
        node.loading = false;
        node.error = None;
    }

    /// Invalidates expanded navigation rows affected by a watcher or successful Shell mutation.
    /// The current directory view and navigation tree intentionally use separate enumeration
    /// requests, so refreshing one must never leave the other with a stale child cache.
    pub(crate) fn navigation_reconciliation_targets(
        &self,
        event: &ExplorerEvent,
    ) -> Option<(TabId, Vec<LocationDescriptor>)> {
        let (tab_id, mut locations) = match event {
            ExplorerEvent::DirectoryChanged {
                tab_id,
                generation,
                changes,
            } if !changes.is_empty() => {
                let tab = self.tabs.tabs().iter().find(|tab| tab.id == *tab_id)?;
                if tab.generation != *generation {
                    return None;
                }
                let location = tab.history.current().map(|entry| entry.location.clone())?;
                (*tab_id, vec![location])
            }
            ExplorerEvent::OperationFinished {
                context,
                outcome: OperationTerminal::Finished | OperationTerminal::Partial { .. },
            } => {
                let tab = self
                    .tabs
                    .tabs()
                    .iter()
                    .find(|tab| tab.id == context.tab_id)?;
                let record = self.operation_center.get(context.request_id)?;
                if tab.generation != context.generation || record.terminal.is_some() {
                    return None;
                }
                (
                    context.tab_id,
                    navigation_locations_for_operation(&record.request),
                )
            }
            ExplorerEvent::ContextMenuFinished {
                context,
                outcome: explorer_model::ContextMenuOutcome::Invoked { .. },
            } => {
                let tab = self
                    .tabs
                    .tabs()
                    .iter()
                    .find(|tab| tab.id == context.tab_id)?;
                if tab.generation != context.generation {
                    return None;
                }
                let location = tab.history.current().map(|entry| entry.location.clone())?;
                (context.tab_id, vec![location])
            }
            _ => return None,
        };
        let mut unique = HashSet::new();
        locations.retain(|location| unique.insert(location.clone()));
        Some((tab_id, locations))
    }

    pub(crate) fn begin_navigation_reconciliation(
        &mut self,
        reconciliation: Option<(TabId, Vec<LocationDescriptor>)>,
    ) -> Vec<ExplorerCommand> {
        let Some((tab_id, locations)) = reconciliation else {
            return Vec::new();
        };
        for location in &locations {
            self.invalidate_navigation_node(tab_id, location);
        }
        locations
            .into_iter()
            .filter_map(|location| {
                let command = self.begin_navigation_node_request_for_tab(tab_id, location);
                if let Some(ExplorerCommand::EnumerateChildContainers {
                    context,
                    menu_generation,
                    parent,
                    ..
                }) = &command
                {
                    tracing::debug!(
                        request_id = ?context.request_id,
                        generation = context.generation.value(),
                        menu_generation,
                        ?parent,
                        "navigation reconciliation request started"
                    );
                }
                command
            })
            .collect()
    }

    fn navigation_request_location(
        &self,
        context: &RequestContext,
        segment_id: explorer_model::BreadcrumbSegmentId,
        request_generation: u64,
    ) -> Option<LocationDescriptor> {
        self.navigation_trees.get(&context.tab_id).and_then(|tree| {
            tree.nodes.iter().find_map(|(location, node)| {
                (node.request.as_ref() == Some(context)
                    && node.request_generation == request_generation
                    && navigation_segment_id(location) == segment_id)
                    .then(|| location.clone())
            })
        })
    }

    /// Toggles every selected reconstructible item transactionally. The caller can restore the
    /// returned snapshot if the persistence bridge rejects the durable update.
    pub(crate) fn toggle_selected_quick_access(&mut self) -> Option<QuickAccessPins> {
        let tab = self.tabs.active_tab();
        let snapshot = tab.visible_snapshot()?;
        let selected = snapshot
            .entries()
            .iter()
            .filter(|entry| tab.selection.contains(&entry.id))
            .cloned()
            .collect::<Vec<_>>();
        if selected.is_empty() {
            return None;
        }
        let previous = self.quick_access.clone();
        let mut changed = false;
        for entry in selected {
            if let Some(id) = self
                .quick_access
                .id_for_descriptor(&entry.location)
                .cloned()
            {
                changed |= self.quick_access.unpin(&id).is_some();
            } else {
                changed |= self.quick_access.pin(ShellIdentity {
                    stable_id: entry.id,
                    descriptor: entry.location.clone(),
                    display_name: entry.display_name,
                    parsing_name: match entry.location {
                        LocationDescriptor::ParsingName(value) => Some(value),
                        _ => None,
                    },
                    serializable: true,
                    nonserializable_reason: None,
                });
            }
        }
        if changed {
            self.quick_access_notice = Some("Quick Access pins updated.".to_owned());
            Some(previous)
        } else {
            None
        }
    }

    pub(crate) fn rollback_quick_access(&mut self, previous: QuickAccessPins) {
        self.quick_access = previous;
        self.quick_access_notice =
            Some("Quick Access could not be saved; the previous order was restored.".to_owned());
    }

    pub(crate) fn record_recent_row(&mut self, row_index: usize, now_epoch_seconds: u64) -> bool {
        let Some(entry) = self
            .tabs
            .active_tab()
            .visible_snapshot()
            .and_then(|snapshot| snapshot.entries().get(row_index))
            .cloned()
        else {
            return false;
        };
        self.recent_items.record(
            ShellIdentity {
                stable_id: entry.id,
                descriptor: entry.location.clone(),
                display_name: entry.display_name,
                parsing_name: match entry.location {
                    LocationDescriptor::ParsingName(value) => Some(value),
                    _ => None,
                },
                serializable: true,
                nonserializable_reason: None,
            },
            now_epoch_seconds,
        )
    }

    pub(crate) fn synthetic_root_entries(
        &self,
        root: explorer_model::SyntheticRoot,
        now_epoch_seconds: u64,
    ) -> Vec<explorer_model::FileEntry> {
        let identities: Vec<ShellIdentity> = match root {
            explorer_model::SyntheticRoot::QuickAccess => self
                .quick_access
                .entries()
                .iter()
                .map(|pin| pin.identity.clone())
                .collect(),
            explorer_model::SyntheticRoot::Home => explorer_model::aggregate_home(
                &self.quick_access,
                self.recent_items.visible(now_epoch_seconds),
            )
            .into_iter()
            .cloned()
            .collect(),
        };
        identities
            .into_iter()
            .map(|identity| explorer_model::FileEntry {
                id: identity.stable_id,
                display_name: identity.display_name,
                location: identity.descriptor,
                is_container: true,
                metadata: explorer_model::FileEntryMetadata {
                    type_display: Some("Pinned or recent location".to_owned()),
                    namespace_capabilities: explorer_model::NamespaceCapabilities::from_public_bits(
                        explorer_model::NamespaceCapabilities::OPEN
                            | explorer_model::NamespaceCapabilities::PIN
                            | explorer_model::NamespaceCapabilities::PROPERTIES
                            | explorer_model::NamespaceCapabilities::CONTEXT_MENU,
                    ),
                    ..explorer_model::FileEntryMetadata::default()
                },
            })
            .collect()
    }

    /// Builds presentation state from validated restored tabs and resets all transient surfaces.
    pub fn with_restored_window_and_drag_threshold(
        tabs: ExplorerWindowState,
        drag_threshold: (f32, f32),
    ) -> Self {
        let tab_focus = tabs
            .tabs()
            .iter()
            .map(|tab| (tab.id, FocusSurface::FileView))
            .collect();
        let mut state = Self::with_initial_location_and_drag_threshold(
            tabs.active_tab()
                .history
                .current()
                .cloned()
                .unwrap_or_else(|| {
                    HistoryEntry::new(LocationDescriptor::file_system(r"C:\"), "This PC")
                }),
            drag_threshold,
        );
        state.tabs = tabs;
        state.tab_focus = tab_focus;
        state.focus.restore_context(FocusSurface::FileView);
        state
    }

    pub const fn current_theme(&self) -> ThemeMode {
        self.current_theme
    }

    pub const fn navigation_pane_width(&self) -> LogicalPx {
        self.navigation_pane_width
    }

    pub const fn focused_surface(&self) -> FocusSurface {
        self.focus.current()
    }

    pub const fn previous_focus(&self) -> Option<FocusSurface> {
        self.focus.previous()
    }

    pub fn tabs(&self) -> &ExplorerWindowState {
        &self.tabs
    }

    pub const fn operation_center(&self) -> &OperationCenterState {
        &self.operation_center
    }

    pub(crate) fn latest_operation_terminal_elapsed(&self, now: Instant) -> Option<Duration> {
        let (request_id, terminal_at) = self.operation_terminal_notice?;
        self.operation_center
            .latest()
            .is_some_and(|record| record.id == request_id && record.phase.is_terminal())
            .then(|| now.saturating_duration_since(terminal_at))
    }

    pub(crate) fn operation_notice_needs_repaint(&self, now: Instant) -> bool {
        self.latest_operation_terminal_elapsed(now)
            .is_some_and(|elapsed| {
                elapsed >= Duration::from_secs(7) && elapsed <= Duration::from_millis(8_050)
            })
    }

    pub(crate) fn accepts_ancestry_context(&self, context: &RequestContext) -> bool {
        self.ancestry_requests
            .get(&context.tab_id)
            .is_some_and(|active| active.validate_event(context).is_ok())
    }

    pub const fn rename_editor(&self) -> Option<&explorer_model::RenameEditorState> {
        self.rename_editor.as_ref()
    }

    pub(crate) fn provisional_new_folder_entry(&self) -> Option<explorer_model::FileEntry> {
        let pending = self.pending_new_folder_rename.as_ref()?;
        let editor = self.rename_editor.as_ref()?;
        if editor.item.id != pending.item_id {
            return None;
        }
        Some(explorer_model::FileEntry {
            id: pending.item_id.clone(),
            display_name: editor.buffer.clone(),
            location: pending.parent.clone(),
            is_container: true,
            metadata: explorer_model::FileEntryMetadata {
                type_display: Some("檔案資料夾".to_owned()),
                ..explorer_model::FileEntryMetadata::default()
            },
        })
    }

    pub fn permanent_delete_confirmation_count(&self) -> Option<usize> {
        self.permanent_delete_confirmation
            .as_ref()
            .map(|confirmation| confirmation.items.len())
    }

    pub(crate) fn permanent_delete_confirmation_focus(
        &self,
    ) -> Option<PermanentDeleteDialogTarget> {
        self.permanent_delete_confirmation
            .as_ref()
            .map(|_| self.permanent_delete_confirmation_focus)
    }

    pub const fn clipboard(&self) -> &explorer_model::ClipboardState {
        &self.clipboard
    }

    pub(crate) fn paste_available(&self) -> bool {
        !matches!(
            self.clipboard,
            explorer_model::ClipboardState::None { .. }
                | explorer_model::ClipboardState::Unsupported { .. }
        ) && self.active_presentation().can_write
    }

    pub const fn view_menu_open(&self) -> bool {
        self.view_menu_open
    }

    pub const fn sort_menu_open(&self) -> bool {
        self.sort_menu_open
    }

    pub const fn new_menu_open(&self) -> bool {
        self.new_menu_open
    }

    pub const fn new_menu_index(&self) -> usize {
        self.new_menu_index
    }

    pub fn new_items(&self) -> &[explorer_model::ShellNewItemDescriptor] {
        &self.new_items
    }

    pub fn configure_new_items(&mut self, mut items: Vec<explorer_model::ShellNewItemDescriptor>) {
        let mut defaults = default_new_items();
        for item in items.drain(..) {
            if item.validate().is_ok()
                && !defaults
                    .iter()
                    .any(|existing| existing.stable_id == item.stable_id)
            {
                defaults.push(item);
            }
        }
        self.new_items = defaults;
        self.new_menu_index = self
            .new_menu_index
            .min(self.new_items.len().saturating_sub(1));
    }

    pub const fn sort_menu_index(&self) -> usize {
        self.sort_menu_index
    }

    pub const fn view_menu_index(&self) -> usize {
        self.view_menu_index
    }

    pub const fn more_menu_open(&self) -> bool {
        self.more_menu_open
    }

    pub const fn more_menu_index(&self) -> usize {
        self.more_menu_index
    }

    pub const fn extensions_menu_open(&self) -> bool {
        self.extensions_menu_open
    }

    pub const fn extension_command_panel(&self) -> Option<ExtensionCommandPanel> {
        self.extension_command_panel
    }

    pub const fn tortoise_git_available(&self) -> bool {
        self.tortoise_git_available
    }

    pub fn loaded_extension_summary(&self) -> Option<&str> {
        self.loaded_extension_summary.as_deref()
    }

    pub(crate) fn set_loaded_extension_summary(&mut self, summary: Option<String>) {
        self.loaded_extension_summary = summary;
    }

    pub(crate) fn set_tortoise_git_available(&mut self, available: bool) {
        self.tortoise_git_available = available;
        if !available {
            self.extensions_menu_open = false;
        }
    }

    pub const fn scrollbar_drag_session(&self) -> Option<ScrollbarDragSession> {
        self.scrollbar_drag
    }

    pub(crate) const fn marquee_session(&self) -> Option<&MarqueeSelectionSession> {
        self.marquee.as_ref()
    }

    pub(crate) fn begin_marquee(&mut self, x: f32, y: f32, additive: bool) -> bool {
        if !x.is_finite()
            || !y.is_finite()
            || self.details_column_resize.is_some()
            || self.side_pane_resize.is_some()
            || self.scrollbar_drag.is_some()
            || self.divider.is_dragging()
        {
            return false;
        }
        let base_selection = if additive {
            self.tabs.active_tab().selection.clone()
        } else {
            explorer_model::SelectionModel::default()
        };
        if !additive {
            self.tabs.active_tab_mut().selection.clear();
        }
        self.marquee = Some(MarqueeSelectionSession {
            origin_x: x,
            origin_y: y,
            current_x: x,
            current_y: y,
            base_selection,
        });
        true
    }

    #[allow(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        clippy::cast_precision_loss,
        reason = "viewport item grids are finite UI-sized values bounded by the visible entry count"
    )]
    pub(crate) fn update_marquee(
        &mut self,
        x: f32,
        y: f32,
        scroll_y: f32,
        viewport_width: f32,
        layout: LayoutTokens,
    ) -> bool {
        if !x.is_finite() || !y.is_finite() {
            return false;
        }
        let Some((origin_x, origin_y, base_selection)) = self.marquee.as_mut().map(|session| {
            session.current_x = x;
            session.current_y = y;
            (
                session.origin_x,
                session.origin_y,
                session.base_selection.clone(),
            )
        }) else {
            return false;
        };
        if (x - origin_x).abs() < 4.0 && (y - origin_y).abs() < 4.0 {
            return true;
        }
        let left = origin_x.min(x);
        let right = origin_x.max(x);
        let top = origin_y.min(y) + scroll_y.max(0.0);
        let bottom = origin_y.max(y) + scroll_y.max(0.0);
        let settings = self.view_settings();
        let order = self.presentation_ids();
        let mut selected = base_selection;
        let metrics = crate::chrome::spatial_grid_metrics(&settings, layout);
        let grid = crate::chrome::spatial_grid_layout(metrics, viewport_width, order.len());
        let metrics = grid.metrics;
        let item_width = metrics.cell_width.max(1.0);
        let item_height = metrics.cell_height.max(1.0);
        let header = if settings.mode == explorer_model::ViewMode::Details {
            layout.details_header_height.value()
        } else {
            0.0
        };
        let columns = grid.columns;
        for (index, id) in order.iter().enumerate() {
            let column = index % columns;
            let row = index / columns;
            let item_left = if columns == 1 {
                0.0
            } else {
                column as f32 * item_width
            };
            let item_top = header + row as f32 * item_height;
            let item_right = if columns == 1 {
                viewport_width.max(item_width)
            } else {
                item_left + item_width
            };
            let item_bottom = item_top + item_height;
            if left <= item_right && right >= item_left && top <= item_bottom && bottom >= item_top
            {
                selected.select_additive(id.clone());
            }
        }
        self.tabs.active_tab_mut().selection = selected;
        true
    }

    pub(crate) fn end_marquee(&mut self) -> bool {
        self.marquee.take().is_some()
    }

    pub(crate) fn begin_scrollbar_drag(&mut self, kind: ScrollbarKind, grab_offset_y: f32) -> bool {
        let Some(session) = ScrollbarDragSession::new(kind, grab_offset_y) else {
            return false;
        };
        self.scrollbar_drag = Some(session);
        true
    }

    pub(crate) fn end_scrollbar_drag(&mut self, _reason: ScrollbarTerminal) -> bool {
        self.scrollbar_drag.take().is_some()
    }

    pub(crate) fn navigation_history_len(&self, direction: NavigationHistoryDirection) -> usize {
        let history = &self.tabs.active_tab().history;
        match direction {
            NavigationHistoryDirection::Back => history.back_entries().len(),
            NavigationHistoryDirection::Forward => history.forward_entries().len(),
        }
    }

    pub(crate) fn navigation_history_entries(
        &self,
        direction: NavigationHistoryDirection,
    ) -> Vec<HistoryEntry> {
        let history = &self.tabs.active_tab().history;
        match direction {
            NavigationHistoryDirection::Back => history.back_entries(),
            NavigationHistoryDirection::Forward => history.forward_entries(),
        }
        .iter()
        .rev()
        .cloned()
        .collect()
    }

    pub(crate) fn navigation_history_menu_direction(&self) -> Option<NavigationHistoryDirection> {
        self.navigation_history_menu.map(|menu| menu.direction)
    }

    pub(crate) fn navigation_history_menu_index(&self) -> usize {
        self.navigation_history_menu
            .map_or(0, |menu| menu.focused_index)
    }

    pub(crate) fn open_navigation_history_menu(
        &mut self,
        direction: NavigationHistoryDirection,
    ) -> bool {
        if self.navigation_history_len(direction) == 0 {
            return false;
        }
        self.close_address_menu();
        self.navigation_history_menu = Some(NavigationHistoryMenuState {
            direction,
            focused_index: 0,
        });
        true
    }

    pub(crate) fn close_navigation_history_menu(&mut self) {
        self.navigation_history_menu = None;
    }

    pub(crate) fn move_navigation_history_focus(&mut self, direction: i8) -> bool {
        let count = self
            .navigation_history_menu
            .map_or(0, |menu| self.navigation_history_len(menu.direction));
        let Some(menu) = &mut self.navigation_history_menu else {
            return false;
        };
        menu.focused_index =
            move_bounded_menu_index(menu.focused_index, direction, count.saturating_sub(1));
        true
    }

    pub(crate) fn set_navigation_history_focus(&mut self, index: usize) -> bool {
        let Some(menu) = self.navigation_history_menu else {
            return false;
        };
        if index >= self.navigation_history_len(menu.direction) {
            return false;
        }
        let Some(menu) = &mut self.navigation_history_menu else {
            return false;
        };
        if menu.focused_index == index {
            return false;
        }
        menu.focused_index = index;
        true
    }

    pub(crate) fn toggle_sort_menu(&mut self) {
        self.sort_menu_open = !self.sort_menu_open;
        if self.sort_menu_open {
            self.details_column_menu = None;
            self.details_filter_menu = None;
            self.sort_menu_index = 0;
            self.close_view_menu();
            self.more_menu_open = false;
            self.extensions_menu_open = false;
            self.new_menu_open = false;
        }
    }

    pub(crate) fn toggle_new_menu(&mut self) {
        self.new_menu_open = !self.new_menu_open;
        if self.new_menu_open {
            self.details_column_menu = None;
            self.details_filter_menu = None;
            self.new_menu_index = 0;
            self.sort_menu_open = false;
            self.close_view_menu();
            self.more_menu_open = false;
            self.extensions_menu_open = false;
        }
    }

    pub(crate) fn close_new_menu(&mut self) {
        self.new_menu_open = false;
    }

    pub(crate) fn move_new_menu_focus(&mut self, direction: i8) {
        self.new_menu_index = move_bounded_menu_index(
            self.new_menu_index,
            direction,
            self.new_items.len().saturating_sub(1),
        );
    }

    pub(crate) fn close_sort_menu(&mut self) {
        self.sort_menu_open = false;
    }

    pub(crate) fn move_sort_menu_focus(&mut self, direction: i8) {
        self.sort_menu_index = move_bounded_menu_index(self.sort_menu_index, direction, 5);
    }

    pub(crate) fn set_sort_menu_focus(&mut self, index: usize) -> bool {
        if !self.sort_menu_open || index > 5 || self.sort_menu_index == index {
            return false;
        }
        self.sort_menu_index = index;
        true
    }

    pub(crate) fn toggle_more_menu(&mut self) {
        self.more_menu_open = !self.more_menu_open;
        if self.more_menu_open {
            self.details_column_menu = None;
            self.details_filter_menu = None;
            self.more_menu_index = 0;
            self.sort_menu_open = false;
            self.close_view_menu();
            self.extensions_menu_open = false;
            self.new_menu_open = false;
        }
    }

    pub(crate) fn close_more_menu(&mut self) {
        self.more_menu_open = false;
    }

    pub(crate) fn toggle_extensions_menu(&mut self) {
        self.extensions_menu_open = !self.extensions_menu_open;
        if !self.extensions_menu_open {
            self.extension_command_panel = None;
        }
        if self.extensions_menu_open {
            self.details_column_menu = None;
            self.details_filter_menu = None;
            self.sort_menu_open = false;
            self.more_menu_open = false;
            self.close_view_menu();
            self.new_menu_open = false;
        }
    }

    pub(crate) fn close_extensions_menu(&mut self) {
        self.extensions_menu_open = false;
        self.extension_command_panel = None;
    }

    pub(crate) fn open_extension_command_panel(&mut self, contribution_id: &str) -> bool {
        let panel = match contribution_id {
            "rust-exif-rename:button" => ExtensionCommandPanel::ExifRename,
            "lua-bulk-folder:button" => ExtensionCommandPanel::BulkFolder,
            _ => return false,
        };
        self.extensions_menu_open = true;
        self.extension_command_panel = Some(panel);
        true
    }

    pub(crate) fn close_extension_command_panel(&mut self) -> bool {
        self.extension_command_panel.take().is_some()
    }

    pub(crate) fn move_more_menu_focus(&mut self, direction: i8) {
        self.more_menu_index = match direction {
            i8::MIN..=-2 => 0,
            -1 => self.more_menu_index.saturating_sub(1),
            1 => self.more_menu_index.saturating_add(1).min(9),
            2..=i8::MAX => 9,
            _ => self.more_menu_index,
        };
    }

    pub(crate) fn set_more_menu_focus(&mut self, index: usize) -> bool {
        if !self.more_menu_open || index > 9 || self.more_menu_index == index {
            return false;
        }
        self.more_menu_index = index;
        true
    }

    pub fn about_dialog(&self) -> Option<&AboutInfoV1> {
        self.about_dialog_open.then_some(&self.about_info)
    }

    pub fn set_about_info(&mut self, info: AboutInfoV1) {
        self.about_info = info;
    }

    pub(crate) fn open_about_dialog(&mut self) {
        self.more_menu_open = false;
        self.about_dialog_open = true;
    }

    pub(crate) fn close_about_dialog(&mut self) {
        self.about_dialog_open = false;
    }

    pub fn folder_options(&self) -> Option<FolderOptionsDraft> {
        self.folder_options.clone()
    }

    pub(crate) fn open_folder_options(&mut self) {
        self.more_menu_open = false;
        let settings = self.view_settings();
        let extension_enabled = self
            .extensions
            .iter()
            .map(|extension| extension.enabled)
            .collect::<Vec<_>>();
        let applied_baseline = FolderOptionsAppliedSnapshotV1 {
            settings: settings.clone(),
            restore_previous_session: self.restore_previous_session,
            extension_enabled: extension_enabled.clone(),
        };
        self.folder_options = Some(FolderOptionsDraft {
            page: if std::env::var_os("SUPEREXPLORER_UITEST_OPEN_FOLDER_OPTIONS").is_some() {
                FolderOptionsPage::View
            } else {
                FolderOptionsPage::General
            },
            settings,
            restore_previous_session: self.restore_previous_session,
            extension_enabled,
            applied_baseline,
            applied_revision: self.folder_options_applied_revision,
            apply_error: None,
        });
    }

    pub fn extensions(&self) -> &[ExtensionOptionV1] {
        &self.extensions
    }

    pub fn configure_extension_desired_states(&mut self, states: &[(String, bool)]) {
        for (package_id, enabled) in states {
            if let Some(extension) = self
                .extensions
                .iter_mut()
                .find(|extension| extension.package_id == package_id)
            {
                extension.enabled = *enabled;
            } else {
                let package_id = Box::leak(package_id.clone().into_boxed_str());
                self.extensions.push(ExtensionOptionV1 {
                    package_id,
                    display_name: package_id,
                    author_name: "Unknown",
                    author_bio: "Locally installed Plugin",
                    author_website: "",
                    purpose: "Loaded automatically from the SuperExplorer plugins directory.",
                    community_website: "",
                    release_date: "Unknown",
                    command_contribution: None,
                    enabled: *enabled,
                });
            }
        }
    }

    pub fn extension_enabled(&self, package_id: &str) -> bool {
        self.extensions
            .iter()
            .find(|extension| extension.package_id == package_id)
            .is_some_and(|extension| extension.enabled)
    }

    pub(crate) fn uninstall_code_lines_column_descriptor(
        &mut self,
        descriptor: &explorer_model::ColumnDescriptor,
    ) -> bool {
        let explorer_model::ColumnId::Extension { package_id, .. } = &descriptor.id else {
            return false;
        };
        let removed = self.column_registry.unregister_package(package_id) != 0;
        if removed {
            self.code_lines_sort_values.remove(&descriptor.id);
            self.presentation_cache
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .clear();
        }
        removed
    }

    pub(crate) fn toggle_folder_option_extension(&mut self, index: usize) {
        if let Some(enabled) = self
            .folder_options
            .as_mut()
            .and_then(|draft| draft.extension_enabled.get_mut(index))
        {
            *enabled = !*enabled;
        }
    }

    pub(crate) fn close_folder_options(&mut self) {
        self.folder_options = None;
    }

    pub(crate) fn set_folder_options_page(&mut self, page: FolderOptionsPage) {
        if let Some(draft) = &mut self.folder_options {
            draft.page = page;
        }
    }

    pub(crate) fn update_folder_options(
        &mut self,
        update: impl FnOnce(&mut explorer_model::ViewSettings),
    ) {
        if let Some(draft) = &mut self.folder_options {
            update(&mut draft.settings);
        }
    }

    pub(crate) fn toggle_restore_previous_session(&mut self) {
        if let Some(draft) = &mut self.folder_options {
            draft.restore_previous_session = !draft.restore_previous_session;
        }
    }

    pub const fn restore_previous_session(&self) -> bool {
        self.restore_previous_session
    }

    pub fn set_restore_previous_session(&mut self, enabled: bool) {
        self.restore_previous_session = enabled;
    }

    pub(crate) fn reset_folder_options(&mut self) {
        if let Some(draft) = &mut self.folder_options {
            draft.settings = explorer_model::ViewSettings::default();
            draft.apply_error = None;
        }
    }

    pub(crate) fn reject_folder_options_apply(
        &mut self,
        reason: FolderOptionsApplyFailureV1,
        error: impl Into<String>,
    ) -> FolderOptionsApplyResultV1 {
        if let Some(draft) = &mut self.folder_options {
            draft.apply_error = Some(error.into());
            return FolderOptionsApplyResultV1::Rejected { reason };
        }
        FolderOptionsApplyResultV1::NoDraft
    }

    pub(crate) fn apply_folder_options(&mut self) -> FolderOptionsApplyResultV1 {
        if let Some(draft) = self.folder_options.clone() {
            let applied = draft.applied_snapshot();
            self.tabs.active_tab_mut().view.settings = applied.settings.clone();
            self.restore_previous_session = applied.restore_previous_session;
            for (extension, enabled) in self
                .extensions
                .iter_mut()
                .zip(applied.extension_enabled.iter().copied())
            {
                extension.enabled = enabled;
            }
            if !self.extension_enabled("rust-lock-owner-column") {
                self.column_registry.unregister_package("rust-lock-owner");
                self.code_lines_sort_values.retain(|column_id, _| {
                    !matches!(column_id, explorer_model::ColumnId::Extension { package_id, .. } if package_id == "rust-lock-owner")
                });
                self.presentation_cache
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner)
                    .clear();
            }
            if !self.extensions.iter().any(|extension| {
                extension.package_id == "rust-folder-size-map-view" && extension.enabled
            }) {
                self.tabs.active_tab_mut().view.settings.extension_view_id = None;
            }
            self.folder_options_applied_revision =
                self.folder_options_applied_revision.saturating_add(1);
            if let Some(draft) = &mut self.folder_options {
                draft.applied_baseline = applied;
                draft.applied_revision = self.folder_options_applied_revision;
                draft.apply_error = None;
            }
            return FolderOptionsApplyResultV1::Applied {
                revision: self.folder_options_applied_revision,
            };
        }
        FolderOptionsApplyResultV1::NoDraft
    }

    pub(crate) fn confirm_folder_options(&mut self) -> FolderOptionsApplyResultV1 {
        let result = self.apply_folder_options();
        if matches!(result, FolderOptionsApplyResultV1::Applied { .. }) {
            self.folder_options = None;
        }
        result
    }

    /// Applies a newer application-wide settings snapshot while preserving an
    /// independently edited Folder Options draft.
    pub fn adopt_applied_folder_options(
        &mut self,
        applied: FolderOptionsAppliedSnapshotV1,
        revision: u64,
    ) {
        if revision <= self.folder_options_applied_revision {
            return;
        }
        self.tabs.active_tab_mut().view.settings = applied.settings.clone();
        self.restore_previous_session = applied.restore_previous_session;
        for (extension, enabled) in self
            .extensions
            .iter_mut()
            .zip(applied.extension_enabled.iter().copied())
        {
            extension.enabled = enabled;
        }
        self.folder_options_applied_revision = revision;
        if let Some(draft) = &mut self.folder_options
            && !draft.is_dirty()
        {
            draft.settings = applied.settings.clone();
            draft.restore_previous_session = applied.restore_previous_session;
            draft.extension_enabled = applied.extension_enabled.clone();
            draft.applied_baseline = applied;
            draft.applied_revision = revision;
            draft.apply_error = None;
        }
    }

    pub const fn view_show_submenu_open(&self) -> bool {
        self.view_show_submenu_open
    }

    pub fn view_settings(&self) -> explorer_model::ViewSettings {
        let mut settings = self.tabs.active_tab().view.settings.clone();
        if let Some(preview) = self.details_column_drag.as_ref()
            && preview.tab_id == self.tabs.active_tab_id()
            && preview.preview_order.iter().all(|column| {
                self.column_registry.contains(column) && settings.details_layout.visible(column)
            })
        {
            settings
                .details_layout
                .reorder_known(preview.preview_order.iter().cloned());
        }
        let file_system = self
            .tabs
            .active_tab()
            .history
            .current()
            .and_then(|entry| entry.location.file_system_kind());
        settings.details_layout.retain_effective(|column| {
            self.column_registry.get(column).is_some_and(|descriptor| {
                file_system.is_some_and(|kind| descriptor.file_systems.contains(kind))
            })
        });
        if matches!(
            file_system,
            Some(explorer_model::FileSystemKind::Adb | explorer_model::FileSystemKind::Sftp)
        ) {
            settings
                .details_layout
                .set_visible(&explorer_model::ColumnId::Permissions, true);
        }
        let sort_applicable = settings.sort.column == explorer_model::ColumnId::Name
            || self
                .column_registry
                .get(&settings.sort.column)
                .is_some_and(|descriptor| {
                    file_system.is_some_and(|kind| descriptor.file_systems.contains(kind))
                });
        if !sort_applicable {
            settings.sort = explorer_model::SortDescriptor::default();
        }
        settings
    }

    pub(crate) fn toggle_view_menu(&mut self) {
        self.view_menu_open = !self.view_menu_open;
        if self.view_menu_open {
            self.details_column_menu = None;
            self.details_filter_menu = None;
            self.view_menu_index = 0;
            self.sort_menu_open = false;
            self.more_menu_open = false;
            self.extensions_menu_open = false;
            self.new_menu_open = false;
        }
        if !self.view_menu_open {
            self.view_show_submenu_open = false;
        }
    }

    pub(crate) fn close_view_menu(&mut self) {
        self.view_menu_open = false;
        self.view_show_submenu_open = false;
    }

    pub(crate) fn move_view_menu_focus(&mut self, direction: i8) {
        self.view_menu_index = move_bounded_menu_index(self.view_menu_index, direction, 11);
    }

    pub(crate) fn set_view_menu_focus(&mut self, index: usize) -> bool {
        if !self.view_menu_open || index > 11 || self.view_menu_index == index {
            return false;
        }
        self.view_menu_index = index;
        true
    }

    pub(crate) fn toggle_view_show_submenu(&mut self) {
        self.view_show_submenu_open = !self.view_show_submenu_open;
    }

    pub(crate) fn set_view_mode(&mut self, mode: explorer_model::ViewMode) {
        let _ = self.end_scrollbar_drag(ScrollbarTerminal::ViewSwitch);
        self.end_marquee();
        self.end_details_column_resize();
        let settings = &mut self.tabs.active_tab_mut().view.settings;
        settings.mode = mode;
        settings.extension_view_id = None;
        settings.icon_size = explorer_model::default_icon_size_for_mode(mode);
        self.close_view_menu();
    }

    /// Stores the extension view identity while retaining the last built-in
    /// mode as its immediate fallback. The UI resolves availability at render
    /// time, so an absent or faulted runtime never becomes a blank surface.
    pub(crate) fn set_extension_view(&mut self, view_id: String) {
        if view_id.trim().is_empty() {
            return;
        }
        let _ = self.end_scrollbar_drag(ScrollbarTerminal::ViewSwitch);
        self.end_marquee();
        self.end_details_column_resize();
        let settings = &mut self.tabs.active_tab_mut().view.settings;
        // Details is the stable built-in fallback whenever this extension is
        // missing or faults during rendering.
        settings.mode = explorer_model::ViewMode::Details;
        if settings.extension_view_id.as_deref() == Some(view_id.as_str()) {
            settings.extension_view_id = None;
        } else {
            settings.extension_view_id = Some(view_id);
        }
        self.close_view_menu();
    }

    /// Moves one Explorer view/size notch while preserving the current identity anchor.
    pub(crate) fn zoom_view(&mut self, direction: i8) {
        const LEVELS: [(explorer_model::ViewMode, u16); 16] = [
            (explorer_model::ViewMode::Content, 32),
            (explorer_model::ViewMode::Tiles, 40),
            (explorer_model::ViewMode::Details, 20),
            (explorer_model::ViewMode::List, 20),
            (explorer_model::ViewMode::SmallIcons, 24),
            (explorer_model::ViewMode::SmallIcons, 32),
            (explorer_model::ViewMode::SmallIcons, 48),
            (explorer_model::ViewMode::MediumIcons, 64),
            (explorer_model::ViewMode::MediumIcons, 72),
            (explorer_model::ViewMode::MediumIcons, 84),
            (explorer_model::ViewMode::LargeIcons, 96),
            (explorer_model::ViewMode::LargeIcons, 108),
            (explorer_model::ViewMode::LargeIcons, 128),
            (explorer_model::ViewMode::ExtraLargeIcons, 256),
            (explorer_model::ViewMode::ExtraLargeIcons, 384),
            (explorer_model::ViewMode::ExtraLargeIcons, 512),
        ];
        let current = self.tabs.active_tab().view.settings.clone();
        let index = LEVELS
            .iter()
            .position(|level| *level == (current.mode, current.icon_size))
            .or_else(|| LEVELS.iter().position(|level| level.0 == current.mode))
            .unwrap_or(2);
        let next = if direction > 0 {
            index.saturating_add(1).min(LEVELS.len() - 1)
        } else {
            index.saturating_sub(1)
        };
        let settings = &mut self.tabs.active_tab_mut().view.settings;
        (settings.mode, settings.icon_size) = LEVELS[next];
        self.close_view_menu();
    }

    pub(crate) fn set_sort_column(&mut self, column: explorer_model::ColumnId) {
        if !self.column_registry.get(&column).is_some_and(|descriptor| {
            self.tabs
                .active_tab()
                .history
                .current()
                .and_then(|entry| entry.location.file_system_kind())
                .is_some_and(|kind| descriptor.file_systems.contains(kind))
        }) {
            return;
        }
        let sort = &mut self.tabs.active_tab_mut().view.settings.sort;
        if sort.column == column {
            sort.direction = match sort.direction {
                explorer_model::SortDirection::Ascending => {
                    explorer_model::SortDirection::Descending
                }
                explorer_model::SortDirection::Descending => {
                    explorer_model::SortDirection::Ascending
                }
            };
        } else {
            let direction = match &column {
                explorer_model::ColumnId::DateModified
                | explorer_model::ColumnId::DateCreated
                | explorer_model::ColumnId::Size
                | explorer_model::ColumnId::FileCount
                | explorer_model::ColumnId::FolderCount => {
                    explorer_model::SortDirection::Descending
                }
                explorer_model::ColumnId::Name
                | explorer_model::ColumnId::Type
                | explorer_model::ColumnId::Authors
                | explorer_model::ColumnId::Tags
                | explorer_model::ColumnId::Title => explorer_model::SortDirection::Ascending,
                explorer_model::ColumnId::Permissions => explorer_model::SortDirection::Ascending,
                explorer_model::ColumnId::Extension { .. } => {
                    explorer_model::SortDirection::Ascending
                }
            };
            sort.column = column;
            sort.direction = direction;
        }
        self.close_sort_menu();
    }

    #[allow(
        clippy::needless_pass_by_value,
        reason = "action dispatch supplies an owned column identity and this reducer boundary deliberately has no borrow lifetime"
    )]
    pub(crate) fn sort_column_supported(&self, column: explorer_model::ColumnId) -> bool {
        if column == explorer_model::ColumnId::Name {
            return true;
        }
        let tab = self.tabs.active_tab();
        if tab
            .history
            .current()
            .is_some_and(|entry| entry.location.path().is_some())
        {
            return true;
        }
        tab.visible_directory_state()
            .snapshot()
            .is_some_and(|snapshot| {
                snapshot.entries().iter().any(|entry| match column {
                    explorer_model::ColumnId::Name => true,
                    explorer_model::ColumnId::DateModified => {
                        entry.metadata.modified_sort_key.is_some()
                            || entry.metadata.modified_display.is_some()
                    }
                    explorer_model::ColumnId::Type => entry.metadata.type_display.is_some(),
                    explorer_model::ColumnId::Size => entry.metadata.size_bytes.is_some(),
                    explorer_model::ColumnId::DateCreated => {
                        entry.metadata.created_sort_key.is_some()
                            || entry.metadata.created_display.is_some()
                    }
                    explorer_model::ColumnId::Authors => entry.metadata.authors_display.is_some(),
                    explorer_model::ColumnId::Tags => entry.metadata.tags_display.is_some(),
                    explorer_model::ColumnId::Title => entry.metadata.title_display.is_some(),
                    explorer_model::ColumnId::Permissions => entry.metadata.unix_mode.is_some(),
                    explorer_model::ColumnId::FileCount | explorer_model::ColumnId::FolderCount => {
                        entry.is_container
                    }
                    explorer_model::ColumnId::Extension { .. } => false,
                })
            })
    }

    pub(crate) fn set_sort_direction(&mut self, direction: explorer_model::SortDirection) {
        self.tabs.active_tab_mut().view.settings.sort.direction = direction;
        self.close_sort_menu();
    }

    #[allow(
        clippy::needless_pass_by_value,
        reason = "the reducer accepts the owned action payload before looking up a host-owned descriptor"
    )]
    pub(crate) fn set_details_column_width(
        &mut self,
        column: explorer_model::ColumnId,
        width: u16,
    ) {
        if !self.details_column_applicable(&column) {
            return;
        }
        if let Some(descriptor) = self.column_registry.get(&column) {
            self.tabs
                .active_tab_mut()
                .view
                .settings
                .details_layout
                .set_width_for(descriptor, width);
        }
    }

    pub fn details_column_menu(&self) -> Option<explorer_model::ColumnId> {
        self.details_column_menu.clone()
    }
    pub fn open_details_column_menu(&mut self, column: explorer_model::ColumnId) {
        if !self.details_column_applicable(&column) {
            return;
        }
        self.details_column_menu = Some(column);
        self.details_filter_menu = None;
        self.sort_menu_open = false;
        self.view_menu_open = false;
        self.more_menu_open = false;
        self.new_menu_open = false;
        self.extensions_menu_open = false;
    }
    pub fn close_details_column_menu(&mut self) {
        self.details_column_menu = None;
    }

    pub fn details_filter_menu(&self) -> Option<explorer_model::ColumnId> {
        self.details_filter_menu.clone()
    }

    pub fn open_details_filter_menu(&mut self, column: explorer_model::ColumnId) {
        if !self.details_column_applicable(&column) {
            return;
        }
        self.details_filter_menu = if self.details_filter_menu == Some(column.clone()) {
            None
        } else {
            Some(column)
        };
        if self.details_filter_menu.is_some() {
            self.details_column_menu = None;
            self.sort_menu_open = false;
            self.close_view_menu();
            self.more_menu_open = false;
            self.extensions_menu_open = false;
            self.new_menu_open = false;
        }
    }

    pub fn close_details_filter_menu(&mut self) {
        self.details_filter_menu = None;
    }

    #[allow(
        clippy::needless_pass_by_value,
        reason = "the UI command boundary owns the column identity passed into its snapshot projection"
    )]
    pub fn details_filter_options(
        &self,
        column: explorer_model::ColumnId,
    ) -> Vec<crate::file_view::DetailsFilterOption> {
        self.tabs
            .active_tab()
            .visible_snapshot()
            .map_or_else(Vec::new, |snapshot| {
                crate::file_view::DetailsFilters::options(snapshot, &column)
            })
    }

    #[allow(
        clippy::needless_pass_by_value,
        reason = "the UI command boundary owns the column identity passed into its snapshot projection"
    )]
    pub fn details_filter_selected(&self, column: explorer_model::ColumnId, key: &str) -> bool {
        self.details_filters
            .get(&self.tabs.active_tab_id())
            .is_some_and(|filters| filters.is_selected(&column, key))
    }

    #[allow(
        clippy::needless_pass_by_value,
        reason = "the UI command boundary owns the column identity passed into its snapshot projection"
    )]
    pub fn details_filter_active(&self, column: explorer_model::ColumnId) -> bool {
        self.details_filters
            .get(&self.tabs.active_tab_id())
            .is_some_and(|filters| filters.is_active(&column))
    }

    pub fn active_details_filters(&self) -> crate::file_view::DetailsFilters {
        self.details_filters
            .get(&self.tabs.active_tab_id())
            .cloned()
            .unwrap_or_default()
    }

    #[allow(
        clippy::needless_pass_by_value,
        reason = "the action reducer consumes owned filter command data before storing stable copies"
    )]
    pub fn toggle_details_filter(&mut self, column: explorer_model::ColumnId, key: String) {
        self.details_filters
            .entry(self.tabs.active_tab_id())
            .or_default()
            .toggle(&column, &key);
        self.details_filter_menu = Some(column);
    }

    pub fn clear_details_filter(&mut self, column: explorer_model::ColumnId) {
        self.details_filters
            .entry(self.tabs.active_tab_id())
            .or_default()
            .clear(&column);
        self.details_filter_menu = Some(column);
    }
    #[allow(
        clippy::needless_pass_by_value,
        reason = "the action reducer receives an owned column identity"
    )]
    pub fn toggle_details_column(&mut self, column: explorer_model::ColumnId) {
        if column == explorer_model::ColumnId::Name {
            return;
        }
        let applicable = self.column_registry.get(&column).is_some_and(|descriptor| {
            self.tabs
                .active_tab()
                .history
                .current()
                .and_then(|entry| entry.location.file_system_kind())
                .is_some_and(|kind| descriptor.file_systems.contains(kind))
        });
        if !applicable {
            return;
        }
        self.tabs
            .active_tab_mut()
            .view
            .settings
            .details_layout
            .toggle_visible(&column);
    }
    #[allow(
        clippy::needless_pass_by_value,
        reason = "the action/UI boundary owns the column identity used for the registry lookup"
    )]
    pub fn details_column_visible(&self, column: explorer_model::ColumnId) -> bool {
        self.column_registry.contains(&column)
            && self.view_settings().details_column_visible(&column)
    }
    fn details_column_applicable(&self, column: &explorer_model::ColumnId) -> bool {
        *column == explorer_model::ColumnId::Name
            || self.column_registry.get(column).is_some_and(|descriptor| {
                self.tabs
                    .active_tab()
                    .history
                    .current()
                    .and_then(|entry| entry.location.file_system_kind())
                    .is_some_and(|kind| descriptor.file_systems.contains(kind))
            })
    }
    pub fn auto_size_all_details_columns(&mut self) {
        let columns = self
            .column_registry
            .iter()
            .map(|descriptor| descriptor.id.clone())
            .collect::<Vec<_>>();
        for column in columns {
            if self.details_column_visible(column.clone()) {
                self.auto_size_details_column(column);
            }
        }
    }
    /// Sizes a Details column from already-owned presentation strings only.
    ///
    /// This intentionally performs no filesystem or Shell query on the UI thread. The estimate
    /// includes the header, row padding, and (for Name) the Explorer icon/gap allocation.
    pub(crate) fn auto_size_details_column(&mut self, column: explorer_model::ColumnId) {
        self.end_details_column_resize();
        let header = match column {
            explorer_model::ColumnId::Name => "名稱",
            explorer_model::ColumnId::DateModified => "修改日期",
            explorer_model::ColumnId::Type => "類型",
            explorer_model::ColumnId::Size => "大小",
            explorer_model::ColumnId::DateCreated => "建立日期",
            explorer_model::ColumnId::Authors => "作者",
            explorer_model::ColumnId::Tags => "標籤",
            explorer_model::ColumnId::Title => "標題",
            explorer_model::ColumnId::FileCount => "File Count",
            explorer_model::ColumnId::FolderCount => "Folder Count",
            explorer_model::ColumnId::Permissions => "Permissions",
            explorer_model::ColumnId::Extension { .. } => "擴充欄位",
        };
        let header_width = estimated_text_width(header) + 32.0;
        let content_width = self
            .tabs
            .active_tab()
            .visible_snapshot()
            .into_iter()
            .flat_map(DirectorySnapshot::entries)
            .map(|entry| match column {
                explorer_model::ColumnId::Name => {
                    estimated_text_width(&entry.display_name) + 20.0 + 20.0
                }
                explorer_model::ColumnId::DateModified => entry
                    .metadata
                    .modified_display
                    .as_deref()
                    .map_or(16.0, |text| estimated_text_width(text) + 16.0),
                explorer_model::ColumnId::Type => entry
                    .metadata
                    .type_display
                    .as_deref()
                    .map_or(16.0, |text| estimated_text_width(text) + 16.0),
                explorer_model::ColumnId::Size => entry.metadata.size_bytes.map_or(16.0, |size| {
                    estimated_text_width(&crate::format_file_size(size)) + 16.0
                }),
                explorer_model::ColumnId::DateCreated => entry
                    .metadata
                    .created_display
                    .as_deref()
                    .map_or(16.0, |text| estimated_text_width(text) + 16.0),
                explorer_model::ColumnId::Authors => entry
                    .metadata
                    .authors_display
                    .as_deref()
                    .map_or(16.0, |text| estimated_text_width(text) + 16.0),
                explorer_model::ColumnId::Tags => entry
                    .metadata
                    .tags_display
                    .as_deref()
                    .map_or(16.0, |text| estimated_text_width(text) + 16.0),
                explorer_model::ColumnId::Title => entry
                    .metadata
                    .title_display
                    .as_deref()
                    .map_or(16.0, |text| estimated_text_width(text) + 16.0),
                explorer_model::ColumnId::FileCount | explorer_model::ColumnId::FolderCount => {
                    estimated_text_width("9999999999") + 16.0
                }
                explorer_model::ColumnId::Permissions => {
                    estimated_text_width(&explorer_model::format_unix_mode(
                        entry.metadata.unix_mode,
                    )) + 16.0
                }
                explorer_model::ColumnId::Extension { .. } => 16.0,
            })
            .fold(header_width, f32::max);
        self.set_details_column_width(
            column,
            clamped_u16(
                content_width.ceil(),
                explorer_model::OrderedColumnLayout::MINIMUM_WIDTH,
                explorer_model::OrderedColumnLayout::MAXIMUM_WIDTH,
            ),
        );
    }

    pub(crate) fn begin_details_column_resize(
        &mut self,
        column: explorer_model::ColumnId,
        pointer_x: f32,
    ) {
        if !pointer_x.is_finite() || !self.details_column_visible(column.clone()) {
            return;
        }
        self.end_marquee();
        let tab_id = self.tabs.active_tab_id();
        let width = self
            .tabs
            .active_tab()
            .view
            .settings
            .details_layout
            .width(&column)
            .unwrap_or(explorer_model::OrderedColumnLayout::MINIMUM_WIDTH);
        self.details_column_resize = Some(DetailsColumnResizeSession {
            tab_id,
            column,
            pointer_x,
            width,
        });
    }

    pub(crate) fn update_details_column_resize(&mut self, pointer_x: f32) {
        let Some(session) = self.details_column_resize.clone() else {
            return;
        };
        if !pointer_x.is_finite() || session.tab_id != self.tabs.active_tab_id() {
            self.details_column_resize = None;
            return;
        }
        let delta = (pointer_x - session.pointer_x).round();
        let width = (f32::from(session.width) + delta).clamp(
            f32::from(explorer_model::OrderedColumnLayout::MINIMUM_WIDTH),
            f32::from(explorer_model::OrderedColumnLayout::MAXIMUM_WIDTH),
        );
        self.set_details_column_width(
            session.column,
            clamped_u16(
                width,
                explorer_model::OrderedColumnLayout::MINIMUM_WIDTH,
                explorer_model::OrderedColumnLayout::MAXIMUM_WIDTH,
            ),
        );
    }

    pub(crate) fn end_details_column_resize(&mut self) {
        self.details_column_resize = None;
    }

    pub const fn details_column_resize_active(&self) -> bool {
        self.details_column_resize.is_some()
    }

    pub(crate) fn move_details_column_before(
        &mut self,
        column: explorer_model::ColumnId,
        before: Option<explorer_model::ColumnId>,
    ) -> bool {
        if !self.details_column_applicable(&column)
            || before
                .as_ref()
                .is_some_and(|target| !self.details_column_applicable(target))
        {
            return false;
        }
        self.tabs
            .active_tab_mut()
            .view
            .settings
            .details_layout
            .move_before(&column, before.as_ref())
    }

    pub(crate) fn update_details_column_drag_preview(
        &mut self,
        column: explorer_model::ColumnId,
        target: explorer_model::ColumnId,
        pointer_x: f32,
        target_left: f32,
        target_right: f32,
    ) -> bool {
        if column == explorer_model::ColumnId::Name
            || !self.details_column_visible(column.clone())
            || !self.details_column_visible(target.clone())
        {
            return self.cancel_details_column_drag();
        }
        let tab_id = self.tabs.active_tab_id();
        let effective_settings = self.view_settings();
        let persisted_order = effective_settings
            .details_layout
            .visible_registered(&self.column_registry)
            .map(|entry| entry.id.clone())
            .collect::<Vec<_>>();
        let original_order = self
            .details_column_drag
            .as_ref()
            .filter(|session| session.tab_id == tab_id && session.column == column)
            .map_or(persisted_order, |session| session.original_order.clone());
        let base_order = self
            .details_column_drag
            .as_ref()
            .filter(|session| session.tab_id == tab_id && session.column == column)
            .map_or_else(
                || original_order.clone(),
                |session| session.preview_order.clone(),
            );
        let Some(resolved) = resolve_details_column_insertion(
            &base_order,
            &column,
            &target,
            pointer_x,
            target_left,
            target_right,
        ) else {
            return false;
        };
        let next = DetailsColumnDragPreviewSession {
            tab_id,
            column,
            original_order,
            preview_order: resolved.order,
            before: resolved.before,
        };
        if self.details_column_drag.as_ref() == Some(&next) {
            return false;
        }
        self.details_column_drag = Some(next);
        true
    }

    pub(crate) fn commit_details_column_drag(&mut self) -> bool {
        let Some(session) = self.details_column_drag.take() else {
            return false;
        };
        if session.tab_id != self.tabs.active_tab_id()
            || session.preview_order == session.original_order
        {
            return false;
        }
        self.move_details_column_before(session.column, session.before)
    }

    pub(crate) fn cancel_details_column_drag(&mut self) -> bool {
        self.details_column_drag.take().is_some()
    }

    pub const fn details_column_drag_active(&self) -> bool {
        self.details_column_drag.is_some()
    }

    pub(crate) fn toggle_details_pane(&mut self) {
        let settings = &mut self.tabs.active_tab_mut().view.settings;
        settings.details_pane = !settings.details_pane;
        if settings.details_pane {
            settings.preview_pane = false;
        }
        self.side_pane_resize = None;
    }

    pub(crate) fn toggle_preview_pane(&mut self) {
        let settings = &mut self.tabs.active_tab_mut().view.settings;
        settings.preview_pane = !settings.preview_pane;
        if settings.preview_pane {
            settings.details_pane = false;
        }
        self.side_pane_resize = None;
    }

    pub(crate) fn begin_side_pane_resize(&mut self, pointer_x: f32) -> bool {
        if !pointer_x.is_finite() {
            return false;
        }
        let settings = self.tabs.active_tab().view.settings.clone();
        let (width, details) = if settings.details_pane {
            (settings.details_pane_width, true)
        } else if settings.preview_pane {
            (settings.preview_pane_width, false)
        } else {
            return false;
        };
        self.side_pane_resize = Some(SidePaneResizeSession {
            tab_id: self.tabs.active_tab_id(),
            pointer_x,
            width,
            details,
        });
        true
    }

    pub(crate) fn update_side_pane_resize(&mut self, pointer_x: f32) -> bool {
        let Some(session) = self.side_pane_resize else {
            return false;
        };
        if !pointer_x.is_finite() || session.tab_id != self.tabs.active_tab_id() {
            self.side_pane_resize = None;
            return false;
        }
        let layout = LayoutTokens::WINDOWS_11;
        let width = clamped_u16(
            f32::from(session.width) - (pointer_x - session.pointer_x),
            clamped_u16(layout.side_pane_min_width.value(), 0, u16::MAX),
            clamped_u16(layout.side_pane_max_width.value(), 0, u16::MAX),
        );
        let settings = &mut self.tabs.active_tab_mut().view.settings;
        if session.details {
            settings.details_pane_width = width;
        } else {
            settings.preview_pane_width = width;
        }
        true
    }

    pub(crate) fn end_side_pane_resize(&mut self) {
        self.side_pane_resize = None;
    }

    pub const fn side_pane_resize_active(&self) -> bool {
        self.side_pane_resize.is_some()
    }

    pub(crate) fn reset_side_pane_width(&mut self) {
        let width = clamped_u16(
            LayoutTokens::WINDOWS_11.side_pane_default_width.value(),
            0,
            u16::MAX,
        );
        let settings = &mut self.tabs.active_tab_mut().view.settings;
        if settings.details_pane {
            settings.details_pane_width = width;
        } else if settings.preview_pane {
            settings.preview_pane_width = width;
        }
    }

    pub(crate) fn adjust_side_pane_width(&mut self, direction: i8) {
        let layout = LayoutTokens::WINDOWS_11;
        let settings = &mut self.tabs.active_tab_mut().view.settings;
        let width = if settings.details_pane {
            &mut settings.details_pane_width
        } else if settings.preview_pane {
            &mut settings.preview_pane_width
        } else {
            return;
        };
        let adjusted = f32::from(*width) + f32::from(direction.signum()) * 24.0;
        *width = clamped_u16(
            adjusted,
            clamped_u16(layout.side_pane_min_width.value(), 0, u16::MAX),
            clamped_u16(layout.side_pane_max_width.value(), 0, u16::MAX),
        );
    }

    pub(crate) fn toggle_item_check_boxes(&mut self) {
        let settings = &mut self.tabs.active_tab_mut().view.settings;
        settings.item_check_boxes = !settings.item_check_boxes;
    }

    pub(crate) fn toggle_file_name_extensions(&mut self) {
        let settings = &mut self.tabs.active_tab_mut().view.settings;
        settings.file_name_extensions = !settings.file_name_extensions;
    }

    pub(crate) fn toggle_hidden_items(&mut self) {
        let settings = &mut self.tabs.active_tab_mut().view.settings;
        settings.hidden_items = !settings.hidden_items;
    }

    pub(crate) fn toggle_compact_view(&mut self) {
        let settings = &mut self.tabs.active_tab_mut().view.settings;
        settings.compact_view = !settings.compact_view;
    }

    pub const fn context_menu_error(&self) -> Option<&explorer_common::ExplorerError> {
        self.context_menu_error.as_ref()
    }

    pub fn thumbnail_cache_notice(&self) -> Option<&str> {
        self.thumbnail_cache_notice.as_deref()
    }

    pub fn quick_access_notice(&self) -> Option<&str> {
        self.quick_access_notice.as_deref()
    }

    pub fn bookmark_notice(&self) -> Option<&str> {
        self.bookmark_notice.as_deref()
    }

    pub(crate) fn set_bookmark_notice(&mut self, notice: impl Into<String>) {
        self.bookmark_notice = Some(notice.into());
    }

    pub(crate) const fn bookmark_overflow_open(&self) -> bool {
        self.bookmark_overflow_open
    }

    pub(crate) fn toggle_bookmark_overflow(&mut self) {
        self.bookmark_overflow_open = !self.bookmark_overflow_open;
    }

    pub(crate) const fn bookmark_folder_menu(&self) -> Option<explorer_model::BookmarkFolderId> {
        self.bookmark_folder_menu
    }

    pub(crate) fn toggle_bookmark_folder_menu(&mut self, id: explorer_model::BookmarkFolderId) {
        self.bookmark_folder_menu = (self.bookmark_folder_menu != Some(id)).then_some(id);
        if !self.expanded_bookmark_folders.remove(&id) {
            self.expanded_bookmark_folders.insert(id);
        }
    }

    pub(crate) fn dismiss_bookmark_browse_menus(&mut self) {
        self.bookmark_overflow_open = false;
        self.bookmark_folder_menu = None;
    }

    pub(crate) const fn bookmark_toolbar_context_menu(
        &self,
    ) -> Option<BookmarkToolbarContextMenuState> {
        self.bookmark_toolbar_context_menu
    }

    pub(crate) fn open_bookmark_toolbar_context_menu(
        &mut self,
        parent_id: Option<explorer_model::BookmarkFolderId>,
        x: f32,
        y: f32,
    ) {
        if parent_id.is_none_or(|id| self.bookmarks.folder(id).is_some()) {
            self.bookmark_toolbar_context_menu = Some(BookmarkToolbarContextMenuState {
                parent_id,
                x: x.max(0.0),
                y: y.max(0.0),
            });
            self.bookmark_overflow_open = false;
            self.bookmark_folder_menu = None;
        }
    }

    pub(crate) fn close_bookmark_toolbar_context_menu(&mut self) {
        self.bookmark_toolbar_context_menu = None;
    }

    pub(crate) const fn bookmark_context_menu(&self) -> Option<BookmarkContextMenuState> {
        self.bookmark_context_menu
    }

    pub(crate) fn open_bookmark_context_menu(
        &mut self,
        id: explorer_model::BookmarkId,
        x: f32,
        y: f32,
    ) {
        if self
            .bookmarks
            .entries()
            .iter()
            .any(|bookmark| bookmark.id == id)
        {
            self.bookmark_context_menu = Some(BookmarkContextMenuState {
                id,
                x: x.max(0.0),
                y: y.max(0.0),
            });
            self.bookmark_toolbar_context_menu = None;
            self.bookmark_overflow_open = false;
            self.bookmark_folder_menu = None;
        }
    }

    pub(crate) fn close_bookmark_context_menu(&mut self) {
        self.bookmark_context_menu = None;
    }

    pub(crate) const fn remote_context_menu(&self) -> Option<RemoteContextMenuState> {
        self.remote_context_menu
    }

    pub(crate) fn close_remote_context_menu(&mut self) {
        self.remote_context_menu = None;
    }

    pub(crate) const fn remote_properties(&self) -> Option<&RemotePropertiesState> {
        self.remote_properties.as_ref()
    }

    pub(crate) fn open_remote_properties(&mut self) -> bool {
        let tab = self.tabs.active_tab();
        let Some(snapshot) = tab.visible_snapshot() else {
            return false;
        };
        let mut selected = snapshot
            .entries()
            .iter()
            .filter(|entry| tab.selection.contains(&entry.id));
        let Some(entry) = selected.next() else {
            return false;
        };
        if selected.next().is_some()
            || !matches!(entry.location, LocationDescriptor::Virtual(ref remote)
                if matches!(remote.provider_id.as_str(), "adb" | "sftp"))
        {
            return false;
        }
        let Some(mode) = entry.metadata.unix_mode else {
            return false;
        };
        self.remote_properties = Some(RemotePropertiesState {
            entry: entry.clone(),
            original_mode: mode & 0o7777,
            mode: mode & 0o7777,
        });
        true
    }

    pub(crate) fn close_remote_properties(&mut self) {
        self.remote_properties = None;
    }

    pub(crate) fn toggle_remote_permission(&mut self, mask: u32) -> bool {
        if mask == 0 || mask & !0o7777 != 0 {
            return false;
        }
        let Some(properties) = self.remote_properties.as_mut() else {
            return false;
        };
        properties.mode ^= mask;
        true
    }

    pub(crate) fn apply_remote_properties_request(&mut self) -> Option<FileOperationRequest> {
        let properties = self.remote_properties.as_mut()?;
        if properties.mode == properties.original_mode {
            return None;
        }
        let mode = properties.mode;
        properties.original_mode = mode;
        Some(FileOperationRequest {
            kind: FileOperationKind::SetUnixMode {
                item: ItemDescriptor {
                    id: properties.entry.id.clone(),
                    location: properties.entry.location.clone(),
                },
                mode,
            },
            flags: explorer_model::FileOperationFlags {
                allow_undo: false,
                ..Default::default()
            },
        })
    }

    pub(crate) fn bookmark_folder_expanded(&self, id: explorer_model::BookmarkFolderId) -> bool {
        self.expanded_bookmark_folders.contains(&id)
    }

    pub(crate) fn request_bookmark_folder_delete(&mut self, id: explorer_model::BookmarkFolderId) {
        if self.bookmarks.folder(id).is_some() {
            self.bookmark_folder_delete_confirmation =
                Some((id, self.bookmarks.descendant_count(id)));
        }
    }

    pub(crate) const fn bookmark_folder_delete_confirmation(
        &self,
    ) -> Option<(explorer_model::BookmarkFolderId, usize)> {
        self.bookmark_folder_delete_confirmation
    }

    pub(crate) fn cancel_bookmark_folder_delete(&mut self) {
        self.bookmark_folder_delete_confirmation = None;
    }

    pub(crate) fn take_bookmark_folder_delete(
        &mut self,
    ) -> Option<explorer_model::BookmarkFolderId> {
        self.bookmark_folder_delete_confirmation
            .take()
            .map(|(id, _)| id)
    }

    pub(crate) fn bookmark_editor(&self) -> Option<&BookmarkEditorDraft> {
        self.bookmark_editor.as_ref()
    }

    pub(crate) fn begin_bookmark_editor(&mut self, id: Option<explorer_model::BookmarkId>) {
        let bookmark =
            id.and_then(|id| self.bookmarks.entries().iter().find(|entry| entry.id == id));
        self.bookmark_editor = Some(match bookmark {
            Some(bookmark) => BookmarkEditorDraft {
                id: Some(bookmark.id),
                name: bookmark.name.clone(),
                target: bookmark.target.clone(),
                parent_id: bookmark.parent_id,
            },
            None => BookmarkEditorDraft {
                id: None,
                name: "Lua command".to_owned(),
                target: explorer_model::BookmarkTarget::LuaScript {
                    source: "-- current_folder is a read-only string\n".to_owned(),
                },
                parent_id: None,
            },
        });
    }

    pub(crate) fn begin_new_bookmark_editor(
        &mut self,
        name: String,
        target: explorer_model::BookmarkTarget,
    ) {
        self.begin_new_bookmark_editor_in(name, target, None);
    }

    pub(crate) fn begin_new_bookmark_editor_in(
        &mut self,
        name: String,
        target: explorer_model::BookmarkTarget,
        parent_id: Option<explorer_model::BookmarkFolderId>,
    ) {
        self.bookmark_editor = Some(BookmarkEditorDraft {
            id: None,
            name,
            target,
            parent_id,
        });
    }

    pub(crate) fn select_bookmark_editor_parent(
        &mut self,
        parent_id: Option<explorer_model::BookmarkFolderId>,
    ) {
        if parent_id.is_none_or(|id| self.bookmarks.folder(id).is_some())
            && let Some(editor) = &mut self.bookmark_editor
        {
            editor.parent_id = parent_id;
        }
    }

    pub(crate) fn cancel_bookmark_editor(&mut self) {
        self.bookmark_editor = None;
    }

    pub(crate) fn restore_bookmark_editor(&mut self, editor: BookmarkEditorDraft) {
        self.bookmark_editor = Some(editor);
    }

    pub(crate) fn begin_bookmark_folder_editor(&mut self, id: explorer_model::BookmarkFolderId) {
        self.bookmark_folder_editor =
            self.bookmarks
                .folder(id)
                .map(|folder| BookmarkFolderEditorDraft {
                    id,
                    name: folder.name.clone(),
                });
    }

    pub(crate) const fn bookmark_folder_editor(&self) -> Option<&BookmarkFolderEditorDraft> {
        self.bookmark_folder_editor.as_ref()
    }

    pub(crate) fn update_bookmark_folder_editor_name(&mut self, name: String) {
        if let Some(editor) = &mut self.bookmark_folder_editor {
            editor.name = name;
        }
    }

    pub(crate) fn cancel_bookmark_folder_editor(&mut self) {
        self.bookmark_folder_editor = None;
    }

    pub(crate) fn restore_bookmark_folder_editor(&mut self, editor: BookmarkFolderEditorDraft) {
        self.bookmark_folder_editor = Some(editor);
    }

    pub(crate) fn commit_bookmark_folder_editor(
        &mut self,
    ) -> Option<explorer_model::BookmarkMutation> {
        let editor = self.bookmark_folder_editor.take()?;
        if editor.name.trim().is_empty() {
            self.bookmark_folder_editor = Some(editor);
            return None;
        }
        Some(self.bookmarks.begin_rename_folder(editor.id, editor.name))
    }

    pub(crate) fn update_bookmark_editor_name(&mut self, name: String) {
        if let Some(editor) = &mut self.bookmark_editor {
            editor.name = name;
        }
    }

    pub(crate) fn update_bookmark_editor_payload(&mut self, payload: String) {
        if let Some(editor) = &mut self.bookmark_editor {
            editor.target = editor.target.with_editable_payload(payload);
        }
    }

    pub(crate) fn commit_bookmark_editor(&mut self) -> Option<explorer_model::BookmarkMutation> {
        let editor = self.bookmark_editor.take()?;
        if editor.name.trim().is_empty() || editor.target.editable_payload().trim().is_empty() {
            self.bookmark_editor = Some(editor);
            return None;
        }
        Some(match editor.id {
            Some(id) => {
                self.bookmarks
                    .begin_update_in(id, editor.name, editor.target, editor.parent_id)
            }
            None => self
                .bookmarks
                .begin_add_to(editor.name, editor.target, editor.parent_id),
        })
    }

    pub const fn session_reset_confirmation(&self) -> Option<explorer_model::SessionResetScope> {
        self.session_reset_confirmation
    }

    pub fn session_reset_notice(&self) -> Option<&str> {
        self.session_reset_notice.as_deref()
    }

    pub(crate) fn begin_session_reset_confirmation(
        &mut self,
        scope: explorer_model::SessionResetScope,
    ) {
        self.session_reset_confirmation = Some(scope);
        self.session_reset_notice = None;
    }

    pub(crate) fn cancel_session_reset_confirmation(&mut self) {
        self.session_reset_confirmation = None;
    }

    pub(crate) fn confirm_session_reset(&mut self) {
        self.confirmed_session_reset = self.session_reset_confirmation.take();
    }

    pub(crate) fn retry_session_reset(&mut self) {
        self.confirmed_session_reset = self.last_session_reset;
    }

    pub(crate) fn take_confirmed_session_reset(
        &mut self,
    ) -> Option<explorer_model::SessionResetScope> {
        self.confirmed_session_reset.take()
    }

    pub(crate) fn finish_session_reset_submission(
        &mut self,
        scope: explorer_model::SessionResetScope,
        accepted: bool,
    ) {
        self.last_session_reset = Some(scope);
        self.session_reset_notice = Some(if accepted {
            "Saved Explorer state reset was accepted; transient failures retry automatically."
                .to_owned()
        } else {
            "Saved Explorer state could not be reset. Choose Retry after persistence recovers."
                .to_owned()
        });
    }

    pub const fn drag_session(&self) -> &explorer_model::DragSession {
        &self.drag_session
    }

    pub const fn drop_target_row(&self) -> Option<usize> {
        self.drop_target_row
    }

    pub(crate) const fn pending_right_drop(&self) -> Option<&PendingRightDrop> {
        self.pending_right_drop.as_ref()
    }

    #[cfg(test)]
    fn tabs_mut(&mut self) -> &mut ExplorerWindowState {
        &mut self.tabs
    }

    pub(crate) fn begin_active_navigation(
        &mut self,
        location: LocationDescriptor,
        refresh: bool,
    ) -> Option<ExplorerCommand> {
        self.clear_file_view_typeahead();
        self.cancel_permanent_delete_confirmation();
        self.cancel_lock_recovery();
        self.clear_external_drag();
        self.close_navigation_history_menu();
        self.cancel_breadcrumb_requests(self.tabs.active_tab_id());
        let cached = if refresh {
            None
        } else {
            self.directory_cache.get(&location)
        };
        let tab = self.tabs.active_tab_mut();
        let context = if refresh {
            tab.begin_refresh_request()?
        } else {
            tab.begin_navigation_request_with_snapshot(cached)?
        };
        Some(if refresh {
            ExplorerCommand::Refresh { context, location }
        } else {
            ExplorerCommand::Navigate { context, location }
        })
    }

    pub(crate) fn begin_active_location_load(&mut self) -> Option<ExplorerCommand> {
        let location = self.tabs.active_tab().history.current()?.location.clone();
        self.begin_active_navigation(location, false)
    }

    /// Starts the active restored tab only while its transient directory state is still idle.
    /// Loading, ready, and terminal-error tabs keep their existing request or presentation.
    pub(crate) fn begin_idle_active_location_load(&mut self) -> Option<ExplorerCommand> {
        if !matches!(self.tabs.active_tab().directory, DirectoryState::Idle) {
            return None;
        }
        self.begin_active_location_load()
    }

    pub(crate) fn begin_back_navigation(&mut self) -> Option<ExplorerCommand> {
        self.cancel_permanent_delete_confirmation();
        self.cancel_lock_recovery();
        self.clear_external_drag();
        self.close_navigation_history_menu();
        let location = self
            .tabs
            .active_tab()
            .history
            .back_destination_at(1)?
            .location
            .clone();
        let cached = self.directory_cache.get(&location);
        let (context, location) = self
            .tabs
            .active_tab_mut()
            .begin_back_request_at_with_snapshot(1, cached)?;
        Some(ExplorerCommand::Navigate { context, location })
    }

    pub(crate) fn begin_forward_navigation(&mut self) -> Option<ExplorerCommand> {
        self.cancel_permanent_delete_confirmation();
        self.cancel_lock_recovery();
        self.clear_external_drag();
        self.close_navigation_history_menu();
        let location = self
            .tabs
            .active_tab()
            .history
            .forward_destination_at(1)?
            .location
            .clone();
        let cached = self.directory_cache.get(&location);
        let (context, location) = self
            .tabs
            .active_tab_mut()
            .begin_forward_request_at_with_snapshot(1, cached)?;
        Some(ExplorerCommand::Navigate { context, location })
    }

    pub(crate) fn begin_history_navigation(
        &mut self,
        direction: NavigationHistoryDirection,
        steps: usize,
    ) -> Option<ExplorerCommand> {
        self.cancel_permanent_delete_confirmation();
        self.cancel_lock_recovery();
        self.clear_external_drag();
        self.close_navigation_history_menu();
        let destination = match direction {
            NavigationHistoryDirection::Back => {
                self.tabs.active_tab().history.back_destination_at(steps)?
            }
            NavigationHistoryDirection::Forward => self
                .tabs
                .active_tab()
                .history
                .forward_destination_at(steps)?,
        }
        .location
        .clone();
        let cached = self.directory_cache.get(&destination);
        let (context, location) = match direction {
            NavigationHistoryDirection::Back => self
                .tabs
                .active_tab_mut()
                .begin_back_request_at_with_snapshot(steps, cached)?,
            NavigationHistoryDirection::Forward => self
                .tabs
                .active_tab_mut()
                .begin_forward_request_at_with_snapshot(steps, cached)?,
        };
        Some(ExplorerCommand::Navigate { context, location })
    }

    pub(crate) fn begin_disabled_virtual_provider_fallback(
        &mut self,
        provider_id: &str,
    ) -> Option<ExplorerCommand> {
        let history = &self.tabs.active_tab().history;
        let current_matches = matches!(
            history.current().map(|entry| &entry.location),
            Some(LocationDescriptor::Virtual(location)) if location.provider_id == provider_id
        );
        if !current_matches {
            return None;
        }
        let steps = history
            .back_entries()
            .iter()
            .rev()
            .position(|entry| {
                !matches!(
                    &entry.location,
                    LocationDescriptor::Virtual(location) if location.provider_id == provider_id
                )
            })?
            .saturating_add(1);
        self.begin_history_navigation(NavigationHistoryDirection::Back, steps)
    }

    pub(crate) fn begin_up_navigation(&mut self) -> Option<ExplorerCommand> {
        let location = &self.tabs.active_tab().history.current()?.location;
        let resolved_virtual_parent = matches!(location, LocationDescriptor::Virtual(_))
            .then(|| {
                let ancestry = &self.tabs.active_tab().view.address.resolved_ancestry;
                (ancestry
                    .last()
                    .is_some_and(|segment| &segment.location == location)
                    && ancestry.len() >= 2)
                    .then(|| ancestry[ancestry.len() - 2].location.clone())
            })
            .flatten();
        let parent = location
            .virtual_parent()
            .or(resolved_virtual_parent)
            .or_else(|| {
                location
                    .path()?
                    .parent()
                    .map(|parent| LocationDescriptor::file_system(parent.to_path_buf()))
            })?;
        self.begin_active_navigation(parent, false)
    }

    pub(crate) fn begin_refresh_navigation(&mut self) -> Option<ExplorerCommand> {
        let location = self.tabs.active_tab().history.current()?.location.clone();
        let command = self.begin_active_navigation(location.clone(), true)?;
        if let Some(context) = command.context()
            && let Some(pending) = self.pending_new_folder_rename.as_mut()
            && pending.tab_id == context.tab_id
            && pending.parent == location
        {
            pending.generation = context.generation;
        }
        Some(command)
    }

    pub(crate) fn begin_active_search(&mut self, input: String) -> Option<ExplorerCommand> {
        if input.trim().is_empty() {
            self.tabs.active_tab_mut().cancel_search();
            return None;
        }
        let location = self.tabs.active_tab().history.current()?.location.clone();
        let context = self
            .tabs
            .active_tab_mut()
            .begin_search_request(input.clone())?;
        Some(ExplorerCommand::StartSearch {
            context,
            location,
            input: explorer_model::SearchInput::new(input),
        })
    }

    pub(crate) fn begin_search_editing(&mut self) {
        let tab = self.tabs.active_tab_mut();
        let input = tab.search.input().unwrap_or_default().to_owned();
        if let explorer_model::TabSearchState::Loading { request, .. } = &tab.search {
            request.cancellation.cancel();
        }
        tab.search = explorer_model::TabSearchState::Editing(input);
    }

    pub(crate) fn update_search_edit_input(&mut self, input: String) -> bool {
        let explorer_model::TabSearchState::Editing(current) =
            &mut self.tabs.active_tab_mut().search
        else {
            return false;
        };
        *current = input;
        true
    }

    /// Applies live search-editor text through the same cancellation boundary as Clear/Escape.
    /// Empty text is not a zero-result query: it restores the underlying directory immediately.
    pub(crate) fn update_active_search_text(&mut self, input: String) -> Option<ExplorerCommand> {
        if input.trim().is_empty() {
            self.leave_active_search();
            self.begin_search_editing();
            let _ = self.update_search_edit_input(String::new());
            return None;
        }
        self.begin_active_search(input)
    }

    pub(crate) fn enter_address_edit(&mut self) {
        self.tabs.active_tab_mut().view.address.enter_editing();
    }

    pub(crate) fn update_address_edit_input(&mut self, input: String) -> bool {
        self.tabs.active_tab_mut().view.address.update_draft(input)
    }

    pub(crate) fn cancel_address_edit(&mut self) {
        let Some(current) = self.tabs.active_tab().history.current().cloned() else {
            return;
        };
        self.tabs
            .active_tab_mut()
            .view
            .address
            .cancel_editing(&current);
    }

    pub(crate) fn address_draft(&self) -> &str {
        &self.tabs.active_tab().view.address.draft
    }

    pub(crate) fn fail_address_submission(&mut self, message: String) {
        self.tabs
            .active_tab_mut()
            .view
            .address
            .navigation_failed(message);
    }

    /// Parses and starts address navigation through the same typed pipeline used by breadcrumb
    /// activation. Invalid/search-like text remains an address error and cannot mutate history or
    /// enter the independent search state.
    pub(crate) fn begin_address_submission(&mut self, input: &str) -> Option<ExplorerCommand> {
        match explorer_search::parse_address(input) {
            Ok(location) => self.begin_active_navigation(location, false),
            Err(error) => {
                self.fail_address_submission(error.message);
                None
            }
        }
    }

    pub(crate) fn open_address_menu(
        &mut self,
        segment_id: explorer_model::BreadcrumbSegmentId,
    ) -> Option<u64> {
        let tab_id = self.tabs.active_tab_id();
        if matches!(
            self.tabs.active_tab().view.address.mode,
            explorer_model::AddressBarMode::EnumeratingMenu {
                segment_id: active,
                ..
            } if active == segment_id
        ) {
            if let Some(pending) = self.breadcrumb_menu_requests.remove(&tab_id) {
                pending.context.cancellation.cancel();
            }
            self.tabs.active_tab_mut().view.address.close_menu();
            return None;
        }
        self.tabs
            .active_tab_mut()
            .view
            .address
            .begin_menu(segment_id)
    }

    pub(crate) fn toggle_address_overflow(&mut self) -> bool {
        if self.tabs.active_tab().view.address.overflow_open {
            return self.tabs.active_tab_mut().view.address.toggle_overflow();
        }
        self.close_address_menu();
        self.tabs.active_tab_mut().view.address.toggle_overflow()
    }

    pub(crate) fn active_address_menu_request(
        &self,
    ) -> Option<(explorer_model::BreadcrumbSegmentId, u64, LocationDescriptor)> {
        let address = &self.tabs.active_tab().view.address;
        let explorer_model::AddressBarMode::EnumeratingMenu {
            segment_id,
            generation,
        } = address.mode
        else {
            return None;
        };
        let location = if segment_id == explorer_model::BreadcrumbSegmentId(0) {
            LocationDescriptor::ParsingName("shell:MyComputerFolder".to_owned())
        } else {
            address
                .resolved_ancestry
                .iter()
                .find(|segment| segment.id == segment_id)?
                .location
                .clone()
        };
        Some((segment_id, generation, location))
    }

    pub(crate) fn begin_ancestry_request(
        &mut self,
        source: &RequestContext,
        location: LocationDescriptor,
    ) -> Option<ExplorerCommand> {
        let tab = self
            .tabs
            .tabs()
            .iter()
            .find(|tab| tab.id == source.tab_id)?;
        if tab.generation != source.generation {
            return None;
        }
        if let Some(previous) = self.ancestry_requests.remove(&source.tab_id) {
            previous.cancellation.cancel();
        }
        let context = RequestContext::new(source.tab_id, source.generation);
        self.ancestry_requests
            .insert(source.tab_id, context.clone());
        Some(ExplorerCommand::ResolveAncestry { context, location })
    }

    pub(crate) fn begin_child_container_request(&mut self) -> Option<ExplorerCommand> {
        let (segment_id, menu_generation, parent) = self.active_address_menu_request()?;
        let tab_id = self.tabs.active_tab().id;
        let generation = self.tabs.active_tab().generation;
        let context = RequestContext::new(tab_id, generation);
        if let Some(previous) = self.breadcrumb_menu_requests.remove(&tab_id) {
            previous.context.cancellation.cancel();
        }
        self.breadcrumb_menu_requests.insert(
            tab_id,
            PendingBreadcrumbMenu {
                context: context.clone(),
                segment_id,
                menu_generation,
            },
        );
        Some(ExplorerCommand::EnumerateChildContainers {
            context,
            parent,
            segment_id,
            menu_generation,
        })
    }

    pub(crate) fn close_address_menu(&mut self) {
        let tab_id = self.tabs.active_tab_id();
        if let Some(pending) = self.breadcrumb_menu_requests.remove(&tab_id) {
            pending.context.cancellation.cancel();
        }
        self.tabs.active_tab_mut().view.address.close_menu();
    }

    pub(crate) fn move_breadcrumb_segment_focus(&mut self, direction: i8) -> bool {
        self.tabs
            .active_tab_mut()
            .view
            .address
            .move_segment_focus(direction)
    }

    pub(crate) fn move_breadcrumb_menu_focus(
        &mut self,
        movement: explorer_model::MenuFocusMovement,
    ) -> bool {
        self.tabs
            .active_tab_mut()
            .view
            .address
            .move_menu_focus(movement)
    }

    pub(crate) fn set_breadcrumb_menu_focus(&mut self, index: usize) -> bool {
        self.tabs
            .active_tab_mut()
            .view
            .address
            .set_menu_focus(index)
    }

    pub(crate) fn typeahead_breadcrumb_menu(&mut self, text: &str) -> bool {
        self.tabs
            .active_tab_mut()
            .view
            .address
            .typeahead_menu_focus(text)
    }

    pub(crate) fn focused_breadcrumb_location(&self) -> Option<LocationDescriptor> {
        self.tabs
            .active_tab()
            .view
            .address
            .focused_segment()
            .map(|segment| segment.location.clone())
    }

    pub(crate) fn focused_breadcrumb_segment_id(
        &self,
    ) -> Option<explorer_model::BreadcrumbSegmentId> {
        self.tabs
            .active_tab()
            .view
            .address
            .focused_segment()
            .map(|segment| segment.id)
    }

    pub(crate) fn focused_breadcrumb_menu_location(&self) -> Option<LocationDescriptor> {
        self.tabs
            .active_tab()
            .view
            .address
            .focused_menu_item()
            .map(|item| item.location.clone())
    }

    pub(crate) fn leave_active_search(&mut self) {
        self.tabs.active_tab_mut().cancel_search();
    }

    fn validate_ancestry_context(&self, context: &RequestContext) -> bool {
        let valid = self.ancestry_requests.get(&context.tab_id) == Some(context)
            && self
                .tabs
                .tabs()
                .iter()
                .any(|tab| tab.id == context.tab_id && tab.generation == context.generation);
        if !valid {
            tracing::debug!(
                request_id = ?context.request_id,
                tab_id = ?context.tab_id,
                generation = context.generation.value(),
                "rejected stale breadcrumb ancestry event"
            );
        }
        valid
    }

    fn validate_breadcrumb_menu_context(
        &self,
        context: &RequestContext,
        segment_id: explorer_model::BreadcrumbSegmentId,
        menu_generation: u64,
    ) -> bool {
        let valid = self
            .breadcrumb_menu_requests
            .get(&context.tab_id)
            .is_some_and(|pending| {
                pending.context == *context
                    && pending.segment_id == segment_id
                    && pending.menu_generation == menu_generation
            })
            && self
                .tabs
                .tabs()
                .iter()
                .any(|tab| tab.id == context.tab_id && tab.generation == context.generation);
        if !valid {
            tracing::debug!(
                request_id = ?context.request_id,
                tab_id = ?context.tab_id,
                generation = context.generation.value(),
                segment_id = segment_id.0,
                menu_generation,
                "rejected stale breadcrumb menu event"
            );
        }
        valid
    }

    #[allow(
        clippy::too_many_lines,
        reason = "the correlated window reducer keeps all mutually exclusive service terminals in one audited dispatch"
    )]
    pub(crate) fn apply_service_event(&mut self, event: ExplorerEvent) -> WindowEventOutcome {
        if let ExplorerEvent::ChildContainersFinished {
            context,
            segment_id,
            menu_generation,
            outcome,
        } = &event
        {
            tracing::debug!(
                request_id = ?context.request_id,
                generation = context.generation.value(),
                menu_generation,
                ?outcome,
                matched = self
                    .navigation_request_location(context, *segment_id, *menu_generation)
                    .is_some(),
                "navigation child enumeration terminal received"
            );
        }
        match &event {
            ExplorerEvent::ChildContainersBatch {
                context,
                segment_id,
                menu_generation,
                children,
            } if self
                .navigation_request_location(context, *segment_id, *menu_generation)
                .is_some() =>
            {
                let navigation_location =
                    self.navigation_request_location(context, *segment_id, *menu_generation);
                if let Some(location) = navigation_location {
                    let Some(tree) = self.navigation_trees.get_mut(&context.tab_id) else {
                        return WindowEventOutcome::IgnoredStale;
                    };
                    let existing_node_count = tree.nodes.len();
                    let Some(node) = tree.nodes.get_mut(&location) else {
                        return WindowEventOutcome::IgnoredStale;
                    };
                    for child in children {
                        if existing_node_count + node.children.len() >= NAVIGATION_TREE_NODE_LIMIT {
                            break;
                        }
                        if !node
                            .children
                            .iter()
                            .any(|item| item.location == child.location)
                        {
                            node.children.push(child.clone());
                        }
                    }
                    node.children
                        .sort_by_cached_key(|item| item.display_name.to_lowercase());
                    return WindowEventOutcome::Applied;
                }
            }
            ExplorerEvent::ChildContainersFinished {
                context,
                segment_id,
                menu_generation,
                outcome,
            } if self
                .navigation_request_location(context, *segment_id, *menu_generation)
                .is_some() =>
            {
                let navigation_location =
                    self.navigation_request_location(context, *segment_id, *menu_generation);
                if let Some(location) = navigation_location {
                    let Some(node) = self
                        .navigation_trees
                        .get_mut(&context.tab_id)
                        .and_then(|tree| tree.nodes.get_mut(&location))
                    else {
                        return WindowEventOutcome::IgnoredStale;
                    };
                    node.loading = false;
                    node.request = None;
                    node.loaded = !matches!(outcome, explorer_model::BreadcrumbTerminal::Cancelled);
                    node.error = match outcome {
                        explorer_model::BreadcrumbTerminal::Partial(error)
                        | explorer_model::BreadcrumbTerminal::Failed(error) => {
                            Some(error.user_message.clone())
                        }
                        _ => None,
                    };
                    return WindowEventOutcome::Applied;
                }
            }
            ExplorerEvent::AncestryBatch { context, segments } => {
                if !self.validate_ancestry_context(context) {
                    return WindowEventOutcome::IgnoredStale;
                }
                let tree = self.navigation_trees.entry(context.tab_id).or_default();
                for pair in segments.windows(2) {
                    let parent = &pair[0];
                    let child = &pair[1];
                    tree.expanded.insert(parent.location.clone());
                    if tree.nodes.len() < NAVIGATION_TREE_NODE_LIMIT {
                        let node = tree.nodes.entry(parent.location.clone()).or_default();
                        if !node
                            .children
                            .iter()
                            .any(|item| item.location == child.location)
                        {
                            node.children.push(explorer_model::BreadcrumbMenuItem {
                                display_name: child.display_name.clone(),
                                location: child.location.clone(),
                            });
                        }
                    }
                }
                let Some(tab) = self.tabs.tab_mut(context.tab_id) else {
                    return WindowEventOutcome::IgnoredStale;
                };
                let current = &mut tab.view.address.resolved_ancestry;
                if current
                    .iter()
                    .map(|segment| segment.id)
                    .eq(segments.iter().map(|segment| segment.id))
                {
                    for (existing, update) in current.iter_mut().zip(segments) {
                        let mut normalized = update.clone();
                        normalized.stabilize_display_name();
                        existing.display_name = normalized.display_name;
                        existing.icon_hint = update.icon_hint;
                        existing.is_container = update.is_container;
                    }
                } else {
                    current.clone_from(segments);
                    for segment in current {
                        segment.stabilize_display_name();
                    }
                }
                return WindowEventOutcome::Applied;
            }
            ExplorerEvent::AncestryFinished { context, .. } => {
                if !self.validate_ancestry_context(context) {
                    return WindowEventOutcome::IgnoredStale;
                }
                self.ancestry_requests.remove(&context.tab_id);
                return WindowEventOutcome::Applied;
            }
            ExplorerEvent::ChildContainersBatch {
                context,
                segment_id,
                menu_generation,
                children,
            } => {
                if !self.validate_breadcrumb_menu_context(context, *segment_id, *menu_generation) {
                    return WindowEventOutcome::IgnoredStale;
                }
                let Some(tab) = self.tabs.tab_mut(context.tab_id) else {
                    return WindowEventOutcome::IgnoredStale;
                };
                for child in children {
                    if !tab
                        .view
                        .address
                        .menu_children
                        .iter()
                        .any(|existing| existing.location == child.location)
                    {
                        tab.view.address.menu_children.push(child.clone());
                    }
                }
                tab.view
                    .address
                    .menu_children
                    .sort_by_cached_key(|child| child.display_name.to_lowercase());
                if tab.view.address.keyboard_menu_index.is_none()
                    && !tab.view.address.menu_children.is_empty()
                {
                    tab.view.address.keyboard_menu_index = Some(0);
                }
                return WindowEventOutcome::Applied;
            }
            ExplorerEvent::ChildContainersFinished {
                context,
                segment_id,
                menu_generation,
                outcome,
            } => {
                if !self.validate_breadcrumb_menu_context(context, *segment_id, *menu_generation) {
                    return WindowEventOutcome::IgnoredStale;
                }
                self.breadcrumb_menu_requests.remove(&context.tab_id);
                let Some(tab) = self.tabs.tab_mut(context.tab_id) else {
                    return WindowEventOutcome::IgnoredStale;
                };
                tab.view.address.menu_loading = false;
                tab.view.address.menu_error = match outcome {
                    explorer_model::BreadcrumbTerminal::Partial(error)
                    | explorer_model::BreadcrumbTerminal::Failed(error) => {
                        Some(error.user_message.clone())
                    }
                    explorer_model::BreadcrumbTerminal::Finished
                    | explorer_model::BreadcrumbTerminal::Empty
                    | explorer_model::BreadcrumbTerminal::Cancelled => None,
                };
                return WindowEventOutcome::Applied;
            }
            _ => {}
        }
        if let ExplorerEvent::ClipboardChanged { state } = &event {
            self.clipboard = state.clone();
            return WindowEventOutcome::Applied;
        }
        if let ExplorerEvent::ContextMenuFinished { context, outcome } = &event {
            if self.pending_context_menu.as_ref() != Some(context) {
                tracing::debug!(
                    request_id = ?context.request_id,
                    pending_request_id = ?self.pending_context_menu.as_ref().map(|pending| pending.request_id),
                    "ignored stale context-menu terminal"
                );
                return WindowEventOutcome::IgnoredStale;
            }
            tracing::debug!(
                request_id = ?context.request_id,
                outcome = ?outcome,
                replacement_queued = self.queued_context_menu.is_some(),
                "applying context-menu terminal"
            );
            self.pending_context_menu = None;
            self.context_menu_error = match outcome {
                explorer_model::ContextMenuOutcome::Failed { error } => Some(error.clone()),
                explorer_model::ContextMenuOutcome::Cancelled
                | explorer_model::ContextMenuOutcome::ReplayRequested { .. }
                | explorer_model::ContextMenuOutcome::Invoked { .. }
                | explorer_model::ContextMenuOutcome::Delegated { .. } => None,
            };
            if let Some((queued_context, request)) = self.queued_context_menu.take()
                && self.tabs.active_tab().id == queued_context.tab_id
                && self.tabs.active_tab().generation == queued_context.generation
            {
                tracing::debug!(
                    request_id = ?queued_context.request_id,
                    "promoting queued context-menu replacement"
                );
                self.pending_context_menu = Some(queued_context.clone());
                self.pending_context_menu_command = Some(ExplorerCommand::ShowContextMenu {
                    context: queued_context,
                    request,
                });
            }
            return WindowEventOutcome::Applied;
        }
        if let ExplorerEvent::LockOwnersDiscovered { context, outcome } = &event {
            let Some(recovery) = self.lock_recovery.as_mut() else {
                return WindowEventOutcome::IgnoredStale;
            };
            if recovery.phase != LockRecoveryPhase::Discovering
                || recovery.request_context != *context
            {
                return WindowEventOutcome::IgnoredStale;
            }
            match outcome {
                LockOwnerDiscoveryTerminal::Ready(owners) => {
                    recovery.owners.clone_from(owners);
                    recovery.phase = LockRecoveryPhase::Ready;
                    recovery.focus_index = 0;
                    recovery.status = format!(
                        "{} application(s) are using the selected item.",
                        owners.len()
                    );
                }
                LockOwnerDiscoveryTerminal::Empty => {
                    recovery.phase = LockRecoveryPhase::Unavailable;
                    recovery.focus_index = 0;
                    "Windows could not identify the application using this item."
                        .clone_into(&mut recovery.status);
                }
                LockOwnerDiscoveryTerminal::Unavailable(error)
                | LockOwnerDiscoveryTerminal::Failed(error) => {
                    recovery.phase = LockRecoveryPhase::Unavailable;
                    recovery.focus_index = 0;
                    recovery.status.clone_from(&error.user_message);
                }
                LockOwnerDiscoveryTerminal::DeadlineElapsed => {
                    recovery.phase = LockRecoveryPhase::Unavailable;
                    recovery.focus_index = 0;
                    "Lock owner discovery reached its deadline.".clone_into(&mut recovery.status);
                }
                LockOwnerDiscoveryTerminal::Cancelled => {
                    self.lock_recovery = None;
                }
            }
            return WindowEventOutcome::Applied;
        }
        if let ExplorerEvent::LockOwnersClosed { context, outcome } = &event {
            let Some(recovery) = self.lock_recovery.as_mut() else {
                return WindowEventOutcome::IgnoredStale;
            };
            if recovery.phase != LockRecoveryPhase::Closing || recovery.request_context != *context
            {
                return WindowEventOutcome::IgnoredStale;
            }
            match outcome {
                LockOwnerCloseTerminal::Closed(outcomes) => {
                    recovery.close_outcomes.clone_from(outcomes);
                    let request = recovery.original_request.clone();
                    recovery.phase = LockRecoveryPhase::Retrying;
                    "Retrying the delete operation…".clone_into(&mut recovery.status);
                    let command = self.queue_file_operation(request);
                    if let Some(recovery) = self.lock_recovery.as_mut() {
                        recovery.retry_operation_id =
                            command.context().map(|value| value.request_id);
                    }
                    self.pending_lock_recovery_command = Some(command);
                }
                LockOwnerCloseTerminal::Partial(outcomes) => {
                    recovery.close_outcomes.clone_from(outcomes);
                    recovery.phase = LockRecoveryPhase::Partial;
                    recovery.focus_index = 0;
                    "Some applications did not close. No process was force-terminated."
                        .clone_into(&mut recovery.status);
                }
                LockOwnerCloseTerminal::Failed(error) => {
                    recovery.phase = LockRecoveryPhase::Partial;
                    recovery.focus_index = 0;
                    recovery.status.clone_from(&error.user_message);
                }
                LockOwnerCloseTerminal::Cancelled => {
                    recovery.phase = LockRecoveryPhase::Ready;
                    recovery.focus_index = 0;
                    "Closing applications was cancelled.".clone_into(&mut recovery.status);
                }
            }
            return WindowEventOutcome::Applied;
        }
        if let ExplorerEvent::ThumbnailCacheCleared { success, .. } = &event {
            self.thumbnail_cache_notice = Some(if *success {
                "縮圖快取已清除".to_owned()
            } else {
                "無法完整清除縮圖快取；可重試，檔案瀏覽仍可使用".to_owned()
            });
            return WindowEventOutcome::Applied;
        }
        if matches!(
            event,
            ExplorerEvent::OperationProgress { .. } | ExplorerEvent::OperationFinished { .. }
        ) {
            if matches!(event, ExplorerEvent::OperationFinished { .. }) {
                self.cancel_permanent_delete_confirmation();
            }
            let cleared_drag = matches!(event, ExplorerEvent::OperationFinished { .. })
                && !matches!(
                    self.drag_session.state(),
                    explorer_model::DragSessionState::Idle
                );
            if cleared_drag {
                self.clear_external_drag();
            }
            let recovery_transition =
                if let ExplorerEvent::OperationFinished { context, outcome } = &event {
                    self.handle_locked_delete_terminal(context, outcome)
                } else {
                    false
                };
            let operation_applied = self.operation_center.apply_event(&event);
            if operation_applied
                && let ExplorerEvent::OperationFinished { context, .. } = &event
                && self
                    .operation_center
                    .latest()
                    .is_some_and(|record| record.id == context.request_id)
            {
                self.operation_terminal_notice = Some((context.request_id, Instant::now()));
            }
            return if operation_applied || cleared_drag || recovery_transition {
                WindowEventOutcome::Applied
            } else {
                WindowEventOutcome::IgnoredStale
            };
        }
        let resolved_tab = match &event {
            ExplorerEvent::LocationResolved { context, .. } => Some(context.tab_id),
            _ => None,
        };
        let completed_directory = match &event {
            ExplorerEvent::DirectoryFinished { context } => self
                .tabs
                .tabs()
                .iter()
                .find(|tab| tab.id == context.tab_id && tab.directory.accepts(context).is_ok())
                .and_then(|tab| {
                    tab.history
                        .current()
                        .map(|entry| (context.tab_id, entry.location.clone()))
                }),
            _ => None,
        };
        let outcome = self.tabs.apply_event(event);
        if outcome == WindowEventOutcome::Applied {
            if let Some((tab_id, location)) = completed_directory
                && let Some(snapshot) = self
                    .tabs
                    .tabs()
                    .iter()
                    .find(|tab| tab.id == tab_id)
                    .and_then(|tab| match &tab.directory {
                        DirectoryState::Ready(snapshot) => Some(snapshot.clone()),
                        _ => None,
                    })
            {
                self.directory_cache.insert(&location, snapshot);
            }
            if let Some(tab_id) = resolved_tab {
                self.details_filters.entry(tab_id).or_default().clear_all();
                self.details_filter_menu = None;
                if self
                    .pending_new_folder_rename
                    .as_ref()
                    .is_some_and(|pending| {
                        pending.tab_id == tab_id
                            && self
                                .tabs
                                .active_tab()
                                .history
                                .current()
                                .is_none_or(|entry| {
                                    entry.location != pending.parent
                                        || self.tabs.active_tab().generation != pending.generation
                                })
                    })
                {
                    self.pending_new_folder_rename = None;
                }
            }
        }
        outcome
    }

    pub(crate) fn begin_file_operation(
        &mut self,
        request: FileOperationRequest,
    ) -> ExplorerCommand {
        self.cancel_lock_recovery();
        self.queue_file_operation(request)
    }

    pub(crate) fn begin_interactive_create_folder(
        &mut self,
        request: FileOperationRequest,
    ) -> bool {
        let (parent, name, use_create_item) = match &request.kind {
            FileOperationKind::CreateFolder { parent, name } => {
                (parent.clone(), name.clone(), false)
            }
            FileOperationKind::CreateItem {
                parent,
                name,
                recipe: explorer_model::ShellNewItemRecipe::Folder,
            } => (parent.clone(), name.clone(), true),
            _ => return false,
        };
        let tab = self.tabs.active_tab();
        let mut identity = b"super-explorer:new-folder-draft:".to_vec();
        identity.extend_from_slice(format!("{:?}", tab.id).as_bytes());
        let item_id = ShellItemId::from_provider_bytes(identity)
            .expect("new-folder draft identity is non-empty");
        self.pending_new_folder_rename = Some(PendingNewFolderRename {
            tab_id: tab.id,
            generation: tab.generation,
            parent: parent.clone(),
            item_id: item_id.clone(),
            use_create_item,
        });
        self.focus(FocusSurface::FileView);
        self.rename_editor = Some(explorer_model::RenameEditorState::begin(
            ItemDescriptor {
                id: item_id,
                location: parent,
            },
            name,
            true,
        ));
        true
    }

    fn queue_file_operation(&mut self, request: FileOperationRequest) -> ExplorerCommand {
        let tab = self.tabs.active_tab();
        let context = RequestContext::new(tab.id, tab.generation);
        let total_items = operation_item_count(&request);
        let mut record = OperationRecord::queued(context.request_id, request.clone(), total_items);
        let started = record.start().is_ok();
        debug_assert!(started, "new operation record starts exactly once");
        let inserted = self.operation_center.insert(record);
        debug_assert!(inserted, "request identifiers are unique");
        self.operation_terminal_notice = None;
        ExplorerCommand::ExecuteFileOperation { context, request }
    }

    pub(crate) const fn lock_recovery(&self) -> Option<&LockRecoveryUiState> {
        self.lock_recovery.as_ref()
    }

    pub(crate) fn take_pending_lock_recovery_command(&mut self) -> Option<ExplorerCommand> {
        self.pending_lock_recovery_command.take()
    }

    pub(crate) fn move_lock_recovery_focus(&mut self, direction: i8) -> bool {
        let Some(recovery) = self.lock_recovery.as_mut() else {
            return false;
        };
        recovery.move_focus(direction);
        true
    }

    pub(crate) fn cancel_lock_recovery(&mut self) -> bool {
        let Some(recovery) = self.lock_recovery.take() else {
            return false;
        };
        recovery.request_context.cancellation.cancel();
        self.pending_lock_recovery_command = None;
        true
    }

    pub(crate) fn retry_locked_delete(&mut self) -> Option<ExplorerCommand> {
        let limits = explorer_common::RoadmapLimits::default();
        let recovery = self.lock_recovery.as_mut()?;
        if !recovery.can_retry() || recovery.retry_count >= limits.lock_recovery_max_retries {
            return None;
        }
        recovery.retry_count += 1;
        recovery.phase = LockRecoveryPhase::Retrying;
        "Retrying the delete operation…".clone_into(&mut recovery.status);
        let request = recovery.original_request.clone();
        let command = self.queue_file_operation(request);
        if let Some(recovery) = self.lock_recovery.as_mut() {
            recovery.retry_operation_id = command.context().map(|value| value.request_id);
        }
        Some(command)
    }

    pub(crate) fn close_lock_owners_and_retry(&mut self) -> Option<ExplorerCommand> {
        let limits = explorer_common::RoadmapLimits::default();
        let recovery = self.lock_recovery.as_mut()?;
        if !recovery.can_close() || recovery.retry_count >= limits.lock_recovery_max_retries {
            return None;
        }
        let owners = recovery
            .owners
            .iter()
            .filter(|owner| owner.can_close())
            .map(|owner| owner.identity)
            .collect::<Vec<_>>();
        if owners.is_empty() {
            return None;
        }
        let resources = delete_resources(&recovery.original_request);
        let tab = self.tabs.active_tab();
        let context = RequestContext::new(tab.id, tab.generation);
        recovery.request_context.cancellation.cancel();
        recovery.request_context = context.clone();
        recovery.retry_count += 1;
        recovery.phase = LockRecoveryPhase::Closing;
        "Asking the selected applications to close…".clone_into(&mut recovery.status);
        Some(ExplorerCommand::CloseLockOwners {
            context,
            request: LockOwnerCloseRequest { resources, owners },
        })
    }

    fn handle_locked_delete_terminal(
        &mut self,
        context: &RequestContext,
        outcome: &OperationTerminal,
    ) -> bool {
        let retry_matches = self
            .lock_recovery
            .as_ref()
            .and_then(|recovery| recovery.retry_operation_id)
            == Some(context.request_id);
        if retry_matches && matches!(outcome, OperationTerminal::Finished) {
            self.lock_recovery = None;
            return true;
        }
        let Some(record) = self.operation_center.get(context.request_id) else {
            return false;
        };
        if !is_delete_request(&record.request) || !terminal_has_lock_error(outcome) {
            if retry_matches {
                self.lock_recovery = None;
                return true;
            }
            return false;
        }
        let tab = self.tabs.active_tab();
        if tab.id != context.tab_id || tab.generation != context.generation {
            return false;
        }
        let limits = explorer_common::RoadmapLimits::default();
        let retry_count = self
            .lock_recovery
            .as_ref()
            .filter(|recovery| recovery.retry_operation_id == Some(context.request_id))
            .map_or(0, |recovery| recovery.retry_count);
        if retry_count >= limits.lock_recovery_max_retries {
            if let Some(recovery) = self.lock_recovery.as_mut() {
                recovery.phase = LockRecoveryPhase::Unavailable;
                "The retry limit was reached. The item was not deleted."
                    .clone_into(&mut recovery.status);
            }
            return true;
        }
        let resources = locked_delete_resources(&record.request, outcome);
        if resources.is_empty() {
            return false;
        }
        let request_context = RequestContext::new(tab.id, tab.generation);
        self.pending_lock_recovery_command = Some(ExplorerCommand::DiscoverLockOwners {
            context: request_context.clone(),
            request: LockOwnerDiscoveryRequest {
                resources: resources.clone(),
            },
        });
        self.lock_recovery = Some(LockRecoveryUiState {
            phase: LockRecoveryPhase::Discovering,
            owners: Vec::new(),
            close_outcomes: Vec::new(),
            status: "Finding applications that are using the selected item…".to_owned(),
            item_count: resources.len(),
            original_request: record.request.clone(),
            request_context,
            retry_operation_id: None,
            retry_count,
            focus_index: 0,
        });
        true
    }

    pub(crate) fn create_folder_request(&self) -> Option<FileOperationRequest> {
        if !self.active_presentation().can_write {
            return None;
        }
        let parent = self.tabs.active_tab().history.current()?.location.clone();
        let existing = self
            .tabs
            .active_tab()
            .visible_snapshot()
            .map(|snapshot| {
                snapshot
                    .entries()
                    .iter()
                    .map(|entry| entry.display_name.to_lowercase())
                    .collect::<HashSet<_>>()
            })
            .unwrap_or_default();
        let mut name = "New folder".to_owned();
        for ordinal in 2..=10_000_u32 {
            if !existing.contains(&name.to_lowercase()) {
                break;
            }
            name = format!("New folder ({ordinal})");
        }
        Some(FileOperationRequest {
            kind: FileOperationKind::CreateFolder { parent, name },
            flags: explorer_model::FileOperationFlags {
                conflict: explorer_model::ConflictDecision::KeepBoth,
                ..explorer_model::FileOperationFlags::default()
            },
        })
    }

    pub(crate) fn create_new_item_request(&self, index: usize) -> Option<FileOperationRequest> {
        if !self.active_presentation().can_write {
            return None;
        }
        let descriptor = self.new_items.get(index)?;
        let parent = self.tabs.active_tab().history.current()?.location.clone();
        let extension = descriptor.extension.as_deref().unwrap_or("");
        let existing = self
            .tabs
            .active_tab()
            .visible_snapshot()
            .map(|snapshot| {
                snapshot
                    .entries()
                    .iter()
                    .map(|entry| entry.display_name.to_lowercase())
                    .collect::<HashSet<_>>()
            })
            .unwrap_or_default();
        let mut name = format!("{}{}", descriptor.default_stem, extension);
        for ordinal in 2..=10_000_u32 {
            if !existing.contains(&name.to_lowercase()) {
                break;
            }
            name = format!("{} ({ordinal}){}", descriptor.default_stem, extension);
        }
        Some(FileOperationRequest {
            kind: FileOperationKind::CreateItem {
                parent,
                name,
                recipe: descriptor.recipe.clone(),
            },
            flags: explorer_model::FileOperationFlags {
                conflict: explorer_model::ConflictDecision::KeepBoth,
                ..explorer_model::FileOperationFlags::default()
            },
        })
    }

    pub(crate) fn begin_inline_rename(&mut self, row_index: usize) -> bool {
        if !self.active_presentation().can_write {
            return false;
        }
        let Some(entry) = self.presentation_entry(row_index) else {
            return false;
        };
        self.rename_editor = Some(explorer_model::RenameEditorState::begin(
            ItemDescriptor {
                id: entry.id,
                location: entry.location,
            },
            entry.display_name,
            entry.is_container,
        ));
        true
    }

    /// Starts Explorer-style keyboard rename for the current item.
    ///
    /// Navigation clears the previous directory's stable selection, while keyboard routing treats
    /// the first visible row as current. Establish that same row as the selection before opening
    /// the editor so F2 remains deterministic immediately after switching directories.
    pub(crate) fn begin_focused_inline_rename(&mut self) -> bool {
        let row_index = self
            .focused_row_index()
            .or_else(|| (self.visible_row_count() > 0).then_some(0));
        let Some(row_index) = row_index else {
            return false;
        };
        if self.focused_row_index().is_none() && !self.select_row(row_index) {
            return false;
        }
        self.begin_inline_rename(row_index)
    }

    pub(crate) fn select_row(&mut self, row_index: usize) -> bool {
        let Some(id) = self.presentation_entry(row_index).map(|entry| entry.id) else {
            return false;
        };
        self.tabs.active_tab_mut().selection.select_only(id);
        true
    }

    pub(crate) fn select_location(&mut self, location: &LocationDescriptor) -> bool {
        let Some(row_index) = self.directory_presentation().and_then(|presentation| {
            (0..presentation.len()).find(|row_index| {
                presentation
                    .entry(*row_index)
                    .is_some_and(|(_, entry)| &entry.location == location)
            })
        }) else {
            return false;
        };
        self.select_row(row_index)
    }

    pub(crate) fn typeahead_file_view(&mut self, text: &str, now: Instant) -> Option<usize> {
        if self.focused_surface() != FocusSurface::FileView
            || self.rename_editor.is_some()
            || text.is_empty()
        {
            return None;
        }
        let normalized = text.to_lowercase();
        if normalized.chars().all(char::is_whitespace) {
            return None;
        }
        let tab = self.tabs.active_tab();
        let tab_id = tab.id;
        let generation = tab.generation;
        let continuation = self.file_view_typeahead.as_ref().is_some_and(|session| {
            session.tab_id == tab_id
                && session.generation == generation
                && now
                    .checked_duration_since(session.last_input)
                    .is_some_and(|elapsed| elapsed <= FILE_VIEW_TYPEAHEAD_TIMEOUT)
        });
        let mut prefix = if continuation {
            let mut prefix = self
                .file_view_typeahead
                .as_ref()
                .map(|session| session.prefix.clone())
                .unwrap_or_default();
            prefix.push_str(&normalized);
            prefix
        } else {
            normalized.clone()
        };
        let presentation = self.directory_presentation()?;
        let row_count = presentation.len();
        let find_from = |prefix: &str, start: usize| {
            (0..row_count)
                .map(|offset| (start + offset) % row_count.max(1))
                .find(|row_index| {
                    presentation.entry(*row_index).is_some_and(|(_, entry)| {
                        entry.display_name.to_lowercase().starts_with(prefix)
                    })
                })
        };
        let mut target = find_from(&prefix, 0);
        if target.is_none() && continuation {
            prefix = normalized;
            let start = self
                .focused_row_index()
                .map_or(0, |current| current.saturating_add(1) % row_count.max(1));
            target = find_from(&prefix, start);
        }
        self.file_view_typeahead = Some(FileViewTypeAhead {
            tab_id,
            generation,
            prefix,
            last_input: now,
        });
        if let Some(row_index) = target {
            let _ = self.select_row(row_index);
        }
        target
    }

    pub(crate) fn clear_file_view_typeahead(&mut self) -> bool {
        self.file_view_typeahead.take().is_some()
    }

    pub(crate) const fn file_view_typeahead_active(&self) -> bool {
        self.file_view_typeahead.is_some()
    }

    #[cfg(test)]
    pub(crate) fn file_view_typeahead_prefix(&self) -> Option<&str> {
        self.file_view_typeahead
            .as_ref()
            .map(|session| session.prefix.as_str())
    }

    pub(crate) fn toggle_row(&mut self, row_index: usize) -> bool {
        let Some(id) = self.presentation_entry(row_index).map(|entry| entry.id) else {
            return false;
        };
        self.tabs.active_tab_mut().selection.toggle(id);
        true
    }

    #[cfg(test)]
    pub(crate) fn select_row_additive(&mut self, row_index: usize) -> bool {
        let Some(id) = self.presentation_entry(row_index).map(|entry| entry.id) else {
            return false;
        };
        self.tabs.active_tab_mut().selection.select_additive(id);
        true
    }

    fn presentation_ids(&self) -> Vec<ShellItemId> {
        self.directory_presentation()
            .map(|presentation| {
                presentation
                    .ordered_indices()
                    .iter()
                    .filter_map(|index| presentation.entries().get(*index))
                    .map(|entry| entry.id.clone())
                    .collect()
            })
            .unwrap_or_default()
    }

    fn presentation_entry(&self, row_index: usize) -> Option<explorer_model::FileEntry> {
        self.directory_presentation().and_then(|presentation| {
            presentation
                .entry(row_index)
                .map(|(_, entry)| entry.clone())
        })
    }

    pub(crate) fn remote_folder_symlink_request(
        &self,
        row_index: usize,
    ) -> Option<(LocationDescriptor, String, String)> {
        let tab = self.tabs.active_tab();
        let parent = tab.history.current()?.location.clone();
        let LocationDescriptor::Virtual(remote_parent) = &parent else {
            return None;
        };
        if !matches!(remote_parent.provider_id.as_str(), "adb" | "sftp")
            || !self.active_presentation().can_write
        {
            return None;
        }
        let entry = self.presentation_entry(row_index)?;
        if !entry.is_container {
            return None;
        }
        let existing = tab
            .visible_snapshot()?
            .entries()
            .iter()
            .map(|entry| entry.display_name.as_str())
            .collect::<HashSet<_>>();
        let name = unique_remote_folder_symlink_name(&entry.display_name, &existing);
        Some((parent, name, entry.display_name))
    }

    pub(crate) fn directory_presentation(&self) -> Option<crate::file_view::DirectoryPresentation> {
        let tab = self.tabs.active_tab();
        let snapshot = tab.visible_snapshot()?;
        let presentation = self
            .presentation_cache
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .resolve_filtered(
                snapshot,
                tab.view.settings.hidden_items,
                &tab.view.settings.sort,
                self.details_filters
                    .get(&self.tabs.active_tab_id())
                    .cloned()
                    .unwrap_or_default(),
            );
        if tab.view.settings.sort.column
            == crate::folder_size_column::folder_size_column_descriptor().id
        {
            Some(presentation.sorted_by_extension_bytes(
                &self.folder_size_sort_values,
                tab.view.settings.sort.direction,
            ))
        } else if tab.view.settings.sort.column == explorer_model::ColumnId::Size {
            let mut values = self.folder_size_sort_values.clone();
            for entry in snapshot.entries() {
                if !crate::folder_size_column::applies_to_shell_entry(
                    entry.is_container,
                    entry.metadata.size_bytes,
                ) {
                    values.insert(entry.id.clone(), entry.metadata.size_bytes);
                }
            }
            Some(presentation.sorted_by_extension_bytes(&values, tab.view.settings.sort.direction))
        } else if let Some(values) = self
            .builtin_count_sort_values
            .get(&tab.view.settings.sort.column)
        {
            Some(presentation.sorted_by_extension_bytes(values, tab.view.settings.sort.direction))
        } else if let Some(values) = self
            .code_lines_sort_values
            .get(&tab.view.settings.sort.column)
        {
            Some(presentation.sorted_by_extension_bytes(values, tab.view.settings.sort.direction))
        } else {
            Some(presentation)
        }
    }

    pub(crate) fn presentation_rebuilds(&self) -> u64 {
        self.presentation_cache
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .rebuilds()
    }

    pub(crate) fn presentation_row_is_container(&self, row_index: usize) -> bool {
        self.presentation_entry(row_index)
            .is_some_and(|entry| entry.is_container)
    }

    pub(crate) fn select_row_range(&mut self, row_index: usize, additive: bool) -> bool {
        let Some(target) = self.presentation_entry(row_index).map(|entry| entry.id) else {
            return false;
        };
        let order = self.presentation_ids();
        self.tabs
            .active_tab_mut()
            .selection
            .select_range(&order, target, additive);
        true
    }

    pub(crate) fn focus_row(&mut self, row_index: usize) -> bool {
        let Some(id) = self.presentation_entry(row_index).map(|entry| entry.id) else {
            return false;
        };
        self.tabs.active_tab_mut().selection.focus_only(id);
        true
    }

    pub(crate) fn select_all_rows(&mut self) {
        let order = self.presentation_ids();
        self.tabs.active_tab_mut().selection.select_all(&order);
    }

    pub(crate) fn invert_selection(&mut self) {
        let order = self.presentation_ids();
        self.tabs.active_tab_mut().selection.invert(&order);
    }

    pub(crate) fn clear_selection(&mut self) {
        self.tabs.active_tab_mut().selection.clear();
    }

    pub(crate) fn focused_row_index(&self) -> Option<usize> {
        let focused = self.tabs.active_tab().selection.focused()?;
        self.presentation_ids().iter().position(|id| id == focused)
    }

    pub(crate) fn visible_row_count(&self) -> usize {
        self.presentation_ids().len()
    }

    pub(crate) fn prepare_context_selection(&mut self, item_id: Option<&ShellItemId>) {
        let Some(item_id) = item_id else {
            self.tabs.active_tab_mut().selection.clear();
            return;
        };
        if !self.presentation_ids().iter().any(|id| id == item_id) {
            return;
        }
        if self.tabs.active_tab().selection.contains(item_id) {
            // Explorer preserves an existing multi-selection on a secondary click, while moving
            // its focus rectangle to the item under the pointer.
            self.tabs
                .active_tab_mut()
                .selection
                .focus_only(item_id.clone());
        } else {
            self.tabs
                .active_tab_mut()
                .selection
                .select_only(item_id.clone());
        }
    }

    pub(crate) fn begin_context_item_gesture(
        &mut self,
        item_id: ShellItemId,
        x: f32,
        y: f32,
        extended_verbs: bool,
    ) -> bool {
        if !self.presentation_ids().iter().any(|id| id == &item_id) {
            return false;
        }
        self.prepare_context_selection(Some(&item_id));
        self.pending_context_hit = Some(item_id);
        self.pending_context_extended_verbs = extended_verbs;
        self.begin_drag_candidate(x, y, explorer_model::DragButton::Right)
    }

    pub(crate) fn restore_context_target_selection(
        &mut self,
        target: &explorer_model::ShellContextMenuTarget,
    ) -> bool {
        let explorer_model::ShellContextMenuTarget::Items { items, .. } = target else {
            return false;
        };
        let visible = self.presentation_ids().into_iter().collect::<HashSet<_>>();
        let mut ids = items
            .iter()
            .map(|item| item.id.clone())
            .filter(|id| visible.contains(id));
        let Some(first) = ids.next() else {
            return false;
        };
        let selection = &mut self.tabs.active_tab_mut().selection;
        selection.select_only(first);
        for id in ids {
            selection.select_additive(id);
        }
        true
    }

    pub(crate) fn begin_context_menu_request(
        &mut self,
        item_id: Option<ShellItemId>,
        owner_window: u64,
        x: i32,
        y: i32,
        client_x: f32,
        client_y: f32,
        keyboard_invoked: bool,
        extended_verbs: bool,
    ) -> Option<ExplorerCommand> {
        if item_id.is_some()
            && !keyboard_invoked
            && !matches!(
                self.drag_session.state(),
                explorer_model::DragSessionState::Candidate {
                    button: explorer_model::DragButton::Right,
                    ..
                }
            )
        {
            return None;
        }
        let tab = self.tabs.active_tab();
        let parent = tab.history.current()?.location.clone();
        let context = RequestContext::new(tab.id, tab.generation);
        let item_id = if item_id.is_some() && !keyboard_invoked {
            let item_id = self.pending_context_hit.take().or(item_id);
            self.pending_context_extended_verbs = false;
            item_id
        } else {
            item_id
        };
        let target = if let Some(item_id) = item_id {
            if !tab.selection.contains(&item_id) {
                return None;
            }
            let items = self.selected_items();
            if items.is_empty() {
                return None;
            }
            explorer_model::ShellContextMenuTarget::Items { parent, items }
        } else {
            explorer_model::ShellContextMenuTarget::Background { parent }
        };
        let custom_virtual_parent = match &target {
            explorer_model::ShellContextMenuTarget::Items { parent, .. }
            | explorer_model::ShellContextMenuTarget::Background { parent } => {
                matches!(parent, LocationDescriptor::Virtual(_))
            }
        };
        if custom_virtual_parent {
            let background = matches!(
                target,
                explorer_model::ShellContextMenuTarget::Background { .. }
            );
            let item_row_index = (!background).then(|| self.focused_row_index()).flatten();
            self.remote_context_menu = Some(RemoteContextMenuState {
                x: client_x.max(0.0),
                y: client_y.max(0.0),
                background,
                paste_available: self.paste_available(),
                item_row_index,
                item_is_container: item_row_index
                    .is_some_and(|row| self.presentation_row_is_container(row)),
            });
            self.pending_context_hit = None;
            self.pending_context_extended_verbs = false;
            return None;
        }
        let request = explorer_model::ContextMenuRequest {
            target,
            owner_window,
            point: explorer_model::MenuPoint { x, y },
            keyboard_invoked,
            invocation_profile: if extended_verbs {
                explorer_model::ContextMenuInvocationProfile::ExplorerExtended
            } else {
                explorer_model::ContextMenuInvocationProfile::Explorer
            },
            color_scheme: if matches!(self.current_theme, ThemeMode::Dark) {
                explorer_model::ContextMenuColorScheme::Dark
            } else {
                explorer_model::ContextMenuColorScheme::Light
            },
            immersive_native_context_menus: tab.view.settings.immersive_native_context_menus,
            paste_available: self.paste_available(),
            requested_verb: None,
            deadline_ms: 2_000,
        };
        if let Some(pending) = self.pending_context_menu.as_ref() {
            let request_id = pending.request_id;
            pending.cancellation.cancel();
            if !keyboard_invoked {
                tracing::debug!(
                    pending_request_id = ?request_id,
                    replacement_request_id = ?context.request_id,
                    x,
                    y,
                    "superseding completed native popup with mouse replacement"
                );
                self.queued_context_menu = None;
                self.pending_context_menu_command = None;
                self.pending_context_menu = Some(context.clone());
                return Some(ExplorerCommand::ShowContextMenu { context, request });
            }
            tracing::debug!(
                pending_request_id = ?request_id,
                replacement_request_id = ?context.request_id,
                x,
                y,
                "queued latest context-menu replacement and cancelled active request"
            );
            self.queued_context_menu = Some((context, request));
            return Some(ExplorerCommand::Cancel { request_id });
        }
        self.pending_context_menu = Some(context.clone());
        Some(ExplorerCommand::ShowContextMenu { context, request })
    }

    pub(crate) fn pending_context_item_id(&self) -> Option<ShellItemId> {
        self.pending_context_hit.clone()
    }

    pub(crate) const fn pending_context_extended_verbs(&self) -> bool {
        self.pending_context_extended_verbs
    }

    pub(crate) fn context_menu_pending(&self) -> bool {
        self.pending_context_menu.is_some()
    }

    pub(crate) fn cancel_pending_context_menu(&mut self) -> Option<ExplorerCommand> {
        self.queued_context_menu = None;
        let context = self.pending_context_menu.as_ref()?;
        context.cancellation.cancel();
        Some(ExplorerCommand::Cancel {
            request_id: context.request_id,
        })
    }

    pub(crate) fn take_pending_context_menu_command(&mut self) -> Option<ExplorerCommand> {
        self.pending_context_menu_command.take()
    }

    pub(crate) fn begin_share_request(&mut self, owner_window: u64) -> Option<ExplorerCommand> {
        self.begin_context_verb_request(owner_window, "Windows.Share")
    }

    pub(crate) fn begin_pin_to_start_request(
        &mut self,
        owner_window: u64,
    ) -> Option<ExplorerCommand> {
        self.begin_context_verb_request(owner_window, "PinToStartScreen")
    }

    pub(crate) fn begin_compress_to_zip_request(
        &mut self,
        owner_window: u64,
    ) -> Option<ExplorerCommand> {
        self.begin_context_verb_request(owner_window, "Windows.CompressToZip")
    }

    pub(crate) fn begin_undo_request(&mut self, owner_window: u64) -> Option<ExplorerCommand> {
        let tab = self.tabs.active_tab();
        let context = RequestContext::new(tab.id, tab.generation);
        let request = explorer_model::ContextMenuRequest {
            target: explorer_model::ShellContextMenuTarget::Background {
                parent: tab.history.current()?.location.clone(),
            },
            owner_window,
            point: explorer_model::MenuPoint { x: 0, y: 0 },
            keyboard_invoked: true,
            invocation_profile: explorer_model::ContextMenuInvocationProfile::Explorer,
            color_scheme: explorer_model::ContextMenuColorScheme::Light,
            immersive_native_context_menus: false,
            paste_available: false,
            requested_verb: Some("undo".to_owned()),
            deadline_ms: 2_000,
        };
        self.pending_context_menu = Some(context.clone());
        Some(ExplorerCommand::ShowContextMenu { context, request })
    }

    pub(crate) fn begin_properties_request(
        &mut self,
        owner_window: u64,
    ) -> Option<ExplorerCommand> {
        self.begin_context_verb_request(owner_window, "properties")
    }

    /// Invokes Properties for the immutable target captured by the native popup session.
    ///
    /// A directory refresh can replace visible row identities while the Shell menu is open.
    /// Reconstructing the target from the later UI selection can then silently drop the command
    /// or apply it to another row. Explorer keeps the popup's data object alive instead, so carry
    /// the captured descriptor across the asynchronous delegation boundary.
    pub(crate) fn begin_properties_request_for_target(
        &mut self,
        owner_window: u64,
        target: explorer_model::ShellContextMenuTarget,
    ) -> Option<ExplorerCommand> {
        if !matches!(
            &target,
            explorer_model::ShellContextMenuTarget::Items { items, .. } if !items.is_empty()
        ) {
            return None;
        }
        let tab = self.tabs.active_tab();
        let context = RequestContext::new(tab.id, tab.generation);
        let request = explorer_model::ContextMenuRequest {
            target,
            owner_window,
            point: explorer_model::MenuPoint { x: 0, y: 0 },
            keyboard_invoked: true,
            invocation_profile: explorer_model::ContextMenuInvocationProfile::Explorer,
            color_scheme: explorer_model::ContextMenuColorScheme::Light,
            immersive_native_context_menus: false,
            paste_available: false,
            requested_verb: Some("properties".to_owned()),
            deadline_ms: 2_000,
        };
        self.pending_context_menu = Some(context.clone());
        Some(ExplorerCommand::ShowContextMenu { context, request })
    }

    pub(crate) fn begin_restore_request(&mut self, owner_window: u64) -> Option<ExplorerCommand> {
        if !self.selected_namespace_command_enabled(explorer_model::NamespaceCommand::Restore) {
            return None;
        }
        self.begin_context_verb_request(owner_window, "undelete")
    }

    pub(crate) fn active_is_recycle_bin(&self) -> bool {
        matches!(
            self.tabs.active_tab().history.current().map(|entry| &entry.location),
            Some(LocationDescriptor::ParsingName(value))
                if value.eq_ignore_ascii_case("shell:RecycleBinFolder")
        )
    }

    /// Uses the Windows-owned Empty Recycle Bin verb. Shell displays the destructive
    /// confirmation; the app does not bypass it or synthesize a successful outcome.
    pub(crate) fn begin_empty_recycle_bin_request(
        &mut self,
        owner_window: u64,
    ) -> Option<ExplorerCommand> {
        if !self.active_is_recycle_bin() {
            return None;
        }
        let tab = self.tabs.active_tab();
        let parent = tab.history.current()?.location.clone();
        let context = RequestContext::new(tab.id, tab.generation);
        self.pending_context_menu = Some(context.clone());
        Some(ExplorerCommand::ShowContextMenu {
            context,
            request: explorer_model::ContextMenuRequest {
                target: explorer_model::ShellContextMenuTarget::Background { parent },
                owner_window,
                point: explorer_model::MenuPoint { x: 0, y: 0 },
                keyboard_invoked: true,
                invocation_profile: explorer_model::ContextMenuInvocationProfile::Explorer,
                color_scheme: explorer_model::ContextMenuColorScheme::Light,
                immersive_native_context_menus: false,
                paste_available: false,
                requested_verb: Some("empty".to_owned()),
                deadline_ms: 10_000,
            },
        })
    }

    fn begin_context_verb_request(
        &mut self,
        owner_window: u64,
        canonical_verb: &str,
    ) -> Option<ExplorerCommand> {
        let tab = self.tabs.active_tab();
        let items = self.selected_items();
        if items.is_empty() {
            return None;
        }
        let context = RequestContext::new(tab.id, tab.generation);
        let request = explorer_model::ContextMenuRequest {
            target: explorer_model::ShellContextMenuTarget::Items {
                parent: tab.history.current()?.location.clone(),
                items,
            },
            owner_window,
            point: explorer_model::MenuPoint { x: 0, y: 0 },
            keyboard_invoked: true,
            invocation_profile: explorer_model::ContextMenuInvocationProfile::Explorer,
            color_scheme: explorer_model::ContextMenuColorScheme::Light,
            immersive_native_context_menus: false,
            paste_available: false,
            requested_verb: Some(canonical_verb.to_owned()),
            deadline_ms: 2_000,
        };
        self.pending_context_menu = Some(context.clone());
        Some(ExplorerCommand::ShowContextMenu { context, request })
    }

    fn selected_items(&self) -> Vec<ItemDescriptor> {
        let tab = self.tabs.active_tab();
        let Some(snapshot) = tab.visible_snapshot() else {
            return Vec::new();
        };
        snapshot
            .entries()
            .iter()
            .filter(|entry| tab.selection.contains(&entry.id))
            .map(|entry| ItemDescriptor {
                id: entry.id.clone(),
                location: entry.location.clone(),
            })
            .collect()
    }

    pub(crate) fn selected_items_for_extension_command(&self) -> Vec<ItemDescriptor> {
        self.selected_items()
    }

    pub(crate) fn current_folder_bookmark_target_and_id(
        &self,
    ) -> Option<(
        explorer_model::BookmarkTarget,
        Option<explorer_model::BookmarkId>,
    )> {
        let location = &self.tabs.active_tab().history.current()?.location;
        let target = bookmark_target_for_current_location(location)?;
        let id = self.bookmarks.id_for_target(&target);
        Some((target, id))
    }

    pub(crate) fn active_location_for_extension_command(&self) -> Option<LocationDescriptor> {
        self.tabs
            .active_tab()
            .history
            .current()
            .map(|entry| entry.location.clone())
    }

    pub(crate) fn selected_paths_clipboard_text(&self) -> Option<String> {
        let paths = self
            .selected_items()
            .into_iter()
            .filter_map(|item| match item.location {
                LocationDescriptor::FileSystem(path) => Some(path.to_string_lossy().into_owned()),
                LocationDescriptor::ParsingName(name) => Some(name),
                LocationDescriptor::ShellNamespace(_)
                | LocationDescriptor::KnownFolder(_)
                | LocationDescriptor::Virtual(_) => None,
            })
            .map(|path| format!("\"{}\"", path.replace('"', "\"\"")))
            .collect::<Vec<_>>();
        (!paths.is_empty()).then(|| paths.join("\r\n"))
    }

    pub(crate) fn selected_namespace_command_enabled(
        &self,
        command: explorer_model::NamespaceCommand,
    ) -> bool {
        let tab = self.tabs.active_tab();
        let directory = tab.visible_directory_state();
        let Some(snapshot) = directory.snapshot() else {
            return false;
        };
        let mut selected = snapshot
            .entries()
            .iter()
            .filter(|entry| tab.selection.contains(&entry.id));
        let Some(first) = selected.next() else {
            return false;
        };
        std::iter::once(first).chain(selected).all(|entry| {
            explorer_model::namespace_command_enabled(
                &explorer_model::NamespaceAvailability::Available,
                entry.metadata.namespace_capabilities,
                command,
            )
        })
    }

    pub(crate) fn row_namespace_command_enabled(
        &self,
        row_index: usize,
        command: explorer_model::NamespaceCommand,
    ) -> bool {
        self.tabs
            .active_tab()
            .visible_directory_state()
            .snapshot()
            .and_then(|snapshot| snapshot.entries().get(row_index))
            .is_some_and(|entry| {
                explorer_model::namespace_command_enabled(
                    &explorer_model::NamespaceAvailability::Available,
                    entry.metadata.namespace_capabilities,
                    command,
                )
            })
    }

    pub(crate) fn item_namespace_command_enabled(
        &self,
        item_id: &ShellItemId,
        command: explorer_model::NamespaceCommand,
    ) -> bool {
        self.directory_presentation().is_some_and(|presentation| {
            (0..presentation.len()).any(|row_index| {
                presentation.entry(row_index).is_some_and(|(_, entry)| {
                    &entry.id == item_id
                        && explorer_model::namespace_command_enabled(
                            &explorer_model::NamespaceAvailability::Available,
                            entry.metadata.namespace_capabilities,
                            command,
                        )
                })
            })
        })
    }

    pub(crate) fn presentation_item_id(&self, row_index: usize) -> Option<ShellItemId> {
        self.presentation_entry(row_index).map(|entry| entry.id)
    }

    pub(crate) fn begin_drag_candidate(
        &mut self,
        x: f32,
        y: f32,
        button: explorer_model::DragButton,
    ) -> bool {
        self.pending_drag_command = None;
        self.drag_session.begin_candidate(x, y, button)
    }

    pub(crate) fn update_drag_pointer(&mut self, x: f32, y: f32) -> bool {
        if !self.drag_session.update_pointer(x, y) {
            return false;
        }
        let items = self.selected_items();
        if items.is_empty() {
            let _ = self
                .drag_session
                .finish(explorer_model::DragSessionState::Cancelled);
            return false;
        }
        let tab = self.tabs.active_tab();
        let button = match self.drag_session.state() {
            explorer_model::DragSessionState::Dragging { button, .. } => *button,
            _ => return false,
        };
        self.pending_drag_command = Some(ExplorerCommand::DataTransfer {
            context: RequestContext::new(tab.id, tab.generation),
            request: explorer_model::DataTransferRequest::BeginDrag {
                items,
                allowed_effects: explorer_model::TransferEffects {
                    copy: true,
                    move_item: true,
                    link: false,
                },
                button,
            },
        });
        true
    }

    pub(crate) fn cancel_drag(&mut self) -> bool {
        self.pending_drag_command = None;
        self.pending_context_hit = None;
        self.pending_context_extended_verbs = false;
        self.drag_session
            .finish(explorer_model::DragSessionState::Cancelled)
    }

    pub(crate) fn take_pending_drag_command(&mut self) -> Option<ExplorerCommand> {
        self.pending_drag_command.take()
    }

    pub(crate) fn queue_external_drop(
        &mut self,
        paths: Vec<std::path::PathBuf>,
        destination_row: Option<usize>,
        effect: explorer_model::DragEffect,
        right_button: bool,
        allowed: explorer_model::TransferEffects,
    ) {
        self.drop_target_row = None;
        self.drag_session.reset();
        let item_destination = destination_row
            .and_then(|row| self.presentation_entry(row))
            .filter(|entry| entry.is_container)
            .map(|entry| entry.location);
        let tab = self.tabs.active_tab();
        let destination =
            item_destination.or_else(|| tab.history.current().map(|entry| entry.location.clone()));
        let Some(destination) = destination else {
            tracing::warn!(
                target = ?explorer_model::DropTargetKind::FileView,
                ?effect,
                "external file drop rejected because no destination was resolved"
            );
            return;
        };
        if paths.is_empty() {
            tracing::warn!(
                destination = %destination,
                ?effect,
                "external file drop rejected because it contained no paths"
            );
            return;
        }
        if let Some(destination_path) = destination.path()
            && !explorer_model::filesystem_drop_destination_is_valid(
                &paths,
                destination_path,
                effect,
            )
        {
            tracing::warn!(
                destination = %destination,
                source_count = paths.len(),
                ?effect,
                "external file drop rejected by filesystem destination validation"
            );
            return;
        }
        if right_button {
            self.pending_right_drop = Some(PendingRightDrop {
                paths,
                destination,
                allowed,
                tab_id: tab.id,
                generation: tab.generation,
            });
            return;
        }
        let sources = paths
            .into_iter()
            .map(LocationDescriptor::file_system)
            .collect::<Vec<_>>();
        self.pending_drag_command = Some(ExplorerCommand::DataTransfer {
            context: RequestContext::new(tab.id, tab.generation),
            request: explorer_model::DataTransferRequest::DropExternal {
                sources,
                destination,
                effect,
                conflict: explorer_model::ConflictDecision::Prompt,
            },
        });
    }

    pub(crate) fn resolve_right_drop(&mut self, effect: explorer_model::DragEffect) {
        let Some(pending) = self.pending_right_drop.take() else {
            return;
        };
        let tab = self.tabs.active_tab();
        if tab.id != pending.tab_id || tab.generation != pending.generation {
            return;
        }
        let allowed = match effect {
            explorer_model::DragEffect::Copy => pending.allowed.copy,
            explorer_model::DragEffect::Move => pending.allowed.move_item,
            explorer_model::DragEffect::None | explorer_model::DragEffect::Link => false,
        };
        if !allowed {
            return;
        }
        self.pending_drag_command = Some(ExplorerCommand::DataTransfer {
            context: RequestContext::new(tab.id, tab.generation),
            request: explorer_model::DataTransferRequest::DropExternal {
                sources: pending
                    .paths
                    .into_iter()
                    .map(LocationDescriptor::file_system)
                    .collect(),
                destination: pending.destination,
                effect,
                conflict: explorer_model::ConflictDecision::Prompt,
            },
        });
    }

    pub(crate) fn update_external_drag_target(
        &mut self,
        destination_row: Option<usize>,
        target: explorer_model::DropTargetKind,
        pointer_y: f32,
        top: f32,
        bottom: f32,
        effect: explorer_model::DragEffect,
    ) {
        const EDGE_ZONE: f32 = 32.0;
        let auto_scroll = if pointer_y <= top + EDGE_ZONE {
            Some(explorer_model::AutoScrollDirection::Up)
        } else if pointer_y >= bottom - EDGE_ZONE {
            Some(explorer_model::AutoScrollDirection::Down)
        } else {
            None
        };
        self.drop_target_row = destination_row;
        self.drag_session
            .begin_external(target, effect, auto_scroll);
    }

    pub(crate) fn clear_external_drag(&mut self) {
        self.drop_target_row = None;
        self.drag_session.reset();
        self.pending_right_drop = None;
    }

    pub(crate) fn recycle_selected_request(&self) -> Option<FileOperationRequest> {
        let items = self.selected_items();
        (!items.is_empty()).then_some(FileOperationRequest {
            kind: FileOperationKind::RecycleDelete { items },
            flags: explorer_model::FileOperationFlags::default(),
        })
    }

    pub(crate) fn selected_items_include_remote(&self) -> bool {
        self.selected_items()
            .iter()
            .any(|item| matches!(item.location, LocationDescriptor::Virtual(_)))
    }

    pub(crate) fn create_shortcut_selected_request(&self) -> Option<FileOperationRequest> {
        let items = self.selected_items();
        (!items.is_empty()).then_some(FileOperationRequest {
            kind: FileOperationKind::CreateShortcut { items },
            flags: explorer_model::FileOperationFlags::default(),
        })
    }

    pub(crate) fn begin_clipboard_request(
        &self,
        mode: explorer_model::ClipboardMode,
    ) -> Option<ExplorerCommand> {
        let items = self.selected_items();
        if items.is_empty() {
            return None;
        }
        let tab = self.tabs.active_tab();
        let request = match mode {
            explorer_model::ClipboardMode::Copy => {
                explorer_model::DataTransferRequest::Copy { items }
            }
            explorer_model::ClipboardMode::Cut => {
                explorer_model::DataTransferRequest::Cut { items }
            }
        };
        Some(ExplorerCommand::DataTransfer {
            context: RequestContext::new(tab.id, tab.generation),
            request,
        })
    }

    pub(crate) fn begin_paste_request(
        &mut self,
        conflict: explorer_model::ConflictDecision,
    ) -> Option<ExplorerCommand> {
        if !self.active_presentation().can_write {
            return None;
        }
        let destination = self.tabs.active_tab().history.current()?.location.clone();
        let total_items = match &self.clipboard {
            explorer_model::ClipboardState::Owned { items, .. } => items.len(),
            explorer_model::ClipboardState::External { item_count, .. } => item_count.unwrap_or(0),
            explorer_model::ClipboardState::None { .. }
            | explorer_model::ClipboardState::Unsupported { .. } => return None,
        };
        let tab = self.tabs.active_tab();
        let context = RequestContext::new(tab.id, tab.generation);
        let placeholder = FileOperationRequest {
            kind: FileOperationKind::Copy {
                items: Vec::new(),
                destination: destination.clone(),
            },
            flags: explorer_model::FileOperationFlags::default(),
        };
        let mut record = OperationRecord::queued(context.request_id, placeholder, total_items);
        let started = record.start().is_ok();
        debug_assert!(started, "new paste record starts exactly once");
        let _ = self.operation_center.insert(record);
        self.operation_terminal_notice = None;
        Some(ExplorerCommand::DataTransfer {
            context,
            request: explorer_model::DataTransferRequest::Paste {
                destination,
                conflict,
            },
        })
    }

    pub(crate) fn download_selected_to_downloads_request(&self) -> Option<FileOperationRequest> {
        let items = self.selected_items();
        if items.is_empty()
            || items.iter().any(|item| {
                item.location.file_system_kind().is_none_or(|kind| {
                    !matches!(
                        kind,
                        explorer_model::FileSystemKind::Adb | explorer_model::FileSystemKind::Sftp
                    )
                })
            })
        {
            return None;
        }
        let profile = std::env::var_os("USERPROFILE")?;
        let destination = std::path::PathBuf::from(profile).join("Downloads");
        Some(FileOperationRequest {
            kind: FileOperationKind::Copy {
                items,
                destination: LocationDescriptor::file_system(destination),
            },
            flags: explorer_model::FileOperationFlags::default(),
        })
    }

    pub(crate) fn begin_permanent_delete_confirmation(&mut self) -> bool {
        if self.permanent_delete_confirmation.is_some() {
            return false;
        }
        let items = self.selected_items();
        if items.is_empty() {
            return false;
        }
        let tab = self.tabs.active_tab();
        self.permanent_delete_confirmation = Some(PermanentDeleteConfirmation {
            items,
            tab_id: tab.id,
            generation: tab.generation,
            nonce: explorer_common::RequestId::new(),
        });
        self.permanent_delete_confirmation_focus = PermanentDeleteDialogTarget::Delete;
        true
    }

    pub(crate) fn permanent_delete_selected_request(&self) -> Option<FileOperationRequest> {
        let items = self.selected_items();
        (!items.is_empty()).then_some(FileOperationRequest {
            kind: FileOperationKind::PermanentDelete {
                items,
                confirmed: true,
            },
            flags: explorer_model::FileOperationFlags {
                require_confirmation: true,
                allow_undo: false,
                ..explorer_model::FileOperationFlags::default()
            },
        })
    }

    pub(crate) fn confirm_permanent_delete(&mut self) -> Option<FileOperationRequest> {
        let confirmation = self.permanent_delete_confirmation.take()?;
        let tab = self.tabs.active_tab();
        if tab.id != confirmation.tab_id || tab.generation != confirmation.generation {
            return None;
        }
        let _consumed_nonce = confirmation.nonce;
        Some(FileOperationRequest {
            kind: FileOperationKind::PermanentDelete {
                items: confirmation.items,
                confirmed: true,
            },
            flags: explorer_model::FileOperationFlags {
                require_confirmation: true,
                allow_undo: false,
                ..explorer_model::FileOperationFlags::default()
            },
        })
    }

    pub(crate) fn cancel_permanent_delete_confirmation(&mut self) -> bool {
        self.permanent_delete_confirmation.take().is_some()
    }

    pub(crate) fn move_permanent_delete_confirmation_focus(&mut self, direction: i8) -> bool {
        if self.permanent_delete_confirmation.is_none() || direction == 0 {
            return false;
        }
        self.permanent_delete_confirmation_focus = match self.permanent_delete_confirmation_focus {
            PermanentDeleteDialogTarget::Cancel => PermanentDeleteDialogTarget::Delete,
            PermanentDeleteDialogTarget::Delete => PermanentDeleteDialogTarget::Cancel,
        };
        true
    }

    pub(crate) fn set_permanent_delete_confirmation_focus(
        &mut self,
        target: PermanentDeleteDialogTarget,
    ) -> bool {
        if self.permanent_delete_confirmation.is_none()
            || self.permanent_delete_confirmation_focus == target
        {
            return false;
        }
        self.permanent_delete_confirmation_focus = target;
        true
    }

    pub(crate) fn update_inline_rename(&mut self, value: String) -> bool {
        let Some(editor) = &mut self.rename_editor else {
            return false;
        };
        editor.update(value);
        true
    }

    pub(crate) fn cancel_inline_rename(&mut self) -> bool {
        let cancelled = self.rename_editor.take().is_some();
        if cancelled {
            self.pending_new_folder_rename = None;
        }
        cancelled
    }

    pub(crate) fn commit_inline_rename(
        &mut self,
        trigger: explorer_model::RenameCommitTrigger,
    ) -> Result<Option<FileOperationRequest>, explorer_common::ExplorerError> {
        let Some(editor) = self.rename_editor.as_ref() else {
            return Ok(None);
        };
        let pending_create = self
            .pending_new_folder_rename
            .as_ref()
            .filter(|pending| pending.item_id == editor.item.id)
            .cloned();
        let edited_id = editor.item.id.clone();
        let edited_name = editor.buffer.clone();
        let collision = self
            .tabs
            .active_tab()
            .visible_snapshot()
            .is_some_and(|snapshot| {
                snapshot.entries().iter().any(|entry| {
                    entry.id != edited_id && entry.display_name.eq_ignore_ascii_case(&edited_name)
                })
            });
        let Some(editor) = self.rename_editor.as_mut() else {
            return Ok(None);
        };
        if let Some(pending) = pending_create {
            if let Err(reason) = explorer_model::validate_windows_file_name(&edited_name) {
                let error = explorer_common::ExplorerError::new(
                    explorer_common::ExplorerErrorKind::Input,
                    "validate new folder name",
                    true,
                    "資料夾名稱無效，請修正後再試一次。",
                    format!("Windows file-name validation failed: {reason:?}"),
                );
                editor.error = Some(error.clone());
                return Err(error);
            }
            if collision {
                let error = explorer_common::ExplorerError::new(
                    explorer_common::ExplorerErrorKind::Conflict,
                    "validate new folder collision",
                    true,
                    "此位置已有同名項目。",
                    "new-folder destination collision detected before submission",
                );
                editor.error = Some(error.clone());
                return Err(error);
            }
            let kind = if pending.use_create_item {
                FileOperationKind::CreateItem {
                    parent: pending.parent,
                    name: edited_name,
                    recipe: explorer_model::ShellNewItemRecipe::Folder,
                }
            } else {
                FileOperationKind::CreateFolder {
                    parent: pending.parent,
                    name: edited_name,
                }
            };
            self.rename_editor = None;
            self.pending_new_folder_rename = None;
            return Ok(Some(FileOperationRequest {
                kind,
                flags: explorer_model::FileOperationFlags {
                    conflict: explorer_model::ConflictDecision::KeepBoth,
                    ..explorer_model::FileOperationFlags::default()
                },
            }));
        }
        let result = editor.commit(trigger, collision)?;
        if result.is_some()
            || self
                .rename_editor
                .as_ref()
                .is_some_and(|editor| editor.error.is_none())
        {
            self.rename_editor = None;
        }
        Ok(result)
    }

    pub(crate) fn watcher_recovery_command(
        &mut self,
        event: &ExplorerEvent,
    ) -> Option<ExplorerCommand> {
        let ExplorerEvent::DirectoryChanged {
            tab_id,
            generation,
            changes,
        } = event
        else {
            return None;
        };
        if changes.is_empty() {
            return None;
        }
        let tab = self.tabs.tab_mut(*tab_id)?;
        if tab.generation != *generation {
            return None;
        }
        let location = tab.history.current()?.location.clone();
        let context = tab.begin_refresh_request()?;
        Some(ExplorerCommand::Refresh { context, location })
    }

    /// A completed Shell mutation refreshes the active view even when the filesystem watcher is
    /// delayed. Matching the generation prevents a duplicate refresh after watcher convergence.
    pub(crate) fn service_event_requires_active_refresh(&self, event: &ExplorerEvent) -> bool {
        let Some(context) = event.context() else {
            return false;
        };
        let tab = self.tabs.active_tab();
        if tab.id != context.tab_id || tab.generation != context.generation {
            return false;
        }
        match event {
            ExplorerEvent::OperationFinished { context, outcome } => {
                matches!(
                    outcome,
                    OperationTerminal::Finished | OperationTerminal::Partial { .. }
                ) && self.operation_center.get(context.request_id).is_some()
            }
            ExplorerEvent::ContextMenuFinished {
                outcome: explorer_model::ContextMenuOutcome::Invoked { .. },
                ..
            } => true,
            _ => false,
        }
    }

    pub(crate) fn open_row_command(
        &mut self,
        row_index: usize,
        new_tab: bool,
    ) -> Option<ExplorerCommand> {
        let entry = self.presentation_entry(row_index)?;
        if matches!(
            entry.metadata.type_display.as_deref(),
            Some("Broken remote link" | "Circular remote link")
        ) {
            return None;
        }
        if entry.is_container {
            if new_tab {
                self.new_tab();
            }
            self.begin_active_navigation(entry.location, false)
        } else {
            let tab = self.tabs.active_tab();
            Some(ExplorerCommand::OpenItem {
                context: RequestContext::new(tab.id, tab.generation),
                item: ItemDescriptor {
                    id: entry.id,
                    location: entry.location,
                },
                disposition: OpenDisposition::DefaultApplication,
            })
        }
    }

    pub(crate) fn open_extension_view_item_command(
        &mut self,
        item_id: ShellItemId,
        location: LocationDescriptor,
        is_container: bool,
        new_tab: bool,
    ) -> Option<ExplorerCommand> {
        if is_container {
            if new_tab {
                self.new_tab();
            }
            self.begin_active_navigation(location, false)
        } else {
            let tab = self.tabs.active_tab();
            Some(ExplorerCommand::OpenItem {
                context: RequestContext::new(tab.id, tab.generation),
                item: ItemDescriptor {
                    id: item_id,
                    location,
                },
                disposition: OpenDisposition::DefaultApplication,
            })
        }
    }

    pub fn active_presentation(&self) -> TabPresentationSnapshot {
        self.tabs.active_presentation()
    }

    pub fn command_availability(&self) -> CommandAvailability {
        CommandAvailability::from_tabs(&self.tabs)
    }

    pub const fn close_requested(&self) -> bool {
        self.close_requested
    }

    pub const fn divider_interaction(&self) -> DividerInteraction {
        self.divider
    }

    pub(crate) fn set_theme(&mut self, theme: ThemeMode) {
        self.current_theme = theme;
    }

    pub(crate) fn set_navigation_pane_width(&mut self, width: LogicalPx) {
        self.navigation_pane_width = width;
    }

    pub(crate) fn focus(&mut self, surface: FocusSurface) {
        if surface != FocusSurface::FileView {
            self.clear_file_view_typeahead();
        }
        self.focus.focus(surface);
        self.tab_focus.insert(self.tabs.active_tab_id(), surface);
    }

    pub(crate) fn restore_previous_focus(&mut self) -> bool {
        let restored = self.focus.restore_previous();
        if restored && self.focus.current() != FocusSurface::FileView {
            self.clear_file_view_typeahead();
        }
        restored
    }

    pub(crate) fn traverse_focus(&mut self, direction: FocusDirection) -> bool {
        let preview_pane = self.view_settings().preview_pane;
        let moved = self.focus.traverse(direction, |surface| {
            surface != FocusSurface::PreviewPane || preview_pane
        });
        if moved {
            if self.focus.current() != FocusSurface::FileView {
                self.clear_file_view_typeahead();
            }
            self.tab_focus
                .insert(self.tabs.active_tab_id(), self.focus.current());
        }
        moved
    }

    pub(crate) fn request_close(&mut self) {
        self.cancel_permanent_delete_confirmation();
        self.cancel_lock_recovery();
        let _ = self.end_scrollbar_drag(ScrollbarTerminal::WindowClose);
        self.end_details_column_resize();
        self.end_side_pane_resize();
        self.end_marquee();
        self.clear_external_drag();
        self.close_requested = true;
    }

    pub(crate) fn new_tab(&mut self) -> TabId {
        self.clear_file_view_typeahead();
        self.cancel_permanent_delete_confirmation();
        self.cancel_lock_recovery();
        let _ = self.end_scrollbar_drag(ScrollbarTerminal::TabSwitch);
        self.end_details_column_resize();
        self.end_side_pane_resize();
        self.end_marquee();
        self.close_navigation_history_menu();
        self.tab_focus
            .insert(self.tabs.active_tab_id(), self.focus.current());
        let id = self.tabs.new_tab();
        self.navigation_trees.entry(id).or_default();
        self.details_filters.entry(id).or_default();
        self.details_column_menu = None;
        self.details_filter_menu = None;
        self.pending_new_tab_command = self.begin_active_location_load();
        self.tab_focus.insert(id, FocusSurface::TabStrip);
        self.focus.restore_context(FocusSurface::TabStrip);
        id
    }

    /// Takes the first navigation command created atomically with `new_tab`.
    pub(crate) fn take_pending_new_tab_command(&mut self) -> Option<ExplorerCommand> {
        self.pending_new_tab_command.take()
    }

    pub(crate) fn activate_tab(&mut self, id: TabId) -> bool {
        self.clear_file_view_typeahead();
        self.cancel_permanent_delete_confirmation();
        self.cancel_lock_recovery();
        let _ = self.end_scrollbar_drag(ScrollbarTerminal::TabSwitch);
        self.end_details_column_resize();
        self.end_side_pane_resize();
        self.end_marquee();
        self.clear_external_drag();
        self.close_navigation_history_menu();
        self.close_address_menu();
        self.details_column_menu = None;
        self.details_filter_menu = None;
        self.tab_focus
            .insert(self.tabs.active_tab_id(), self.focus.current());
        if !self.tabs.activate(id) {
            return false;
        }
        let target = self
            .tab_focus
            .get(&id)
            .copied()
            .unwrap_or(FocusSurface::FileView);
        self.focus.restore_context(target);
        true
    }

    pub(crate) fn close_tab(&mut self, id: TabId) -> TabCloseOutcome {
        self.cancel_permanent_delete_confirmation();
        self.cancel_lock_recovery();
        if id == self.tabs.active_tab_id() {
            let _ = self.end_scrollbar_drag(ScrollbarTerminal::TabSwitch);
            self.end_details_column_resize();
            self.end_side_pane_resize();
            self.end_marquee();
            self.clear_external_drag();
            self.close_navigation_history_menu();
        }
        let was_active = id == self.tabs.active_tab_id();
        self.cancel_breadcrumb_requests(id);
        let outcome = self.tabs.close(id);
        if outcome == TabCloseOutcome::Closed {
            self.tab_focus.remove(&id);
            self.navigation_focus.remove(&id);
            self.details_filters.remove(&id);
            self.details_column_menu = None;
            self.details_filter_menu = None;
            if let Some(tree) = self.navigation_trees.remove(&id) {
                for node in tree.nodes.into_values() {
                    if let Some(request) = node.request {
                        request.cancellation.cancel();
                    }
                }
            }
            if was_active {
                let active = self.tabs.active_tab_id();
                let target = self
                    .tab_focus
                    .get(&active)
                    .copied()
                    .unwrap_or(FocusSurface::FileView);
                self.focus.restore_context(target);
            }
        }
        if outcome == TabCloseOutcome::CloseWindow {
            self.request_close();
        }
        outcome
    }

    fn cancel_breadcrumb_requests(&mut self, tab_id: TabId) {
        if let Some(context) = self.ancestry_requests.remove(&tab_id) {
            context.cancellation.cancel();
        }
        if let Some(pending) = self.breadcrumb_menu_requests.remove(&tab_id) {
            pending.context.cancellation.cancel();
        }
        if let Some(tab) = self.tabs.tab_mut(tab_id) {
            tab.view.address.close_menu();
        }
    }

    pub(crate) fn reorder_tab(&mut self, id: TabId, destination_index: usize) -> bool {
        self.tabs.reorder(id, destination_index)
    }

    pub(crate) fn cycle_tab(&mut self, direction: i8) -> bool {
        let tabs = self.tabs.tabs();
        if tabs.len() < 2 {
            return false;
        }
        let Some(index) = tabs
            .iter()
            .position(|tab| tab.id == self.tabs.active_tab_id())
        else {
            return false;
        };
        let destination = if direction < 0 {
            index.checked_sub(1).unwrap_or(tabs.len() - 1)
        } else {
            (index + 1) % tabs.len()
        };
        let id = tabs[destination].id;
        self.activate_tab(id)
    }

    pub(crate) fn begin_divider_drag(&mut self, pointer_x: f32) -> bool {
        self.divider.begin(pointer_x, self.navigation_pane_width)
    }

    pub(crate) fn update_divider_drag(&mut self, pointer_x: f32) -> bool {
        let Some(width) = self.divider.update(pointer_x, LayoutTokens::WINDOWS_11) else {
            return false;
        };
        self.navigation_pane_width = width;
        true
    }

    pub(crate) fn finish_divider_drag(&mut self) -> bool {
        self.divider.finish()
    }

    pub(crate) fn reset_divider(&mut self) {
        self.navigation_pane_width = self.divider.reset(LayoutTokens::WINDOWS_11);
    }

    pub(crate) fn adjust_divider(&mut self, direction: i8) {
        self.navigation_pane_width = DividerInteraction::keyboard_adjust(
            self.navigation_pane_width,
            direction,
            LayoutTokens::WINDOWS_11,
        );
    }
}

fn estimated_text_width(text: &str) -> f32 {
    text.chars()
        .map(|character| {
            if character.is_ascii() {
                7.2
            } else if matches!(
                character as u32,
                0x2E80..=0x9FFF | 0xF900..=0xFAFF | 0xFF00..=0xFFEF
            ) {
                14.0
            } else {
                8.5
            }
        })
        .sum()
}

fn default_new_items() -> Vec<explorer_model::ShellNewItemDescriptor> {
    vec![
        explorer_model::ShellNewItemDescriptor {
            stable_id: "folder".to_owned(),
            display_name: "Folder".to_owned(),
            extension: None,
            default_stem: "New folder".to_owned(),
            recipe: explorer_model::ShellNewItemRecipe::Folder,
        },
        explorer_model::ShellNewItemDescriptor {
            stable_id: ".txt".to_owned(),
            display_name: "Text Document".to_owned(),
            extension: Some(".txt".to_owned()),
            default_stem: "New Text Document".to_owned(),
            recipe: explorer_model::ShellNewItemRecipe::EmptyFile,
        },
        explorer_model::ShellNewItemDescriptor {
            stable_id: ".bmp".to_owned(),
            display_name: "Bitmap image".to_owned(),
            extension: Some(".bmp".to_owned()),
            default_stem: "New Bitmap Image".to_owned(),
            recipe: explorer_model::ShellNewItemRecipe::Data(vec![
                0x42, 0x4d, 58, 0, 0, 0, 0, 0, 0, 0, 54, 0, 0, 0, 40, 0, 0, 0, 1, 0, 0, 0, 1, 0, 0,
                0, 1, 0, 24, 0, 0, 0, 0, 0, 4, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
                0, 0, 0, 0, 0,
            ]),
        },
        explorer_model::ShellNewItemDescriptor {
            stable_id: ".zip".to_owned(),
            display_name: "Compressed (zipped) Folder".to_owned(),
            extension: Some(".zip".to_owned()),
            default_stem: "New Compressed (zipped) Folder".to_owned(),
            recipe: explorer_model::ShellNewItemRecipe::Data(vec![
                0x50, 0x4b, 0x05, 0x06, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
            ]),
        },
    ]
}

fn move_bounded_menu_index(current: usize, direction: i8, last: usize) -> usize {
    match direction {
        i8::MIN..=-2 => 0,
        -1 => current.saturating_sub(1),
        1 => current.saturating_add(1).min(last),
        2..=i8::MAX => last,
        _ => current,
    }
}

fn operation_item_count(request: &FileOperationRequest) -> usize {
    match &request.kind {
        FileOperationKind::CreateFolder { .. }
        | FileOperationKind::CreateItem { .. }
        | FileOperationKind::Rename { .. }
        | FileOperationKind::SetUnixMode { .. } => 1,
        FileOperationKind::Copy { items, .. }
        | FileOperationKind::Move { items, .. }
        | FileOperationKind::RecycleDelete { items }
        | FileOperationKind::PermanentDelete { items, .. }
        | FileOperationKind::CreateShortcut { items } => items.len(),
    }
}

fn navigation_locations_for_operation(request: &FileOperationRequest) -> Vec<LocationDescriptor> {
    fn item_parent(item: &ItemDescriptor) -> Option<LocationDescriptor> {
        item.location
            .path()
            .and_then(std::path::Path::parent)
            .map(LocationDescriptor::file_system)
    }

    match &request.kind {
        FileOperationKind::CreateFolder { parent, .. }
        | FileOperationKind::CreateItem { parent, .. } => vec![parent.clone()],
        FileOperationKind::Rename { item, .. } | FileOperationKind::SetUnixMode { item, .. } => {
            item.location.virtual_parent().into_iter().collect()
        }
        FileOperationKind::Copy { destination, .. } => vec![destination.clone()],
        FileOperationKind::Move { items, destination } => items
            .iter()
            .filter_map(item_parent)
            .chain(std::iter::once(destination.clone()))
            .collect(),
        FileOperationKind::RecycleDelete { items }
        | FileOperationKind::PermanentDelete { items, .. }
        | FileOperationKind::CreateShortcut { items } => {
            items.iter().filter_map(item_parent).collect()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{
        AppViewState, CommandKind, DirectoryCacheKey, DirectorySnapshotCache,
        bookmark_target_for_current_location, resolve_details_column_insertion,
        unique_remote_folder_symlink_name,
    };

    #[test]
    fn remote_folder_symlink_name_uses_first_free_windows_style_suffix() {
        let existing = HashSet::from(["photos", "photos - 捷徑", "photos - 捷徑 (2)"]);
        assert_eq!(
            unique_remote_folder_symlink_name("photos", &existing),
            "photos - 捷徑 (3)"
        );

        let empty = HashSet::new();
        assert_eq!(
            unique_remote_folder_symlink_name("photos", &empty),
            "photos - 捷徑"
        );
    }
    use crate::{focus::FocusSurface, layout::LayoutTokens, theme::ThemeMode};
    use std::{
        collections::HashSet,
        time::{Duration, Instant},
    };

    fn cache_test_location(
        provider: &str,
        authority: &str,
        path: &[&str],
        generation: u64,
    ) -> explorer_model::LocationDescriptor {
        explorer_model::LocationDescriptor::Virtual(explorer_model::VirtualLocationDescriptor {
            provider_id: provider.to_owned(),
            public_authority: Some(authority.to_owned()),
            container_identity: [generation as u8; 16],
            container_generation: generation,
            entry_id: Some(generation),
            components: path
                .iter()
                .map(|component| (*component).to_owned())
                .collect(),
        })
    }

    fn cache_test_snapshot(prefix: &str, count: usize) -> explorer_model::DirectorySnapshot {
        let mut snapshot = explorer_model::DirectorySnapshot::default();
        for index in 0..count {
            let mut identity = prefix.as_bytes().to_vec();
            identity.extend_from_slice(&(index as u64).to_le_bytes());
            let _ = snapshot.upsert(explorer_model::FileEntry {
                id: explorer_model::ShellItemId::from_provider_bytes(identity).unwrap(),
                display_name: format!("{prefix}-{index}"),
                location: explorer_model::LocationDescriptor::file_system(format!(
                    r"C:\{prefix}\{index}"
                )),
                is_container: false,
                metadata: explorer_model::FileEntryMetadata::default(),
            });
        }
        snapshot
    }

    #[test]
    fn directory_cache_keys_normalize_local_and_ignore_virtual_transient_identity() {
        assert_eq!(
            DirectoryCacheKey::for_location(&explorer_model::LocationDescriptor::file_system(
                r"C:\Users\Fixture\"
            )),
            DirectoryCacheKey::for_location(&explorer_model::LocationDescriptor::file_system(
                r"c:/users/fixture"
            ))
        );
        let adb_old = cache_test_location("adb", "emulator-5554", &["sdcard", "Download"], 1);
        let adb_new = cache_test_location("adb", "emulator-5554", &["sdcard", "Download"], 99);
        assert_eq!(
            DirectoryCacheKey::for_location(&adb_old),
            DirectoryCacheKey::for_location(&adb_new)
        );
        assert_ne!(
            DirectoryCacheKey::for_location(&adb_old),
            DirectoryCacheKey::for_location(&cache_test_location(
                "adb",
                "HA245TSY",
                &["sdcard", "Download"],
                1
            ))
        );
        assert_ne!(
            DirectoryCacheKey::for_location(&adb_old),
            DirectoryCacheKey::for_location(&cache_test_location(
                "adb",
                "emulator-5554",
                &["sdcard", "Android"],
                1
            ))
        );
        let sftp_upper = cache_test_location("sftp", "EXAMPLE.TEST", &["home"], 1);
        let sftp_lower = cache_test_location("sftp", "example.test", &["home"], 2);
        assert_eq!(
            DirectoryCacheKey::for_location(&sftp_upper),
            DirectoryCacheKey::for_location(&sftp_lower)
        );
    }

    #[test]
    fn directory_snapshot_cache_enforces_lru_and_row_limits_without_destructive_rejection() {
        let a = explorer_model::LocationDescriptor::file_system(r"C:\a");
        let b = explorer_model::LocationDescriptor::file_system(r"C:\b");
        let c = explorer_model::LocationDescriptor::file_system(r"C:\c");
        let mut cache = DirectorySnapshotCache::with_limits(2, 3);
        assert!(cache.insert(&a, cache_test_snapshot("a", 1)));
        assert!(cache.insert(&b, cache_test_snapshot("b", 1)));
        assert!(cache.get(&a).is_some());
        assert!(cache.insert(&c, cache_test_snapshot("c", 1)));
        assert!(cache.get(&a).is_some());
        assert!(cache.get(&b).is_none());
        assert!(cache.get(&c).is_some());

        let existing_keys = cache.entries.keys().cloned().collect::<HashSet<_>>();
        assert!(!cache.insert(&b, cache_test_snapshot("oversized", 4)));
        assert_eq!(
            cache.entries.keys().cloned().collect::<HashSet<_>>(),
            existing_keys
        );
        assert!(cache.insert(&b, cache_test_snapshot("b2", 2)));
        assert!(cache.total_rows <= 3);
        assert!(cache.entries.len() <= 2);
    }

    #[test]
    fn bookmark_star_targets_local_adb_and_sftp_folders() {
        let fixture = std::env::temp_dir().join(format!(
            "superexplorer-bookmark-star-{}",
            std::process::id()
        ));
        std::fs::create_dir_all(&fixture).expect("create bookmark star fixture"); // architecture-check: allow test fixture
        let file = fixture.join("selected-child.txt");
        std::fs::write(&file, b"fixture").expect("create selected child fixture"); // architecture-check: allow test fixture

        let folder_location = explorer_model::LocationDescriptor::file_system(&fixture);
        assert_eq!(
            bookmark_target_for_current_location(&folder_location),
            Some(explorer_model::BookmarkTarget::Folder {
                location: folder_location,
            })
        );
        assert!(
            bookmark_target_for_current_location(&explorer_model::LocationDescriptor::file_system(
                &file
            ))
            .is_none()
        );
        assert!(
            bookmark_target_for_current_location(&explorer_model::LocationDescriptor::ParsingName(
                "shell:Downloads".to_owned()
            ))
            .is_none()
        );

        for address in [
            "adb://emulator-5554/sdcard/Android",
            "sftp://production/root/uploads",
        ] {
            let location = explorer_model::RemoteAddress::parse(address)
                .unwrap()
                .to_deterministic_location(1)
                .unwrap();
            assert_eq!(
                bookmark_target_for_current_location(&location),
                Some(explorer_model::BookmarkTarget::Folder { location })
            );
        }

        std::fs::remove_file(file).expect("remove selected child fixture"); // architecture-check: allow test fixture
        std::fs::remove_dir(fixture).expect("remove bookmark star fixture"); // architecture-check: allow test fixture
    }

    #[test]
    fn bookmark_manager_edits_reorders_deletes_and_restores_typed_entries() {
        let mut state = AppViewState::default();
        state.bookmarks.begin_add(
            "Folder".into(),
            explorer_model::BookmarkTarget::Folder {
                location: explorer_model::LocationDescriptor::file_system(r"C:\folder"),
            },
        );
        state.bookmarks.begin_add(
            "File".into(),
            explorer_model::BookmarkTarget::File {
                location: explorer_model::LocationDescriptor::file_system(r"C:\folder\file.txt"),
            },
        );
        state.bookmarks.begin_add(
            "Command".into(),
            explorer_model::BookmarkTarget::LuaScript {
                source: "assert(current_folder)".into(),
            },
        );

        let file_id = state.bookmarks().entries()[1].id;
        state.begin_bookmark_editor(Some(file_id));
        state.update_bookmark_editor_name("Renamed file".into());
        state.update_bookmark_editor_payload(r"C:\other\renamed.txt".into());
        assert!(
            state
                .commit_bookmark_editor()
                .expect("typed edit mutation")
                .changed()
        );
        assert!(matches!(
            &state.bookmarks().entries()[1].target,
            explorer_model::BookmarkTarget::FilePath { path }
                if path == r"C:\other\renamed.txt"
        ));

        let command_id = state.bookmarks().entries()[2].id;
        state.begin_bookmark_editor(Some(command_id));
        state.update_bookmark_editor_payload("return current_folder".into());
        assert!(
            state
                .commit_bookmark_editor()
                .expect("Lua source mutation")
                .changed()
        );
        assert!(matches!(
            &state.bookmarks().entries()[2].target,
            explorer_model::BookmarkTarget::LuaScript { source }
                if source == "return current_folder"
        ));

        assert!(state.reorder_bookmark(file_id, 0).changed());
        assert!(state.remove_bookmark(command_id).changed());
        assert_eq!(state.bookmarks().entries()[0].id, file_id);

        let persisted = serde_json::to_vec(state.bookmarks()).expect("persist bookmarks");
        let restored = serde_json::from_slice(&persisted).expect("restore bookmarks");
        let mut restarted = AppViewState::default();
        restarted.configure_bookmarks(restored);
        assert_eq!(restarted.bookmarks(), state.bookmarks());
    }

    #[test]
    fn bookmark_toolbar_context_tracks_root_and_folder_without_filesystem_mutation() {
        let mut state = AppViewState::default();
        assert!(state.add_bookmark_folder("Work".into(), None).changed());
        let folder = state.bookmarks().folders()[0].id;
        state.open_bookmark_toolbar_context_menu(Some(folder), 90.0, 40.0);
        assert_eq!(
            state.bookmark_toolbar_context_menu(),
            Some(super::BookmarkToolbarContextMenuState {
                parent_id: Some(folder),
                x: 90.0,
                y: 40.0,
            })
        );
        state.close_bookmark_toolbar_context_menu();
        assert_eq!(state.bookmark_toolbar_context_menu(), None);
        assert_eq!(state.bookmarks().folders()[0].name, "Work");
    }

    #[test]
    fn arbitrary_path_editor_preserves_exact_text_and_rejects_empty_target() {
        let mut state = AppViewState::default();
        state.begin_new_bookmark_editor_in(
            "Future".into(),
            explorer_model::BookmarkTarget::FolderPath {
                path: String::new(),
            },
            None,
        );
        assert!(state.commit_bookmark_editor().is_none());
        assert!(state.bookmark_editor().is_some());
        let exact = r#"?:\\offline\future<>"#.to_owned();
        state.update_bookmark_editor_payload(exact.clone());
        assert!(
            state
                .commit_bookmark_editor()
                .expect("raw path add")
                .changed()
        );
        assert_eq!(
            state.bookmarks().entries()[0].target.editable_payload(),
            exact
        );
    }

    #[test]
    fn bookmark_editor_selects_persistent_folder_destination_and_folder_crud_rolls_back() {
        let mut state = AppViewState::default();
        let add_folder = state.add_bookmark_folder("Work".into(), None);
        assert!(add_folder.changed());
        let folder_id = state.bookmarks().folders()[0].id;

        state.begin_new_bookmark_editor(
            "Command".into(),
            explorer_model::BookmarkTarget::LuaScript {
                source: "return 1".into(),
            },
        );
        state.select_bookmark_editor_parent(Some(folder_id));
        let add = state
            .commit_bookmark_editor()
            .expect("folder destination mutation");
        assert!(add.changed());
        assert_eq!(state.bookmarks().entries()[0].parent_id, Some(folder_id));

        state.begin_bookmark_folder_editor(folder_id);
        state.update_bookmark_folder_editor_name("Projects".into());
        assert!(
            state
                .commit_bookmark_folder_editor()
                .expect("rename mutation")
                .changed()
        );
        assert_eq!(state.bookmarks().folders()[0].name, "Projects");

        state.request_bookmark_folder_delete(folder_id);
        assert_eq!(
            state.bookmark_folder_delete_confirmation(),
            Some((folder_id, 1))
        );
        let remove = state.remove_bookmark_folder(folder_id, true);
        assert!(remove.changed());
        assert!(state.bookmarks().entries().is_empty());
        state.rollback_bookmark(remove);
        assert_eq!(state.bookmarks().entries()[0].parent_id, Some(folder_id));
    }

    fn state_with_rows() -> AppViewState {
        let mut state = AppViewState::with_initial_location(explorer_model::HistoryEntry::new(
            explorer_model::LocationDescriptor::file_system(r"C:\fixture"),
            "fixture",
        ));
        let command = state.begin_active_location_load().expect("load command");
        let context = command.context().expect("context").clone();
        assert_eq!(
            state.apply_service_event(explorer_model::ExplorerEvent::LocationResolved {
                context: context.clone(),
                metadata: explorer_model::LocationMetadata {
                    descriptor: explorer_model::LocationDescriptor::file_system(r"C:\fixture"),
                    display_title: "fixture".to_owned(),
                    can_go_up: true,
                    can_write: true,
                },
            }),
            explorer_model::WindowEventOutcome::Applied
        );
        let entries = vec![
            explorer_model::FileEntry {
                id: explorer_model::ShellItemId::from_provider_bytes([1]).expect("folder id"),
                display_name: "folder".to_owned(),
                location: explorer_model::LocationDescriptor::file_system(r"C:\fixture\folder"),
                is_container: true,
                metadata: explorer_model::FileEntryMetadata {
                    namespace_capabilities: explorer_model::NamespaceCapabilities::from_public_bits(
                        explorer_model::NamespaceCapabilities::OPEN
                            | explorer_model::NamespaceCapabilities::ENUMERATE
                            | explorer_model::NamespaceCapabilities::COPY
                            | explorer_model::NamespaceCapabilities::DELETE
                            | explorer_model::NamespaceCapabilities::RENAME
                            | explorer_model::NamespaceCapabilities::PROPERTIES
                            | explorer_model::NamespaceCapabilities::CONTEXT_MENU
                            | explorer_model::NamespaceCapabilities::PIN,
                    ),
                    ..Default::default()
                },
            },
            explorer_model::FileEntry {
                id: explorer_model::ShellItemId::from_provider_bytes([2]).expect("file id"),
                display_name: "file.txt".to_owned(),
                location: explorer_model::LocationDescriptor::file_system(r"C:\fixture\file.txt"),
                is_container: false,
                metadata: explorer_model::FileEntryMetadata {
                    namespace_capabilities: explorer_model::NamespaceCapabilities::from_public_bits(
                        explorer_model::NamespaceCapabilities::OPEN
                            | explorer_model::NamespaceCapabilities::COPY
                            | explorer_model::NamespaceCapabilities::DELETE
                            | explorer_model::NamespaceCapabilities::RENAME
                            | explorer_model::NamespaceCapabilities::PROPERTIES
                            | explorer_model::NamespaceCapabilities::CONTEXT_MENU
                            | explorer_model::NamespaceCapabilities::PIN
                            | explorer_model::NamespaceCapabilities::THUMBNAIL
                            | explorer_model::NamespaceCapabilities::PREVIEW,
                    ),
                    ..Default::default()
                },
            },
        ];
        assert_eq!(
            state.apply_service_event(explorer_model::ExplorerEvent::DirectoryBatch {
                context: context.clone(),
                entries,
            }),
            explorer_model::WindowEventOutcome::Applied
        );
        let _ =
            state.apply_service_event(explorer_model::ExplorerEvent::DirectoryFinished { context });
        state
    }

    fn complete_cached_directory(
        state: &mut AppViewState,
        command: &explorer_model::ExplorerCommand,
        location: explorer_model::LocationDescriptor,
        entries: Vec<explorer_model::FileEntry>,
    ) {
        let context = command.context().expect("navigation context").clone();
        assert_eq!(
            state.apply_service_event(explorer_model::ExplorerEvent::LocationResolved {
                context: context.clone(),
                metadata: explorer_model::LocationMetadata {
                    descriptor: location,
                    display_title: "cache fixture".to_owned(),
                    can_go_up: true,
                    can_write: true,
                },
            }),
            explorer_model::WindowEventOutcome::Applied
        );
        assert_eq!(
            state.apply_service_event(explorer_model::ExplorerEvent::DirectoryBatch {
                context: context.clone(),
                entries,
            }),
            explorer_model::WindowEventOutcome::Applied
        );
        assert_eq!(
            state.apply_service_event(explorer_model::ExplorerEvent::DirectoryFinished { context }),
            explorer_model::WindowEventOutcome::Applied
        );
    }

    fn cached_virtual_entry(
        identity: u8,
        name: &str,
        location: explorer_model::LocationDescriptor,
    ) -> explorer_model::FileEntry {
        explorer_model::FileEntry {
            id: explorer_model::ShellItemId::from_provider_bytes([identity]).unwrap(),
            display_name: name.to_owned(),
            location,
            is_container: true,
            metadata: explorer_model::FileEntryMetadata::default(),
        }
    }

    #[test]
    fn remote_folder_symlink_request_targets_clicked_folder_and_avoids_collisions() {
        let mut state = AppViewState::default();
        let parent = cache_test_location("adb", "emulator-5554", &["data", "local", "tmp"], 7);
        let command = state
            .begin_active_navigation(parent.clone(), false)
            .expect("remote navigation");
        complete_cached_directory(
            &mut state,
            &command,
            parent.clone(),
            vec![
                cached_virtual_entry(
                    1,
                    "photos",
                    cache_test_location(
                        "adb",
                        "emulator-5554",
                        &["data", "local", "tmp", "photos"],
                        7,
                    ),
                ),
                cached_virtual_entry(
                    2,
                    "photos - 捷徑",
                    cache_test_location(
                        "adb",
                        "emulator-5554",
                        &["data", "local", "tmp", "photos - 捷徑"],
                        7,
                    ),
                ),
            ],
        );

        let row = state
            .directory_presentation()
            .expect("presentation")
            .ordered_indices()
            .iter()
            .position(|index| {
                state
                    .tabs()
                    .active_tab()
                    .visible_snapshot()
                    .and_then(|snapshot| snapshot.entries().get(*index))
                    .is_some_and(|entry| entry.display_name == "photos")
            })
            .expect("photos row");
        assert_eq!(
            state.remote_folder_symlink_request(row),
            Some((parent, "photos - 捷徑 (2)".to_owned(), "photos".to_owned(),))
        );
    }

    #[test]
    fn specified_sftp_parent_and_test_revisit_seed_cache_before_background_convergence() {
        let parent = explorer_model::RemoteAddress::parse("sftp://45.32.49.125/home/linuxuser")
            .unwrap()
            .to_deterministic_location(1)
            .unwrap();
        let child = explorer_model::RemoteAddress::parse("sftp://45.32.49.125/home/linuxuser/test")
            .unwrap()
            .to_deterministic_location(2)
            .unwrap();
        let mut state = AppViewState::with_initial_location(explorer_model::HistoryEntry::new(
            parent.clone(),
            "linuxuser",
        ));

        let parent_load = state.begin_active_location_load().unwrap();
        complete_cached_directory(
            &mut state,
            &parent_load,
            parent.clone(),
            vec![cached_virtual_entry(1, "test", child.clone())],
        );
        let child_load = state.begin_active_navigation(child.clone(), false).unwrap();
        assert!(
            state
                .tabs()
                .active_tab()
                .directory
                .snapshot()
                .unwrap()
                .entries()
                .is_empty()
        );
        complete_cached_directory(
            &mut state,
            &child_load,
            child.clone(),
            vec![cached_virtual_entry(
                2,
                "inside.txt",
                explorer_model::LocationDescriptor::file_system(r"C:\virtual\inside.txt"),
            )],
        );

        let up = state.begin_up_navigation().expect("Backspace up command");
        assert_eq!(
            match &up {
                explorer_model::ExplorerCommand::Navigate { location, .. } =>
                    location.editable_text(),
                _ => panic!("up must navigate"),
            },
            "sftp://45.32.49.125/home/linuxuser"
        );
        assert_eq!(
            state
                .tabs()
                .active_tab()
                .directory
                .snapshot()
                .unwrap()
                .entries()[0]
                .display_name,
            "test"
        );

        let up_context = up.context().unwrap().clone();
        state.apply_service_event(explorer_model::ExplorerEvent::LocationResolved {
            context: up_context.clone(),
            metadata: explorer_model::LocationMetadata {
                descriptor: parent.clone(),
                display_title: "linuxuser".to_owned(),
                can_go_up: true,
                can_write: true,
            },
        });
        state.apply_service_event(explorer_model::ExplorerEvent::DirectoryBatch {
            context: up_context.clone(),
            entries: vec![cached_virtual_entry(
                3,
                "fresh",
                cache_test_location("sftp", "45.32.49.125", &["home", "linuxuser", "fresh"], 3),
            )],
        });
        state.apply_service_event(explorer_model::ExplorerEvent::DirectoryFinished {
            context: up_context,
        });
        assert_eq!(
            state
                .tabs()
                .active_tab()
                .directory
                .snapshot()
                .unwrap()
                .entries()[0]
                .display_name,
            "fresh"
        );

        let child_again = state.begin_active_navigation(child, false).unwrap();
        assert!(matches!(
            child_again,
            explorer_model::ExplorerCommand::Navigate { .. }
        ));
        assert_eq!(
            state
                .tabs()
                .active_tab()
                .directory
                .snapshot()
                .unwrap()
                .entries()[0]
                .display_name,
            "inside.txt"
        );
    }

    #[test]
    fn back_and_forward_navigation_seed_their_cached_target_snapshots() {
        let a = explorer_model::LocationDescriptor::file_system(r"C:\cache-a");
        let b = explorer_model::LocationDescriptor::file_system(r"C:\cache-b");
        let mut state =
            AppViewState::with_initial_location(explorer_model::HistoryEntry::new(a.clone(), "A"));
        let load_a = state.begin_active_location_load().unwrap();
        complete_cached_directory(
            &mut state,
            &load_a,
            a.clone(),
            vec![cached_virtual_entry(11, "a-row", a.clone())],
        );
        let load_b = state.begin_active_navigation(b.clone(), false).unwrap();
        complete_cached_directory(
            &mut state,
            &load_b,
            b.clone(),
            vec![cached_virtual_entry(12, "b-row", b.clone())],
        );

        let back = state.begin_back_navigation().unwrap();
        assert_eq!(
            state
                .tabs()
                .active_tab()
                .directory
                .snapshot()
                .unwrap()
                .entries()[0]
                .display_name,
            "a-row"
        );
        complete_cached_directory(
            &mut state,
            &back,
            a,
            vec![cached_virtual_entry(
                13,
                "a-fresh",
                explorer_model::LocationDescriptor::file_system(r"C:\cache-a\fresh"),
            )],
        );
        let forward = state.begin_forward_navigation().unwrap();
        assert!(matches!(
            forward,
            explorer_model::ExplorerCommand::Navigate { .. }
        ));
        assert_eq!(
            state
                .tabs()
                .active_tab()
                .directory
                .snapshot()
                .unwrap()
                .entries()[0]
                .display_name,
            "b-row"
        );
    }

    fn visible_detail_order(state: &AppViewState) -> Vec<explorer_model::ColumnId> {
        let settings = state.view_settings();
        settings
            .details_layout
            .visible_registered(state.column_registry())
            .map(|entry| entry.id.clone())
            .collect()
    }

    #[test]
    fn details_column_midpoints_resolve_symmetrically_after_removing_source() {
        use explorer_model::ColumnId::{DateModified, Name, Size, Type};

        let order = vec![Name, DateModified, Type, Size];
        let right =
            resolve_details_column_insertion(&order, &DateModified, &Type, 50.0, 0.0, 100.0)
                .expect("exact midpoint uses the right insertion slot");
        assert_eq!(right.order, vec![Name, Type, DateModified, Size]);
        assert_eq!(right.before, Some(Size));

        let left = resolve_details_column_insertion(&order, &Size, &Type, 49.0, 0.0, 100.0)
            .expect("left insertion slot");
        assert_eq!(left.order, vec![Name, DateModified, Size, Type]);
        assert_eq!(left.before, Some(Type));

        let terminal =
            resolve_details_column_insertion(&order, &DateModified, &Size, 100.0, 0.0, 100.0)
                .expect("terminal insertion slot");
        assert_eq!(terminal.order, vec![Name, Type, Size, DateModified]);
        assert_eq!(terminal.before, None);
    }

    #[test]
    fn details_column_midpoint_rejects_invalid_input_and_name_drag() {
        use explorer_model::ColumnId::{DateModified, Name, Type};

        let order = vec![Name, DateModified, Type];
        assert!(
            resolve_details_column_insertion(&order, &DateModified, &Type, f32::NAN, 0.0, 1.0)
                .is_none()
        );
        assert!(
            resolve_details_column_insertion(&order, &DateModified, &Type, 0.5, 1.0, 1.0).is_none()
        );
        assert!(resolve_details_column_insertion(&order, &Name, &Type, 0.5, 0.0, 1.0).is_none());
    }

    #[test]
    fn details_column_preview_is_transient_repeatable_and_cancellable() {
        use explorer_model::ColumnId::{DateModified, Name, Size, Type};

        let mut state = AppViewState::default();
        let original = visible_detail_order(&state);
        assert_eq!(original[..4], [Name, DateModified, Type, Size]);
        assert!(state.update_details_column_drag_preview(DateModified, Type, 75.0, 0.0, 100.0,));
        assert_eq!(
            visible_detail_order(&state)[..4],
            [Name, Type, DateModified, Size]
        );
        assert!(!state.update_details_column_drag_preview(DateModified, Type, 75.0, 0.0, 100.0,));
        assert!(state.cancel_details_column_drag());
        assert_eq!(visible_detail_order(&state), original);
    }

    #[test]
    fn details_column_preview_commits_once_and_invalid_identity_cancels() {
        use explorer_model::ColumnId::{DateModified, Name, Size, Type};

        let mut state = AppViewState::default();
        assert!(state.update_details_column_drag_preview(DateModified, Type, 75.0, 0.0, 100.0,));
        assert!(state.commit_details_column_drag());
        assert!(!state.commit_details_column_drag());
        assert_eq!(
            visible_detail_order(&state)[..4],
            [Name, Type, DateModified, Size]
        );
        let committed = visible_detail_order(&state);
        assert!(!state.cancel_details_column_drag());
        assert_eq!(visible_detail_order(&state), committed);

        assert!(state.update_details_column_drag_preview(DateModified, Size, 75.0, 0.0, 100.0,));
        let unavailable = explorer_model::ColumnId::Extension {
            package_id: "missing-package".to_owned(),
            column_id: "missing-column".to_owned(),
        };
        assert!(state.update_details_column_drag_preview(
            DateModified,
            unavailable,
            75.0,
            0.0,
            100.0,
        ));
        assert!(!state.details_column_drag_active());
    }

    #[test]
    fn default_state_has_real_tab_availability_and_token_values() {
        let mut state = AppViewState::default();
        assert_eq!(state.current_theme(), ThemeMode::Light);
        assert_eq!(
            state.navigation_pane_width(),
            LayoutTokens::WINDOWS_11.navigation_pane_default_width
        );
        assert_eq!(state.focused_surface(), FocusSurface::FileView);
        assert_eq!(state.previous_focus(), None);
        assert!(!state.close_requested());

        let commands = state.command_availability();
        assert!(!commands.is_enabled(CommandKind::Back));
        assert!(!commands.is_enabled(CommandKind::Forward));
        assert!(!commands.is_enabled(CommandKind::Up));
        assert!(commands.is_enabled(CommandKind::NewTab));
        assert!(commands.is_enabled(CommandKind::CloseTab));
        assert!(!commands.is_enabled(CommandKind::NextTab));
        let first = state.tabs().active_tab_id();
        let second = state.new_tab();
        assert_ne!(first, second);
        assert!(
            state
                .command_availability()
                .is_enabled(CommandKind::NextTab)
        );
    }

    #[test]
    fn file_view_typeahead_accumulates_prefix_cycles_matches_and_expires() {
        let mut state = state_with_rows();
        let now = Instant::now();

        assert_eq!(state.typeahead_file_view("F", now), Some(0));
        assert_eq!(state.file_view_typeahead_prefix(), Some("f"));
        assert_eq!(
            state.typeahead_file_view("i", now + Duration::from_millis(100)),
            Some(1)
        );
        assert_eq!(state.file_view_typeahead_prefix(), Some("fi"));
        assert_eq!(state.focused_row_index(), Some(1));

        assert!(state.clear_file_view_typeahead());
        assert_eq!(state.file_view_typeahead_prefix(), None);
        assert_eq!(
            state.typeahead_file_view("f", now + Duration::from_millis(200)),
            Some(0)
        );
        assert_eq!(
            state.typeahead_file_view("f", now + Duration::from_millis(300)),
            Some(1),
            "a repeated prefix cycles from the current item"
        );

        assert_eq!(
            state.typeahead_file_view("f", now + Duration::from_millis(1_500)),
            Some(0),
            "input after the Explorer-style timeout starts a new prefix"
        );
        assert_eq!(state.file_view_typeahead_prefix(), Some("f"));
    }

    #[test]
    fn file_view_typeahead_clears_on_focus_navigation_and_tab_changes() {
        let mut state = state_with_rows();
        let now = Instant::now();
        assert_eq!(state.typeahead_file_view("fi", now), Some(1));
        state.focus(FocusSurface::Search);
        assert!(!state.file_view_typeahead_active());

        state.focus(FocusSurface::FileView);
        assert_eq!(state.typeahead_file_view("f", now), Some(0));
        let _ = state.begin_active_navigation(
            explorer_model::LocationDescriptor::file_system(r"C:\replacement"),
            false,
        );
        assert!(!state.file_view_typeahead_active());

        assert_eq!(state.typeahead_file_view("f", now), None);
        state.new_tab();
        assert!(!state.file_view_typeahead_active());
    }

    #[test]
    fn slow_incremental_directory_keeps_scroll_tab_switch_and_cancellation_available() {
        let mut state = AppViewState::with_initial_location(explorer_model::HistoryEntry::new(
            explorer_model::LocationDescriptor::file_system(r"C:\slow"),
            "slow",
        ));
        let command = state.begin_active_location_load().expect("load command");
        let context = command.context().expect("context").clone();
        let entries = explorer_test_support::synthetic_directory_entries(64);
        assert_eq!(
            state.apply_service_event(explorer_model::ExplorerEvent::DirectoryBatch {
                context: context.clone(),
                entries,
            }),
            explorer_model::WindowEventOutcome::Applied
        );
        let snapshot = state
            .tabs()
            .active_tab()
            .visible_snapshot()
            .expect("first batch is visible");
        assert_eq!(snapshot.entries().len(), 64);
        let range = crate::file_view::fixed_virtual_range(64, 24.0, 240.0, 240.0, 2);
        assert!(!range.items.is_empty());

        let first_tab = state.tabs().active_tab_id();
        let second_tab = state.new_tab();
        assert_eq!(state.tabs().active_tab_id(), second_tab);
        assert!(state.activate_tab(first_tab));
        let replacement = state
            .begin_active_navigation(
                explorer_model::LocationDescriptor::file_system(r"C:\replacement"),
                false,
            )
            .expect("replacement navigation");
        assert!(context.cancellation.is_cancelled());
        assert_ne!(
            replacement
                .context()
                .expect("replacement context")
                .generation,
            context.generation
        );
    }

    fn restored_two_tab_state() -> (AppViewState, explorer_model::TabId, explorer_model::TabId) {
        let first = explorer_model::TabState::new(explorer_model::HistoryEntry::new(
            explorer_model::LocationDescriptor::file_system(r"C:\restored-first"),
            "restored-first",
        ));
        let first_id = first.id;
        let second = explorer_model::TabState::new(explorer_model::HistoryEntry::new(
            explorer_model::LocationDescriptor::file_system(r"D:\restored-second"),
            "restored-second",
        ));
        let second_id = second.id;
        let tabs = explorer_model::ExplorerWindowState::from_restored_tabs(
            vec![first, second],
            first_id,
            explorer_model::HistoryEntry::new(
                explorer_model::LocationDescriptor::file_system(r"C:\fallback"),
                "fallback",
            ),
        )
        .expect("valid restored tabs");
        (
            AppViewState::with_restored_window_and_drag_threshold(tabs, (4.0, 4.0)),
            first_id,
            second_id,
        )
    }

    #[test]
    fn idle_restored_tab_starts_once_with_its_current_location() {
        let (mut state, first_id, second_id) = restored_two_tab_state();
        assert_eq!(state.tabs().active_tab_id(), first_id);
        let command = state
            .begin_idle_active_location_load()
            .expect("idle restored tab loads");
        assert!(matches!(
            command,
            explorer_model::ExplorerCommand::Navigate { location, .. }
                if location == explorer_model::LocationDescriptor::file_system(r"C:\restored-first")
        ));
        assert!(matches!(
            state.tabs().active_tab().directory,
            explorer_model::DirectoryState::Loading { .. }
        ));
        assert!(state.begin_idle_active_location_load().is_none());

        assert!(state.activate_tab(second_id));
        assert!(state.begin_idle_active_location_load().is_some());
        assert!(state.begin_idle_active_location_load().is_none());
    }

    #[test]
    fn ready_and_failed_restored_tabs_are_not_retried_by_activation() {
        let (mut ready, _, _) = restored_two_tab_state();
        let ready_command = ready
            .begin_idle_active_location_load()
            .expect("ready fixture starts");
        let ready_context = ready_command.context().expect("ready context").clone();
        assert_eq!(
            ready.apply_service_event(explorer_model::ExplorerEvent::DirectoryFinished {
                context: ready_context,
            }),
            explorer_model::WindowEventOutcome::Applied
        );
        assert!(matches!(
            ready.tabs().active_tab().directory,
            explorer_model::DirectoryState::Ready(_)
        ));
        assert!(ready.begin_idle_active_location_load().is_none());

        let (mut failed, _, _) = restored_two_tab_state();
        let failed_command = failed
            .begin_idle_active_location_load()
            .expect("failed fixture starts");
        let failed_context = failed_command.context().expect("failed context").clone();
        assert_eq!(
            failed.apply_service_event(explorer_model::ExplorerEvent::Failed {
                context: failed_context,
                error: explorer_common::ExplorerError::new(
                    explorer_common::ExplorerErrorKind::Availability,
                    "restored-tab fixture",
                    true,
                    "The restored folder could not be loaded.",
                    "controlled admission failure",
                ),
            }),
            explorer_model::WindowEventOutcome::Applied
        );
        assert!(matches!(
            failed.tabs().active_tab().directory,
            explorer_model::DirectoryState::Error { .. }
        ));
        assert!(failed.begin_idle_active_location_load().is_none());
    }

    #[test]
    fn progressive_thumbnail_event_preserves_row_interaction_state() {
        let mut state = state_with_rows();
        assert!(state.select_row(1));
        assert!(state.begin_inline_rename(1));
        state.set_sort_column(explorer_model::ColumnId::Size);
        let before_selection = state.tabs().active_tab().selection.clone();
        let before_focus = state.focused_row_index();
        let before_rename = state.rename_editor().cloned();
        let before_settings = state.view_settings();
        let tab = state.tabs().active_tab();
        let context = explorer_model::RequestContext::new(tab.id, tab.generation);
        let key = explorer_model::ThumbnailRequestKey {
            item_id: explorer_model::ShellItemId::from_provider_bytes([2]).expect("item"),
            physical_size: 96,
            dpi: 96,
            mode: explorer_model::ThumbnailMode::Thumbnail,
            source_generation: 1,
            theme: explorer_model::ShellIconTheme::Light,
            association_generation: 1,
            overlay_generation: 1,
        };
        assert_eq!(
            state.apply_service_event(explorer_model::ExplorerEvent::ThumbnailFinished {
                context,
                key,
                outcome: explorer_model::ThumbnailTerminal::Ready {
                    source: explorer_model::ThumbnailSource::Provider,
                    pixels: explorer_model::ThumbnailPixels {
                        width: 1,
                        height: 1,
                        stride: 4,
                        bytes: vec![0; 4],
                    },
                },
            }),
            explorer_model::WindowEventOutcome::IgnoredUnrelated
        );
        assert_eq!(state.tabs().active_tab().selection, before_selection);
        assert_eq!(state.focused_row_index(), before_focus);
        assert_eq!(
            state.rename_editor().map(|editor| (
                editor.item.clone(),
                editor.buffer.clone(),
                editor.selection.clone()
            )),
            before_rename.map(|editor| (editor.item, editor.buffer, editor.selection))
        );
        assert_eq!(state.view_settings(), before_settings);
    }

    #[test]
    fn saved_state_reset_requires_confirmation_and_supports_retry_notice() {
        let mut state = AppViewState::default();
        state.begin_session_reset_confirmation(explorer_model::SessionResetScope::Session);
        assert_eq!(
            state.session_reset_confirmation(),
            Some(explorer_model::SessionResetScope::Session)
        );
        state.cancel_session_reset_confirmation();
        assert!(state.take_confirmed_session_reset().is_none());
        state.begin_session_reset_confirmation(explorer_model::SessionResetScope::ViewSettings);
        state.confirm_session_reset();
        let scope = state
            .take_confirmed_session_reset()
            .expect("confirmed reset");
        state.finish_session_reset_submission(scope, false);
        assert!(state.session_reset_notice().is_some());
        state.retry_session_reset();
        assert_eq!(state.take_confirmed_session_reset(), Some(scope));
        state.finish_session_reset_submission(scope, true);
        assert!(
            state
                .session_reset_notice()
                .is_some_and(|notice| notice.contains("accepted"))
        );
    }

    #[test]
    fn details_column_resize_is_clamped_and_owned_by_the_active_tab() {
        let mut state = AppViewState::default();
        let first = state.tabs().active_tab_id();
        state.begin_details_column_resize(explorer_model::ColumnId::Name, 100.0);
        assert!(state.details_column_resize_active());
        state.update_details_column_resize(160.0);
        assert_eq!(
            state
                .view_settings()
                .details_column_width(&explorer_model::ColumnId::Name),
            340
        );
        state.update_details_column_resize(-10_000.0);
        assert_eq!(
            state
                .view_settings()
                .details_column_width(&explorer_model::ColumnId::Name),
            explorer_model::OrderedColumnLayout::MINIMUM_WIDTH
        );
        state.end_details_column_resize();
        assert!(!state.details_column_resize_active());
        state.update_details_column_resize(500.0);
        assert_eq!(
            state
                .view_settings()
                .details_column_width(&explorer_model::ColumnId::Name),
            explorer_model::OrderedColumnLayout::MINIMUM_WIDTH
        );
        state.set_details_column_width(explorer_model::ColumnId::Name, 280);
        state.begin_details_column_resize(explorer_model::ColumnId::Name, 0.0);
        state.update_details_column_resize(10_000.0);
        assert_eq!(
            state
                .view_settings()
                .details_column_width(&explorer_model::ColumnId::Name),
            explorer_model::OrderedColumnLayout::MAXIMUM_WIDTH
        );
        state.end_details_column_resize();
        state.set_details_column_width(
            explorer_model::ColumnId::Name,
            explorer_model::OrderedColumnLayout::MINIMUM_WIDTH,
        );

        state.begin_details_column_resize(explorer_model::ColumnId::Name, 0.0);
        let second = state.new_tab();
        assert!(!state.details_column_resize_active());
        assert_ne!(first, second);
        assert_eq!(
            state
                .view_settings()
                .details_column_width(&explorer_model::ColumnId::Name),
            explorer_model::OrderedColumnLayout::MINIMUM_WIDTH
        );
        state.set_details_column_width(explorer_model::ColumnId::Name, 500);
        assert!(state.activate_tab(first));
        assert_eq!(
            state
                .view_settings()
                .details_column_width(&explorer_model::ColumnId::Name),
            explorer_model::OrderedColumnLayout::MINIMUM_WIDTH
        );
        assert!(state.activate_tab(second));
        assert_eq!(
            state
                .view_settings()
                .details_column_width(&explorer_model::ColumnId::Name),
            500
        );
        state.begin_details_column_resize(explorer_model::ColumnId::Name, 0.0);
        state.set_view_mode(explorer_model::ViewMode::List);
        assert!(!state.details_column_resize_active());
        state.begin_details_column_resize(explorer_model::ColumnId::Name, 0.0);
        state.request_close();
        assert!(!state.details_column_resize_active());
    }

    #[test]
    fn side_panes_are_mutually_exclusive_resizable_and_tab_isolated() {
        let mut state = AppViewState::default();
        let first = state.tabs().active_tab_id();
        state.toggle_details_pane();
        assert!(state.view_settings().details_pane);
        assert!(!state.view_settings().preview_pane);
        assert!(state.begin_side_pane_resize(1_000.0));
        assert!(state.side_pane_resize_active());
        assert!(state.update_side_pane_resize(900.0));
        assert_eq!(state.view_settings().details_pane_width, 393);

        state.toggle_preview_pane();
        assert!(!state.view_settings().details_pane);
        assert!(state.view_settings().preview_pane);
        assert!(!state.side_pane_resize_active());
        assert!(state.begin_side_pane_resize(1_000.0));
        assert!(state.update_side_pane_resize(10_000.0));
        assert_eq!(
            state.view_settings().preview_pane_width,
            super::clamped_u16(
                LayoutTokens::WINDOWS_11.side_pane_min_width.value(),
                0,
                u16::MAX,
            )
        );

        let second = state.new_tab();
        assert!(!state.side_pane_resize_active());
        assert!(!state.view_settings().details_pane);
        assert!(state.view_settings().preview_pane);
        state.toggle_preview_pane();
        assert!(!state.view_settings().preview_pane);
        assert!(state.activate_tab(first));
        assert!(state.view_settings().preview_pane);
        assert_eq!(
            state.view_settings().preview_pane_width,
            super::clamped_u16(
                LayoutTokens::WINDOWS_11.side_pane_min_width.value(),
                0,
                u16::MAX,
            )
        );
        assert!(state.activate_tab(second));
        assert!(!state.view_settings().preview_pane);
    }

    #[test]
    fn details_column_auto_size_uses_owned_snapshot_text_and_header_only_when_empty() {
        let mut empty = AppViewState::default();
        empty.auto_size_details_column(explorer_model::ColumnId::Name);
        assert_eq!(
            empty
                .view_settings()
                .details_column_width(&explorer_model::ColumnId::Name),
            60,
            "empty folders size from the localized header without I/O"
        );

        let mut state = state_with_rows();
        state.begin_details_column_resize(explorer_model::ColumnId::Name, 10.0);
        state.auto_size_details_column(explorer_model::ColumnId::Name);
        assert!(!state.details_column_resize_active());
        assert_eq!(
            state
                .view_settings()
                .details_column_width(&explorer_model::ColumnId::Name),
            98,
            "name estimate includes the longest owned row plus icon and padding"
        );

        let mut snapshot = explorer_model::DirectorySnapshot::default();
        let _ = snapshot.upsert(explorer_model::FileEntry {
            id: explorer_model::ShellItemId::from_provider_bytes([9]).expect("long name id"),
            display_name: "x".repeat(500),
            location: explorer_model::LocationDescriptor::file_system(r"C:\fixture\long"),
            is_container: false,
            metadata: explorer_model::FileEntryMetadata::default(),
        });
        state.tabs.active_tab_mut().directory = explorer_model::DirectoryState::Ready(snapshot);
        state.auto_size_details_column(explorer_model::ColumnId::Name);
        assert_eq!(
            state
                .view_settings()
                .details_column_width(&explorer_model::ColumnId::Name),
            explorer_model::OrderedColumnLayout::MAXIMUM_WIDTH
        );
    }

    #[test]
    fn sort_menu_reducer_is_per_tab_and_shares_column_direction_state() {
        let mut state = AppViewState::default();
        let first = state.tabs().active_tab_id();
        let second = state.new_tab();
        let _ = state.take_pending_new_tab_command();
        state.set_sort_column(explorer_model::ColumnId::Size);
        assert_eq!(
            state.view_settings().sort,
            explorer_model::SortDescriptor {
                column: explorer_model::ColumnId::Size,
                direction: explorer_model::SortDirection::Descending,
            }
        );
        state.set_sort_direction(explorer_model::SortDirection::Ascending);
        assert!(state.activate_tab(first));
        assert_eq!(
            state.view_settings().sort,
            explorer_model::SortDescriptor::default()
        );
        assert!(state.activate_tab(second));
        assert_eq!(
            state.view_settings().sort.column,
            explorer_model::ColumnId::Size
        );
        assert_eq!(
            state.view_settings().sort.direction,
            explorer_model::SortDirection::Ascending
        );
    }

    #[test]
    fn non_path_namespace_disables_columns_without_owned_property_values() {
        let mut state = AppViewState::with_initial_location(explorer_model::HistoryEntry::new(
            explorer_model::LocationDescriptor::ParsingName("shell:RecycleBinFolder".to_owned()),
            "Recycle Bin",
        ));
        assert!(state.sort_column_supported(explorer_model::ColumnId::Name));
        assert!(!state.sort_column_supported(explorer_model::ColumnId::Size));

        let command = state.begin_active_location_load().expect("namespace load");
        let context = command.context().expect("request context").clone();
        let entry = explorer_model::FileEntry {
            id: explorer_model::ShellItemId::from_provider_bytes([77]).expect("identity"),
            display_name: "owned value".to_owned(),
            location: explorer_model::LocationDescriptor::ParsingName(
                "shell:RecycleBinFolder\\owned".to_owned(),
            ),
            is_container: false,
            metadata: explorer_model::FileEntryMetadata {
                size_bytes: Some(42),
                type_display: Some("File".to_owned()),
                ..explorer_model::FileEntryMetadata::default()
            },
        };
        let _ = state.apply_service_event(explorer_model::ExplorerEvent::LocationResolved {
            context: context.clone(),
            metadata: explorer_model::LocationMetadata {
                descriptor: explorer_model::LocationDescriptor::ParsingName(
                    "shell:RecycleBinFolder".to_owned(),
                ),
                display_title: "Recycle Bin".to_owned(),
                can_go_up: false,
                can_write: false,
            },
        });
        let _ = state.apply_service_event(explorer_model::ExplorerEvent::DirectoryBatch {
            context: context.clone(),
            entries: vec![entry],
        });
        let _ =
            state.apply_service_event(explorer_model::ExplorerEvent::DirectoryFinished { context });
        assert!(state.sort_column_supported(explorer_model::ColumnId::Size));
        assert!(state.sort_column_supported(explorer_model::ColumnId::Type));
        assert!(!state.sort_column_supported(explorer_model::ColumnId::Authors));
    }

    #[test]
    fn details_column_menu_applies_all_columns_immediately_per_tab() {
        let mut state = AppViewState::default();
        let first = state.tabs().active_tab_id();
        state.open_details_column_menu(explorer_model::ColumnId::Size);
        assert_eq!(
            state.details_column_menu(),
            Some(explorer_model::ColumnId::Size)
        );
        state.toggle_details_column(explorer_model::ColumnId::Name);
        assert!(state.details_column_visible(explorer_model::ColumnId::Name));
        state.toggle_details_column(explorer_model::ColumnId::Size);
        assert!(!state.details_column_visible(explorer_model::ColumnId::Size));
        let second = state.new_tab();
        let _ = state.take_pending_new_tab_command();
        assert!(!state.details_column_visible(explorer_model::ColumnId::Size));
        state.toggle_details_column(explorer_model::ColumnId::Size);
        assert!(state.details_column_visible(explorer_model::ColumnId::Size));
        assert!(state.activate_tab(first));
        assert!(!state.details_column_visible(explorer_model::ColumnId::Size));
        assert!(state.activate_tab(second));
        assert!(state.details_column_visible(explorer_model::ColumnId::Size));

        state.open_details_column_menu(explorer_model::ColumnId::Name);
        state.toggle_details_column(explorer_model::ColumnId::Authors);
        assert!(state.details_column_visible(explorer_model::ColumnId::Authors));
        assert_eq!(
            state.details_column_menu(),
            Some(explorer_model::ColumnId::Name),
            "immediate toggles keep the menu open for additional column choices"
        );
        state.toggle_details_column(explorer_model::ColumnId::Authors);
        assert!(!state.details_column_visible(explorer_model::ColumnId::Authors));
    }

    #[test]
    fn pointer_rows_resolve_against_the_sorted_presentation_order() {
        let mut state = state_with_rows();
        let zeta_id = explorer_model::ShellItemId::from_provider_bytes([3]).expect("zeta id");
        let mut snapshot = state
            .tabs()
            .active_tab()
            .visible_snapshot()
            .expect("ready fixture snapshot")
            .clone();
        let _ = snapshot.upsert(explorer_model::FileEntry {
            id: zeta_id.clone(),
            display_name: "zeta.txt".to_owned(),
            location: explorer_model::LocationDescriptor::file_system(r"C:\fixture\zeta.txt"),
            is_container: false,
            metadata: explorer_model::FileEntryMetadata::default(),
        });
        state.tabs_mut().active_tab_mut().directory =
            explorer_model::DirectoryState::Ready(snapshot);
        state.set_sort_column(explorer_model::ColumnId::Name);
        state.set_sort_direction(explorer_model::SortDirection::Descending);

        assert!(
            state.select_row(1),
            "second visible row is zeta after sorting"
        );
        assert!(state.tabs().active_tab().selection.contains(&zeta_id));
        assert_eq!(state.focused_row_index(), Some(1));
        let explorer_model::ExplorerCommand::OpenItem { item, .. } = state
            .open_row_command(1, false)
            .expect("visible file opens through its presentation index")
        else {
            panic!("visible file must use the default application command");
        };
        assert_eq!(item.id, zeta_id);
        let explorer_model::ExplorerCommand::ShowContextMenu { request, .. } = state
            .begin_share_request(42)
            .expect("selected visible item can invoke Share")
        else {
            panic!("Share uses the Shell context command boundary");
        };
        assert_eq!(request.requested_verb.as_deref(), Some("Windows.Share"));
        let explorer_model::ShellContextMenuTarget::Items { items, .. } = request.target else {
            panic!("Share targets the selected items");
        };
        assert_eq!(items[0].id, zeta_id);

        let explorer_model::ExplorerCommand::ShowContextMenu { request, .. } = state
            .begin_pin_to_start_request(42)
            .expect("selected visible item can invoke Pin to Start")
        else {
            panic!("Pin to Start uses the Shell canonical verb boundary");
        };
        assert_eq!(request.requested_verb.as_deref(), Some("PinToStartScreen"));

        let explorer_model::ExplorerCommand::ShowContextMenu { request, .. } = state
            .begin_properties_request(42)
            .expect("selected visible item can invoke Properties")
        else {
            panic!("Properties uses the Shell canonical verb boundary");
        };
        assert_eq!(request.requested_verb.as_deref(), Some("properties"));
    }

    #[test]
    fn successful_shell_mutation_requests_one_generation_safe_refresh() {
        let mut state = state_with_rows();
        let request = state.create_folder_request().expect("writable fixture");
        let command = state.begin_file_operation(request);
        let context = command.context().expect("operation context").clone();
        let event = explorer_model::ExplorerEvent::OperationFinished {
            context: context.clone(),
            outcome: explorer_model::OperationTerminal::Finished,
        };

        assert!(state.service_event_requires_active_refresh(&event));
        assert_eq!(
            state.apply_service_event(event.clone()),
            explorer_model::WindowEventOutcome::Applied
        );
        let refresh = state
            .begin_refresh_navigation()
            .expect("successful mutation refreshes the active folder");
        assert!(matches!(
            refresh,
            explorer_model::ExplorerCommand::Refresh { .. }
        ));
        assert!(
            !state.service_event_requires_active_refresh(&event),
            "the old operation generation cannot schedule a duplicate refresh"
        );
    }

    #[test]
    fn interactive_new_folder_uses_first_case_insensitive_available_name() {
        let mut state = state_with_rows();
        let mut entries = state
            .tabs()
            .active_tab()
            .visible_snapshot()
            .expect("fixture snapshot")
            .entries()
            .to_vec();
        for (id, name) in [(31_u8, "new FOLDER"), (32, "New folder (2)")] {
            entries.push(explorer_model::FileEntry {
                id: explorer_model::ShellItemId::from_provider_bytes([id]).expect("folder id"),
                display_name: name.to_owned(),
                location: explorer_model::LocationDescriptor::file_system(format!(
                    r"C:\fixture\{name}"
                )),
                is_container: true,
                metadata: explorer_model::FileEntryMetadata::default(),
            });
        }
        let refresh = state.begin_refresh_navigation().expect("refresh command");
        let context = refresh.context().expect("refresh context").clone();
        let _ = state.apply_service_event(explorer_model::ExplorerEvent::DirectoryBatch {
            context: context.clone(),
            entries,
        });
        let _ =
            state.apply_service_event(explorer_model::ExplorerEvent::DirectoryFinished { context });
        let request = state.create_folder_request().expect("writable fixture");
        assert!(matches!(
            request.kind,
            explorer_model::FileOperationKind::CreateFolder { ref name, .. }
                if name == "New folder (3)"
        ));
    }

    #[test]
    fn operation_terminal_notice_tracks_latest_request_and_rejects_stale_completion() {
        let mut state = state_with_rows();
        let first = state
            .create_folder_request()
            .expect("first operation request");
        let first_command = state.begin_file_operation(first);
        let first_context = first_command.context().expect("first context").clone();
        let first_terminal = explorer_model::ExplorerEvent::OperationFinished {
            context: first_context.clone(),
            outcome: explorer_model::OperationTerminal::Finished,
        };
        assert_eq!(
            state.apply_service_event(first_terminal),
            explorer_model::WindowEventOutcome::Applied
        );
        assert!(
            state
                .latest_operation_terminal_elapsed(Instant::now())
                .is_some()
        );

        let second = state
            .create_folder_request()
            .expect("second operation request");
        let _second_command = state.begin_file_operation(second);
        assert!(
            state
                .latest_operation_terminal_elapsed(Instant::now())
                .is_none(),
            "a new latest operation replaces the old terminal lifecycle"
        );
        let duplicate_old_terminal = explorer_model::ExplorerEvent::OperationFinished {
            context: first_context,
            outcome: explorer_model::OperationTerminal::Finished,
        };
        assert_eq!(
            state.apply_service_event(duplicate_old_terminal),
            explorer_model::WindowEventOutcome::IgnoredStale
        );
        assert!(
            state
                .latest_operation_terminal_elapsed(Instant::now())
                .is_none(),
            "a stale completion cannot restart the latest notice"
        );
    }

    #[test]
    fn interactive_new_folder_is_provisional_until_rename_commit() {
        let mut state = state_with_rows();
        let request = state.create_folder_request().expect("writable fixture");
        assert!(state.begin_interactive_create_folder(request));
        let editor = state
            .rename_editor()
            .expect("draft enters rename immediately");
        assert_eq!(editor.buffer, "New folder");
        assert_eq!(
            state
                .provisional_new_folder_entry()
                .expect("visible draft")
                .display_name,
            "New folder"
        );
        assert_eq!(state.focused_surface(), FocusSurface::FileView);
        assert!(state.update_inline_rename("Renamed before create".to_owned()));
        let request = state
            .commit_inline_rename(explorer_model::RenameCommitTrigger::Enter)
            .expect("valid name")
            .expect("commit creates folder");
        assert!(matches!(
            request.kind,
            explorer_model::FileOperationKind::CreateFolder { ref name, .. }
                if name == "Renamed before create"
        ));
        assert!(state.rename_editor().is_none());
        assert!(state.provisional_new_folder_entry().is_none());
    }

    #[test]
    fn cancelling_interactive_new_folder_discards_draft_without_operation() {
        let mut state = state_with_rows();
        let request = state.create_folder_request().expect("writable fixture");
        assert!(state.begin_interactive_create_folder(request));
        assert!(state.cancel_inline_rename());
        assert!(state.rename_editor().is_none());
        assert!(state.pending_new_folder_rename.is_none());
        assert!(state.provisional_new_folder_entry().is_none());
    }

    #[test]
    fn scrollbar_drag_is_transient_and_ends_on_tab_or_view_switch() {
        use crate::interaction::{ScrollbarKind, ScrollbarTerminal};

        let mut state = AppViewState::default();
        assert!(!state.begin_scrollbar_drag(ScrollbarKind::FileView, f32::NAN));
        assert!(state.begin_scrollbar_drag(ScrollbarKind::FileView, 4.0));
        let _second = state.new_tab();
        assert_eq!(state.scrollbar_drag_session(), None);

        assert!(state.begin_scrollbar_drag(ScrollbarKind::Navigation, 2.0));
        state.set_view_mode(explorer_model::ViewMode::List);
        assert_eq!(state.scrollbar_drag_session(), None);
        assert!(!state.end_scrollbar_drag(ScrollbarTerminal::Escape));
    }

    #[test]
    fn marquee_selects_intersecting_rows_and_is_transient_across_tabs() {
        let mut state = state_with_rows();
        let layout = LayoutTokens::WINDOWS_11;
        assert!(state.begin_marquee(8.0, layout.details_header_height.value() + 1.0, false));
        assert!(state.update_marquee(
            400.0,
            layout.details_header_height.value() + layout.file_row_height.value() * 2.0,
            0.0,
            800.0,
            layout,
        ));
        assert_eq!(state.tabs().active_tab().selection.len(), 2);
        assert!(state.marquee_session().is_some());
        state.new_tab();
        assert!(state.marquee_session().is_none());
    }

    #[test]
    fn small_icon_marquee_uses_the_same_local_cells_as_rendering() {
        let mut state = state_with_rows();
        let layout = LayoutTokens::WINDOWS_11;
        state.set_view_mode(explorer_model::ViewMode::SmallIcons);
        assert!(state.begin_marquee(241.0, 1.0, false));
        assert!(state.update_marquee(479.0, 31.0, 0.0, 480.0, layout));
        let order = state.presentation_ids();
        let selection = &state.tabs().active_tab().selection;
        assert_eq!(selection.len(), 1);
        assert!(!selection.contains(&order[0]));
        assert!(selection.contains(&order[1]));
    }

    #[test]
    fn details_resize_and_marquee_cannot_own_the_same_pointer_gesture() {
        let mut state = state_with_rows();

        state.begin_details_column_resize(explorer_model::ColumnId::Name, 100.0);
        assert!(state.details_column_resize_active());
        assert!(!state.begin_marquee(8.0, 40.0, false));
        assert!(state.marquee_session().is_none());

        state.end_details_column_resize();
        assert!(state.begin_marquee(8.0, 40.0, false));
        state.begin_details_column_resize(explorer_model::ColumnId::Name, 100.0);
        assert!(state.details_column_resize_active());
        assert!(state.marquee_session().is_none());
    }

    #[test]
    fn remote_details_projection_hides_local_columns_without_rewriting_preferences() {
        let location =
            explorer_model::LocationDescriptor::try_virtual("adb", [7; 16], 1, None, Vec::new())
                .unwrap();
        let mut state = AppViewState::with_initial_location(explorer_model::HistoryEntry::new(
            location, "device",
        ));
        state
            .tabs
            .active_tab_mut()
            .view
            .settings
            .details_layout
            .set_visible(&explorer_model::ColumnId::Authors, true);
        state.tabs.active_tab_mut().view.settings.sort.column = explorer_model::ColumnId::Authors;

        let effective = state.view_settings();
        assert!(effective.details_column_visible(&explorer_model::ColumnId::Name));
        assert!(effective.details_column_visible(&explorer_model::ColumnId::Permissions));
        assert!(
            effective
                .details_layout
                .entry(&explorer_model::ColumnId::Authors)
                .is_none()
        );
        assert_eq!(effective.sort, explorer_model::SortDescriptor::default());
        assert!(
            state
                .tabs
                .active_tab()
                .view
                .settings
                .details_column_visible(&explorer_model::ColumnId::Authors)
        );
        assert_eq!(
            state.tabs.active_tab().view.settings.sort.column,
            explorer_model::ColumnId::Authors
        );
    }

    #[test]
    fn new_tab_copies_the_current_resolved_location_and_not_transient_view_state() {
        let location = explorer_model::LocationDescriptor::file_system(r"D:\test\真實資料夾");
        let mut state = AppViewState::with_initial_location(explorer_model::HistoryEntry::new(
            location.clone(),
            "真實資料夾",
        ));
        state
            .tabs_mut()
            .active_tab_mut()
            .view
            .address
            .enter_editing();
        assert!(
            state
                .tabs_mut()
                .active_tab_mut()
                .view
                .address
                .update_draft(r"C:\尚未提交".to_owned())
        );
        let first = state.tabs().active_tab_id();
        let second = state.new_tab();
        assert_ne!(first, second);
        let command = state
            .take_pending_new_tab_command()
            .expect("new tab and initial load are one mutation");
        let explorer_model::ExplorerCommand::Navigate {
            context,
            location: command_location,
        } = command
        else {
            panic!("new tab must queue Navigate");
        };
        assert_eq!(context.tab_id, second);
        assert_eq!(context.generation, state.tabs().active_tab().generation);
        assert_eq!(command_location, location);
        let active = state.tabs().active_tab();
        assert_eq!(
            active.history.current().map(|entry| &entry.location),
            Some(&location)
        );
        assert_eq!(active.view.address.draft, r"D:\test\真實資料夾");
        assert!(matches!(
            active.view.address.mode,
            explorer_model::AddressBarMode::Browsing
        ));
    }

    #[test]
    fn drag_threshold_drop_target_and_terminal_cleanup_are_generation_safe() {
        let mut state = state_with_rows();
        assert!(state.select_row(1));
        assert!(state.begin_drag_candidate(10.0, 10.0, explorer_model::DragButton::Left));
        assert!(!state.update_drag_pointer(13.0, 13.0));
        assert!(state.update_drag_pointer(14.0, 10.0));
        assert!(matches!(
            state.take_pending_drag_command(),
            Some(explorer_model::ExplorerCommand::DataTransfer {
                request: explorer_model::DataTransferRequest::BeginDrag { .. },
                ..
            })
        ));

        state.update_external_drag_target(
            Some(0),
            explorer_model::DropTargetKind::FolderItem,
            101.0,
            100.0,
            300.0,
            explorer_model::DragEffect::Copy,
        );
        assert_eq!(state.drop_target_row(), Some(0));
        assert!(matches!(
            state.drag_session().state(),
            explorer_model::DragSessionState::Dragging {
                auto_scroll: Some(explorer_model::AutoScrollDirection::Up),
                ..
            }
        ));
        let next_tab = state.new_tab();
        assert!(state.activate_tab(next_tab));
        assert!(matches!(
            state.drag_session().state(),
            explorer_model::DragSessionState::Idle
        ));
        assert_eq!(state.drop_target_row(), None);
    }

    #[test]
    fn shift_extended_selection_still_starts_one_left_drag_for_the_full_selection() {
        let mut state = state_with_rows();
        assert!(state.select_row(0));
        assert!(state.select_row_range(1, false));
        assert_eq!(state.tabs().active_tab().selection.len(), 2);
        assert!(state.begin_drag_candidate(10.0, 10.0, explorer_model::DragButton::Left));
        assert!(state.update_drag_pointer(14.0, 10.0));
        let command = state
            .take_pending_drag_command()
            .expect("Shift-extended selection remains draggable");
        let explorer_model::ExplorerCommand::DataTransfer {
            request:
                explorer_model::DataTransferRequest::BeginDrag {
                    items,
                    allowed_effects,
                    button,
                },
            ..
        } = command
        else {
            panic!("threshold crossing must queue one native drag");
        };
        assert_eq!(items.len(), 2);
        assert!(allowed_effects.copy);
        assert!(allowed_effects.move_item);
        assert_eq!(button, explorer_model::DragButton::Left);
        assert!(state.take_pending_drag_command().is_none());
    }

    #[test]
    fn left_drag_external_drop_routes_background_and_folder_to_typed_request() {
        let mut state = state_with_rows();
        state.queue_external_drop(
            vec![r"C:\outside\one.txt".into()],
            Some(0),
            explorer_model::DragEffect::Copy,
            false,
            explorer_model::TransferEffects::COPY,
        );
        let command = state
            .take_pending_drag_command()
            .expect("folder drop command");
        assert!(matches!(
            command,
            explorer_model::ExplorerCommand::DataTransfer {
                request: explorer_model::DataTransferRequest::DropExternal {
                    destination,
                    effect: explorer_model::DragEffect::Copy,
                    ..
                },
                ..
            } if destination == explorer_model::LocationDescriptor::file_system(r"C:\fixture\folder")
        ));

        state.queue_external_drop(
            vec![r"C:\outside\two.txt".into()],
            None,
            explorer_model::DragEffect::Move,
            false,
            explorer_model::TransferEffects::MOVE,
        );
        assert!(matches!(
            state.take_pending_drag_command(),
            Some(explorer_model::ExplorerCommand::DataTransfer {
                request: explorer_model::DataTransferRequest::DropExternal {
                    destination,
                    effect: explorer_model::DragEffect::Move,
                    conflict: explorer_model::ConflictDecision::Prompt,
                    ..
                },
                ..
            }) if destination == explorer_model::LocationDescriptor::file_system(r"C:\fixture")
        ));
    }

    #[test]
    fn left_drag_external_drop_rejects_self_descendant_and_same_parent_move_targets() {
        let mut state = state_with_rows();
        state.queue_external_drop(
            vec![r"C:\fixture\folder".into()],
            Some(0),
            explorer_model::DragEffect::Move,
            false,
            explorer_model::TransferEffects::MOVE,
        );
        assert!(state.take_pending_drag_command().is_none());

        state.queue_external_drop(
            vec![r"C:\fixture\file.txt".into()],
            None,
            explorer_model::DragEffect::Move,
            false,
            explorer_model::TransferEffects::MOVE,
        );
        assert!(state.take_pending_drag_command().is_none());

        state.queue_external_drop(
            vec![r"C:\fixture\file.txt".into()],
            None,
            explorer_model::DragEffect::Copy,
            false,
            explorer_model::TransferEffects::COPY,
        );
        assert!(matches!(
            state.take_pending_drag_command(),
            Some(explorer_model::ExplorerCommand::DataTransfer {
                request: explorer_model::DataTransferRequest::DropExternal {
                    effect: explorer_model::DragEffect::Copy,
                    ..
                },
                ..
            })
        ));
    }

    #[test]
    fn right_drag_requires_terminal_choice_and_rejects_changed_target_generation() {
        let mut state = state_with_rows();
        let allowed = explorer_model::TransferEffects {
            copy: true,
            move_item: true,
            link: false,
        };
        state.queue_external_drop(
            vec![r"C:\outside\one.txt".into()],
            Some(0),
            explorer_model::DragEffect::Move,
            true,
            allowed,
        );
        assert!(state.pending_right_drop().is_some());
        assert!(state.take_pending_drag_command().is_none());
        state.resolve_right_drop(explorer_model::DragEffect::None);
        assert!(state.take_pending_drag_command().is_none());

        state.queue_external_drop(
            vec![r"C:\outside\one.txt".into()],
            Some(0),
            explorer_model::DragEffect::Move,
            true,
            allowed,
        );
        state.resolve_right_drop(explorer_model::DragEffect::Move);
        assert!(matches!(
            state.take_pending_drag_command(),
            Some(explorer_model::ExplorerCommand::DataTransfer {
                request: explorer_model::DataTransferRequest::DropExternal {
                    effect: explorer_model::DragEffect::Move,
                    ..
                },
                ..
            })
        ));

        state.queue_external_drop(
            vec![r"C:\outside\one.txt".into()],
            Some(0),
            explorer_model::DragEffect::Copy,
            true,
            allowed,
        );
        let next = state.new_tab();
        assert!(state.activate_tab(next));
        state.resolve_right_drop(explorer_model::DragEffect::Copy);
        assert!(state.take_pending_drag_command().is_none());
    }

    #[test]
    fn context_menu_preserves_selected_multi_set_and_background_clears_it() {
        let mut state = state_with_rows();
        assert!(state.select_row(0));
        assert!(state.select_row_additive(1));
        let first_id = state.presentation_item_id(0).expect("first item identity");
        state.prepare_context_selection(Some(&first_id));
        assert_eq!(state.tabs().active_tab().selection.len(), 2);
        assert_eq!(
            state.tabs().active_tab().selection.focused(),
            Some(&first_id),
            "right-clicking an already selected item preserves the set but focuses the hit row"
        );
        assert!(state.begin_drag_candidate(0.0, 0.0, explorer_model::DragButton::Right));
        let command = state
            .begin_context_menu_request(Some(first_id), 42, 100, 200, 10.0, 20.0, false, false)
            .expect("context menu request");
        assert!(matches!(
            command,
            explorer_model::ExplorerCommand::ShowContextMenu {
                request: explorer_model::ContextMenuRequest {
                    target: explorer_model::ShellContextMenuTarget::Items { items, .. },
                    owner_window: 42,
                    point: explorer_model::MenuPoint { x: 100, y: 200 },
                    ..
                },
                ..
            } if items.len() == 2
        ));
        state.prepare_context_selection(None);
        assert!(state.tabs().active_tab().selection.is_empty());
    }

    #[test]
    fn navigation_context_regression_uses_hit_identity_and_never_first_row() {
        let mut state = state_with_rows();
        assert!(state.select_row(0));
        let first = state.presentation_item_id(0).expect("first identity");
        let clicked = state.presentation_item_id(1).expect("clicked identity");
        assert!(state.begin_context_item_gesture(clicked.clone(), 3.0, 4.0, true));
        assert!(state.pending_context_extended_verbs());
        assert_eq!(
            state.tabs().active_tab().selection.focused(),
            Some(&clicked)
        );
        let command = state
            // A GPUI re-render may deliver mouse-up-out through a stale row closure. The
            // mouse-down stable identity remains authoritative and must override it.
            .begin_context_menu_request(Some(first), 42, 10, 20, 1.0, 2.0, false, false)
            .expect("clicked item menu");
        assert!(!state.pending_context_extended_verbs());
        let explorer_model::ExplorerCommand::ShowContextMenu { request, .. } = command else {
            panic!("item menu command");
        };
        let explorer_model::ShellContextMenuTarget::Items { items, .. } = request.target else {
            panic!("item target");
        };
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].id, clicked);

        let mut stale_state = state_with_rows();
        assert!(stale_state.select_row(0));
        let missing_id =
            explorer_model::ShellItemId::from_provider_bytes(b"stale-right-click".to_vec())
                .expect("bounded stale identity");
        stale_state.prepare_context_selection(Some(&missing_id));
        assert!(
            stale_state
                .begin_context_menu_request(Some(missing_id), 42, 10, 20, 1.0, 2.0, false, false)
                .is_none()
        );
    }

    #[test]
    fn context_menu_failure_is_recoverable_and_rejects_stale_correlation() {
        let mut state = state_with_rows();
        let first = state
            .begin_context_menu_request(None, 42, 100, 200, 10.0, 20.0, false, false)
            .expect("first context menu");
        let explorer_model::ExplorerCommand::ShowContextMenu { context, .. } = first else {
            panic!("expected context-menu command");
        };
        let stale_context = explorer_model::RequestContext::new(context.tab_id, context.generation);
        assert_eq!(
            state.apply_service_event(explorer_model::ExplorerEvent::ContextMenuFinished {
                context: stale_context,
                outcome: explorer_model::ContextMenuOutcome::Cancelled,
            }),
            explorer_model::WindowEventOutcome::IgnoredStale
        );
        let error = explorer_common::ExplorerError::new(
            explorer_common::ExplorerErrorKind::Availability,
            "controlled context menu timeout",
            true,
            "內容功能表未回應，仍可繼續操作。",
            "correlation=fixture",
        );
        assert_eq!(
            state.apply_service_event(explorer_model::ExplorerEvent::ContextMenuFinished {
                context,
                outcome: explorer_model::ContextMenuOutcome::Failed {
                    error: error.clone(),
                },
            }),
            explorer_model::WindowEventOutcome::Applied
        );
        assert_eq!(state.context_menu_error(), Some(&error));

        let second = state
            .begin_context_menu_request(None, 42, 100, 200, 10.0, 20.0, false, false)
            .expect("UI remains available for a second menu");
        let explorer_model::ExplorerCommand::ShowContextMenu {
            context: second_context,
            ..
        } = second
        else {
            panic!("expected second context-menu command");
        };
        assert_eq!(
            state.apply_service_event(explorer_model::ExplorerEvent::ContextMenuFinished {
                context: second_context,
                outcome: explorer_model::ContextMenuOutcome::Cancelled,
            }),
            explorer_model::WindowEventOutcome::Applied
        );
        assert!(state.context_menu_error().is_none());
    }

    #[test]
    fn remote_context_menu_uses_client_coordinates_not_screen_coordinates() {
        let mut state = AppViewState::with_initial_location(explorer_model::HistoryEntry::new(
            explorer_model::LocationDescriptor::Virtual(
                explorer_model::VirtualLocationDescriptor {
                    provider_id: "adb".to_owned(),
                    public_authority: Some("device".to_owned()),
                    container_identity: [7; 16],
                    container_generation: 1,
                    entry_id: None,
                    components: vec!["sdcard".to_owned()],
                },
            ),
            "device",
        ));

        assert!(
            state
                .begin_context_menu_request(None, 42, 1_200, 900, 120.0, 80.0, false, false)
                .is_none(),
            "remote custom menus do not submit a Windows Shell command"
        );
        assert_eq!(
            state.remote_context_menu(),
            Some(super::RemoteContextMenuState {
                x: 120.0,
                y: 80.0,
                background: true,
                paste_available: false,
                item_row_index: None,
                item_is_container: false,
            })
        );
    }

    #[test]
    fn host_context_command_terminal_clears_pending_without_false_failure() {
        let mut state = state_with_rows();
        assert!(state.select_row(0));
        let selected = state.presentation_item_id(0).expect("selected identity");
        state.prepare_context_selection(Some(&selected));
        assert!(state.begin_drag_candidate(3.0, 4.0, explorer_model::DragButton::Right));
        let command = state
            .begin_context_menu_request(
                Some(selected.clone()),
                42,
                100,
                200,
                10.0,
                20.0,
                false,
                false,
            )
            .expect("context menu");
        let explorer_model::ExplorerCommand::ShowContextMenu { context, request } = command else {
            panic!("expected context-menu command");
        };
        let target = request.target;
        assert!(
            state.select_row(1),
            "simulate a selection race after popup dismissal"
        );
        assert!(state.restore_context_target_selection(&target));
        assert_eq!(
            state.tabs().active_tab().selection.focused(),
            Some(&selected)
        );
        assert_eq!(
            state.apply_service_event(explorer_model::ExplorerEvent::ContextMenuFinished {
                context,
                outcome: explorer_model::ContextMenuOutcome::Delegated {
                    command_offset: 4,
                    command: explorer_model::ContextMenuHostCommand::Delete,
                    target,
                },
            }),
            explorer_model::WindowEventOutcome::Applied
        );
        assert!(!state.context_menu_pending());
        assert!(state.context_menu_error().is_none());
    }

    #[test]
    fn delegated_properties_reuses_popup_target_after_selection_changes() {
        let mut state = state_with_rows();
        assert!(state.select_row(1));
        let selected = state.presentation_item_id(1).expect("selected identity");
        state.prepare_context_selection(Some(&selected));
        assert!(state.begin_drag_candidate(3.0, 4.0, explorer_model::DragButton::Right));
        let explorer_model::ExplorerCommand::ShowContextMenu { request, .. } = state
            .begin_context_menu_request(
                Some(selected.clone()),
                42,
                100,
                200,
                10.0,
                20.0,
                false,
                false,
            )
            .expect("context menu")
        else {
            panic!("expected context-menu command");
        };
        let popup_target = request.target;

        assert!(
            state.select_row(0),
            "simulate selection churn while popup closes"
        );
        let explorer_model::ExplorerCommand::ShowContextMenu { request, .. } = state
            .begin_properties_request_for_target(84, popup_target.clone())
            .expect("captured target can invoke Properties")
        else {
            panic!("Properties uses the Shell context command boundary");
        };

        assert_eq!(request.owner_window, 84);
        assert_eq!(request.requested_verb.as_deref(), Some("properties"));
        assert_eq!(request.target, popup_target);
        let explorer_model::ShellContextMenuTarget::Items { items, .. } = request.target else {
            panic!("Properties must keep the item target");
        };
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].id, selected);
        assert_ne!(
            state.tabs().active_tab().selection.focused(),
            Some(&selected),
            "the request must not be reconstructed from the current selection"
        );
    }

    #[test]
    fn pending_context_menu_cancels_then_promotes_the_latest_replacement_once() {
        let mut state = state_with_rows();
        let first = state
            .begin_context_menu_request(None, 42, 100, 200, 10.0, 20.0, false, false)
            .expect("first context menu");
        let explorer_model::ExplorerCommand::ShowContextMenu { context, .. } = first else {
            panic!("expected context-menu command");
        };
        assert!(state.context_menu_pending());
        let first_request_id = context.request_id;
        let cancel = state
            .begin_context_menu_request(None, 42, 300, 400, 30.0, 40.0, true, true)
            .expect("second right-click cancels the visible popup");
        assert!(context.cancellation.is_cancelled());
        assert!(matches!(
            cancel,
            explorer_model::ExplorerCommand::Cancel { request_id }
                if request_id == context.request_id
        ));
        assert_eq!(
            state.apply_service_event(explorer_model::ExplorerEvent::ContextMenuFinished {
                context,
                outcome: explorer_model::ContextMenuOutcome::Cancelled,
            }),
            explorer_model::WindowEventOutcome::Applied
        );
        assert!(state.context_menu_pending());
        let replacement = state
            .take_pending_context_menu_command()
            .expect("cancel terminal promotes the replacement menu");
        let explorer_model::ExplorerCommand::ShowContextMenu {
            context: replacement_context,
            request,
        } = replacement
        else {
            panic!("expected replacement context-menu command");
        };
        assert_ne!(replacement_context.request_id, first_request_id);
        assert_eq!(request.point, explorer_model::MenuPoint { x: 300, y: 400 });
        assert_eq!(
            request.invocation_profile,
            explorer_model::ContextMenuInvocationProfile::ExplorerExtended
        );
        assert!(state.take_pending_context_menu_command().is_none());
    }

    #[test]
    fn rapid_context_menu_replacement_keeps_latest_target_and_ignores_stale_terminals() {
        let mut state = state_with_rows();
        let first = state
            .begin_context_menu_request(None, 42, 100, 200, 10.0, 20.0, false, false)
            .expect("first context menu");
        let explorer_model::ExplorerCommand::ShowContextMenu {
            context: first_context,
            ..
        } = first
        else {
            panic!("expected context-menu command");
        };

        let second = state
            .begin_context_menu_request(None, 42, 300, 400, 30.0, 40.0, false, false)
            .expect("first mouse replacement");
        assert!(matches!(
            second,
            explorer_model::ExplorerCommand::ShowContextMenu { ref request, .. }
                if request.point == explorer_model::MenuPoint { x: 300, y: 400 }
        ));
        let latest = state
            .begin_context_menu_request(None, 42, 500, 600, 50.0, 60.0, false, true)
            .expect("latest mouse replacement");
        let explorer_model::ExplorerCommand::ShowContextMenu {
            context: latest_context,
            request: latest_request,
        } = latest
        else {
            panic!("expected immediate mouse replacement command");
        };
        assert_eq!(
            latest_request.point,
            explorer_model::MenuPoint { x: 500, y: 600 }
        );

        let stale_context = explorer_model::RequestContext::new(
            state.tabs().active_tab().id,
            state.tabs().active_tab().generation,
        );
        assert_eq!(
            state.apply_service_event(explorer_model::ExplorerEvent::ContextMenuFinished {
                context: stale_context,
                outcome: explorer_model::ContextMenuOutcome::Cancelled,
            }),
            explorer_model::WindowEventOutcome::IgnoredStale
        );
        assert!(state.take_pending_context_menu_command().is_none());

        assert_eq!(
            state.apply_service_event(explorer_model::ExplorerEvent::ContextMenuFinished {
                context: first_context.clone(),
                outcome: explorer_model::ContextMenuOutcome::Cancelled,
            }),
            explorer_model::WindowEventOutcome::IgnoredStale
        );
        assert!(state.context_menu_pending());
        assert!(state.take_pending_context_menu_command().is_none());

        assert_eq!(
            state.apply_service_event(explorer_model::ExplorerEvent::ContextMenuFinished {
                context: latest_context,
                outcome: explorer_model::ContextMenuOutcome::Cancelled,
            }),
            explorer_model::WindowEventOutcome::Applied
        );
        assert!(!state.context_menu_pending());
    }

    #[test]
    fn history_menu_orders_nearest_first_and_starts_exact_multi_step_request() {
        let current = explorer_model::HistoryEntry::new(
            explorer_model::LocationDescriptor::file_system(r"D:\current"),
            "current",
        );
        let mut state = AppViewState::with_initial_location(current.clone());
        state.tabs_mut().active_tab_mut().history =
            explorer_model::NavigationHistory::from_resolved_parts(
                vec![
                    explorer_model::HistoryEntry::new(
                        explorer_model::LocationDescriptor::file_system(r"D:\oldest"),
                        "oldest",
                    ),
                    explorer_model::HistoryEntry::new(
                        explorer_model::LocationDescriptor::file_system(r"D:\nearest"),
                        "nearest",
                    ),
                ],
                current,
                vec![explorer_model::HistoryEntry::new(
                    explorer_model::LocationDescriptor::file_system(r"D:\forward"),
                    "forward",
                )],
            );

        assert_eq!(
            state
                .navigation_history_entries(crate::actions::NavigationHistoryDirection::Back)
                .iter()
                .map(|entry| entry.display_title.as_str())
                .collect::<Vec<_>>(),
            ["nearest", "oldest"]
        );
        assert!(
            state.open_navigation_history_menu(crate::actions::NavigationHistoryDirection::Back)
        );
        assert!(state.set_navigation_history_focus(1));
        assert!(!state.set_navigation_history_focus(1));
        assert!(!state.set_navigation_history_focus(2));
        assert_eq!(state.navigation_history_menu_index(), 1);
        assert!(state.set_navigation_history_focus(0));
        assert!(state.move_navigation_history_focus(1));
        assert_eq!(state.navigation_history_menu_index(), 1);
        let explorer_model::ExplorerCommand::Navigate { location, .. } = state
            .begin_history_navigation(crate::actions::NavigationHistoryDirection::Back, 2)
            .expect("two-step Back request")
        else {
            panic!("history activation routes one Navigate command");
        };
        assert_eq!(
            location,
            explorer_model::LocationDescriptor::file_system(r"D:\oldest")
        );
        assert!(state.navigation_history_menu_direction().is_none());
    }

    #[test]
    fn navigation_availability_switches_with_active_tab_history() {
        let mut state = AppViewState::with_initial_location(explorer_model::HistoryEntry::new(
            explorer_model::LocationDescriptor::file_system(r"C:\fixture\A"),
            "A",
        ));
        let first = state.tabs().active_tab_id();
        state.tabs_mut().active_tab_mut().history.commit_navigation(
            explorer_model::HistoryEntry::new(
                explorer_model::LocationDescriptor::file_system(r"C:\fixture\B"),
                "B",
            ),
        );
        assert!(state.command_availability().is_enabled(CommandKind::Back));
        assert!(state.command_availability().is_enabled(CommandKind::Up));
        let second = state.new_tab();
        assert!(state.command_availability().is_enabled(CommandKind::Back));
        let inherited_current = state
            .tabs()
            .active_tab()
            .history
            .current()
            .expect("new tab inherits a current history entry")
            .clone();
        state.tabs_mut().active_tab_mut().history =
            explorer_model::NavigationHistory::with_initial(inherited_current);
        assert!(!state.command_availability().is_enabled(CommandKind::Back));
        assert!(state.activate_tab(first));
        assert!(state.command_availability().is_enabled(CommandKind::Back));
        assert!(state.activate_tab(second));
        assert!(!state.command_availability().is_enabled(CommandKind::Back));
    }

    #[test]
    #[allow(
        clippy::too_many_lines,
        reason = "one transactional navigation scenario covers Back success, Forward failure, and Up routing"
    )]
    fn back_forward_and_up_create_real_commands_and_commit_only_on_success() {
        let mut state = AppViewState::with_initial_location(explorer_model::HistoryEntry::new(
            explorer_model::LocationDescriptor::file_system(r"C:\fixture\A"),
            "A",
        ));
        state.tabs_mut().active_tab_mut().history.commit_navigation(
            explorer_model::HistoryEntry::new(
                explorer_model::LocationDescriptor::file_system(r"C:\fixture\B"),
                "B",
            ),
        );
        let back = state.begin_back_navigation().expect("Back command");
        let explorer_model::ExplorerCommand::Navigate {
            context: back_context,
            location: back_location,
        } = back
        else {
            panic!("expected Back navigation");
        };
        assert_eq!(
            back_location,
            explorer_model::LocationDescriptor::file_system(r"C:\fixture\A")
        );
        assert_eq!(
            state
                .tabs()
                .active_tab()
                .history
                .current()
                .expect("current before resolve")
                .display_title,
            "B"
        );
        assert_eq!(
            state.apply_service_event(explorer_model::ExplorerEvent::LocationResolved {
                context: back_context,
                metadata: explorer_model::LocationMetadata {
                    descriptor: back_location,
                    display_title: "A resolved".to_owned(),
                    can_go_up: true,
                    can_write: true,
                },
            }),
            explorer_model::WindowEventOutcome::Applied
        );
        assert_eq!(
            state
                .tabs()
                .active_tab()
                .history
                .current()
                .expect("Back committed")
                .display_title,
            "A resolved"
        );

        let forward = state.begin_forward_navigation().expect("Forward command");
        let explorer_model::ExplorerCommand::Navigate {
            context: forward_context,
            location: forward_location,
        } = forward
        else {
            panic!("expected Forward navigation");
        };
        let failure = explorer_common::ExplorerError::new(
            explorer_common::ExplorerErrorKind::Availability,
            "forward fixture",
            true,
            "無法前進",
            "controlled failure",
        );
        assert_eq!(
            state.apply_service_event(explorer_model::ExplorerEvent::Failed {
                context: forward_context,
                error: failure,
            }),
            explorer_model::WindowEventOutcome::Applied
        );
        assert_eq!(
            state
                .tabs()
                .active_tab()
                .history
                .current()
                .expect("failed Forward preserves history")
                .display_title,
            "A resolved"
        );
        assert_eq!(
            forward_location,
            explorer_model::LocationDescriptor::file_system(r"C:\fixture\B")
        );
        assert!(
            state
                .command_availability()
                .is_enabled(CommandKind::Forward)
        );

        let up = state.begin_up_navigation().expect("Up command");
        assert!(matches!(
            up,
            explorer_model::ExplorerCommand::Navigate { location, .. }
                if location == explorer_model::LocationDescriptor::file_system(r"C:\fixture")
        ));
    }

    #[test]
    fn address_submission_never_falls_through_to_search_or_commits_invalid_history() {
        let initial = explorer_model::LocationDescriptor::file_system(r"C:\fixture");
        let mut state = AppViewState::with_initial_location(explorer_model::HistoryEntry::new(
            initial.clone(),
            "fixture",
        ));
        state.enter_address_edit();
        assert!(
            state
                .begin_address_submission("name:report type:pdf")
                .is_none()
        );
        assert_eq!(
            state
                .tabs()
                .active_tab()
                .history
                .current()
                .map(|entry| &entry.location),
            Some(&initial)
        );
        assert!(matches!(
            state.tabs().active_tab().search,
            explorer_model::TabSearchState::Idle
        ));
        assert!(matches!(
            state.tabs().active_tab().view.address.mode,
            explorer_model::AddressBarMode::NavigationError
        ));

        state.enter_address_edit();
        let command = state
            .begin_address_submission(r"D:\valid")
            .expect("absolute path starts typed navigation");
        let explorer_model::ExplorerCommand::Navigate { context, location } = command else {
            panic!("address submission must reuse Navigate");
        };
        assert_eq!(
            state
                .tabs()
                .active_tab()
                .history
                .current()
                .map(|entry| &entry.location),
            Some(&initial),
            "history commits only after resolution"
        );
        assert_eq!(
            location,
            explorer_model::LocationDescriptor::file_system(r"D:\valid")
        );
        assert_eq!(
            state.apply_service_event(explorer_model::ExplorerEvent::LocationResolved {
                context,
                metadata: explorer_model::LocationMetadata {
                    descriptor: location.clone(),
                    display_title: "valid".to_owned(),
                    can_go_up: true,
                    can_write: true,
                },
            }),
            explorer_model::WindowEventOutcome::Applied
        );
        assert_eq!(
            state
                .tabs()
                .active_tab()
                .history
                .current()
                .map(|entry| &entry.location),
            Some(&location)
        );
        assert!(matches!(
            state.tabs().active_tab().view.address.mode,
            explorer_model::AddressBarMode::Browsing
        ));
    }

    #[test]
    fn up_from_virtual_archive_root_uses_the_resolved_filesystem_parent() {
        let virtual_root = explorer_model::LocationDescriptor::try_virtual(
            "rust-7z",
            [7; 16],
            1,
            None,
            Vec::new(),
        )
        .unwrap();
        let filesystem_parent = explorer_model::LocationDescriptor::file_system(r"D:\fixture");
        let mut state = AppViewState::with_initial_location(explorer_model::HistoryEntry::new(
            virtual_root.clone(),
            "fixture.7z",
        ));
        state.tabs.active_tab_mut().view.address.resolved_ancestry = vec![
            explorer_model::BreadcrumbSegment {
                id: explorer_model::BreadcrumbSegmentId(1),
                display_name: "fixture".to_owned(),
                location: filesystem_parent.clone(),
                icon_hint: explorer_model::BreadcrumbIconHint::Folder,
                is_container: true,
            },
            explorer_model::BreadcrumbSegment {
                id: explorer_model::BreadcrumbSegmentId(2),
                display_name: "fixture.7z".to_owned(),
                location: virtual_root,
                icon_hint: explorer_model::BreadcrumbIconHint::Archive,
                is_container: true,
            },
        ];

        assert!(matches!(
            state.begin_up_navigation(),
            Some(explorer_model::ExplorerCommand::Navigate { location, .. })
                if location == filesystem_parent
        ));
    }

    #[test]
    fn writable_virtual_entry_with_rename_capability_opens_inline_editor() {
        let virtual_root = explorer_model::LocationDescriptor::try_virtual(
            "rust-7z",
            [9; 16],
            1,
            None,
            Vec::new(),
        )
        .unwrap();
        let entry_location = explorer_model::LocationDescriptor::try_virtual(
            "rust-7z",
            [9; 16],
            1,
            Some(42),
            vec!["hello.txt".to_owned()],
        )
        .unwrap();
        let mut state = AppViewState::with_initial_location(explorer_model::HistoryEntry::new(
            virtual_root.clone(),
            "fixture.7z",
        ));
        let command = state.begin_active_location_load().unwrap();
        let context = command.context().unwrap().clone();
        assert_eq!(
            state.apply_service_event(explorer_model::ExplorerEvent::LocationResolved {
                context: context.clone(),
                metadata: explorer_model::LocationMetadata {
                    descriptor: virtual_root,
                    display_title: "fixture.7z".to_owned(),
                    can_go_up: true,
                    can_write: true,
                },
            }),
            explorer_model::WindowEventOutcome::Applied
        );
        let entry = explorer_model::FileEntry {
            id: explorer_model::ShellItemId::from_provider_bytes([42]).unwrap(),
            display_name: "hello.txt".to_owned(),
            location: entry_location,
            is_container: false,
            metadata: explorer_model::FileEntryMetadata {
                namespace_capabilities: explorer_model::NamespaceCapabilities::from_public_bits(
                    explorer_model::NamespaceCapabilities::OPEN
                        | explorer_model::NamespaceCapabilities::RENAME
                        | explorer_model::NamespaceCapabilities::DELETE,
                ),
                ..explorer_model::FileEntryMetadata::default()
            },
        };
        assert_eq!(
            state.apply_service_event(explorer_model::ExplorerEvent::DirectoryBatch {
                context: context.clone(),
                entries: vec![entry],
            }),
            explorer_model::WindowEventOutcome::Applied
        );
        assert_eq!(
            state.apply_service_event(explorer_model::ExplorerEvent::DirectoryFinished { context }),
            explorer_model::WindowEventOutcome::Applied
        );
        assert!(state.select_row(0));
        assert!(state.row_namespace_command_enabled(0, explorer_model::NamespaceCommand::Rename));
        assert!(state.begin_focused_inline_rename());
        assert_eq!(state.rename_editor().unwrap().buffer, "hello.txt");
    }

    #[test]
    fn resolved_known_folder_address_displays_and_resubmits_canonical_path() {
        let requested =
            explorer_model::LocationDescriptor::ParsingName("shell:Personal".to_owned());
        let canonical =
            explorer_model::LocationDescriptor::file_system(r"C:\Users\fixture\Documents");
        let mut state = AppViewState::with_initial_location(explorer_model::HistoryEntry::new(
            requested,
            "Documents",
        ));
        let command = state
            .begin_active_location_load()
            .expect("initial known-folder load");
        let explorer_model::ExplorerCommand::Navigate { context, .. } = command else {
            panic!("initial location load must navigate");
        };
        assert_eq!(
            state.apply_service_event(explorer_model::ExplorerEvent::LocationResolved {
                context,
                metadata: explorer_model::LocationMetadata {
                    descriptor: canonical.clone(),
                    display_title: "Documents".to_owned(),
                    can_go_up: true,
                    can_write: true,
                },
            }),
            explorer_model::WindowEventOutcome::Applied
        );

        assert_eq!(state.address_draft(), r"C:\Users\fixture\Documents");
        state.enter_address_edit();
        let copied = state.address_draft().to_owned();
        let command = state
            .begin_address_submission(&copied)
            .expect("canonical path resubmits");
        assert!(matches!(
            command,
            explorer_model::ExplorerCommand::Navigate { location, .. }
                if location == canonical
        ));
    }

    #[test]
    fn clear_search_cancels_work_restores_directory_and_keeps_history_per_tab() {
        let mut state = state_with_rows();
        let first = state.tabs().active_tab_id();
        let first_command = state
            .begin_active_search("報告".to_owned())
            .expect("search starts");
        let first_context = first_command.context().expect("search context").clone();
        assert_eq!(state.tabs().active_tab().search_history, ["報告"]);
        state.leave_active_search();
        assert!(first_context.cancellation.is_cancelled());
        assert!(matches!(
            state.tabs().active_tab().search,
            explorer_model::TabSearchState::Idle
        ));
        assert!(state.tabs().active_tab().visible_snapshot().is_some());
        assert_eq!(state.tabs().active_tab().search_history, ["報告"]);

        let second = state.new_tab();
        assert_ne!(first, second);
        assert!(state.tabs().active_tab().search_history.is_empty());
        let _ = state.begin_active_search("第二頁".to_owned());
        assert_eq!(state.tabs().active_tab().search_history, ["第二頁"]);
        assert!(state.activate_tab(first));
        assert_eq!(state.tabs().active_tab().search_history, ["報告"]);
    }

    #[test]
    fn deleting_all_search_text_cancels_the_generation_and_restores_the_directory() {
        let mut state = state_with_rows();
        let directory_names = state
            .tabs()
            .active_tab()
            .visible_snapshot()
            .expect("directory snapshot")
            .entries()
            .iter()
            .map(|entry| entry.display_name.clone())
            .collect::<Vec<_>>();
        let search = state
            .update_active_search_text("needle".to_owned())
            .expect("search command");
        let context = search.context().expect("search context").clone();
        assert!(matches!(
            state.tabs().active_tab().search,
            explorer_model::TabSearchState::Loading { .. }
        ));

        assert!(state.update_active_search_text(String::new()).is_none());
        assert!(context.cancellation.is_cancelled());
        assert!(matches!(
            &state.tabs().active_tab().search,
            explorer_model::TabSearchState::Editing(input) if input.is_empty()
        ));
        let restored_names = state
            .tabs()
            .active_tab()
            .visible_snapshot()
            .expect("restored directory snapshot")
            .entries()
            .iter()
            .map(|entry| entry.display_name.clone())
            .collect::<Vec<_>>();
        assert_eq!(restored_names, directory_names);
    }

    #[test]
    fn breadcrumb_protocol_merges_identity_updates_batches_and_rejects_stale_events() {
        let mut state = AppViewState::with_initial_location(explorer_model::HistoryEntry::new(
            explorer_model::LocationDescriptor::file_system(r"D:\fixture"),
            "fixture",
        ));
        let tab_id = state.tabs().active_tab_id();
        let generation = state.tabs().active_tab().generation;
        let source = explorer_model::RequestContext::new(tab_id, generation);
        let command = state
            .begin_ancestry_request(
                &source,
                explorer_model::LocationDescriptor::file_system(r"D:\fixture"),
            )
            .expect("ancestry command");
        let context = command.context().expect("context").clone();
        let early = vec![
            explorer_model::BreadcrumbSegment {
                id: explorer_model::BreadcrumbSegmentId(0),
                display_name: "本機".into(),
                location: explorer_model::LocationDescriptor::ParsingName(
                    "shell:MyComputerFolder".into(),
                ),
                icon_hint: explorer_model::BreadcrumbIconHint::Computer,
                is_container: true,
            },
            explorer_model::BreadcrumbSegment {
                id: explorer_model::BreadcrumbSegmentId(1),
                display_name: "D:".into(),
                location: explorer_model::LocationDescriptor::file_system(r"D:\"),
                icon_hint: explorer_model::BreadcrumbIconHint::Drive,
                is_container: true,
            },
        ];
        assert_eq!(
            state.apply_service_event(explorer_model::ExplorerEvent::AncestryBatch {
                context: context.clone(),
                segments: early.clone(),
            }),
            explorer_model::WindowEventOutcome::Applied
        );
        let mut enriched = early;
        enriched[1].display_name = "新增磁碟區 (D:)".into();
        assert_eq!(
            state.apply_service_event(explorer_model::ExplorerEvent::AncestryBatch {
                context: context.clone(),
                segments: enriched,
            }),
            explorer_model::WindowEventOutcome::Applied
        );
        assert_eq!(
            state.tabs().active_tab().view.address.resolved_ancestry[1].id,
            explorer_model::BreadcrumbSegmentId(1)
        );
        assert_eq!(
            state.tabs().active_tab().view.address.resolved_ancestry[1].display_name,
            "D:",
            "late Shell volume metadata must not resize or rename a drive breadcrumb"
        );
        let stale_context = explorer_model::RequestContext::new(tab_id, generation);
        assert_eq!(
            state.apply_service_event(explorer_model::ExplorerEvent::AncestryFinished {
                context: stale_context,
                outcome: explorer_model::BreadcrumbTerminal::Finished,
            }),
            explorer_model::WindowEventOutcome::IgnoredStale
        );
        assert_eq!(
            state.apply_service_event(explorer_model::ExplorerEvent::AncestryFinished {
                context,
                outcome: explorer_model::BreadcrumbTerminal::Finished,
            }),
            explorer_model::WindowEventOutcome::Applied
        );

        let segment_id = explorer_model::BreadcrumbSegmentId(1);
        let menu_generation = state
            .open_address_menu(segment_id)
            .expect("menu generation");
        let menu = state.begin_child_container_request().expect("menu command");
        let menu_context = menu.context().expect("menu context").clone();
        let child = explorer_model::BreadcrumbMenuItem {
            display_name: "子資料夾".into(),
            location: explorer_model::LocationDescriptor::file_system(r"D:\子資料夾"),
        };
        assert_eq!(
            state.apply_service_event(explorer_model::ExplorerEvent::ChildContainersBatch {
                context: menu_context.clone(),
                segment_id,
                menu_generation,
                children: vec![child.clone(), child],
            }),
            explorer_model::WindowEventOutcome::Applied
        );
        assert_eq!(
            state.tabs().active_tab().view.address.menu_children.len(),
            1
        );
        assert_eq!(
            state.apply_service_event(explorer_model::ExplorerEvent::ChildContainersFinished {
                context: menu_context,
                segment_id,
                menu_generation,
                outcome: explorer_model::BreadcrumbTerminal::Partial(
                    explorer_common::ExplorerError::new(
                        explorer_common::ExplorerErrorKind::Availability,
                        "fixture menu",
                        true,
                        "部分資料夾無法列出。",
                        "fixture partial",
                    ),
                ),
            }),
            explorer_model::WindowEventOutcome::Applied
        );
        assert_eq!(
            state.tabs().active_tab().view.address.menu_children.len(),
            1
        );
        assert_eq!(
            state.tabs().active_tab().view.address.menu_error.as_deref(),
            Some("部分資料夾無法列出。")
        );
    }

    #[test]
    fn breadcrumb_menu_presentation_covers_loading_ready_empty_cancel_error_and_late_events() {
        let outcomes = [
            (explorer_model::BreadcrumbTerminal::Finished, false),
            (explorer_model::BreadcrumbTerminal::Empty, false),
            (explorer_model::BreadcrumbTerminal::Cancelled, false),
            (
                explorer_model::BreadcrumbTerminal::Partial(explorer_common::ExplorerError::new(
                    explorer_common::ExplorerErrorKind::Availability,
                    "partial fixture",
                    true,
                    "部分資料夾無法列出。",
                    "fixture partial",
                )),
                true,
            ),
            (
                explorer_model::BreadcrumbTerminal::Failed(explorer_common::ExplorerError::new(
                    explorer_common::ExplorerErrorKind::Availability,
                    "failed fixture",
                    true,
                    "無法列出資料夾。",
                    "fixture failure",
                )),
                true,
            ),
        ];
        for (outcome, expects_error) in outcomes {
            let mut state = AppViewState::with_initial_location(explorer_model::HistoryEntry::new(
                explorer_model::LocationDescriptor::file_system(r"D:\fixture"),
                "fixture",
            ));
            let segment_id = explorer_model::BreadcrumbSegmentId(0);
            let menu_generation = state.open_address_menu(segment_id).expect("open menu");
            assert!(state.tabs().active_tab().view.address.menu_loading);
            let command = state.begin_child_container_request().expect("menu request");
            let context = command.context().expect("context").clone();
            if !matches!(
                outcome,
                explorer_model::BreadcrumbTerminal::Empty
                    | explorer_model::BreadcrumbTerminal::Cancelled
            ) {
                assert_eq!(
                    state.apply_service_event(
                        explorer_model::ExplorerEvent::ChildContainersBatch {
                            context: context.clone(),
                            segment_id,
                            menu_generation,
                            children: vec![explorer_model::BreadcrumbMenuItem {
                                display_name: "child".to_owned(),
                                location: explorer_model::LocationDescriptor::file_system(
                                    r"D:\fixture\child",
                                ),
                            }],
                        },
                    ),
                    explorer_model::WindowEventOutcome::Applied
                );
            }
            assert_eq!(
                state.apply_service_event(explorer_model::ExplorerEvent::ChildContainersFinished {
                    context: context.clone(),
                    segment_id,
                    menu_generation,
                    outcome,
                },),
                explorer_model::WindowEventOutcome::Applied
            );
            let address = &state.tabs().active_tab().view.address;
            assert!(!address.menu_loading);
            assert_eq!(address.menu_error.is_some(), expects_error);
            if !address.menu_children.is_empty() {
                assert_eq!(address.menu_children[0].display_name, "child");
            }
            assert_eq!(
                state.apply_service_event(explorer_model::ExplorerEvent::ChildContainersFinished {
                    context,
                    segment_id,
                    menu_generation,
                    outcome: explorer_model::BreadcrumbTerminal::Empty,
                },),
                explorer_model::WindowEventOutcome::IgnoredStale
            );
        }
    }

    #[test]
    fn breadcrumb_requests_cancel_on_navigation_menu_close_tab_switch_and_tab_close() {
        let mut state = AppViewState::with_initial_location(explorer_model::HistoryEntry::new(
            explorer_model::LocationDescriptor::file_system(r"D:\fixture"),
            "fixture",
        ));
        let tab_id = state.tabs().active_tab_id();
        let source =
            explorer_model::RequestContext::new(tab_id, state.tabs().active_tab().generation);
        let ancestry = state
            .begin_ancestry_request(
                &source,
                explorer_model::LocationDescriptor::file_system(r"D:\fixture"),
            )
            .expect("ancestry request");
        let ancestry_context = ancestry.context().expect("context").clone();
        let _navigation = state
            .begin_active_navigation(
                explorer_model::LocationDescriptor::file_system(r"D:\fixture\next"),
                false,
            )
            .expect("navigation");
        assert!(ancestry_context.cancellation.is_cancelled());

        let segment_id = explorer_model::BreadcrumbSegmentId(0);
        state.open_address_menu(segment_id).expect("open menu");
        let menu = state.begin_child_container_request().expect("menu request");
        let menu_context = menu.context().expect("menu context").clone();
        state.close_address_menu();
        assert!(menu_context.cancellation.is_cancelled());

        state.open_address_menu(segment_id).expect("reopen menu");
        let switched_menu = state
            .begin_child_container_request()
            .expect("switch menu request");
        let switched_context = switched_menu.context().expect("context").clone();
        let second = state.new_tab();
        assert!(state.activate_tab(tab_id));
        assert!(state.activate_tab(second));
        assert!(switched_context.cancellation.is_cancelled());

        let second_source =
            explorer_model::RequestContext::new(second, state.tabs().active_tab().generation);
        let closing = state
            .begin_ancestry_request(
                &second_source,
                explorer_model::LocationDescriptor::file_system(r"D:\fixture"),
            )
            .expect("closing ancestry");
        let closing_context = closing.context().expect("context").clone();
        assert_eq!(
            state.close_tab(second),
            explorer_model::TabCloseOutcome::Closed
        );
        assert!(closing_context.cancellation.is_cancelled());
    }

    #[test]
    fn two_tab_address_drafts_and_concurrent_menu_requests_reject_late_events() {
        let mut state = AppViewState::with_initial_location(explorer_model::HistoryEntry::new(
            explorer_model::LocationDescriptor::file_system(r"C:\fixture-a"),
            "fixture-a",
        ));
        let first = state.tabs().active_tab_id();
        state.enter_address_edit();
        assert!(state.update_address_edit_input(r"C:\draft-a".to_owned()));
        let segment_id = explorer_model::BreadcrumbSegmentId(0);
        let first_menu_generation = state.open_address_menu(segment_id).expect("first menu");
        let first_menu = state
            .begin_child_container_request()
            .expect("first menu request");
        let first_context = first_menu.context().expect("first context").clone();

        let second = state.new_tab();
        let _ = state.take_pending_new_tab_command();
        state.tabs_mut().active_tab_mut().history.commit_navigation(
            explorer_model::HistoryEntry::new(
                explorer_model::LocationDescriptor::file_system(r"D:\fixture-b"),
                "fixture-b",
            ),
        );
        state.enter_address_edit();
        assert!(state.update_address_edit_input(r"D:\draft-b".to_owned()));
        let second_menu_generation = state.open_address_menu(segment_id).expect("second menu");
        let second_menu = state
            .begin_child_container_request()
            .expect("second menu request");
        let second_context = second_menu.context().expect("second context").clone();
        assert!(!first_context.cancellation.is_cancelled());
        assert!(!second_context.cancellation.is_cancelled());

        assert!(state.activate_tab(first));
        assert!(second_context.cancellation.is_cancelled());
        assert_eq!(state.address_draft(), r"C:\draft-a");
        assert_eq!(
            state.apply_service_event(explorer_model::ExplorerEvent::ChildContainersBatch {
                context: first_context.clone(),
                segment_id,
                menu_generation: first_menu_generation,
                children: vec![explorer_model::BreadcrumbMenuItem {
                    display_name: "child-a".to_owned(),
                    location: explorer_model::LocationDescriptor::file_system(
                        r"C:\fixture-a\child-a",
                    ),
                }],
            }),
            explorer_model::WindowEventOutcome::Applied
        );

        assert!(state.activate_tab(second));
        assert!(first_context.cancellation.is_cancelled());
        assert_eq!(state.address_draft(), r"D:\draft-b");
        assert_eq!(
            state.apply_service_event(explorer_model::ExplorerEvent::ChildContainersFinished {
                context: second_context.clone(),
                segment_id,
                menu_generation: second_menu_generation,
                outcome: explorer_model::BreadcrumbTerminal::Finished,
            }),
            explorer_model::WindowEventOutcome::IgnoredStale
        );
        assert_eq!(
            state.close_tab(second),
            explorer_model::TabCloseOutcome::Closed
        );
        assert_eq!(state.tabs().active_tab_id(), first);
        assert_eq!(state.address_draft(), r"C:\draft-a");
        assert_eq!(
            state.apply_service_event(explorer_model::ExplorerEvent::ChildContainersFinished {
                context: second_context,
                segment_id,
                menu_generation: second_menu_generation,
                outcome: explorer_model::BreadcrumbTerminal::Finished,
            }),
            explorer_model::WindowEventOutcome::IgnoredStale
        );
    }

    #[test]
    fn clicking_the_same_breadcrumb_chevron_toggles_one_menu_session() {
        let mut state = AppViewState::with_initial_location(explorer_model::HistoryEntry::new(
            explorer_model::LocationDescriptor::file_system(r"D:\fixture"),
            "fixture",
        ));
        let root = explorer_model::BreadcrumbSegmentId(0);
        assert_eq!(state.open_address_menu(root), Some(1));
        let command = state
            .begin_child_container_request()
            .expect("first menu request");
        let context = command.context().expect("context").clone();
        assert_eq!(state.open_address_menu(root), None);
        assert!(context.cancellation.is_cancelled());
        assert!(matches!(
            state.tabs().active_tab().view.address.mode,
            explorer_model::AddressBarMode::Browsing
        ));
        assert!(state.begin_child_container_request().is_none());
        assert_eq!(state.open_address_menu(root), Some(2));
    }

    #[test]
    fn rows_create_folder_navigation_or_default_file_open_commands() {
        let mut current = state_with_rows();
        let command = current
            .open_row_command(0, false)
            .expect("current-tab folder");
        let context = command.context().expect("folder context").clone();
        assert!(matches!(
            &command,
            explorer_model::ExplorerCommand::Navigate { location, .. }
                if location.path() == Some(std::path::Path::new(r"C:\fixture\folder"))
        ));
        assert_eq!(
            current.apply_service_event(explorer_model::ExplorerEvent::LocationResolved {
                context,
                metadata: explorer_model::LocationMetadata {
                    descriptor: explorer_model::LocationDescriptor::file_system(
                        r"C:\fixture\folder",
                    ),
                    display_title: "folder".to_owned(),
                    can_go_up: true,
                    can_write: true,
                },
            }),
            explorer_model::WindowEventOutcome::Applied
        );
        let tab = current.tabs().active_tab();
        assert_eq!(
            tab.history
                .current()
                .and_then(|entry| entry.location.path()),
            Some(std::path::Path::new(r"C:\fixture\folder"))
        );
        assert!(tab.history.can_go_back());
        assert_eq!(tab.view.address.draft, r"C:\fixture\folder");
        assert_eq!(
            tab.view
                .address
                .resolved_ancestry
                .last()
                .map(|segment| segment.display_name.as_str()),
            Some("folder")
        );

        let mut new_tab = state_with_rows();
        let first = new_tab.tabs().active_tab_id();
        let command = new_tab.open_row_command(0, true).expect("new-tab folder");
        assert_ne!(new_tab.tabs().active_tab_id(), first);
        assert!(matches!(
            command,
            explorer_model::ExplorerCommand::Navigate { .. }
        ));

        let mut file = state_with_rows();
        let before_count = file.active_presentation().item_count;
        let command = file.open_row_command(1, false).expect("file open");
        assert!(matches!(
            command,
            explorer_model::ExplorerCommand::OpenItem {
                disposition: explorer_model::OpenDisposition::DefaultApplication,
                ..
            }
        ));
        assert_eq!(file.active_presentation().item_count, before_count);

        let mut nested = state_with_rows();
        let nested_folder = nested
            .open_extension_view_item_command(
                explorer_model::ShellItemId::from_provider_bytes([9]).unwrap(),
                explorer_model::LocationDescriptor::file_system(r"C:\fixture\folder\nested"),
                true,
                false,
            )
            .expect("nested folder navigation");
        assert!(matches!(
            nested_folder,
            explorer_model::ExplorerCommand::Navigate { location, .. }
                if location.path() == Some(std::path::Path::new(r"C:\fixture\folder\nested"))
        ));
        let nested_file = nested
            .open_extension_view_item_command(
                explorer_model::ShellItemId::from_provider_bytes([10]).unwrap(),
                explorer_model::LocationDescriptor::file_system(
                    r"C:\fixture\folder\nested\child.txt",
                ),
                false,
                false,
            )
            .expect("nested file open");
        assert!(matches!(
            nested_file,
            explorer_model::ExplorerCommand::OpenItem {
                disposition: explorer_model::OpenDisposition::DefaultApplication,
                ..
            }
        ));
    }

    #[test]
    fn remote_directory_links_navigate_but_invalid_links_remain_selected() {
        let root_location = explorer_model::RemoteAddress::parse("adb://device-9/data")
            .unwrap()
            .to_deterministic_location(1)
            .unwrap();
        let mut state = AppViewState::with_initial_location(explorer_model::HistoryEntry::new(
            root_location.clone(),
            "data",
        ));
        let command = state.begin_active_location_load().expect("remote load");
        let context = command.context().expect("context").clone();
        let _ = state.apply_service_event(explorer_model::ExplorerEvent::LocationResolved {
            context: context.clone(),
            metadata: explorer_model::LocationMetadata {
                descriptor: root_location,
                display_title: "data".to_owned(),
                can_go_up: true,
                can_write: true,
            },
        });
        let link_location = explorer_model::RemoteAddress::parse("adb://device-9/data/a-folder")
            .unwrap()
            .to_deterministic_location(1)
            .unwrap();
        let broken_id = explorer_model::ShellItemId::from_provider_bytes([22]).unwrap();
        let circular_id = explorer_model::ShellItemId::from_provider_bytes([23]).unwrap();
        let entries = vec![
            explorer_model::FileEntry {
                id: explorer_model::ShellItemId::from_provider_bytes([21]).unwrap(),
                display_name: "a-folder".to_owned(),
                location: link_location.clone(),
                is_container: true,
                metadata: explorer_model::FileEntryMetadata {
                    type_display: Some("Remote folder link".to_owned()),
                    ..Default::default()
                },
            },
            explorer_model::FileEntry {
                id: broken_id.clone(),
                display_name: "b-broken".to_owned(),
                location: explorer_model::RemoteAddress::parse("adb://device-9/data/b-broken")
                    .unwrap()
                    .to_deterministic_location(1)
                    .unwrap(),
                is_container: false,
                metadata: explorer_model::FileEntryMetadata {
                    type_display: Some("Broken remote link".to_owned()),
                    ..Default::default()
                },
            },
            explorer_model::FileEntry {
                id: circular_id.clone(),
                display_name: "c-cycle".to_owned(),
                location: explorer_model::RemoteAddress::parse("adb://device-9/data/c-cycle")
                    .unwrap()
                    .to_deterministic_location(1)
                    .unwrap(),
                is_container: false,
                metadata: explorer_model::FileEntryMetadata {
                    type_display: Some("Circular remote link".to_owned()),
                    ..Default::default()
                },
            },
        ];
        let _ = state.apply_service_event(explorer_model::ExplorerEvent::DirectoryBatch {
            context: context.clone(),
            entries,
        });
        let _ =
            state.apply_service_event(explorer_model::ExplorerEvent::DirectoryFinished { context });

        assert!(state.select_row(1));
        assert!(state.open_row_command(1, false).is_none());
        assert!(state.tabs().active_tab().selection.contains(&broken_id));

        assert!(state.select_row(2));
        assert!(state.open_row_command(2, false).is_none());
        assert!(state.tabs().active_tab().selection.contains(&circular_id));

        assert!(matches!(
            state.open_row_command(0, false),
            Some(explorer_model::ExplorerCommand::Navigate { location, .. })
                if location == link_location
        ));
    }

    #[test]
    fn watcher_overflow_starts_correlated_refresh_and_rejects_stale_generation() {
        let mut state = state_with_rows();
        let tab_id = state.tabs().active_tab_id();
        let generation = state.tabs().active_tab().generation;
        let event = explorer_model::ExplorerEvent::DirectoryChanged {
            tab_id,
            generation,
            changes: vec![explorer_model::DirectoryDelta::Overflow],
        };
        let command = state
            .watcher_recovery_command(&event)
            .expect("overflow refresh command");
        assert!(matches!(
            command,
            explorer_model::ExplorerCommand::Refresh { .. }
        ));
        assert_eq!(state.active_presentation().item_count, 2);
        assert!(
            state
                .watcher_recovery_command(&explorer_model::ExplorerEvent::DirectoryChanged {
                    tab_id,
                    generation,
                    changes: vec![explorer_model::DirectoryDelta::Overflow],
                })
                .is_none()
        );
    }

    #[test]
    fn watcher_item_change_uses_refresh_snapshot_dedupe_path() {
        let mut state = state_with_rows();
        let tab = state.tabs().active_tab();
        let event = explorer_model::ExplorerEvent::DirectoryChanged {
            tab_id: tab.id,
            generation: tab.generation,
            changes: vec![explorer_model::DirectoryDelta::Remove(
                explorer_model::ShellItemId::from_provider_bytes([2]).unwrap(),
            )],
        };
        let command = state
            .watcher_recovery_command(&event)
            .expect("normal watcher delta refreshes through stable-ID snapshot");
        assert!(matches!(
            command,
            explorer_model::ExplorerCommand::Refresh { .. }
        ));
        assert_eq!(state.active_presentation().item_count, 2);
    }

    #[test]
    fn operation_center_tracks_progress_terminal_and_rejects_late_progress() {
        let mut state = AppViewState::default();
        let request = explorer_model::FileOperationRequest {
            kind: explorer_model::FileOperationKind::CreateFolder {
                parent: explorer_model::LocationDescriptor::file_system(r"C:\fixture"),
                name: "new".to_owned(),
            },
            flags: explorer_model::FileOperationFlags::default(),
        };
        let command = state.begin_file_operation(request);
        let context = command.context().expect("operation context").clone();
        assert_eq!(
            state.apply_service_event(explorer_model::ExplorerEvent::OperationProgress {
                context: context.clone(),
                progress: explorer_model::OperationProgress {
                    completed_items: 1,
                    total_items: 1,
                    completed_bytes: 1,
                    total_bytes: Some(1),
                    phase: explorer_model::TransferProgressPhase::Transferring,
                    current_item: None,
                },
            }),
            explorer_model::WindowEventOutcome::Applied
        );
        assert_eq!(
            state.apply_service_event(explorer_model::ExplorerEvent::OperationFinished {
                context: context.clone(),
                outcome: explorer_model::OperationTerminal::Finished,
            }),
            explorer_model::WindowEventOutcome::Applied
        );
        assert_eq!(
            state.apply_service_event(explorer_model::ExplorerEvent::OperationProgress {
                context,
                progress: explorer_model::OperationProgress {
                    completed_items: 1,
                    total_items: 1,
                    completed_bytes: 1,
                    total_bytes: Some(1),
                    phase: explorer_model::TransferProgressPhase::Transferring,
                    current_item: None,
                },
            }),
            explorer_model::WindowEventOutcome::IgnoredStale
        );
    }

    #[test]
    fn inline_rename_enter_escape_blur_and_collision_keep_truthful_editor_state() {
        let mut state = state_with_rows();
        assert!(state.begin_inline_rename(1));
        assert_eq!(
            state.rename_editor().expect("editor").selection,
            0.."file".len()
        );
        assert!(state.update_inline_rename("folder".to_owned()));
        assert!(
            state
                .commit_inline_rename(explorer_model::RenameCommitTrigger::Enter)
                .is_err()
        );
        assert_eq!(
            state.rename_editor().expect("collision retained").buffer,
            "folder"
        );
        assert!(state.update_inline_rename("renamed.txt".to_owned()));
        assert!(
            state
                .commit_inline_rename(explorer_model::RenameCommitTrigger::Blur)
                .expect("valid blur")
                .is_some()
        );
        assert!(state.rename_editor().is_none());

        assert!(state.begin_inline_rename(0));
        assert!(state.cancel_inline_rename());
        assert!(state.rename_editor().is_none());
    }

    #[test]
    fn f2_after_navigation_focus_reset_selects_first_visible_row_for_rename() {
        let mut state = state_with_rows();
        state.clear_selection();

        assert_eq!(state.focused_row_index(), None);
        assert!(state.begin_focused_inline_rename());
        assert_eq!(state.focused_row_index(), Some(0));
        assert_eq!(state.tabs().active_tab().selection.len(), 1);
        assert_eq!(
            state
                .rename_editor()
                .expect("first visible row editor")
                .buffer,
            "folder"
        );
    }

    #[test]
    fn selection_focus_and_range_reuse_the_shared_presentation_cache() {
        let mut state = state_with_rows();
        let first = state.directory_presentation().expect("presentation");
        let rebuilds = state.presentation_rebuilds();

        assert!(state.select_row(0));
        assert!(state.focus_row(1));
        assert!(state.select_row_range(1, false));

        let after = state.directory_presentation().expect("reused presentation");
        assert_eq!(state.presentation_rebuilds(), rebuilds);
        assert!(std::sync::Arc::ptr_eq(
            first.ordered_indices(),
            after.ordered_indices()
        ));
        assert!(std::sync::Arc::ptr_eq(first.entries(), after.entries()));
    }

    #[test]
    fn permanent_delete_confirmation_cancel_creates_no_request_and_confirm_is_explicit() {
        let mut state = state_with_rows();
        assert!(state.select_row(1));
        assert!(state.begin_permanent_delete_confirmation());
        assert!(state.cancel_permanent_delete_confirmation());
        assert!(state.confirm_permanent_delete().is_none());

        assert!(state.begin_permanent_delete_confirmation());
        assert!(!state.begin_permanent_delete_confirmation());
        assert_eq!(state.permanent_delete_confirmation_count(), Some(1));
        let request = state.confirm_permanent_delete().expect("confirmed request");
        assert!(state.confirm_permanent_delete().is_none());
        assert!(matches!(
            request.kind,
            explorer_model::FileOperationKind::PermanentDelete {
                confirmed: true,
                ..
            }
        ));
    }

    #[test]
    fn shift_delete_request_uses_only_the_windows_shell_confirmation() {
        let mut state = state_with_rows();
        assert!(state.select_row(1));

        let request = state
            .permanent_delete_selected_request()
            .expect("selected item permanent delete request");

        assert_eq!(state.permanent_delete_confirmation_count(), None);
        assert!(request.flags.require_confirmation);
        assert!(!request.flags.allow_undo);
        assert!(matches!(
            request.kind,
            explorer_model::FileOperationKind::PermanentDelete {
                confirmed: true,
                ref items,
            } if items.len() == 1
        ));
    }

    #[test]
    fn permanent_delete_confirmation_focus_is_visible_bounded_and_lifecycle_owned() {
        use crate::actions::PermanentDeleteDialogTarget::{Cancel, Delete};

        let mut state = state_with_rows();
        assert!(state.select_row(1));
        assert_eq!(state.permanent_delete_confirmation_focus(), None);
        assert!(state.begin_permanent_delete_confirmation());
        assert_eq!(state.permanent_delete_confirmation_focus(), Some(Delete));

        assert!(state.move_permanent_delete_confirmation_focus(1));
        assert_eq!(state.permanent_delete_confirmation_focus(), Some(Cancel));
        assert!(state.move_permanent_delete_confirmation_focus(-1));
        assert_eq!(state.permanent_delete_confirmation_focus(), Some(Delete));
        assert!(state.set_permanent_delete_confirmation_focus(Cancel));
        assert!(!state.set_permanent_delete_confirmation_focus(Cancel));

        assert!(state.cancel_permanent_delete_confirmation());
        assert_eq!(state.permanent_delete_confirmation_focus(), None);
        assert!(!state.move_permanent_delete_confirmation_focus(1));
        assert!(!state.set_permanent_delete_confirmation_focus(Delete));
    }

    #[test]
    fn permanent_delete_confirmation_is_cleared_by_navigation_tabs_completion_and_shutdown() {
        let armed = || {
            let mut state = state_with_rows();
            assert!(state.select_row(1));
            assert!(state.begin_permanent_delete_confirmation());
            state
        };

        let mut navigation = armed();
        assert!(navigation.begin_active_location_load().is_some());
        assert_eq!(navigation.permanent_delete_confirmation_count(), None);

        let mut tab_switch = armed();
        let first = tab_switch.tabs().active_tab_id();
        let other = tab_switch.new_tab();
        assert_eq!(tab_switch.permanent_delete_confirmation_count(), None);
        assert!(tab_switch.activate_tab(first));
        assert!(tab_switch.select_row(1));
        assert!(tab_switch.begin_permanent_delete_confirmation());
        assert_eq!(
            tab_switch.close_tab(other),
            explorer_model::TabCloseOutcome::Closed
        );
        assert_eq!(tab_switch.permanent_delete_confirmation_count(), None);

        let mut completion = armed();
        let request = completion
            .create_folder_request()
            .expect("operation request");
        let command = completion.begin_file_operation(request);
        let context = command.context().expect("operation context").clone();
        let _ = completion.apply_service_event(explorer_model::ExplorerEvent::OperationFinished {
            context,
            outcome: explorer_model::OperationTerminal::Cancelled,
        });
        assert_eq!(completion.permanent_delete_confirmation_count(), None);

        let mut shutdown = armed();
        shutdown.request_close();
        assert_eq!(shutdown.permanent_delete_confirmation_count(), None);
    }

    #[test]
    fn permanent_delete_confirm_cancel_and_repeat_are_single_use_without_recycle_fallback() {
        let mut state = state_with_rows();
        assert!(state.select_row(1));

        assert!(state.begin_permanent_delete_confirmation());
        assert!(!state.begin_permanent_delete_confirmation());
        assert!(state.cancel_permanent_delete_confirmation());
        assert!(!state.cancel_permanent_delete_confirmation());
        assert!(state.confirm_permanent_delete().is_none());

        assert!(state.begin_permanent_delete_confirmation());
        let request = state.confirm_permanent_delete().expect("confirmed request");
        assert!(matches!(
            request.kind,
            explorer_model::FileOperationKind::PermanentDelete {
                confirmed: true,
                ref items,
            } if items.len() == 1
        ));
        assert!(request.flags.require_confirmation);
        assert!(!request.flags.allow_undo);
        assert!(!matches!(
            request.kind,
            explorer_model::FileOperationKind::RecycleDelete { .. }
        ));
        assert!(state.confirm_permanent_delete().is_none());
    }

    #[test]
    fn new_menu_is_bounded_mutually_exclusive_and_builds_owned_create_item_requests() {
        let mut state = state_with_rows();
        assert!(state.new_items().len() >= 4);
        state.toggle_new_menu();
        assert!(state.new_menu_open());
        state.move_new_menu_focus(i8::MAX);
        assert_eq!(state.new_menu_index(), state.new_items().len() - 1);
        state.toggle_sort_menu();
        assert!(!state.new_menu_open());
        assert!(state.sort_menu_open());

        let request = state.create_new_item_request(1).expect("text request");
        assert!(matches!(
            request.kind,
            explorer_model::FileOperationKind::CreateItem {
                name,
                recipe: explorer_model::ShellNewItemRecipe::EmptyFile,
                ..
            } if name == "New Text Document.txt"
        ));
    }

    #[test]
    fn context_paste_uses_current_folder_for_background_file_and_folder_hits() {
        let remote_item = explorer_model::ItemDescriptor {
            id: explorer_model::ShellItemId::from_provider_bytes([9]).unwrap(),
            location: explorer_model::LocationDescriptor::try_virtual(
                "sftp",
                [7; 16],
                1,
                None,
                vec![
                    "home".to_owned(),
                    "linuxuser".to_owned(),
                    "report.txt".to_owned(),
                ],
            )
            .unwrap(),
        };
        for hit in [
            None,
            Some(explorer_model::ShellItemId::from_provider_bytes([1]).unwrap()),
            Some(explorer_model::ShellItemId::from_provider_bytes([2]).unwrap()),
        ] {
            let mut state = state_with_rows();
            let _ = state.apply_service_event(explorer_model::ExplorerEvent::ClipboardChanged {
                state: explorer_model::ClipboardState::Owned {
                    mode: explorer_model::ClipboardMode::Copy,
                    items: vec![remote_item.clone()],
                    effects: explorer_model::TransferEffects::COPY,
                    generation: 1,
                },
            });
            if let Some(item_id) = hit.as_ref() {
                state.prepare_context_selection(Some(item_id));
            }
            let explorer_model::ExplorerCommand::ShowContextMenu { request, .. } = state
                .begin_context_menu_request(hit, 42, 100, 200, 10.0, 20.0, true, false)
                .expect("local context menu")
            else {
                panic!("expected local context command");
            };
            assert!(request.paste_available);
            let explorer_model::ExplorerCommand::DataTransfer {
                request: explorer_model::DataTransferRequest::Paste { destination, .. },
                ..
            } = state
                .begin_paste_request(explorer_model::ConflictDecision::Prompt)
                .expect("paste request")
            else {
                panic!("expected paste command");
            };
            assert_eq!(
                destination,
                explorer_model::LocationDescriptor::file_system(r"C:\fixture")
            );
        }
    }

    #[test]
    fn context_menu_paste_availability_rejects_empty_and_read_only_state() {
        let mut empty = state_with_rows();
        let explorer_model::ExplorerCommand::ShowContextMenu { request, .. } = empty
            .begin_context_menu_request(None, 42, 0, 0, 0.0, 0.0, true, false)
            .expect("empty clipboard context menu")
        else {
            panic!("expected context command");
        };
        assert!(!request.paste_available);

        let mut read_only = AppViewState::with_initial_location(explorer_model::HistoryEntry::new(
            explorer_model::LocationDescriptor::file_system(r"C:\read-only"),
            "read-only",
        ));
        let command = read_only.begin_active_location_load().unwrap();
        let context = command.context().unwrap().clone();
        let _ = read_only.apply_service_event(explorer_model::ExplorerEvent::LocationResolved {
            context,
            metadata: explorer_model::LocationMetadata {
                descriptor: explorer_model::LocationDescriptor::file_system(r"C:\read-only"),
                display_title: "read-only".to_owned(),
                can_go_up: true,
                can_write: false,
            },
        });
        let _ = read_only.apply_service_event(explorer_model::ExplorerEvent::ClipboardChanged {
            state: explorer_model::ClipboardState::External {
                effects: explorer_model::TransferEffects::COPY,
                item_count: Some(1),
                generation: 1,
            },
        });
        let explorer_model::ExplorerCommand::ShowContextMenu { request, .. } = read_only
            .begin_context_menu_request(None, 42, 0, 0, 0.0, 0.0, true, false)
            .expect("read-only context menu")
        else {
            panic!("expected context command");
        };
        assert!(!request.paste_available);
    }

    #[test]
    fn registered_virtual_destination_uses_custom_paste_without_provider_allowlist() {
        let destination = explorer_model::LocationDescriptor::try_virtual(
            "archive",
            [8; 16],
            1,
            None,
            vec!["current".to_owned()],
        )
        .unwrap();
        let mut state = AppViewState::with_initial_location(explorer_model::HistoryEntry::new(
            destination.clone(),
            "archive",
        ));
        let context = state
            .begin_active_location_load()
            .unwrap()
            .context()
            .unwrap()
            .clone();
        let _ = state.apply_service_event(explorer_model::ExplorerEvent::LocationResolved {
            context,
            metadata: explorer_model::LocationMetadata {
                descriptor: destination.clone(),
                display_title: "archive".to_owned(),
                can_go_up: true,
                can_write: true,
            },
        });
        let _ = state.apply_service_event(explorer_model::ExplorerEvent::ClipboardChanged {
            state: explorer_model::ClipboardState::Owned {
                mode: explorer_model::ClipboardMode::Copy,
                items: vec![explorer_model::ItemDescriptor {
                    id: explorer_model::ShellItemId::from_provider_bytes([9]).unwrap(),
                    location: explorer_model::LocationDescriptor::try_virtual(
                        "adb",
                        [7; 16],
                        1,
                        None,
                        vec!["data".to_owned(), "report.txt".to_owned()],
                    )
                    .unwrap(),
                }],
                effects: explorer_model::TransferEffects::COPY,
                generation: 1,
            },
        });

        assert!(
            state
                .begin_context_menu_request(None, 42, 100, 200, 10.0, 20.0, false, false)
                .is_none()
        );
        assert!(state.remote_context_menu().unwrap().paste_available);
        let explorer_model::ExplorerCommand::DataTransfer {
            request:
                explorer_model::DataTransferRequest::Paste {
                    destination: actual,
                    ..
                },
            ..
        } = state
            .begin_paste_request(explorer_model::ConflictDecision::Prompt)
            .unwrap()
        else {
            panic!("expected paste request");
        };
        assert_eq!(actual, destination);
    }

    #[test]
    fn clipboard_cut_survives_source_tab_close_and_stale_state_disables_paste() {
        let mut state = state_with_rows();
        assert!(state.select_row(1));
        let cut = state
            .begin_clipboard_request(explorer_model::ClipboardMode::Cut)
            .expect("cut command");
        let explorer_model::ExplorerCommand::DataTransfer {
            request: explorer_model::DataTransferRequest::Cut { items },
            ..
        } = cut
        else {
            panic!("expected cut command");
        };
        assert_eq!(items.len(), 1);
        let source_tab = state.tabs().active_tab_id();
        assert_eq!(
            state.apply_service_event(explorer_model::ExplorerEvent::ClipboardChanged {
                state: explorer_model::ClipboardState::Owned {
                    mode: explorer_model::ClipboardMode::Cut,
                    items,
                    effects: explorer_model::TransferEffects::MOVE,
                    generation: 7,
                },
            }),
            explorer_model::WindowEventOutcome::Applied
        );
        let _destination_tab = state.new_tab();
        let destination_load = state
            .begin_active_location_load()
            .expect("destination load");
        let destination_context = destination_load.context().unwrap().clone();
        let _ = state.apply_service_event(explorer_model::ExplorerEvent::LocationResolved {
            context: destination_context,
            metadata: explorer_model::LocationMetadata {
                descriptor: explorer_model::LocationDescriptor::file_system(r"C:\destination"),
                display_title: "destination".to_owned(),
                can_go_up: true,
                can_write: true,
            },
        });
        assert_eq!(
            state.close_tab(source_tab),
            explorer_model::TabCloseOutcome::Closed
        );
        assert!(
            state
                .begin_paste_request(explorer_model::ConflictDecision::Prompt)
                .is_some()
        );
        let _ = state.apply_service_event(explorer_model::ExplorerEvent::ClipboardChanged {
            state: explorer_model::ClipboardState::None { generation: 8 },
        });
        assert!(
            state
                .begin_paste_request(explorer_model::ConflictDecision::Prompt)
                .is_none()
        );
    }

    #[test]
    fn folder_options_use_a_cancelable_draft_and_apply_to_the_active_tab() {
        let mut state = AppViewState::default();
        state.open_folder_options();
        state.update_folder_options(|settings| {
            settings.hidden_items = true;
            settings.icon_cache_memory_mb = 1_024;
            settings.cache_budgets.mft_lru_mb = 2_048;
        });
        assert!(!state.view_settings().hidden_items);
        assert_eq!(state.view_settings().icon_cache_memory_mb, 32);
        assert_eq!(state.view_settings().cache_budgets.mft_lru_mb, 512);
        state.close_folder_options();
        assert!(!state.view_settings().hidden_items);

        state.open_folder_options();
        state.set_folder_options_page(crate::actions::FolderOptionsPage::View);
        state.update_folder_options(|settings| {
            settings.hidden_items = true;
            settings.file_name_extensions = false;
            settings.icon_cache_memory_mb = 1_024;
            settings.cache_budgets.mft_lru_mb = 2_048;
        });
        state.confirm_folder_options();
        assert!(state.folder_options().is_none());
        assert!(state.view_settings().hidden_items);
        assert!(!state.view_settings().file_name_extensions);
        assert_eq!(state.view_settings().icon_cache_memory_mb, 1_024);
        assert_eq!(state.view_settings().cache_budgets.mft_lru_mb, 2_048);
    }

    #[test]
    fn cache_budget_apply_reopens_from_committed_value_without_stale_512() {
        let mut state = AppViewState::default();
        state.open_folder_options();
        state.update_folder_options(|settings| settings.cache_budgets.mft_lru_mb = 2_048);
        state.apply_folder_options();
        assert_eq!(state.view_settings().cache_budgets.mft_lru_mb, 2_048);

        state.update_folder_options(|settings| settings.cache_budgets.mft_lru_mb = 4_096);
        state.close_folder_options();
        assert_eq!(state.view_settings().cache_budgets.mft_lru_mb, 2_048);

        state.open_folder_options();
        assert_eq!(
            state
                .folder_options()
                .expect("reopened draft")
                .settings
                .cache_budgets
                .mft_lru_mb,
            2_048
        );
        state.update_folder_options(|settings| settings.cache_budgets.mft_lru_mb = 16_384);
        state.confirm_folder_options();
        assert_eq!(state.view_settings().cache_budgets.mft_lru_mb, 16_384);
    }

    #[test]
    fn folder_options_apply_replaces_baseline_and_confirm_closes_only_with_a_draft() {
        let mut state = AppViewState::default();
        assert_eq!(
            state.confirm_folder_options(),
            super::FolderOptionsApplyResultV1::NoDraft
        );
        state.open_folder_options();
        assert!(!state.folder_options().unwrap().is_dirty());
        state.update_folder_options(|settings| settings.hidden_items = true);
        assert!(state.folder_options().unwrap().is_dirty());
        assert_eq!(
            state.apply_folder_options(),
            super::FolderOptionsApplyResultV1::Applied { revision: 1 }
        );
        let draft = state.folder_options().expect("Apply keeps the draft open");
        assert!(!draft.is_dirty());
        assert_eq!(draft.applied_revision, 1);
        state.update_folder_options(|settings| settings.file_name_extensions = false);
        assert_eq!(
            state.confirm_folder_options(),
            super::FolderOptionsApplyResultV1::Applied { revision: 2 }
        );
        assert!(state.folder_options().is_none());
    }

    #[test]
    fn external_applied_settings_update_only_adopts_into_a_clean_draft() {
        let mut clean = AppViewState::default();
        clean.open_folder_options();
        let mut applied = clean
            .folder_options()
            .expect("clean draft")
            .applied_snapshot();
        applied.settings.hidden_items = true;
        clean.adopt_applied_folder_options(applied.clone(), 7);
        let clean_draft = clean.folder_options().expect("adopted clean draft");
        assert!(clean_draft.settings.hidden_items);
        assert_eq!(clean_draft.applied_revision, 7);
        assert!(!clean_draft.is_dirty());

        let mut dirty = AppViewState::default();
        dirty.open_folder_options();
        dirty.update_folder_options(|settings| settings.file_name_extensions = false);
        dirty.adopt_applied_folder_options(applied, 7);
        let dirty_draft = dirty.folder_options().expect("preserved dirty draft");
        assert!(!dirty_draft.settings.file_name_extensions);
        assert!(!dirty_draft.settings.hidden_items);
        assert_eq!(dirty_draft.applied_revision, 0);
        assert!(dirty_draft.is_dirty());
        assert!(dirty.view_settings().hidden_items);
    }

    #[test]
    fn one_applied_revision_broadcasts_to_two_live_windows() {
        let mut owner = AppViewState::default();
        owner.open_folder_options();
        owner.update_folder_options(|settings| settings.hidden_items = true);
        let applied = owner
            .folder_options()
            .expect("owner draft")
            .applied_snapshot();
        assert_eq!(
            owner.apply_folder_options(),
            super::FolderOptionsApplyResultV1::Applied { revision: 1 }
        );

        let mut first_peer = AppViewState::default();
        let mut second_peer = AppViewState::default();
        first_peer.adopt_applied_folder_options(applied.clone(), 1);
        second_peer.adopt_applied_folder_options(applied, 1);
        assert!(first_peer.view_settings().hidden_items);
        assert!(second_peer.view_settings().hidden_items);
    }

    #[test]
    fn cancel_escape_and_title_close_share_an_idempotent_discard_transition() {
        for _close_reason in ["cancel", "escape", "title-close"] {
            let mut state = AppViewState::default();
            state.open_folder_options();
            state.update_folder_options(|settings| settings.hidden_items = true);
            state.close_folder_options();
            state.close_folder_options();
            assert!(state.folder_options().is_none());
            assert!(!state.view_settings().hidden_items);
        }
    }

    #[test]
    fn rejected_apply_keeps_the_dirty_draft_and_typed_failure() {
        let mut state = AppViewState::default();
        state.open_folder_options();
        state.update_folder_options(|settings| settings.hidden_items = true);
        assert_eq!(
            state.reject_folder_options_apply(
                super::FolderOptionsApplyFailureV1::Persistence,
                "save failed",
            ),
            super::FolderOptionsApplyResultV1::Rejected {
                reason: super::FolderOptionsApplyFailureV1::Persistence,
            }
        );
        let draft = state.folder_options().expect("failed apply remains open");
        assert!(draft.is_dirty());
        assert_eq!(draft.apply_error.as_deref(), Some("save failed"));
        assert!(!state.view_settings().hidden_items);
    }

    #[test]
    fn folder_options_manage_all_eight_extensions_with_cancel_apply_and_view_fallback() {
        let mut state = AppViewState::default();
        assert_eq!(state.extensions().len(), 8);
        assert!(state.extensions().iter().all(|extension| {
            !extension.author_name.is_empty()
                && !extension.author_bio.is_empty()
                && extension.author_website.starts_with("https://")
                && !extension.purpose.is_empty()
                && extension.community_website.starts_with("https://")
                && extension.release_date.len() == 10
        }));
        state.open_folder_options();
        state.set_folder_options_page(crate::actions::FolderOptionsPage::Extensions);
        state.toggle_folder_option_extension(1);
        assert!(state.extensions()[1].enabled);
        state.close_folder_options();
        assert!(
            state.extensions()[1].enabled,
            "Cancel must discard the draft"
        );

        state.tabs.active_tab_mut().view.settings.extension_view_id =
            Some("rust-folder-size-map:view".to_owned());
        state.open_folder_options();
        state.toggle_folder_option_extension(1);
        state.apply_folder_options();
        assert!(!state.extensions()[1].enabled);
        assert_eq!(state.view_settings().extension_view_id, None);

        let lock_owner = crate::code_lines_column::lock_owner_column_descriptor();
        assert!(state.install_code_lines_column_descriptor(lock_owner.clone()));
        assert!(state.column_registry().contains(&lock_owner.id));
        state.open_folder_options();
        state.toggle_folder_option_extension(4);
        state.apply_folder_options();
        assert!(!state.extensions()[4].enabled);
        assert!(!state.column_registry().contains(&lock_owner.id));
    }

    #[test]
    fn lua_and_rust_code_lines_columns_keep_independent_identity_and_sort_state() {
        let mut state = AppViewState::default();
        let rust = crate::code_lines_column::code_lines_column_descriptor();
        let mut lua = rust.clone();
        lua.id = explorer_model::ColumnId::Extension {
            package_id: "lua-tokei-code-lines-column".to_owned(),
            column_id: crate::code_lines_column::CODE_LINES_COLUMN_ID.to_owned(),
        };
        lua.display_name = "Code lines".to_owned();
        assert_ne!(rust.id, lua.id);
        assert!(state.install_code_lines_column_descriptor(rust.clone()));
        assert!(state.install_code_lines_column_descriptor(lua.clone()));
        assert!(state.column_registry().contains(&rust.id));
        assert!(state.column_registry().contains(&lua.id));

        let item = explorer_model::ShellItemId::from_provider_bytes([0x42]).unwrap();
        assert!(state.set_code_lines_sort_values(
            rust.id.clone(),
            std::collections::HashMap::from([(item.clone(), Some(1_250))]),
        ));
        assert!(state.set_code_lines_sort_values(
            lua.id.clone(),
            std::collections::HashMap::from([(item, Some(75))]),
        ));
        assert_eq!(
            state.code_lines_sort_values[&rust.id].values().next(),
            Some(&Some(1_250))
        );
        assert_eq!(
            state.code_lines_sort_values[&lua.id].values().next(),
            Some(&Some(75))
        );

        assert!(state.uninstall_code_lines_column_descriptor(&lua));
        assert!(state.column_registry().contains(&rust.id));
        assert!(!state.column_registry().contains(&lua.id));
        assert!(state.code_lines_sort_values.contains_key(&rust.id));
        assert!(!state.code_lines_sort_values.contains_key(&lua.id));
    }

    #[test]
    fn selecting_the_active_extension_view_toggles_back_to_details() {
        let mut state = AppViewState::default();
        let view_id = "rust-folder-size-map-view:folder-size-map".to_owned();
        state.set_extension_view(view_id.clone());
        assert_eq!(
            state.view_settings().extension_view_id.as_deref(),
            Some(view_id.as_str())
        );
        assert_eq!(
            state.view_settings().mode,
            explorer_model::ViewMode::Details
        );
        state.set_extension_view(view_id);
        assert_eq!(state.view_settings().extension_view_id, None);
        assert_eq!(
            state.view_settings().mode,
            explorer_model::ViewMode::Details
        );
    }

    #[test]
    fn about_dialog_uses_configured_build_metadata_and_closes_without_mutating_tabs() {
        let mut state = AppViewState::default();
        let tab = state.tabs().active_tab_id();
        state.set_about_info(super::AboutInfoV1 {
            version: "1.2.3".to_owned(),
            build_date: "2026-08-04".to_owned(),
            git_hash: "0123456789abcdef".to_owned(),
            author: "Damody".to_owned(),
        });
        state.open_about_dialog();
        let info = state.about_dialog().expect("about dialog");
        assert_eq!(info.version, "1.2.3");
        assert_eq!(info.git_hash, "0123456789abcdef");
        state.close_about_dialog();
        assert!(state.about_dialog().is_none());
        assert_eq!(state.tabs().active_tab_id(), tab);
    }

    #[test]
    fn command_and_details_header_menus_are_mutually_exclusive() {
        let mut state = AppViewState::default();
        state.open_details_column_menu(explorer_model::ColumnId::Name);
        state.toggle_view_menu();
        assert!(state.details_column_menu().is_none());
        assert!(state.view_menu_open());

        state.open_details_filter_menu(explorer_model::ColumnId::Size);
        assert!(!state.view_menu_open());
        assert_eq!(
            state.details_filter_menu(),
            Some(explorer_model::ColumnId::Size)
        );
        state.toggle_sort_menu();
        assert!(state.details_filter_menu().is_none());
        assert!(state.sort_menu_open());
    }

    #[test]
    fn ctrl_wheel_zoom_moves_bounded_notches_and_preserves_anchor() {
        let mut state = AppViewState::default();
        let anchor = state.tabs().active_tab().view.anchor.clone();
        state.set_view_mode(explorer_model::ViewMode::Details);
        state.zoom_view(-1);
        assert_eq!(
            (state.view_settings().mode, state.view_settings().icon_size),
            (explorer_model::ViewMode::Tiles, 40)
        );
        state.zoom_view(-1);
        assert_eq!(
            (state.view_settings().mode, state.view_settings().icon_size),
            (explorer_model::ViewMode::Content, 32)
        );
        state.zoom_view(1);
        state.zoom_view(1);
        assert_eq!(
            state.view_settings().mode,
            explorer_model::ViewMode::Details
        );
        assert_eq!(state.tabs().active_tab().view.anchor, anchor);
        for _ in 0..20 {
            state.zoom_view(1);
        }
        assert_eq!(
            (state.view_settings().mode, state.view_settings().icon_size),
            (explorer_model::ViewMode::ExtraLargeIcons, 512)
        );
        for _ in 0..20 {
            state.zoom_view(-1);
        }
        assert_eq!(
            (state.view_settings().mode, state.view_settings().icon_size),
            (explorer_model::ViewMode::Content, 32)
        );
    }

    #[test]
    fn more_menu_keyboard_focus_covers_all_ten_commands() {
        let mut state = AppViewState::default();
        state.toggle_more_menu();
        state.move_more_menu_focus(i8::MAX);
        assert_eq!(state.more_menu_index(), 9);
        state.move_more_menu_focus(-1);
        assert_eq!(state.more_menu_index(), 8);
        state.move_more_menu_focus(i8::MIN);
        assert_eq!(state.more_menu_index(), 0);
    }

    #[test]
    fn command_menu_pointer_focus_tracks_hovered_rows_and_rejects_invalid_indices() {
        let mut state = AppViewState::default();
        assert!(!state.set_sort_menu_focus(1));
        state.toggle_sort_menu();
        assert!(state.set_sort_menu_focus(3));
        assert_eq!(state.sort_menu_index(), 3);
        assert!(!state.set_sort_menu_focus(3));
        assert!(!state.set_sort_menu_focus(6));

        state.toggle_view_menu();
        assert!(state.set_view_menu_focus(9));
        assert_eq!(state.view_menu_index(), 9);
        assert!(!state.set_view_menu_focus(12));

        state.toggle_more_menu();
        assert!(state.set_more_menu_focus(9));
        assert_eq!(state.more_menu_index(), 9);
        assert!(state.set_more_menu_focus(2));
        assert_eq!(state.more_menu_index(), 2);
        assert!(!state.set_more_menu_focus(10));
    }

    #[test]
    fn recycle_bin_empty_uses_windows_owned_confirming_shell_verb() {
        let mut state = AppViewState::with_initial_location(explorer_model::HistoryEntry::new(
            explorer_model::LocationDescriptor::ParsingName("shell:RecycleBinFolder".to_owned()),
            "Recycle Bin",
        ));

        let explorer_model::ExplorerCommand::ShowContextMenu { request, .. } = state
            .begin_empty_recycle_bin_request(42)
            .expect("Recycle Bin exposes the Windows-owned empty command")
        else {
            panic!("empty Recycle Bin must cross the Shell context-menu boundary");
        };
        assert_eq!(request.owner_window, 42);
        assert_eq!(request.requested_verb.as_deref(), Some("empty"));
        assert_eq!(request.deadline_ms, 10_000);
        assert!(matches!(
            request.target,
            explorer_model::ShellContextMenuTarget::Background {
                parent: explorer_model::LocationDescriptor::ParsingName(ref value)
            } if value.eq_ignore_ascii_case("shell:RecycleBinFolder")
        ));
    }

    #[test]
    fn recycle_bin_restore_requires_item_capability_and_uses_undelete_verb() {
        let mut state = AppViewState::with_initial_location(explorer_model::HistoryEntry::new(
            explorer_model::LocationDescriptor::ParsingName("shell:RecycleBinFolder".to_owned()),
            "Recycle Bin",
        ));
        let command = state.begin_active_location_load().expect("load command");
        let context = command.context().expect("context").clone();
        assert_eq!(
            state.apply_service_event(explorer_model::ExplorerEvent::LocationResolved {
                context: context.clone(),
                metadata: explorer_model::LocationMetadata {
                    descriptor: explorer_model::LocationDescriptor::ParsingName(
                        "shell:RecycleBinFolder".to_owned(),
                    ),
                    display_title: "Recycle Bin".to_owned(),
                    can_go_up: true,
                    can_write: false,
                },
            }),
            explorer_model::WindowEventOutcome::Applied
        );
        let restored_id = explorer_model::ShellItemId::from_provider_bytes([9])
            .expect("Recycle Bin item identity");
        assert_eq!(
            state.apply_service_event(explorer_model::ExplorerEvent::DirectoryBatch {
                context: context.clone(),
                entries: vec![explorer_model::FileEntry {
                    id: restored_id,
                    display_name: "deleted.txt".to_owned(),
                    location: explorer_model::LocationDescriptor::ShellNamespace(vec![0, 0]),
                    is_container: false,
                    metadata: explorer_model::FileEntryMetadata {
                        namespace_capabilities:
                            explorer_model::NamespaceCapabilities::from_public_bits(
                                explorer_model::NamespaceCapabilities::RESTORE
                                    | explorer_model::NamespaceCapabilities::CONTEXT_MENU,
                            ),
                        ..Default::default()
                    },
                }],
            }),
            explorer_model::WindowEventOutcome::Applied
        );
        let _ =
            state.apply_service_event(explorer_model::ExplorerEvent::DirectoryFinished { context });
        assert!(state.select_row(0));

        let explorer_model::ExplorerCommand::ShowContextMenu { request, .. } = state
            .begin_restore_request(77)
            .expect("RESTORE-capable item exposes the Shell restore command")
        else {
            panic!("restore must cross the Shell context-menu boundary");
        };
        assert_eq!(request.owner_window, 77);
        assert_eq!(request.requested_verb.as_deref(), Some("undelete"));
    }

    #[test]
    fn copy_paths_uses_presentation_selection_and_explorer_quoting() {
        let mut state = state_with_rows();
        state.select_all_rows();
        assert_eq!(
            state.selected_paths_clipboard_text().as_deref(),
            Some("\"C:\\fixture\\folder\"\r\n\"C:\\fixture\\file.txt\"")
        );
    }

    #[test]
    fn quick_access_toggle_persists_deduplicates_rolls_back_and_feeds_home() {
        let mut state = state_with_rows();
        state.select_all_rows();
        let previous = state
            .toggle_selected_quick_access()
            .expect("selected reconstructible rows are pinned");
        assert_eq!(state.persisted_quick_access().len(), 2);
        assert_eq!(
            state
                .synthetic_root_entries(explorer_model::SyntheticRoot::QuickAccess, 10)
                .len(),
            2
        );
        assert_eq!(
            state
                .synthetic_root_entries(explorer_model::SyntheticRoot::Home, 10)
                .len(),
            2
        );
        state.rollback_quick_access(previous);
        assert!(state.persisted_quick_access().is_empty());

        state.configure_quick_access(vec![explorer_model::PersistedQuickAccessPin {
            location: explorer_model::LocationDescriptor::file_system(r"C:\fixture\folder"),
            display_name: "folder".to_owned(),
            order: 0,
        }]);
        assert_eq!(state.persisted_quick_access().len(), 1);
        assert_eq!(state.quick_access_navigation_pins().len(), 1);
    }

    #[test]
    fn broker_health_is_quiet_when_ready_and_actionable_on_every_failure() {
        let mut state = AppViewState::default();
        assert_eq!(state.broker_health(), super::BrokerUiHealth::Healthy);
        assert_eq!(state.broker_health().message(), None);
        for health in [
            super::BrokerUiHealth::Unavailable,
            super::BrokerUiHealth::VersionMismatch,
            super::BrokerUiHealth::Crash,
            super::BrokerUiHealth::Timeout,
            super::BrokerUiHealth::Quarantined,
        ] {
            state.set_broker_health(health);
            assert!(state.broker_health().message().is_some());
        }
        state.set_broker_health(super::BrokerUiHealth::Retrying);
        assert!(state.broker_health().message().is_some());
    }

    #[test]
    fn navigation_tree_expands_loads_collapses_and_isolates_tabs() {
        let mut state = AppViewState::default();
        let first_tab = state.tabs().active_tab_id();
        let root = explorer_model::LocationDescriptor::file_system(r"C:\");
        assert!(state.toggle_navigation_node(root.clone()));
        let command = state
            .begin_navigation_node_request(root.clone())
            .expect("first expansion loads children asynchronously");
        let explorer_model::ExplorerCommand::EnumerateChildContainers {
            context,
            segment_id,
            menu_generation,
            ..
        } = command
        else {
            panic!("navigation expansion must use child-container enumeration");
        };
        let child = explorer_model::BreadcrumbMenuItem {
            display_name: "fixture".to_owned(),
            location: explorer_model::LocationDescriptor::file_system(r"C:\fixture"),
        };
        assert_eq!(
            state.apply_service_event(explorer_model::ExplorerEvent::ChildContainersBatch {
                context: context.clone(),
                segment_id,
                menu_generation,
                children: vec![child.clone()],
            }),
            explorer_model::WindowEventOutcome::Applied
        );
        assert_eq!(state.navigation_node_children(&root), &[child]);
        let icon_locations = state.navigation_icon_locations();
        assert!(icon_locations.contains(&root));
        assert!(
            icon_locations.contains(&explorer_model::LocationDescriptor::file_system(
                r"C:\fixture"
            ))
        );
        assert_eq!(
            state.apply_service_event(explorer_model::ExplorerEvent::ChildContainersFinished {
                context,
                segment_id,
                menu_generation,
                outcome: explorer_model::BreadcrumbTerminal::Finished,
            }),
            explorer_model::WindowEventOutcome::Applied
        );
        assert!(!state.toggle_navigation_node(root.clone()));
        assert!(!state.navigation_node_expanded(&root));
        assert!(state.navigation_icon_locations().is_empty());

        let second_tab = state.new_tab();
        assert_ne!(first_tab, second_tab);
        assert!(!state.navigation_node_expanded(&root));
        assert!(state.activate_tab(first_tab));
        assert!(!state.navigation_node_expanded(&root));
        assert_eq!(state.navigation_node_children(&root).len(), 1);
    }

    #[test]
    fn watcher_change_invalidates_and_reenumerates_the_expanded_current_navigation_node() {
        let mut state = AppViewState::default();
        let tab = state.tabs().active_tab();
        let tab_id = tab.id;
        let generation = tab.generation;
        let root = explorer_model::LocationDescriptor::file_system(r"C:\");
        assert!(state.toggle_navigation_node(root.clone()));
        let command = state
            .begin_navigation_node_request(root.clone())
            .expect("expanded root request");
        let explorer_model::ExplorerCommand::EnumerateChildContainers {
            context,
            segment_id,
            menu_generation,
            ..
        } = command
        else {
            panic!("navigation request");
        };
        let stale_child = explorer_model::BreadcrumbMenuItem {
            display_name: "deleted-folder".to_owned(),
            location: explorer_model::LocationDescriptor::file_system(r"C:\deleted-folder"),
        };
        let _ = state.apply_service_event(explorer_model::ExplorerEvent::ChildContainersBatch {
            context: context.clone(),
            segment_id,
            menu_generation,
            children: vec![stale_child],
        });
        let _ = state.apply_service_event(explorer_model::ExplorerEvent::ChildContainersFinished {
            context,
            segment_id,
            menu_generation,
            outcome: explorer_model::BreadcrumbTerminal::Finished,
        });
        assert_eq!(state.navigation_node_children(&root).len(), 1);

        let event = explorer_model::ExplorerEvent::DirectoryChanged {
            tab_id,
            generation,
            changes: vec![explorer_model::DirectoryDelta::Overflow],
        };
        let reconciliation = state.navigation_reconciliation_targets(&event);
        let refresh = state
            .watcher_recovery_command(&event)
            .expect("watcher refresh advances the active generation");
        let commands = state.begin_navigation_reconciliation(reconciliation);
        assert!(state.navigation_node_children(&root).is_empty());
        assert!(state.navigation_node_loading(&root));
        assert_eq!(commands.len(), 1);
        assert!(matches!(
            &commands[0],
            explorer_model::ExplorerCommand::EnumerateChildContainers { parent, .. }
                if parent == &root
        ));
        assert_eq!(
            commands[0]
                .context()
                .map(|context| (context.tab_id, context.generation)),
            refresh
                .context()
                .map(|context| (context.tab_id, context.generation))
        );
        let explorer_model::ExplorerCommand::EnumerateChildContainers {
            context,
            segment_id,
            menu_generation,
            ..
        } = commands.into_iter().next().expect("replacement request")
        else {
            panic!("navigation replacement command");
        };
        let surviving_child = explorer_model::BreadcrumbMenuItem {
            display_name: "survivor".to_owned(),
            location: explorer_model::LocationDescriptor::file_system(r"C:\survivor"),
        };
        assert_eq!(
            state.apply_service_event(explorer_model::ExplorerEvent::ChildContainersBatch {
                context: context.clone(),
                segment_id,
                menu_generation,
                children: vec![surviving_child.clone()],
            }),
            explorer_model::WindowEventOutcome::Applied
        );
        assert_eq!(
            state.apply_service_event(explorer_model::ExplorerEvent::ChildContainersFinished {
                context,
                segment_id,
                menu_generation,
                outcome: explorer_model::BreadcrumbTerminal::Finished,
            }),
            explorer_model::WindowEventOutcome::Applied
        );
        assert!(!state.navigation_node_loading(&root));
        assert_eq!(state.navigation_node_children(&root), &[surviving_child]);
    }

    #[test]
    fn successful_delete_invalidates_the_source_parent_even_before_watcher_delivery() {
        let mut state = AppViewState::default();
        let parent = explorer_model::LocationDescriptor::file_system(r"C:\fixture");
        assert!(state.toggle_navigation_node(parent.clone()));
        let command = state
            .begin_navigation_node_request(parent.clone())
            .expect("expanded parent request");
        let explorer_model::ExplorerCommand::EnumerateChildContainers {
            context,
            segment_id,
            menu_generation,
            ..
        } = command
        else {
            panic!("navigation request");
        };
        let child = explorer_model::BreadcrumbMenuItem {
            display_name: "delete-me".to_owned(),
            location: explorer_model::LocationDescriptor::file_system(r"C:\fixture\delete-me"),
        };
        let _ = state.apply_service_event(explorer_model::ExplorerEvent::ChildContainersBatch {
            context: context.clone(),
            segment_id,
            menu_generation,
            children: vec![child],
        });
        let _ = state.apply_service_event(explorer_model::ExplorerEvent::ChildContainersFinished {
            context,
            segment_id,
            menu_generation,
            outcome: explorer_model::BreadcrumbTerminal::Finished,
        });

        let item = explorer_model::ItemDescriptor {
            id: explorer_model::ShellItemId::from_provider_bytes(b"delete-me".to_vec())
                .expect("identity"),
            location: explorer_model::LocationDescriptor::file_system(r"C:\fixture\delete-me"),
        };
        let operation = state.begin_file_operation(explorer_model::FileOperationRequest {
            kind: explorer_model::FileOperationKind::PermanentDelete {
                items: vec![item],
                confirmed: true,
            },
            flags: explorer_model::FileOperationFlags {
                allow_undo: false,
                require_confirmation: true,
                ..explorer_model::FileOperationFlags::default()
            },
        });
        let explorer_model::ExplorerCommand::ExecuteFileOperation { context, .. } = operation
        else {
            panic!("file operation command");
        };
        let event = explorer_model::ExplorerEvent::OperationFinished {
            context,
            outcome: explorer_model::OperationTerminal::Finished,
        };
        let reconciliation = state.navigation_reconciliation_targets(&event);
        let commands = state.begin_navigation_reconciliation(reconciliation);
        assert!(state.navigation_node_children(&parent).is_empty());
        assert_eq!(commands.len(), 1);
        assert!(matches!(
            &commands[0],
            explorer_model::ExplorerCommand::EnumerateChildContainers { parent: refreshed, .. }
                if refreshed == &parent
        ));
    }

    fn finish_delete_with_code(
        state: &mut AppViewState,
        request: explorer_model::FileOperationRequest,
        native_code: i32,
    ) {
        let command = state.begin_file_operation(request);
        let context = command.context().expect("operation context").clone();
        let error = explorer_common::ExplorerError::new(
            explorer_common::ExplorerErrorKind::Availability,
            "delete fixture",
            true,
            "The item could not be deleted.",
            "controlled native failure",
        )
        .with_native_code(native_code);
        assert_eq!(
            state.apply_service_event(explorer_model::ExplorerEvent::OperationFinished {
                context,
                outcome: explorer_model::OperationTerminal::Failed(error),
            }),
            explorer_model::WindowEventOutcome::Applied
        );
    }

    fn lock_delete_request(permanent: bool) -> explorer_model::FileOperationRequest {
        let item = explorer_model::ItemDescriptor {
            id: explorer_model::ShellItemId::from_provider_bytes(b"locked-item".to_vec())
                .expect("identity"),
            location: explorer_model::LocationDescriptor::file_system(r"C:\fixture\locked.txt"),
        };
        let kind = if permanent {
            explorer_model::FileOperationKind::PermanentDelete {
                items: vec![item],
                confirmed: true,
            }
        } else {
            explorer_model::FileOperationKind::RecycleDelete { items: vec![item] }
        };
        explorer_model::FileOperationRequest {
            kind,
            flags: explorer_model::FileOperationFlags {
                allow_undo: !permanent,
                require_confirmation: permanent,
                ..explorer_model::FileOperationFlags::default()
            },
        }
    }

    fn eligible_lock_owner() -> explorer_model::LockOwner {
        explorer_model::LockOwner {
            identity: explorer_model::LockOwnerIdentity {
                process_id: 4242,
                creation_time_100ns: 99,
            },
            display_name: "Controlled editor".to_owned(),
            application_type: explorer_model::LockOwnerApplicationType::MainWindow,
            restartable: true,
            eligibility: explorer_model::LockOwnerEligibility::Eligible,
        }
    }

    #[test]
    fn locked_delete_sharing_violation_discovers_owner_and_plain_retry_preserves_semantics() {
        for permanent in [false, true] {
            let mut state = AppViewState::default();
            let original = lock_delete_request(permanent);
            finish_delete_with_code(&mut state, original.clone(), 32);
            let discovery = state
                .take_pending_lock_recovery_command()
                .expect("one discovery command");
            assert!(state.take_pending_lock_recovery_command().is_none());
            let context = discovery.context().expect("discovery context").clone();
            assert_eq!(
                state.apply_service_event(explorer_model::ExplorerEvent::LockOwnersDiscovered {
                    context,
                    outcome: explorer_model::LockOwnerDiscoveryTerminal::Ready(vec![
                        eligible_lock_owner(),
                    ]),
                }),
                explorer_model::WindowEventOutcome::Applied
            );
            let retry = state.retry_locked_delete().expect("explicit retry");
            let explorer_model::ExplorerCommand::ExecuteFileOperation { request, .. } = retry
            else {
                panic!("retry command");
            };
            assert_eq!(request, original);
            assert!(
                state.retry_locked_delete().is_none(),
                "one click submits once"
            );
        }
    }

    #[test]
    fn locked_delete_close_owner_revalidates_through_service_then_retries_once() {
        let mut state = AppViewState::default();
        let original = lock_delete_request(false);
        finish_delete_with_code(&mut state, original.clone(), 33);
        let discovery = state
            .take_pending_lock_recovery_command()
            .expect("discovery");
        let discovery_context = discovery.context().expect("context").clone();
        let owner = eligible_lock_owner();
        let _ = state.apply_service_event(explorer_model::ExplorerEvent::LockOwnersDiscovered {
            context: discovery_context,
            outcome: explorer_model::LockOwnerDiscoveryTerminal::Ready(vec![owner.clone()]),
        });
        let close = state
            .close_lock_owners_and_retry()
            .expect("explicit graceful close");
        let close_context = close.context().expect("close context").clone();
        assert!(state.close_lock_owners_and_retry().is_none());
        assert_eq!(
            state.apply_service_event(explorer_model::ExplorerEvent::LockOwnersClosed {
                context: close_context,
                outcome: explorer_model::LockOwnerCloseTerminal::Closed(vec![
                    explorer_model::LockOwnerCloseOutcome {
                        identity: owner.identity,
                        result: explorer_model::LockOwnerCloseResult::Closed,
                    },
                ]),
            }),
            explorer_model::WindowEventOutcome::Applied
        );
        let retry = state
            .take_pending_lock_recovery_command()
            .expect("exactly one retry");
        assert!(state.take_pending_lock_recovery_command().is_none());
        assert!(matches!(
            retry,
            explorer_model::ExplorerCommand::ExecuteFileOperation { request, .. } if request == original
        ));
    }

    #[test]
    fn locked_delete_non_lock_failure_stale_terminal_and_cancel_never_close_or_retry() {
        let mut state = AppViewState::default();
        finish_delete_with_code(&mut state, lock_delete_request(false), 5);
        assert!(state.lock_recovery().is_none());
        assert!(state.take_pending_lock_recovery_command().is_none());

        finish_delete_with_code(&mut state, lock_delete_request(false), 32);
        let discovery = state
            .take_pending_lock_recovery_command()
            .expect("discovery");
        let mut stale_context = discovery.context().expect("context").clone();
        stale_context.request_id = explorer_common::RequestId::new();
        assert_eq!(
            state.apply_service_event(explorer_model::ExplorerEvent::LockOwnersDiscovered {
                context: stale_context,
                outcome: explorer_model::LockOwnerDiscoveryTerminal::Ready(vec![
                    eligible_lock_owner(),
                ]),
            }),
            explorer_model::WindowEventOutcome::IgnoredStale
        );
        assert!(state.cancel_lock_recovery());
        assert!(
            discovery
                .context()
                .expect("context")
                .cancellation
                .is_cancelled()
        );
        assert!(state.retry_locked_delete().is_none());
        assert!(state.close_lock_owners_and_retry().is_none());
    }

    #[test]
    fn locked_delete_partial_close_duplicate_event_and_navigation_are_safe() {
        let mut state = AppViewState::default();
        finish_delete_with_code(&mut state, lock_delete_request(false), 32);
        let discovery = state
            .take_pending_lock_recovery_command()
            .expect("discovery");
        let discovery_context = discovery.context().expect("context").clone();
        let owner = eligible_lock_owner();
        let ready = explorer_model::ExplorerEvent::LockOwnersDiscovered {
            context: discovery_context,
            outcome: explorer_model::LockOwnerDiscoveryTerminal::Ready(vec![owner.clone()]),
        };
        assert_eq!(
            state.apply_service_event(ready.clone()),
            explorer_model::WindowEventOutcome::Applied
        );
        assert_eq!(
            state
                .lock_recovery()
                .map(super::LockRecoveryUiState::focused_target),
            Some(super::LockRecoveryFocusTarget::CloseAndRetry)
        );
        let surface_before_modal_tab = state.focused_surface();
        let trace = crate::actions::dispatch_action(
            &mut state,
            crate::actions::ExplorerAction::FocusNext,
            crate::actions::ActionSource::Keyboard,
        );
        assert_eq!(trace.handled_surface, surface_before_modal_tab);
        assert_eq!(
            state
                .lock_recovery()
                .map(super::LockRecoveryUiState::focused_target),
            Some(super::LockRecoveryFocusTarget::Retry)
        );
        let _ = crate::actions::dispatch_action(
            &mut state,
            crate::actions::ExplorerAction::FocusNext,
            crate::actions::ActionSource::Keyboard,
        );
        assert_eq!(
            state
                .lock_recovery()
                .map(super::LockRecoveryUiState::focused_target),
            Some(super::LockRecoveryFocusTarget::Cancel)
        );
        let _ = crate::actions::dispatch_action(
            &mut state,
            crate::actions::ExplorerAction::FocusPrevious,
            crate::actions::ActionSource::Keyboard,
        );
        assert_eq!(
            state
                .lock_recovery()
                .map(super::LockRecoveryUiState::focused_target),
            Some(super::LockRecoveryFocusTarget::Retry)
        );
        assert_eq!(
            state.apply_service_event(ready),
            explorer_model::WindowEventOutcome::IgnoredStale
        );
        let close = state.close_lock_owners_and_retry().expect("close command");
        assert_eq!(
            state.apply_service_event(explorer_model::ExplorerEvent::LockOwnersClosed {
                context: close.context().expect("context").clone(),
                outcome: explorer_model::LockOwnerCloseTerminal::Partial(vec![
                    explorer_model::LockOwnerCloseOutcome {
                        identity: owner.identity,
                        result: explorer_model::LockOwnerCloseResult::Refused,
                    },
                ]),
            }),
            explorer_model::WindowEventOutcome::Applied
        );
        assert!(state.take_pending_lock_recovery_command().is_none());
        assert_eq!(
            state.lock_recovery().map(|value| value.phase),
            Some(super::LockRecoveryPhase::Partial)
        );
        let _ = state.begin_active_navigation(
            explorer_model::LocationDescriptor::file_system(r"C:\elsewhere"),
            false,
        );
        assert!(state.lock_recovery().is_none());
    }

    #[test]
    fn locked_delete_retry_limit_suppresses_unbounded_resubmission() {
        let mut state = AppViewState::default();
        let original = lock_delete_request(false);
        finish_delete_with_code(&mut state, original, 32);
        for attempt in 1..=explorer_common::RoadmapLimits::default().lock_recovery_max_retries {
            let discovery = state
                .take_pending_lock_recovery_command()
                .expect("discovery");
            let _ =
                state.apply_service_event(explorer_model::ExplorerEvent::LockOwnersDiscovered {
                    context: discovery.context().expect("context").clone(),
                    outcome: explorer_model::LockOwnerDiscoveryTerminal::Empty,
                });
            let retry = state.retry_locked_delete().expect("bounded retry");
            let context = retry.context().expect("retry context").clone();
            let error = explorer_common::ExplorerError::new(
                explorer_common::ExplorerErrorKind::Availability,
                "delete retry",
                true,
                "still locked",
                "controlled lock",
            )
            .with_native_code(32);
            let _ = state.apply_service_event(explorer_model::ExplorerEvent::OperationFinished {
                context,
                outcome: explorer_model::OperationTerminal::Failed(error),
            });
            if attempt < explorer_common::RoadmapLimits::default().lock_recovery_max_retries {
                assert!(state.pending_lock_recovery_command.is_some());
            }
        }
        assert!(state.take_pending_lock_recovery_command().is_none());
        assert_eq!(
            state.lock_recovery().map(|value| value.phase),
            Some(super::LockRecoveryPhase::Unavailable)
        );
        assert!(state.retry_locked_delete().is_none());
    }
}
