#![cfg_attr(
    not(test),
    deny(
        clippy::unwrap_used,
        clippy::expect_used,
        clippy::panic,
        clippy::todo,
        clippy::unimplemented
    )
)]
//! GPUI-facing presentation boundary. This crate intentionally has no Shell dependency.
#![allow(
    clippy::must_use_candidate,
    reason = "declarative GPUI view builders and state queries are routinely composed or conditionally ignored"
)]

pub mod actions;
pub mod automation;
pub mod chrome;
pub mod code_lines_column;
pub mod diagnostics;
pub mod file_view;
mod fluent_assets;
pub mod folder_size_column;
pub mod size_map_view;
pub use fluent_assets::ExplorerAssets;
mod formatting;
pub use formatting::format_file_size;
pub mod focus;
pub mod geometry;
pub mod harness;
pub mod icons;
pub mod interaction;
pub mod layout;
pub mod navigation_pane;
pub mod performance;
mod pointer_capture;
pub use pointer_capture::{PointerCaptureFactory, PointerCaptureSession};
pub mod qos;
pub mod state;
pub mod theme;
pub mod typography;

use std::{
    collections::{HashMap, HashSet, VecDeque},
    sync::Arc,
    time::{Duration, Instant},
};

const SHELL_TEXTURE_CACHE_CAPACITY: usize = 512;
const SHELL_TEXTURE_CACHE_BYTE_BUDGET: usize = 64 * 1024 * 1024;
const BASE_ICON_CACHE_CAPACITY: usize = 256;
const BASE_ICON_CACHE_BYTE_BUDGET: usize = 32 * 1024 * 1024;
const FILE_VIEWPORT_ICON_REQUEST_CAP: usize = 64;
const FOREGROUND_SERVICE_EVENT_CAPACITY: usize = 512;
const ENRICHMENT_SERVICE_EVENT_CAPACITY: usize = 512;

fn prelayout_icon_range(item_count: usize, layout_ready: bool) -> Option<std::ops::Range<usize>> {
    (!layout_ready).then_some(0..item_count.min(FILE_VIEWPORT_ICON_REQUEST_CAP))
}

fn folder_size_result_is_current(
    result: &folder_size_column::FolderSizeResultV1,
    current: &explorer_model::RequestContext,
) -> bool {
    result.context.tab_id == current.tab_id && result.context.generation == current.generation
}

fn prime_top_icon_range(
    item_count: usize,
    scroll_offset: f32,
    range: std::ops::Range<usize>,
) -> std::ops::Range<usize> {
    if scroll_offset <= f32::EPSILON {
        0..range
            .end
            .max(item_count.min(FILE_VIEWPORT_ICON_REQUEST_CAP))
    } else {
        range
    }
}

fn is_enrichment_service_event(event: &explorer_model::ExplorerEvent) -> bool {
    matches!(
        event,
        explorer_model::ExplorerEvent::ShellIconLoaded { .. }
            | explorer_model::ExplorerEvent::ShellIconFailed { .. }
            | explorer_model::ExplorerEvent::ThumbnailFinished { .. }
    )
}

fn file_icon_cache_key(
    entry: &explorer_model::FileEntry,
    theme: explorer_model::ShellIconTheme,
    dpi: u16,
    logical_size: u16,
    generation: u64,
) -> explorer_model::ShellIconKey {
    let mut key = navigation_pane::file_icon_key_for_size(entry, theme, dpi, logical_size);
    key.association_generation = generation;
    key.overlay_generation = generation;
    key
}

#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "the finite scale-derived DPI is rounded and clamped to the complete u16 range"
)]
fn dpi_from_scale(scale_factor: f32) -> u16 {
    (scale_factor * 96.0)
        .round()
        .clamp(96.0, f32::from(u16::MAX)) as u16
}

fn physical_client_to_logical(value: f32, scale_factor: f32) -> Option<f32> {
    (value.is_finite() && scale_factor.is_finite() && scale_factor > 0.0)
        .then_some(value / scale_factor)
}

fn preview_virtual_key(key: &str) -> Option<u32> {
    if key.len() == 1 {
        let byte = key.as_bytes()[0];
        if byte.is_ascii_alphanumeric() {
            return Some(u32::from(byte.to_ascii_uppercase()));
        }
    }
    Some(match key {
        "backspace" => 0x08,
        "enter" => 0x0D,
        "escape" => 0x1B,
        "space" => 0x20,
        "pageup" => 0x21,
        "pagedown" => 0x22,
        "end" => 0x23,
        "home" => 0x24,
        "left" => 0x25,
        "up" => 0x26,
        "right" => 0x27,
        "down" => 0x28,
        "delete" => 0x2E,
        "f1" => 0x70,
        "f2" => 0x71,
        "f3" => 0x72,
        "f4" => 0x73,
        "f5" => 0x74,
        "f6" => 0x75,
        "f7" => 0x76,
        "f8" => 0x77,
        "f9" => 0x78,
        "f10" => 0x79,
        "f11" => 0x7A,
        "f12" => 0x7B,
        _ => return None,
    })
}

fn action_for_host_context_command(
    command: explorer_model::ContextMenuHostCommand,
) -> ExplorerAction {
    match command {
        explorer_model::ContextMenuHostCommand::Open => ExplorerAction::OpenFocused,
        explorer_model::ContextMenuHostCommand::Cut => ExplorerAction::CutSelected,
        explorer_model::ContextMenuHostCommand::Copy => ExplorerAction::CopySelected,
        explorer_model::ContextMenuHostCommand::CopyPath => ExplorerAction::CopySelectedPaths,
        explorer_model::ContextMenuHostCommand::CreateShortcut => {
            ExplorerAction::CreateShortcutSelected
        }
        explorer_model::ContextMenuHostCommand::Delete => ExplorerAction::RecycleDeleteSelected,
        explorer_model::ContextMenuHostCommand::Rename => ExplorerAction::BeginRenameFocused,
        explorer_model::ContextMenuHostCommand::Share => ExplorerAction::ShareSelected,
        explorer_model::ContextMenuHostCommand::PinToStart => ExplorerAction::PinSelectedToStart,
        explorer_model::ContextMenuHostCommand::ToggleQuickAccess => {
            ExplorerAction::AddSelectedToFavorites
        }
        explorer_model::ContextMenuHostCommand::Properties => {
            ExplorerAction::ShowPropertiesSelected
        }
    }
}

fn captured_scrollbar_axis_to_logical(
    kind: interaction::ScrollbarKind,
    position: (f32, f32),
    scale_factor: f32,
) -> Option<f32> {
    let x = physical_client_to_logical(position.0, scale_factor)?;
    let y = physical_client_to_logical(position.1, scale_factor)?;
    Some(if kind == interaction::ScrollbarKind::FileViewHorizontal {
        x
    } else {
        y
    })
}

/// Merges directory safety batches drained for one UI transaction. Provider batch caps remain
/// cancellation boundaries; the model receives one mutation batch per correlated request/frame.
#[cfg(test)]
fn coalesce_directory_events(
    events: Vec<explorer_model::ExplorerEvent>,
) -> Vec<explorer_model::ExplorerEvent> {
    let mut coalesced = Vec::<explorer_model::ExplorerEvent>::with_capacity(events.len());
    for event in events {
        if let explorer_model::ExplorerEvent::DirectoryBatch { context, entries } = event {
            if let Some(explorer_model::ExplorerEvent::DirectoryBatch {
                entries: existing, ..
            }) = coalesced.iter_mut().find(|candidate| {
                matches!(candidate, explorer_model::ExplorerEvent::DirectoryBatch { context: candidate_context, .. } if candidate_context == &context)
            }) {
                existing.extend(entries);
            } else {
                coalesced.push(explorer_model::ExplorerEvent::DirectoryBatch { context, entries });
            }
        } else {
            coalesced.push(event);
        }
    }
    coalesced
}

struct VisibleItemIconCache {
    entries: HashMap<explorer_model::ShellIconKey, Arc<RenderImage>>,
    order: VecDeque<explorer_model::ShellIconKey>,
    latest_association: HashMap<explorer_model::LocationDescriptor, u64>,
    latest_overlay: HashMap<explorer_model::LocationDescriptor, u64>,
    capacity: usize,
    byte_budget: usize,
    current_bytes: usize,
    hits: u64,
    misses: u64,
    negative_hits: u64,
    evictions: u64,
    stale_rejections: u64,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
struct VisibleItemIconCacheStats {
    entries: usize,
    entry_budget: usize,
    current_bytes: usize,
    byte_budget: usize,
    hits: u64,
    misses: u64,
    negative_hits: u64,
    evictions: u64,
    stale_rejections: u64,
}

impl Default for VisibleItemIconCache {
    fn default() -> Self {
        Self {
            entries: HashMap::new(),
            order: VecDeque::new(),
            latest_association: HashMap::new(),
            latest_overlay: HashMap::new(),
            capacity: SHELL_TEXTURE_CACHE_CAPACITY,
            byte_budget: SHELL_TEXTURE_CACHE_BYTE_BUDGET,
            current_bytes: 0,
            hits: 0,
            misses: 0,
            negative_hits: 0,
            evictions: 0,
            stale_rejections: 0,
        }
    }
}

impl VisibleItemIconCache {
    fn record_negative_hit(&mut self) {
        self.negative_hits = self.negative_hits.saturating_add(1);
    }

    fn clear_overlay_dependent(&mut self) {
        self.entries.clear();
        self.order.clear();
        self.latest_association.clear();
        self.latest_overlay.clear();
        self.current_bytes = 0;
    }

    fn insert(&mut self, key: &explorer_model::ShellIconKey, texture: Arc<RenderImage>) -> bool {
        let latest = self
            .latest_association
            .entry(key.location.clone())
            .or_default();
        let latest_overlay = self.latest_overlay.entry(key.location.clone()).or_default();
        if key.association_generation < *latest || key.overlay_generation < *latest_overlay {
            self.stale_rejections = self.stale_rejections.saturating_add(1);
            return false;
        }
        if key.association_generation > *latest || key.overlay_generation > *latest_overlay {
            *latest = key.association_generation;
            *latest_overlay = key.overlay_generation;
            self.entries.retain(|candidate, _| {
                candidate.location != key.location
                    || candidate.association_generation >= key.association_generation
                        && candidate.overlay_generation >= key.overlay_generation
            });
            self.order
                .retain(|candidate| self.entries.contains_key(candidate));
            self.recalculate_bytes();
        }
        let replaced = self.entries.insert(key.clone(), texture);
        if replaced.is_none() {
            self.current_bytes = self.current_bytes.saturating_add(estimated_icon_bytes(key));
        }
        self.touch(key);
        while self.entries.len() > self.capacity || self.current_bytes > self.byte_budget {
            if let Some(oldest) = self.order.pop_front()
                && self.entries.remove(&oldest).is_some()
            {
                self.current_bytes = self
                    .current_bytes
                    .saturating_sub(estimated_icon_bytes(&oldest));
                self.evictions = self.evictions.saturating_add(1);
            }
        }
        true
    }

    fn get(&mut self, key: &explorer_model::ShellIconKey) -> Option<Arc<RenderImage>> {
        if let Some(texture) = self.entries.get(key).cloned() {
            self.hits = self.hits.saturating_add(1);
            self.touch(key);
            Some(texture)
        } else {
            self.misses = self.misses.saturating_add(1);
            None
        }
    }

    fn get_compatible_navigation_icon(
        &mut self,
        location: &explorer_model::LocationDescriptor,
        theme: explorer_model::ShellIconTheme,
        dpi: u16,
    ) -> Option<(explorer_model::ShellIconKey, Arc<RenderImage>)> {
        let exact = navigation_pane::shell_icon_key(location, theme, dpi);
        let key = if self.entries.contains_key(&exact) {
            exact
        } else {
            self.entries
                .keys()
                .filter(|key| key.location == *location && key.theme == theme && key.dpi == dpi)
                .max_by_key(|key| {
                    (
                        key.association_generation,
                        key.overlay_generation,
                        key.item_id.is_some(),
                        key.size_bucket == exact.size_bucket,
                    )
                })
                .cloned()?
        };
        self.get(&key).map(|texture| (key, texture))
    }

    fn touch(&mut self, key: &explorer_model::ShellIconKey) {
        self.order.retain(|candidate| candidate != key);
        self.order.push_back(key.clone());
    }

    fn invalidate_environment(&mut self, dpi: u16, theme: explorer_model::ShellIconTheme) {
        self.entries
            .retain(|key, _| key.dpi == dpi && key.theme == theme);
        self.order.retain(|key| self.entries.contains_key(key));
        self.recalculate_bytes();
    }

    fn recalculate_bytes(&mut self) {
        self.current_bytes = self
            .entries
            .keys()
            .map(estimated_icon_bytes)
            .fold(0_usize, usize::saturating_add);
    }

    fn stats(&self) -> VisibleItemIconCacheStats {
        VisibleItemIconCacheStats {
            entries: self.entries.len(),
            entry_budget: self.capacity,
            current_bytes: self.current_bytes,
            byte_budget: self.byte_budget,
            hits: self.hits,
            misses: self.misses,
            negative_hits: self.negative_hits,
            evictions: self.evictions,
            stale_rejections: self.stale_rejections,
        }
    }
}

fn estimated_icon_bytes(key: &explorer_model::ShellIconKey) -> usize {
    usize::from(key.size_bucket)
        .saturating_mul(usize::from(key.size_bucket))
        .saturating_mul(4)
}

#[derive(Default)]
struct BaseIconCache {
    entries: HashMap<explorer_model::BaseIconKey, Arc<RenderImage>>,
    hashes: HashMap<explorer_model::BaseIconKey, u64>,
    order: VecDeque<explorer_model::BaseIconKey>,
    current_bytes: usize,
    hits: u64,
    misses: u64,
    evictions: u64,
}

impl BaseIconCache {
    fn insert(&mut self, key: explorer_model::BaseIconKey, texture: Arc<RenderImage>, hash: u64) {
        let replaced = self.entries.insert(key.clone(), texture).is_some();
        self.hashes.insert(key.clone(), hash);
        if !replaced {
            self.current_bytes = self.current_bytes.saturating_add(base_icon_bytes(&key));
        }
        self.order.retain(|candidate| candidate != &key);
        self.order.push_back(key);
        while self.entries.len() > BASE_ICON_CACHE_CAPACITY
            || self.current_bytes > BASE_ICON_CACHE_BYTE_BUDGET
        {
            if let Some(oldest) = self.order.pop_front()
                && self.entries.remove(&oldest).is_some()
            {
                self.current_bytes = self.current_bytes.saturating_sub(base_icon_bytes(&oldest));
                self.hashes.remove(&oldest);
                self.evictions = self.evictions.saturating_add(1);
            }
        }
    }

    fn get(&mut self, key: &explorer_model::BaseIconKey) -> Option<Arc<RenderImage>> {
        if let Some(texture) = self.entries.get(key).cloned() {
            self.hits = self.hits.saturating_add(1);
            self.order.retain(|candidate| candidate != key);
            self.order.push_back(key.clone());
            Some(texture)
        } else {
            self.misses = self.misses.saturating_add(1);
            None
        }
    }

    fn invalidate_environment(&mut self, dpi: u16, theme: explorer_model::ShellIconTheme) {
        self.entries
            .retain(|key, _| key.dpi == dpi && key.theme == theme);
        self.hashes.retain(|key, _| self.entries.contains_key(key));
        self.order.retain(|key| self.entries.contains_key(key));
        self.current_bytes = self.entries.keys().map(base_icon_bytes).sum();
    }
}

fn icon_payload_hash(payload: &explorer_model::ShellIconPayload) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    payload.width.hash(&mut hasher);
    payload.height.hash(&mut hasher);
    payload.rgba.hash(&mut hasher);
    hasher.finish()
}

fn base_icon_bytes(key: &explorer_model::BaseIconKey) -> usize {
    usize::from(key.size_bucket)
        .saturating_mul(usize::from(key.size_bucket))
        .saturating_mul(4)
}

fn initial_icon_epochs() -> explorer_model::IconInvalidationEpochs {
    let mut epochs = explorer_model::IconInvalidationEpochs::default();
    epochs.advance_association();
    epochs
}

fn base_icon_request_location(
    class: &explorer_model::BaseIconClass,
) -> Option<explorer_model::LocationDescriptor> {
    let path = match class {
        explorer_model::BaseIconClass::Folder => {
            navigation_pane::GENERIC_SHELL_FOLDER_ICON_PATH.to_owned()
        }
        explorer_model::BaseIconClass::Extension(extension) => {
            format!(r"C:\__super_explorer_base__.{extension}")
        }
        explorer_model::BaseIconClass::ExtensionlessFile
        | explorer_model::BaseIconClass::Identity(_) => return None,
    };
    Some(explorer_model::LocationDescriptor::file_system(path))
}

fn uses_shared_base_icon(class: &explorer_model::BaseIconClass) -> bool {
    matches!(class, explorer_model::BaseIconClass::Folder)
}

fn advance_item_overlay_epoch(
    epochs: &mut HashMap<explorer_model::ShellItemId, u64>,
    id: &explorer_model::ShellItemId,
) -> u64 {
    let epoch = epochs.entry(id.clone()).or_default();
    *epoch = epoch.saturating_add(1);
    *epoch
}

use explorer_model::{ExplorerService, ExplorerServiceError, WorkspaceModel};
use gpui::{
    AnyWindowHandle, App, Bounds, ClipboardItem, Context, Focusable, IntoElement, Render,
    RenderImage, Role, SharedString, Window, WindowBounds, WindowOptions, div, prelude::*, px,
    size,
};
use gpui_elements::editable_text::{EditableTextState, StringStorage, TextChanged};

use crate::{
    actions::{ActionSource, ExplorerAction, dispatch_action},
    layout::LayoutTokens,
    performance::measure_callback,
    state::AppViewState,
    theme::{ThemeMode, ThemeTokens},
    typography::TypographyTokens,
};

pub const INITIAL_WINDOW_WIDTH: f32 = 1_120.0;
pub const INITIAL_WINDOW_HEIGHT: f32 = 720.0;
pub const MINIMUM_WINDOW_WIDTH: f32 = 640.0;
pub const MINIMUM_WINDOW_HEIGHT: f32 = 480.0;
pub const PRODUCT_NAME: &str = "SuperExplorer";

/// Projects the native window/taskbar title from the last successfully resolved active location.
/// Address-bar drafts are deliberately excluded from this projection.
pub fn active_window_title(tabs: &explorer_model::ExplorerWindowState) -> String {
    window_title_for_history_entry(tabs.active_tab().history.current())
}

/// Projects one resolved history entry into a user-facing native window title.
pub fn window_title_for_history_entry(entry: Option<&explorer_model::HistoryEntry>) -> String {
    let Some(entry) = entry else {
        return PRODUCT_NAME.to_owned();
    };
    if let Some(path) = entry.location.path() {
        let title = path.as_os_str().to_string_lossy();
        if !title.trim().is_empty() {
            return title.into_owned();
        }
    }

    let display_title = entry.display_title.trim();
    let exposes_internal_identity = match &entry.location {
        explorer_model::LocationDescriptor::ParsingName(parsing_name) => {
            display_title.eq_ignore_ascii_case(parsing_name.trim())
        }
        explorer_model::LocationDescriptor::FileSystem(_)
        | explorer_model::LocationDescriptor::ShellNamespace(_)
        | explorer_model::LocationDescriptor::KnownFolder(_) => false,
    };
    if display_title.is_empty() || exposes_internal_identity {
        PRODUCT_NAME.to_owned()
    } else {
        display_title.to_owned()
    }
}

fn default_durable_window_placement() -> explorer_model::PersistedWindowPlacement {
    explorer_model::PersistedWindowPlacement {
        normal_bounds: explorer_model::PersistedRect {
            left: 0,
            top: 0,
            width: 1_120,
            height: 720,
        },
        source_work_area: explorer_model::PersistedRect {
            left: 0,
            top: 0,
            width: 1_920,
            height: 1_080,
        },
        source_dpi: 96,
        maximized: false,
    }
}

#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    reason = "finite GPUI screen coordinates are rounded and saturated to the persisted i32 contract"
)]
fn persisted_rect(bounds: Bounds<gpui::Pixels>) -> explorer_model::PersistedRect {
    let coordinate = |value: gpui::Pixels| {
        f32::from(value)
            .round()
            .clamp(i32::MIN as f32, i32::MAX as f32) as i32
    };
    explorer_model::PersistedRect {
        left: coordinate(bounds.origin.x),
        top: coordinate(bounds.origin.y),
        width: coordinate(bounds.size.width).max(1),
        height: coordinate(bounds.size.height).max(1),
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum VisualFixtureState {
    Empty,
    Populated,
    Error,
    MultiTab,
    Operation,
    DragCue,
    Search,
    Focused,
}

const _: () = assert!(INITIAL_WINDOW_WIDTH >= MINIMUM_WINDOW_WIDTH);
const _: () = assert!(INITIAL_WINDOW_HEIGHT >= MINIMUM_WINDOW_HEIGHT);
const _: () = assert!(MINIMUM_WINDOW_WIDTH > 0.0);
const _: () = assert!(MINIMUM_WINDOW_HEIGHT > 0.0);

/// Initial UI state backed by the pure workspace model.
#[derive(Debug, Default)]
pub struct ExplorerUiState {
    model: WorkspaceModel,
}

impl ExplorerUiState {
    /// Provides read-only access while the first GPUI entity is introduced.
    pub const fn model(&self) -> &WorkspaceModel {
        &self.model
    }
}

/// Theme and layout data owned by the root and shared with feature components.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct UiTokens {
    pub theme: ThemeTokens,
    pub layout: LayoutTokens,
    pub typography: TypographyTokens,
}

impl Default for UiTokens {
    fn default() -> Self {
        Self {
            theme: ThemeTokens::light(),
            layout: LayoutTokens::WINDOWS_11,
            typography: TypographyTokens::WINDOWS_11_ZH_TW,
        }
    }
}

/// Minimal render root used by the M0 executable checkpoint.
pub struct ExplorerRoot {
    tokens: UiTokens,
    state: AppViewState,
    service: Option<Arc<dyn ExplorerService>>,
    folder_scripts: Option<explorer_automation::FolderScriptHandle>,
    pending_foreground_events: VecDeque<explorer_model::ExplorerEvent>,
    pending_enrichment_events: VecDeque<explorer_model::ExplorerEvent>,
    enrichment_retry_needed: bool,
    service_qos: explorer_jobs::InteractionFirstQos,
    service_delivery: qos::UiDeliveryCounters,
    navigation_started: HashMap<explorer_model::RequestId, Instant>,
    first_batch_seen: HashSet<explorer_model::RequestId>,
    shell_icons: VisibleItemIconCache,
    base_icons: BaseIconCache,
    icon_epochs: explorer_model::IconInvalidationEpochs,
    pending_base_icons: HashMap<explorer_model::ShellIconKey, explorer_model::BaseIconKey>,
    pending_visible_bases: HashMap<explorer_model::ShellIconKey, explorer_model::BaseIconKey>,
    failed_base_icons: HashSet<explorer_model::BaseIconKey>,
    item_overlay_epochs: HashMap<explorer_model::ShellItemId, u64>,
    negative_icon_keys: HashSet<explorer_model::ShellIconKey>,
    negative_icon_order: VecDeque<explorer_model::ShellIconKey>,
    shell_icon_dpi: u16,
    pending_icon_keys: HashSet<explorer_model::ShellIconKey>,
    pending_icon_contexts: HashMap<explorer_model::ShellIconKey, explorer_model::RequestContext>,
    pending_thumbnail_keys: HashSet<explorer_model::ThumbnailRequestKey>,
    thumbnail_scheduler: explorer_jobs::ThumbnailScheduler,
    thumbnail_memory_cache: explorer_jobs::ThumbnailMemoryCache,
    thumbnail_requests: HashMap<
        explorer_model::ThumbnailRequestKey,
        (
            explorer_model::RequestContext,
            explorer_model::LocationDescriptor,
            explorer_model::ThumbnailConsumer,
        ),
    >,
    thumbnail_presentations:
        HashMap<explorer_model::ThumbnailRequestKey, explorer_model::ShellIconKey>,
    preview_thumbnail_key: Option<explorer_model::ThumbnailRequestKey>,
    preview_texture: Option<Arc<RenderImage>>,
    preview_thumbnail_failed: bool,
    preview_coordinator: explorer_jobs::PreviewCoordinator,
    preview_clock: Instant,
    preview_selection_signature: Option<(
        explorer_model::TabId,
        Option<explorer_model::ShellItemId>,
        bool,
    )>,
    preview_host_boundary: Option<(u64, i32, i32, u32, u32, u32)>,
    navigation_scroll: gpui::ScrollHandle,
    file_scroll: gpui::ScrollHandle,
    file_viewport_width: f32,
    file_performance: Arc<performance::FileViewPerformanceCounters>,
    focus_handle: Option<gpui::FocusHandle>,
    breadcrumb_menu_focus: Option<gpui::FocusHandle>,
    command_menu_focus: Option<gpui::FocusHandle>,
    address_input: Option<gpui::Entity<EditableTextState>>,
    search_input: Option<gpui::Entity<EditableTextState>>,
    rename_input: Option<gpui::Entity<EditableTextState>>,
    pointer_capture_factory: Option<PointerCaptureFactory>,
    pointer_capture: Option<Box<dyn PointerCaptureSession>>,
    durable_state_observer: Option<DurableStateObserver>,
    durable_window_placement: explorer_model::PersistedWindowPlacement,
    session_reset_observer: Option<SessionResetObserver>,
    broker_retry_observer: Option<BrokerRetryObserver>,
    last_window_title: Option<String>,
    navigation_history_release_deadline: Option<Instant>,
    safe_mode_offers: Vec<SafeModeOfferV1>,
    safe_mode_confirm: Option<SafeModeConfirmObserverV1>,
    safe_mode_confirmation_error: Option<String>,
    extension_ui_pump: Option<Box<dyn ExtensionUiPumpPortV1>>,
    visual_column_runtime: Option<folder_size_column::VisualColumnRuntimeHandleV1>,
    folder_size_visuals: Option<folder_size_column::FolderSizeColumnVisuals>,
    folder_size_requested: HashSet<(
        explorer_model::TabId,
        explorer_model::Generation,
        explorer_model::ShellItemId,
    )>,
    folder_size_display_override: Option<folder_size_column::FolderSizeDisplayMode>,
    code_lines_runtime: Option<code_lines_column::CodeLinesRuntimeHandleV1>,
    code_lines_visuals: Option<code_lines_column::CodeLinesColumnVisuals>,
    code_lines_requested: HashSet<(
        explorer_model::TabId,
        explorer_model::Generation,
        explorer_model::ShellItemId,
    )>,
    code_lines_display_override: Option<code_lines_column::CodeLinesDisplayMode>,
    size_map_runtime: Option<size_map_view::SizeMapRuntimeHandleV1>,
    size_map_visuals: Option<size_map_view::SizeMapVisualsV1>,
    size_map_visual_context: Option<explorer_model::RequestContext>,
    size_map_requested: HashSet<(
        explorer_model::TabId,
        explorer_model::Generation,
        explorer_model::ShellItemId,
    )>,
}

/// Receives owned durable model snapshots after accepted reducer transitions.
pub type DurableStateObserver = Arc<
    dyn Fn(
            explorer_model::ExplorerWindowState,
            bool,
            Vec<explorer_model::PersistedQuickAccessPin>,
            explorer_model::PersistedWindowPlacement,
        ) -> bool
        + Send
        + Sync,
>;
/// Sends explicit reset commands to the app-owned background session store.
pub type SessionResetObserver =
    Arc<dyn Fn(explorer_model::SessionResetScope) -> bool + Send + Sync>;
pub type BrokerRetryObserver = Arc<dyn Fn() -> state::BrokerUiHealth + Send + Sync>;
/// Path-free Safe Mode identity shown before a native extension can be re-enabled.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SafeModeOfferV1 {
    /// Lifecycle-local opaque token returned unchanged to the application.
    pub presentation_token: u64,
    pub package_id: Option<String>,
    pub primary_interface_namespace: Option<u32>,
    pub primary_interface_value: Option<u64>,
    pub operation: String,
}
/// Application-owned confirmation bridge. `Ok(())` is the sole condition that removes an offer.
pub type SafeModeConfirmObserverV1 = Arc<dyn Fn(u64) -> Result<(), String> + Send + Sync>;

/// App-owned extension presentation pump. The UI crate only asks whether a
/// checked, coalesced extension invalidation is due; it never depends on the
/// extension-host crate or projects extension job identities itself.
pub trait ExtensionUiPumpPortV1 {
    /// Returns true only when one current, coalesced UI invalidation should be
    /// delivered on this GPUI thread.
    fn poll_due(&mut self, now: Instant) -> bool;
}

fn extension_ui_pump_due(pump: Option<&mut Box<dyn ExtensionUiPumpPortV1>>, now: Instant) -> bool {
    pump.is_some_and(|pump| pump.poll_due(now))
}

fn is_durable_action(action: &ExplorerAction) -> bool {
    matches!(
        action,
        ExplorerAction::NewTab
            | ExplorerAction::CloseActiveTab
            | ExplorerAction::CloseTab { .. }
            | ExplorerAction::ReorderTab { .. }
            | ExplorerAction::ActivateTab { .. }
            | ExplorerAction::NextTab
            | ExplorerAction::PreviousTab
            | ExplorerAction::SetViewMode(_)
            | ExplorerAction::SetExtensionView { .. }
            | ExplorerAction::ZoomView { .. }
            | ExplorerAction::SetColumnId(_)
            | ExplorerAction::SetSortDirection(_)
            | ExplorerAction::SetDetailsColumnWidth { .. }
            | ExplorerAction::AutoSizeDetailsColumn { .. }
            | ExplorerAction::EndDetailsColumnResize
            | ExplorerAction::EndSidePaneResize
            | ExplorerAction::ResetSidePaneWidth
            | ExplorerAction::ToggleDetailsPane
            | ExplorerAction::TogglePreviewPane
            | ExplorerAction::ToggleItemCheckBoxes
            | ExplorerAction::ToggleFileNameExtensions
            | ExplorerAction::ToggleHiddenItems
            | ExplorerAction::ToggleCompactView
            | ExplorerAction::ApplyFolderOptions
            | ExplorerAction::ConfirmFolderOptions
            | ExplorerAction::ResetFolderOptions
            | ExplorerAction::ToggleRestorePreviousSession
    )
}

fn is_passive_pointer_action(action: &ExplorerAction) -> bool {
    matches!(
        action,
        ExplorerAction::UpdateMarquee { .. }
            | ExplorerAction::UpdateFileDrag { .. }
            | ExplorerAction::CancelFileDrag
            | ExplorerAction::UpdateExternalDrag { .. }
            | ExplorerAction::UpdateDetailsColumnResize { .. }
            | ExplorerAction::EndDetailsColumnResize
            | ExplorerAction::UpdateSidePaneResize { .. }
            | ExplorerAction::EndSidePaneResize
            | ExplorerAction::UpdateScrollbarDrag { .. }
            | ExplorerAction::EndScrollbarDrag { .. }
            | ExplorerAction::UpdateNavigationPaneResize { .. }
            | ExplorerAction::EndNavigationPaneResize
    )
}

fn should_end_address_edit(action: &ExplorerAction, source: ActionSource) -> bool {
    let address_action = matches!(
        action,
        ExplorerAction::FocusAddress
            | ExplorerAction::EnterAddressEdit
            | ExplorerAction::UpdateAddressDraft(_)
            | ExplorerAction::SubmitAddress(_)
            | ExplorerAction::CancelAddressEdit
            | ExplorerAction::SubmitFocusedInput
            | ExplorerAction::CancelFocusedInput
    );
    !(address_action || source == ActionSource::Mouse && is_passive_pointer_action(action))
}

fn should_end_inline_rename(action: &ExplorerAction, source: ActionSource) -> bool {
    source == ActionSource::Mouse
        && !matches!(
            action,
            ExplorerAction::BeginRenameFocused
                | ExplorerAction::CommitInlineRename
                | ExplorerAction::CancelInlineRename
        )
        && !is_passive_pointer_action(action)
}

fn file_view_global_command_action(event: &gpui::KeyDownEvent) -> Option<ExplorerAction> {
    if event.keystroke.modifiers.control {
        match event.keystroke.key.as_str() {
            "a" => return Some(ExplorerAction::SelectAllItems),
            "i" => return Some(ExplorerAction::InvertSelection),
            "c" => return Some(ExplorerAction::CopySelected),
            "x" => return Some(ExplorerAction::CutSelected),
            "v" => return Some(ExplorerAction::Paste),
            _ => {}
        }
    }
    match event.keystroke.key.as_str() {
        "backspace" => Some(ExplorerAction::Back),
        "f3" => Some(ExplorerAction::FocusSearch),
        _ => None,
    }
}

fn file_view_item_command_action(
    event: &gpui::KeyDownEvent,
    current: usize,
) -> Option<ExplorerAction> {
    match event.keystroke.key.as_str() {
        "enter" if event.keystroke.modifiers.alt => Some(ExplorerAction::ShowPropertiesSelected),
        "enter" => Some(ExplorerAction::OpenItem {
            row_index: current,
            new_tab: event.keystroke.modifiers.control,
        }),
        "space" if event.keystroke.modifiers.control => {
            Some(ExplorerAction::SelectAdditionalItem { row_index: current })
        }
        "space" => Some(ExplorerAction::SelectItem { row_index: current }),
        "f2" => Some(ExplorerAction::BeginRenameFocused),
        "delete" if event.keystroke.modifiers.shift => Some(ExplorerAction::RequestPermanentDelete),
        "delete" => Some(ExplorerAction::RecycleDeleteSelected),
        _ => None,
    }
}

impl ExplorerRoot {
    fn remember_negative_icon(&mut self, key: explorer_model::ShellIconKey) {
        if self.negative_icon_keys.insert(key.clone()) {
            self.negative_icon_order.push_back(key);
        }
        while self.negative_icon_order.len() > 2_048 {
            if let Some(oldest) = self.negative_icon_order.pop_front() {
                self.negative_icon_keys.remove(&oldest);
            }
        }
    }

    fn refresh_tortoise_git_status(&mut self) -> bool {
        if !self.state.tortoise_git_available() {
            return false;
        }
        let newest_item_epoch = self
            .item_overlay_epochs
            .values()
            .copied()
            .max()
            .unwrap_or_default();
        self.icon_epochs.advance_overlay_past(newest_item_epoch);
        self.item_overlay_epochs.clear();
        self.shell_icons.clear_overlay_dependent();
        self.negative_icon_keys.clear();
        self.negative_icon_order.clear();
        self.pending_icon_keys.clear();
        self.pending_icon_contexts.clear();
        self.pending_visible_bases.clear();

        let thumbnail_consumers = self
            .thumbnail_requests
            .iter()
            .map(|(key, (_, _, consumer))| (key.clone(), *consumer))
            .collect::<Vec<_>>();
        for (key, consumer) in thumbnail_consumers {
            let _ = self.thumbnail_scheduler.cancel_consumer(&key, consumer);
        }
        self.pending_thumbnail_keys.clear();
        self.thumbnail_requests.clear();
        self.thumbnail_presentations.clear();

        let tab = self.state.tabs().active_tab();
        let context = explorer_model::RequestContext::new(tab.id, tab.generation);
        let entries = tab
            .visible_snapshot()
            .map(|snapshot| snapshot.entries().to_vec())
            .unwrap_or_default();
        self.submit_file_icon_loads(&context, &entries);
        self.submit_navigation_icon_loads();
        true
    }

    pub fn new(tokens: UiTokens) -> Self {
        Self {
            tokens,
            state: AppViewState::default(),
            service: None,
            folder_scripts: None,
            pending_foreground_events: VecDeque::new(),
            pending_enrichment_events: VecDeque::new(),
            enrichment_retry_needed: false,
            service_qos: explorer_jobs::InteractionFirstQos::default(),
            service_delivery: qos::UiDeliveryCounters::default(),
            navigation_started: HashMap::new(),
            first_batch_seen: HashSet::new(),
            shell_icons: VisibleItemIconCache::default(),
            base_icons: BaseIconCache::default(),
            icon_epochs: initial_icon_epochs(),
            pending_base_icons: HashMap::new(),
            pending_visible_bases: HashMap::new(),
            failed_base_icons: HashSet::new(),
            item_overlay_epochs: HashMap::new(),
            negative_icon_keys: HashSet::new(),
            negative_icon_order: VecDeque::new(),
            shell_icon_dpi: 96,
            pending_icon_keys: HashSet::new(),
            pending_icon_contexts: HashMap::new(),
            pending_thumbnail_keys: HashSet::new(),
            thumbnail_scheduler: explorer_jobs::ThumbnailScheduler::new(512, 4, 64 * 1024 * 1024),
            thumbnail_memory_cache: explorer_jobs::ThumbnailMemoryCache::new(
                128 * 1024 * 1024,
                2_048,
            ),
            thumbnail_requests: HashMap::new(),
            thumbnail_presentations: HashMap::new(),
            preview_thumbnail_key: None,
            preview_texture: None,
            preview_thumbnail_failed: false,
            preview_coordinator: explorer_jobs::PreviewCoordinator::new(Duration::from_millis(75)),
            preview_clock: Instant::now(),
            preview_selection_signature: None,
            preview_host_boundary: None,
            navigation_scroll: gpui::ScrollHandle::new(),
            file_scroll: gpui::ScrollHandle::new(),
            file_viewport_width: 0.0,
            file_performance: Arc::new(performance::FileViewPerformanceCounters::default()),
            focus_handle: None,
            breadcrumb_menu_focus: None,
            command_menu_focus: None,
            address_input: None,
            search_input: None,
            rename_input: None,
            pointer_capture_factory: None,
            pointer_capture: None,
            durable_state_observer: None,
            durable_window_placement: default_durable_window_placement(),
            session_reset_observer: None,
            broker_retry_observer: None,
            last_window_title: None,
            navigation_history_release_deadline: None,
            safe_mode_offers: Vec::new(),
            safe_mode_confirm: None,
            safe_mode_confirmation_error: None,
            extension_ui_pump: None,
            visual_column_runtime: None,
            folder_size_visuals: None,
            folder_size_requested: HashSet::new(),
            folder_size_display_override: None,
            code_lines_runtime: None,
            code_lines_visuals: None,
            code_lines_requested: HashSet::new(),
            code_lines_display_override: None,
            size_map_runtime: None,
            size_map_visuals: None,
            size_map_visual_context: None,
            size_map_requested: HashSet::new(),
        }
    }

    /// Installs the application-owned extension invalidation pump before the
    /// normal 16 ms service loop starts. Replacing a dormant pump is safe
    /// during composition; workers cannot reach this UI-owned object.
    pub fn attach_extension_ui_pump(&mut self, pump: Box<dyn ExtensionUiPumpPortV1>) {
        self.extension_ui_pump = Some(pump);
    }

    /// Connects the application-owned folder-size provider to the Details UI.
    /// The provider is deliberately narrow: it receives filesystem container
    /// paths and returns copied exact-byte values, never Shell/extension-host
    /// objects.
    pub fn attach_visual_column_runtime(
        &mut self,
        runtime: folder_size_column::VisualColumnRuntimeHandleV1,
    ) {
        let mut config = runtime.config();
        if !folder_size_column::is_supported_folder_size_descriptor(&config.descriptor)
            || !self
                .state
                .install_visual_column_descriptor(config.descriptor.clone())
        {
            tracing::warn!("rejected unsupported visual-column runtime configuration");
            return;
        }
        self.visual_column_runtime = Some(runtime);
        if let Some(display) = self.folder_size_display_override {
            config.folder_size_display = display;
        }
        self.folder_size_visuals = Some(folder_size_column::FolderSizeColumnVisuals {
            config,
            context: None,
            values: HashMap::new(),
        });
        self.folder_size_requested.clear();
    }

    /// Connects the one Rust tokei batch-column example to production Details.
    pub fn attach_code_lines_runtime(
        &mut self,
        runtime: code_lines_column::CodeLinesRuntimeHandleV1,
    ) {
        let mut config = runtime.config();
        if !code_lines_column::is_supported_code_lines_descriptor(&config.descriptor)
            || !self
                .state
                .install_code_lines_column_descriptor(config.descriptor.clone())
        {
            tracing::warn!("rejected unsupported Code lines runtime configuration");
            return;
        }
        if let Some(display) = self.code_lines_display_override {
            config.display = display;
        }
        self.code_lines_runtime = Some(runtime);
        self.code_lines_visuals = Some(code_lines_column::CodeLinesColumnVisuals {
            config,
            context: None,
            values: HashMap::new(),
            errors: HashMap::new(),
        });
        self.code_lines_requested.clear();
    }

    /// Connects the application-owned Size Map adapter. A malformed or
    /// unsupported configuration remains dormant and the built-in Details
    /// fallback continues to render.
    pub fn attach_size_map_runtime(&mut self, runtime: size_map_view::SizeMapRuntimeHandleV1) {
        let config = runtime.config();
        if !size_map_view::is_supported_size_map_config(&config) {
            tracing::warn!("rejected unsupported Size Map runtime configuration");
            return;
        }
        self.size_map_runtime = Some(runtime);
        self.size_map_visuals = Some(size_map_view::SizeMapVisualsV1::default());
        self.size_map_visual_context = None;
        self.size_map_requested.clear();
    }

    fn size_map_is_active(&self) -> bool {
        let Some(runtime) = self.size_map_runtime.as_ref() else {
            return false;
        };
        let config = runtime.config();
        size_map_view::is_supported_size_map_config(&config)
            && self
                .state
                .view_settings()
                .effective_extension_view_id(|id| id == config.view_id)
                .is_some()
    }

    /// Ends the transient session for the single Size Map view. The stable
    /// directory generation is not enough on its own: returning to the same
    /// tab after another tab or built-in view must submit a fresh request ID so
    /// the app runtime cancels (and the UI rejects) the earlier work.
    fn invalidate_size_map_session(&mut self) {
        self.size_map_visual_context = None;
        self.size_map_requested.clear();
        if let Some(visuals) = self.size_map_visuals.as_mut() {
            visuals.values.clear();
        }
    }

    fn invalidate_size_map_after_action(&mut self, action: &ExplorerAction) {
        if matches!(
            action,
            ExplorerAction::NewTab
                | ExplorerAction::CloseActiveTab
                | ExplorerAction::CloseTab { .. }
                | ExplorerAction::ActivateTab { .. }
                | ExplorerAction::NextTab
                | ExplorerAction::PreviousTab
                | ExplorerAction::SetViewMode(_)
                | ExplorerAction::SetExtensionView { .. }
        ) {
            self.invalidate_size_map_session();
        }
    }

    fn size_map_result_is_current(
        result: &size_map_view::SizeMapMeasureResultV1,
        context: &explorer_model::RequestContext,
    ) -> bool {
        result.context == *context
    }

    fn submit_size_map_requests(&mut self) {
        if !self.size_map_is_active() {
            return;
        }
        let Some(runtime) = self.size_map_runtime.clone() else {
            return;
        };
        let (tab_id, generation, entries) = {
            let tab = self.state.tabs().active_tab();
            let Some(snapshot) = tab.visible_snapshot() else {
                return;
            };
            (tab.id, tab.generation, snapshot.entries().to_vec())
        };
        let context_is_current = self
            .size_map_visual_context
            .as_ref()
            .is_some_and(|context| context.tab_id == tab_id && context.generation == generation);
        if !context_is_current {
            self.invalidate_size_map_session();
            self.size_map_visual_context =
                Some(explorer_model::RequestContext::new(tab_id, generation));
        }
        let context = self
            .size_map_visual_context
            .clone()
            .expect("Size Map context was initialized above");
        let requests = entries
            .iter()
            .filter(|entry| entry.is_container)
            .filter_map(|entry| match &entry.location {
                explorer_model::LocationDescriptor::FileSystem(path)
                    if self
                        .size_map_requested
                        .insert((tab_id, generation, entry.id.clone())) =>
                {
                    Some(size_map_view::SizeMapMeasureRequestV1 {
                        context: context.clone(),
                        item_id: entry.id.clone(),
                        path: path.clone(),
                    })
                }
                _ => None,
            })
            .collect::<Vec<_>>();
        if !requests.is_empty() {
            runtime.submit_measure_requests(requests);
        }
    }

    fn pump_size_map_runtime(&mut self) -> bool {
        let Some(runtime) = self.size_map_runtime.clone() else {
            return false;
        };
        let render_ready = runtime.drain_render_results();
        if !size_map_view::is_supported_size_map_config(&runtime.config()) {
            return false;
        }
        let results = runtime.drain_measure_results();
        if !self.size_map_is_active() {
            // Result delivery is intentionally consumed while Details (or a
            // different tab view) is active. A later Size Map activation gets
            // a fresh request ID, so no hidden result can become visible.
            let had_session = self.size_map_visual_context.is_some()
                || self
                    .size_map_visuals
                    .as_ref()
                    .is_some_and(|visuals| !visuals.values.is_empty())
                || !self.size_map_requested.is_empty();
            self.invalidate_size_map_session();
            return render_ready || had_session || !results.is_empty();
        }
        let active_tab = self.state.tabs().active_tab();
        let context = self
            .size_map_visual_context
            .as_ref()
            .filter(|context| {
                context.tab_id == active_tab.id && context.generation == active_tab.generation
            })
            .cloned();
        let Some(context) = context else {
            // Rendering submits the fresh session first. Until then, discard
            // completions from the previous tab/view context instead of
            // recreating that context from the pump.
            self.invalidate_size_map_session();
            return render_ready || !results.is_empty();
        };
        let visible_ids = active_tab
            .visible_snapshot()
            .map(|snapshot| {
                snapshot
                    .entries()
                    .iter()
                    .map(|entry| entry.id.clone())
                    .collect::<HashSet<_>>()
            })
            .unwrap_or_default();
        let visuals = self.size_map_visuals.get_or_insert_with(Default::default);
        let previous_count = visuals.values.len();
        visuals.values.retain(|id, _| visible_ids.contains(id));
        let mut changed = render_ready || visuals.values.len() != previous_count;
        for result in results
            .into_iter()
            .filter(|result| Self::size_map_result_is_current(result, &context))
        {
            let value = size_map_view::SizeMapMeasuredValueV1 {
                exact_bytes: result.exact_bytes,
                partial: result.partial,
                error: result.error,
            };
            if visuals.values.insert(result.item_id, value.clone()) != Some(value) {
                changed = true;
            }
        }
        changed
    }

    fn submit_folder_size_requests(&mut self) {
        let Some(runtime) = self.visual_column_runtime.as_mut() else {
            return;
        };
        let (tab_id, generation, entries) = {
            let tab = self.state.tabs().active_tab();
            let Some(snapshot) = tab.visible_snapshot() else {
                return;
            };
            (tab.id, tab.generation, snapshot.entries().to_vec())
        };
        let request_context = explorer_model::RequestContext::new(tab_id, generation);
        let requests = entries
            .iter()
            .filter(|entry| entry.is_container)
            .filter_map(|entry| match &entry.location {
                explorer_model::LocationDescriptor::FileSystem(path)
                    if self.folder_size_requested.insert((
                        tab_id,
                        generation,
                        entry.id.clone(),
                    )) =>
                {
                    Some(folder_size_column::FolderSizeRequestV1 {
                        context: request_context.clone(),
                        item_id: entry.id.clone(),
                        path: path.clone(),
                    })
                }
                explorer_model::LocationDescriptor::FileSystem(_) => None,
                _ => None,
            })
            .collect::<Vec<_>>();
        if !requests.is_empty() {
            runtime.submit_folder_size_requests(requests);
        }
    }

    fn pump_visual_column_runtime(&mut self) -> bool {
        let Some(runtime) = self.visual_column_runtime.as_mut() else {
            return false;
        };
        let render_ready = runtime.drain_render_results();
        let mut config = runtime.config();
        let results = runtime.drain_folder_size_results();
        if let Some(display) = self.folder_size_display_override {
            config.folder_size_display = display;
        }
        if !folder_size_column::is_supported_folder_size_descriptor(&config.descriptor) {
            return false;
        }
        let descriptor_changed = self
            .folder_size_visuals
            .as_ref()
            .is_none_or(|visuals| visuals.config.descriptor != config.descriptor);
        let mut changed = render_ready;
        if descriptor_changed
            && self
                .state
                .install_visual_column_descriptor(config.descriptor.clone())
        {
            changed = true;
        }
        let visuals = self.folder_size_visuals.get_or_insert_with(|| {
            folder_size_column::FolderSizeColumnVisuals {
                config: config.clone(),
                context: None,
                values: HashMap::new(),
            }
        });
        let active_tab = self.state.tabs().active_tab();
        let current_context =
            explorer_model::RequestContext::new(active_tab.id, active_tab.generation);
        if visuals.begin_context(&current_context) {
            self.folder_size_requested.clear();
            changed = true;
        }
        let visible_ids = self
            .state
            .tabs()
            .active_tab()
            .visible_snapshot()
            .map(|snapshot| {
                snapshot
                    .entries()
                    .iter()
                    .map(|entry| entry.id.clone())
                    .collect::<HashSet<_>>()
            })
            .unwrap_or_default();
        let previous_value_count = visuals.values.len();
        visuals
            .values
            .retain(|item_id, _| visible_ids.contains(item_id));
        if visuals.values.len() != previous_value_count {
            changed = true;
        }
        if visuals.config != config {
            visuals.config = config;
            changed = true;
        }
        for result in results
            .into_iter()
            .filter(|result| folder_size_result_is_current(result, &current_context))
        {
            let value = folder_size_column::FolderSizeValueV1 {
                exact_bytes: result.exact_bytes,
                partial: result.partial,
                error: result.error,
            };
            if visuals.values.insert(result.item_id, value.clone()) != Some(value) {
                changed = true;
            }
        }
        if changed {
            self.state
                .set_folder_size_sort_values(visuals.exact_sort_values());
        }
        changed
    }

    fn submit_code_lines_requests(&mut self) {
        if self.code_lines_runtime.is_none() {
            return;
        }
        let (tab_id, generation, entries) = {
            let tab = self.state.tabs().active_tab();
            let Some(snapshot) = tab.visible_snapshot() else {
                return;
            };
            (tab.id, tab.generation, snapshot.entries().to_vec())
        };
        let request_context = explorer_model::RequestContext::new(tab_id, generation);
        // This runs in the render path before we clone visuals into the GPUI
        // tree, so a Shell item ID reused by F5/navigation cannot leak an old
        // value for even one frame while the 16 ms pump is idle.
        self.begin_code_lines_context(request_context.clone());
        let requests = entries
            .iter()
            .filter(|entry| !entry.is_container)
            .filter_map(|entry| match &entry.location {
                explorer_model::LocationDescriptor::FileSystem(path)
                    if self
                        .code_lines_requested
                        .insert((tab_id, generation, entry.id.clone())) =>
                {
                    Some(code_lines_column::CodeLinesRequestV1 {
                        context: request_context.clone(),
                        item_id: entry.id.clone(),
                        path: path.clone(),
                    })
                }
                _ => None,
            })
            .collect::<Vec<_>>();
        if !requests.is_empty() {
            if let Some(runtime) = self.code_lines_runtime.as_ref() {
                runtime.submit_code_lines_requests(requests);
            }
        }
    }

    /// Starts the current Code lines presentation synchronously. The worker
    /// still enforces the same context on accepted results; this only removes
    /// already-painted values before the next render snapshot is built.
    fn begin_code_lines_context(&mut self, context: explorer_model::RequestContext) -> bool {
        let Some(visuals) = self.code_lines_visuals.as_mut() else {
            return false;
        };
        if !visuals.begin_context(context.clone()) {
            return false;
        }
        self.code_lines_requested.retain(|(tab, generation, _)| {
            *tab == context.tab_id && *generation == context.generation
        });
        self.state.set_code_lines_sort_values(HashMap::new());
        true
    }

    fn pump_code_lines_runtime(&mut self) -> bool {
        let Some(runtime) = self.code_lines_runtime.as_ref() else {
            return false;
        };
        let render_ready = runtime.drain_render_results();
        let mut config = runtime.config();
        let results = runtime.drain_code_lines_results();
        if let Some(display) = self.code_lines_display_override {
            config.display = display;
        }
        if !code_lines_column::is_supported_code_lines_descriptor(&config.descriptor) {
            return false;
        }
        let descriptor_changed = self
            .code_lines_visuals
            .as_ref()
            .is_none_or(|visuals| visuals.config.descriptor != config.descriptor);
        let mut changed = render_ready
            || (descriptor_changed
                && self
                    .state
                    .install_code_lines_column_descriptor(config.descriptor.clone()));
        let active_tab = self.state.tabs().active_tab();
        let current_context =
            explorer_model::RequestContext::new(active_tab.id, active_tab.generation);
        let context_changed = self.begin_code_lines_context(current_context.clone());
        let visuals = self.code_lines_visuals.get_or_insert_with(|| {
            code_lines_column::CodeLinesColumnVisuals {
                config: config.clone(),
                context: Some(current_context.clone()),
                values: HashMap::new(),
                errors: HashMap::new(),
            }
        });
        let visible_ids = self
            .state
            .tabs()
            .active_tab()
            .visible_snapshot()
            .map(|snapshot| {
                snapshot
                    .entries()
                    .iter()
                    .map(|entry| entry.id.clone())
                    .collect::<HashSet<_>>()
            })
            .unwrap_or_default();
        let old_values = visuals.values.len();
        let old_errors = visuals.errors.len();
        visuals.values.retain(|id, _| visible_ids.contains(id));
        visuals.errors.retain(|id, _| visible_ids.contains(id));
        changed |= visuals.values.len() != old_values || visuals.errors.len() != old_errors;
        if visuals.config != config {
            visuals.config = config;
            changed = true;
        }
        changed |= context_changed;
        for result in results.into_iter().filter(|result| {
            result.context.tab_id == current_context.tab_id
                && result.context.generation == current_context.generation
        }) {
            if let Some(value) = result.value {
                visuals.errors.remove(&result.item_id);
                if visuals.values.insert(result.item_id, value.clone()) != Some(value) {
                    changed = true;
                }
            } else {
                visuals.values.remove(&result.item_id);
                if let Some(error) = result.error
                    && visuals.errors.insert(result.item_id, error.clone()) != Some(error)
                {
                    changed = true;
                }
            }
        }
        if changed {
            self.state
                .set_code_lines_sort_values(visuals.exact_sort_values());
        }
        changed
    }

    /// Shows the one explicitly loaded development plugin in the existing
    /// Extensions menu. This is presentation-only and carries no ABI object.
    pub fn configure_loaded_extension_summary(&mut self, summary: Option<String>) {
        self.state.set_loaded_extension_summary(summary);
    }

    /// Builds a deterministic presentation-only state for screenshot regression tests.
    pub fn for_visual_fixture(tokens: UiTokens, fixture: VisualFixtureState) -> Self {
        let mut root = Self::new(tokens);
        root.state = AppViewState::with_initial_location(explorer_model::HistoryEntry::new(
            explorer_model::LocationDescriptor::file_system(r"C:\VisualFixture"),
            "Visual Fixture",
        ));
        seed_visual_directory(&mut root.state, fixture);
        root
    }

    #[doc(hidden)]
    pub fn for_directory_fixture(
        tokens: UiTokens,
        entries: Vec<explorer_model::FileEntry>,
        mode: explorer_model::ViewMode,
    ) -> Self {
        Self::directory_fixture(tokens, entries, mode, true)
    }

    #[doc(hidden)]
    pub fn for_loading_directory_fixture(
        tokens: UiTokens,
        entries: Vec<explorer_model::FileEntry>,
        mode: explorer_model::ViewMode,
    ) -> Self {
        Self::directory_fixture(tokens, entries, mode, false)
    }

    /// Read-only inspection for app-level fixture tests. Production callers
    /// cannot mutate the view state through this seam.
    #[doc(hidden)]
    #[must_use]
    pub fn fixture_visible_entry_count(&self) -> Option<usize> {
        self.state
            .tabs()
            .active_tab()
            .visible_snapshot()
            .map(|snapshot| snapshot.entries().len())
    }

    fn directory_fixture(
        tokens: UiTokens,
        entries: Vec<explorer_model::FileEntry>,
        mode: explorer_model::ViewMode,
        finish: bool,
    ) -> Self {
        let mut root = Self::new(tokens);
        let _ = dispatch_action(
            &mut root.state,
            ExplorerAction::SetViewMode(mode),
            ActionSource::Programmatic,
        );
        let Some(command) = root.state.begin_active_location_load() else {
            return root;
        };
        let Some(context) = command.context().cloned() else {
            return root;
        };
        let _ = root
            .state
            .apply_service_event(explorer_model::ExplorerEvent::DirectoryBatch {
                context: context.clone(),
                entries,
            });
        if finish {
            let _ = root
                .state
                .apply_service_event(explorer_model::ExplorerEvent::DirectoryFinished { context });
        }
        root
    }

    #[doc(hidden)]
    pub fn set_file_scroll_offset_for_test(&mut self, offset: f32) {
        self.file_scroll
            .set_offset(gpui::point(px(0.0), px(-offset.max(0.0))));
    }

    #[doc(hidden)]
    pub fn file_performance_snapshot_for_test(&self) -> performance::FileViewPerformanceSnapshot {
        self.file_performance.snapshot()
    }

    /// Returns aggregate result-delivery diagnostics without exposing request or item identity.
    pub fn service_qos_snapshot_for_test(&self) -> qos::UiQosSnapshot {
        qos::UiQosSnapshot {
            integrated_results: self.service_delivery.integrated_results(),
            deferred_results: self.service_delivery.deferred_results(),
            frame_budget_exhaustions: self.service_delivery.frame_budget_exhaustions(),
            degradation: self.service_qos.degradation_level(),
            observations: self.service_qos.observation_snapshot(),
        }
    }

    fn pending_service_event_count(&self) -> usize {
        self.pending_foreground_events
            .len()
            .saturating_add(self.pending_enrichment_events.len())
    }

    fn enqueue_service_event(&mut self, event: explorer_model::ExplorerEvent) {
        if is_enrichment_service_event(&event) {
            if self.pending_enrichment_events.len() == ENRICHMENT_SERVICE_EVENT_CAPACITY {
                // Prefer the newest visible enrichment and shed the oldest optional result. The
                // foreground queue has independent reserved capacity and is never displaced.
                // Terminal bookkeeping is completed before shedding so scheduler capacity and
                // retry admission cannot become wedged.
                if let Some(discarded) = self.pending_enrichment_events.pop_front() {
                    self.discard_enrichment_event(discarded);
                }
                self.service_qos.observations_mut().record_overload();
            }
            self.pending_enrichment_events.push_back(event);
        } else if self.pending_foreground_events.len() < FOREGROUND_SERVICE_EVENT_CAPACITY {
            self.pending_foreground_events.push_back(event);
        } else {
            // The receiver reserves enough foreground space before polling, so this is only a
            // defensive guard against a future caller bypassing that admission rule.
            self.service_qos.observations_mut().record_overload();
            tracing::error!(
                capacity = FOREGROUND_SERVICE_EVENT_CAPACITY,
                "foreground service-event admission invariant was exceeded"
            );
        }
    }

    fn discard_enrichment_event(&mut self, event: explorer_model::ExplorerEvent) {
        self.enrichment_retry_needed = true;
        match event {
            explorer_model::ExplorerEvent::ShellIconLoaded { payload, .. } => {
                self.pending_icon_keys.remove(&payload.key);
                self.pending_icon_contexts.remove(&payload.key);
                self.pending_base_icons.remove(&payload.key);
                self.pending_visible_bases.remove(&payload.key);
            }
            explorer_model::ExplorerEvent::ShellIconFailed { key, .. } => {
                self.pending_icon_keys.remove(&key);
                self.pending_icon_contexts.remove(&key);
                self.pending_base_icons.remove(&key);
                self.pending_visible_bases.remove(&key);
            }
            explorer_model::ExplorerEvent::ThumbnailFinished { key, .. } => {
                self.pending_thumbnail_keys.remove(&key);
                let _ = self.thumbnail_scheduler.complete(&key);
                self.thumbnail_requests.remove(&key);
                self.thumbnail_presentations.remove(&key);
                if self.preview_thumbnail_key.as_ref() == Some(&key) {
                    self.preview_thumbnail_key = None;
                    self.preview_texture = None;
                    self.preview_thumbnail_failed = true;
                }
                self.pump_thumbnail_scheduler();
            }
            _ => {
                debug_assert!(false, "only optional enrichment terminals may be shed");
            }
        }
    }

    fn recover_discarded_enrichment(&mut self) {
        if self.enrichment_retry_needed {
            self.enrichment_retry_needed = false;
            self.resume_visual_refinement();
        }
    }

    fn request_enrichment_retry(&mut self) {
        if self.service.is_some() {
            self.enrichment_retry_needed = true;
        }
    }

    fn pop_next_service_event(&mut self) -> Option<explorer_model::ExplorerEvent> {
        self.pending_foreground_events
            .pop_front()
            .or_else(|| self.pending_enrichment_events.pop_front())
    }

    fn accepts_presentation_event(&self, event: &explorer_model::ExplorerEvent) -> bool {
        // File operation progress is deliberately independent from navigation generation: a copy
        // that outlives navigation must remain visible and cancellable. Clipboard changes are
        // process-wide. Watcher events are validated here before their cache side effects and are
        // validated again by the reducer.
        if matches!(
            event,
            explorer_model::ExplorerEvent::ClipboardChanged { .. }
        ) {
            return true;
        }
        if let explorer_model::ExplorerEvent::DirectoryChanged {
            tab_id, generation, ..
        } = event
        {
            return self
                .state
                .tabs()
                .tabs()
                .iter()
                .any(|tab| tab.id == *tab_id && tab.generation == *generation);
        }
        if let explorer_model::ExplorerEvent::OperationProgress { context, .. }
        | explorer_model::ExplorerEvent::OperationFinished { context, .. } = event
        {
            return self
                .state
                .operation_center()
                .get(context.request_id)
                .is_some_and(|record| !record.phase.is_terminal());
        }
        if let explorer_model::ExplorerEvent::AncestryBatch { context, .. }
        | explorer_model::ExplorerEvent::AncestryFinished { context, .. } = event
        {
            return self.state.accepts_ancestry_context(context);
        }
        if let explorer_model::ExplorerEvent::ShellIconLoaded { context, payload } = event {
            return self
                .pending_icon_contexts
                .get(&payload.key)
                .is_some_and(|expected| expected.validate_event(context).is_ok());
        }
        if let explorer_model::ExplorerEvent::ShellIconFailed { context, key, .. } = event {
            return self
                .pending_icon_contexts
                .get(key)
                .is_some_and(|expected| expected.validate_event(context).is_ok());
        }
        if let explorer_model::ExplorerEvent::ThumbnailFinished { context, key, .. } = event {
            return self.pending_thumbnail_keys.contains(key)
                && self
                    .thumbnail_requests
                    .get(key)
                    .is_some_and(|(expected, _, _)| expected.validate_event(context).is_ok());
        }
        let Some(context) = event.context() else {
            return true;
        };
        if context.cancellation.is_cancelled() {
            return false;
        }
        let Some(tab) = self
            .state
            .tabs()
            .tabs()
            .iter()
            .find(|tab| tab.id == context.tab_id && tab.generation == context.generation)
        else {
            return false;
        };
        let active_search = match &tab.search {
            explorer_model::TabSearchState::Loading { request, .. } => {
                request.validate_event(context).is_ok()
            }
            _ => false,
        };
        match event {
            explorer_model::ExplorerEvent::LocationResolved { .. }
            | explorer_model::ExplorerEvent::DirectoryBatch { .. }
            | explorer_model::ExplorerEvent::DirectoryFinished { .. } => {
                tab.directory.accepts(context).is_ok()
            }
            explorer_model::ExplorerEvent::SearchBatch { .. }
            | explorer_model::ExplorerEvent::SearchStatus { .. }
            | explorer_model::ExplorerEvent::SearchFinished { .. } => active_search,
            explorer_model::ExplorerEvent::Failed { .. } => {
                tab.directory.accepts(context).is_ok() || active_search
            }
            _ => true,
        }
    }

    fn visual_refinement_allowed(&self) -> bool {
        self.optional_work_allowed(explorer_jobs::QosWorkClass::VisualRefinement)
    }

    fn optional_work_allowed(&self, work: explorer_jobs::QosWorkClass) -> bool {
        !self.service_qos.should_shed(work)
    }

    /// Recreates only current-generation visual work after pressure recovery. The request
    /// constructors retain their existing generation checks, so recovery cannot revive work for
    /// closed tabs or replaced navigation.
    fn resume_visual_refinement(&mut self) {
        let tab = self.state.tabs().active_tab();
        let context = explorer_model::RequestContext::new(tab.id, tab.generation);
        let entries = tab
            .visible_snapshot()
            .map(|snapshot| snapshot.entries().to_vec())
            .unwrap_or_default();
        self.submit_file_icon_loads(&context, &entries);
        self.submit_navigation_icon_loads();
    }

    #[doc(hidden)]
    pub fn dispatch_action_for_test(
        &mut self,
        action: ExplorerAction,
        source: ActionSource,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.handle_action(action, source, window, cx);
    }

    /// Connects deterministic visual data to production read-only Shell assets without replacing
    /// its fixture directory snapshot.
    pub fn attach_service_for_shell_assets(&mut self, service: Arc<dyn ExplorerService>) {
        self.service = Some(service);
        self.submit_navigation_icon_loads();
    }

    pub fn configure_shell_icon_scale(&mut self, scale_factor: f32) {
        let dpi = dpi_from_scale(scale_factor);
        if dpi == self.shell_icon_dpi {
            return;
        }
        self.shell_icon_dpi = dpi;
        self.icon_epochs.advance_association();
        let theme = match self.tokens.theme.mode {
            ThemeMode::Light => explorer_model::ShellIconTheme::Light,
            ThemeMode::Dark => explorer_model::ShellIconTheme::Dark,
        };
        self.shell_icons.invalidate_environment(dpi, theme);
        self.base_icons.invalidate_environment(dpi, theme);
        self.pending_base_icons.clear();
        self.pending_visible_bases.clear();
        self.failed_base_icons.clear();
        self.submit_navigation_icon_loads();
    }

    pub fn configure_tortoise_git_available(&mut self, available: bool) {
        self.state.set_tortoise_git_available(available);
    }

    pub fn configure_new_items(&mut self, items: Vec<explorer_model::ShellNewItemDescriptor>) {
        self.state.configure_new_items(items);
    }

    pub fn with_service(tokens: UiTokens, service: Arc<dyn ExplorerService>) -> Self {
        Self::with_service_and_drag_threshold(tokens, service, (4.0, 4.0))
    }

    pub fn with_service_and_drag_threshold(
        tokens: UiTokens,
        service: Arc<dyn ExplorerService>,
        drag_threshold: (f32, f32),
    ) -> Self {
        Self::with_service_drag_threshold_and_initial_location(
            tokens,
            service,
            drag_threshold,
            explorer_model::HistoryEntry::new(
                explorer_model::LocationDescriptor::file_system(r"C:\"),
                "This PC",
            ),
        )
    }

    pub fn with_service_drag_threshold_and_initial_location(
        tokens: UiTokens,
        service: Arc<dyn ExplorerService>,
        drag_threshold: (f32, f32),
        initial_location: explorer_model::HistoryEntry,
    ) -> Self {
        let mut root = Self {
            tokens,
            state: AppViewState::with_initial_location_and_drag_threshold(
                initial_location,
                drag_threshold,
            ),
            service: Some(service),
            folder_scripts: None,
            pending_foreground_events: VecDeque::new(),
            pending_enrichment_events: VecDeque::new(),
            enrichment_retry_needed: false,
            service_qos: explorer_jobs::InteractionFirstQos::default(),
            service_delivery: qos::UiDeliveryCounters::default(),
            navigation_started: HashMap::new(),
            first_batch_seen: HashSet::new(),
            shell_icons: VisibleItemIconCache::default(),
            base_icons: BaseIconCache::default(),
            icon_epochs: initial_icon_epochs(),
            pending_base_icons: HashMap::new(),
            pending_visible_bases: HashMap::new(),
            failed_base_icons: HashSet::new(),
            item_overlay_epochs: HashMap::new(),
            negative_icon_keys: HashSet::new(),
            negative_icon_order: VecDeque::new(),
            shell_icon_dpi: 96,
            pending_icon_keys: HashSet::new(),
            pending_icon_contexts: HashMap::new(),
            pending_thumbnail_keys: HashSet::new(),
            thumbnail_scheduler: explorer_jobs::ThumbnailScheduler::new(512, 4, 64 * 1024 * 1024),
            thumbnail_memory_cache: explorer_jobs::ThumbnailMemoryCache::new(
                128 * 1024 * 1024,
                2_048,
            ),
            thumbnail_requests: HashMap::new(),
            thumbnail_presentations: HashMap::new(),
            preview_thumbnail_key: None,
            preview_texture: None,
            preview_thumbnail_failed: false,
            preview_coordinator: explorer_jobs::PreviewCoordinator::new(Duration::from_millis(75)),
            preview_clock: Instant::now(),
            preview_selection_signature: None,
            preview_host_boundary: None,
            navigation_scroll: gpui::ScrollHandle::new(),
            file_scroll: gpui::ScrollHandle::new(),
            file_viewport_width: 0.0,
            file_performance: Arc::new(performance::FileViewPerformanceCounters::default()),
            focus_handle: None,
            breadcrumb_menu_focus: None,
            command_menu_focus: None,
            address_input: None,
            search_input: None,
            rename_input: None,
            pointer_capture_factory: None,
            pointer_capture: None,
            durable_state_observer: None,
            durable_window_placement: default_durable_window_placement(),
            session_reset_observer: None,
            broker_retry_observer: None,
            last_window_title: None,
            navigation_history_release_deadline: None,
            safe_mode_offers: Vec::new(),
            safe_mode_confirm: None,
            safe_mode_confirmation_error: None,
            extension_ui_pump: None,
            visual_column_runtime: None,
            folder_size_visuals: None,
            folder_size_requested: HashSet::new(),
            folder_size_display_override: None,
            code_lines_runtime: None,
            code_lines_visuals: None,
            code_lines_requested: HashSet::new(),
            code_lines_display_override: None,
            size_map_runtime: None,
            size_map_visuals: None,
            size_map_visual_context: None,
            size_map_requested: HashSet::new(),
        };
        root.submit_active_location_load();
        root.submit_navigation_icon_loads();
        root
    }

    /// Creates a root from validated restored tabs while keeping all transient UI state fresh.
    pub fn with_service_drag_threshold_and_restored_window(
        tokens: UiTokens,
        service: Arc<dyn ExplorerService>,
        drag_threshold: (f32, f32),
        restored: explorer_model::ExplorerWindowState,
    ) -> Self {
        let mut root = Self {
            tokens,
            state: AppViewState::with_restored_window_and_drag_threshold(restored, drag_threshold),
            service: Some(service),
            folder_scripts: None,
            pending_foreground_events: VecDeque::new(),
            pending_enrichment_events: VecDeque::new(),
            enrichment_retry_needed: false,
            service_qos: explorer_jobs::InteractionFirstQos::default(),
            service_delivery: qos::UiDeliveryCounters::default(),
            navigation_started: HashMap::new(),
            first_batch_seen: HashSet::new(),
            shell_icons: VisibleItemIconCache::default(),
            base_icons: BaseIconCache::default(),
            icon_epochs: initial_icon_epochs(),
            pending_base_icons: HashMap::new(),
            pending_visible_bases: HashMap::new(),
            failed_base_icons: HashSet::new(),
            item_overlay_epochs: HashMap::new(),
            negative_icon_keys: HashSet::new(),
            negative_icon_order: VecDeque::new(),
            shell_icon_dpi: 96,
            pending_icon_keys: HashSet::new(),
            pending_icon_contexts: HashMap::new(),
            pending_thumbnail_keys: HashSet::new(),
            thumbnail_scheduler: explorer_jobs::ThumbnailScheduler::new(512, 4, 64 * 1024 * 1024),
            thumbnail_memory_cache: explorer_jobs::ThumbnailMemoryCache::new(
                128 * 1024 * 1024,
                2_048,
            ),
            thumbnail_requests: HashMap::new(),
            thumbnail_presentations: HashMap::new(),
            preview_thumbnail_key: None,
            preview_texture: None,
            preview_thumbnail_failed: false,
            preview_coordinator: explorer_jobs::PreviewCoordinator::new(Duration::from_millis(75)),
            preview_clock: Instant::now(),
            preview_selection_signature: None,
            preview_host_boundary: None,
            navigation_scroll: gpui::ScrollHandle::new(),
            file_scroll: gpui::ScrollHandle::new(),
            file_viewport_width: 0.0,
            file_performance: Arc::new(performance::FileViewPerformanceCounters::default()),
            focus_handle: None,
            breadcrumb_menu_focus: None,
            command_menu_focus: None,
            address_input: None,
            search_input: None,
            rename_input: None,
            pointer_capture_factory: None,
            pointer_capture: None,
            durable_state_observer: None,
            durable_window_placement: default_durable_window_placement(),
            session_reset_observer: None,
            broker_retry_observer: None,
            last_window_title: None,
            navigation_history_release_deadline: None,
            safe_mode_offers: Vec::new(),
            safe_mode_confirm: None,
            safe_mode_confirmation_error: None,
            extension_ui_pump: None,
            visual_column_runtime: None,
            folder_size_visuals: None,
            folder_size_requested: HashSet::new(),
            folder_size_display_override: None,
            code_lines_runtime: None,
            code_lines_visuals: None,
            code_lines_requested: HashSet::new(),
            code_lines_display_override: None,
            size_map_runtime: None,
            size_map_visuals: None,
            size_map_visual_context: None,
            size_map_requested: HashSet::new(),
        };
        root.submit_active_location_load();
        root.submit_navigation_icon_loads();
        root
    }

    /// Installs the native pointer-capture adapter supplied by the application composition root.
    pub fn attach_pointer_capture_factory(&mut self, factory: PointerCaptureFactory) {
        self.pointer_capture_factory = Some(factory);
    }

    /// Attaches the app-owned persistence bridge; callbacks receive no GPUI entities or handles.
    pub fn attach_durable_state_observer(
        &mut self,
        observer: DurableStateObserver,
        window: &Window,
        cx: &App,
    ) {
        self.durable_state_observer = Some(observer);
        self.capture_durable_window_placement(window, cx);
        self.notify_durable_state();
    }

    /// Attaches the background reset command bridge.
    pub fn attach_session_reset_observer(&mut self, observer: SessionResetObserver) {
        self.session_reset_observer = Some(observer);
    }

    /// Applies the loaded General-page restore preference before user interaction begins.
    pub fn configure_restore_previous_session(&mut self, enabled: bool) {
        self.state.set_restore_previous_session(enabled);
    }

    /// Applies ordered pins loaded by the application-owned session store.
    pub fn configure_quick_access(&mut self, pins: Vec<explorer_model::PersistedQuickAccessPin>) {
        self.state.configure_quick_access(pins);
    }

    /// Configures the privacy-safe broker status and its explicit user retry bridge.
    pub fn configure_broker_health(
        &mut self,
        health: state::BrokerUiHealth,
        retry: BrokerRetryObserver,
    ) {
        self.state.set_broker_health(health);
        self.broker_retry_observer = Some(retry);
    }

    /// Installs startup-recovered Safe Mode offers before the first render.
    pub fn configure_safe_mode_offers(
        &mut self,
        offers: Vec<SafeModeOfferV1>,
        confirm: SafeModeConfirmObserverV1,
    ) {
        self.safe_mode_offers = offers;
        self.safe_mode_confirm = Some(confirm);
        self.safe_mode_confirmation_error = None;
    }

    #[must_use]
    pub fn safe_mode_offer_count(&self) -> usize {
        self.safe_mode_offers.len()
    }

    fn confirm_safe_mode_offer(&mut self, presentation_token: u64) {
        let Some(confirm) = self.safe_mode_confirm.as_ref() else {
            return;
        };
        let Some(index) = self
            .safe_mode_offers
            .iter()
            .position(|offer| offer.presentation_token == presentation_token)
        else {
            return;
        };
        match confirm(presentation_token) {
            Ok(()) => {
                self.safe_mode_offers.remove(index);
                self.safe_mode_confirmation_error = None;
            }
            Err(error) => self.safe_mode_confirmation_error = Some(error),
        }
    }

    fn notify_durable_state(&self) -> bool {
        if let Some(observer) = &self.durable_state_observer {
            return observer(
                self.state.tabs().clone(),
                self.state.restore_previous_session(),
                self.state.persisted_quick_access(),
                self.durable_window_placement,
            );
        }
        false
    }

    fn capture_durable_window_placement(&mut self, window: &Window, cx: &App) {
        // `WindowOptions::window_bounds` and `Window::window_bounds` use the same
        // restore-coordinate convention. `Window::bounds` is the live global
        // client rectangle on Windows and feeding it back as restore bounds
        // accumulates the non-client frame offset on every restart.
        let window_bounds = window.window_bounds();
        let maximized = matches!(window_bounds, WindowBounds::Maximized(_));
        self.durable_window_placement.normal_bounds = persisted_rect(window_bounds.get_bounds());
        self.durable_window_placement.source_work_area = window.display(cx).map_or_else(
            || persisted_rect(window.bounds()),
            |display| persisted_rect(display.bounds()),
        );
        self.durable_window_placement.source_dpi = u32::from(dpi_from_scale(window.scale_factor()));
        self.durable_window_placement.maximized = maximized;
    }

    /// Connects exact-directory `super_explorer.lua` lifecycle observation.
    pub fn attach_folder_scripts(&mut self, handle: explorer_automation::FolderScriptHandle) {
        self.folder_scripts = Some(handle);
    }

    fn acquire_pointer_capture(&self, window: &Window) -> Option<Box<dyn PointerCaptureSession>> {
        let hwnd = pointer_capture::window_handle_value(window)?;
        self.pointer_capture_factory.as_ref()?(hwnd)
    }

    #[allow(
        clippy::too_many_lines,
        reason = "the pump keeps one ordered async service-event lifecycle and its cancellation points auditable"
    )]
    pub fn start_service_pump(&mut self, window_handle: AnyWindowHandle, cx: &mut Context<Self>) {
        let Some(service) = self.service.clone() else {
            return;
        };
        let folder_scripts = self.folder_scripts.clone();
        cx.spawn(async move |this, cx| {
            let mut last_folder_script_refresh = Instant::now();
            let mut last_namespace_refresh = Instant::now();
            loop {
                cx.background_executor()
                    .timer(Duration::from_millis(16))
                    .await;
                if this
                    .update(cx, |this, cx| {
                        // Each pump owns side effects. Evaluate all of them;
                        // boolean short-circuiting can otherwise starve later
                        // extension views whenever an earlier pump is busy.
                        let extension_changed =
                            extension_ui_pump_due(this.extension_ui_pump.as_mut(), Instant::now());
                        let visual_column_changed = this.pump_visual_column_runtime();
                        let code_lines_changed = this.pump_code_lines_runtime();
                        let size_map_changed = this.pump_size_map_runtime();
                        if extension_changed
                            || visual_column_changed
                            || code_lines_changed
                            || size_map_changed
                        {
                            cx.notify();
                        }
                    })
                    .is_err()
                {
                    break;
                }
                if this
                    .update(cx, |this, _| this.poll_preview_handler())
                    .is_err()
                {
                    break;
                }
                    let maintenance_allowed = match this.update(cx, |this, _| {
                        this.optional_work_allowed(explorer_jobs::QosWorkClass::Maintenance)
                    }) {
                        Ok(allowed) => allowed,
                        Err(_) => break,
                    };
                    if maintenance_allowed
                        && last_folder_script_refresh.elapsed() >= Duration::from_millis(500)
                    {
                    if let Some(handle) = &folder_scripts
                        && let Err(error) = handle.refresh_changed()
                    {
                        tracing::warn!(%error, "folder automation refresh failed");
                    }
                    last_folder_script_refresh = Instant::now();
                }
                if last_namespace_refresh.elapsed() >= Duration::from_secs(30) {
                    let refreshed = this.update(cx, |this, _| {
                        let is_non_path = this
                            .state
                            .tabs()
                            .active_tab()
                            .history
                            .current()
                            .is_some_and(|entry| entry.location.path().is_none());
                        if this.optional_work_allowed(explorer_jobs::QosWorkClass::Maintenance)
                            && is_non_path
                            && let Some(command) = this.state.begin_refresh_navigation()
                        {
                            tracing::debug!("bounded refresh for namespace without notifications");
                            this.submit_command(command);
                        }
                    });
                    if refreshed.is_err() {
                        break;
                    }
                    last_namespace_refresh = Instant::now();
                }
                let foreground_room = match this.update(cx, |this, _| {
                    FOREGROUND_SERVICE_EVENT_CAPACITY
                        .saturating_sub(this.pending_foreground_events.len())
                }) {
                    Ok(room) => room,
                    Err(_) => break,
                };
                let receive_limit = foreground_room
                    .min(explorer_jobs::FrameDrainBudget::DEFAULT_ITEM_LIMIT);
                let mut received_events = Vec::with_capacity(receive_limit);
                let mut disconnected = false;
                while received_events.len() < receive_limit {
                    match service.try_recv() {
                        Ok(Some(event)) => received_events.push(event),
                        Ok(None) => break,
                        Err(error) => {
                            tracing::error!(?error, "Explorer service event endpoint failed");
                            explorer_common::record_process_error_message(
                                explorer_common::ErrorSeverity::Error,
                                "ui",
                                "receive_service_event",
                                &format!("service endpoint: {error:?}"),
                                Some(file!()),
                            );
                            disconnected = true;
                            break;
                        }
                    }
                }
                let mut delegated_actions = Vec::new();
                if this
                    .update(cx, |this, cx| {
                            for event in received_events {
                                this.enqueue_service_event(event);
                            }
                            let budget = this.service_qos.frame_drain_budget();
                            let started = Instant::now();
                            let mut integrated = 0_usize;
                            while budget.admit_next(integrated, started.elapsed()).is_ok() {
                                let Some(event) = this.pop_next_service_event() else {
                                    break;
                                };
                                integrated = integrated.saturating_add(1);
                                if !this.accepts_presentation_event(&event) {
                                    this.service_qos
                                        .observations_mut()
                                        .record_stale_result();
                                    tracing::debug!(
                                        context = ?event.context(),
                                        "rejected superseded service result at presentation boundary"
                                    );
                                    continue;
                                }
                                let delegated_action = match &event {
                                    explorer_model::ExplorerEvent::ContextMenuFinished {
                                        outcome:
                                            explorer_model::ContextMenuOutcome::Delegated {
                                                command,
                                                target,
                                                ..
                                            },
                                        ..
                                    } => Some((*command, target.clone())),
                                    _ => None,
                                };
                                let context = event.context().cloned();
                                let terminal = event.is_terminal();
                                let directory_enrichment_terminal = matches!(
                                    &event,
                                    explorer_model::ExplorerEvent::DirectoryFinished { .. }
                                        | explorer_model::ExplorerEvent::SearchFinished { .. }
                                );
                                let navigation_children = matches!(
                                    &event,
                                    explorer_model::ExplorerEvent::ChildContainersBatch { .. }
                                );
                                this.observe_service_event(&event);
                                if let explorer_model::ExplorerEvent::DirectoryChanged {
                                    changes,
                                    ..
                                } = &event
                                {
                                    for change in changes {
                                        let id = match change {
                                            explorer_model::DirectoryDelta::Upsert(entry) => {
                                                &entry.id
                                            }
                                            explorer_model::DirectoryDelta::Remove(id) => id,
                                            explorer_model::DirectoryDelta::Overflow => continue,
                                        };
                                        advance_item_overlay_epoch(
                                            &mut this.item_overlay_epochs,
                                            id,
                                        );
                                    }
                                }
                                if let explorer_model::ExplorerEvent::AncestryBatch {
                                    context,
                                    segments,
                                } = &event
                                {
                                    this.submit_location_icon_loads(
                                        context,
                                        segments.iter().map(|segment| &segment.location),
                                    );
                                }
                                if let explorer_model::ExplorerEvent::ShellIconLoaded {
                                    payload,
                                    ..
                                } = &event
                                    && let Some(texture) = shell_icon_texture(payload)
                                {
                                    if let Some(base_key) =
                                        this.pending_base_icons.remove(&payload.key)
                                    {
                                        this.base_icons.insert(
                                            base_key,
                                            texture,
                                            icon_payload_hash(payload),
                                        );
                                    } else if let Some(base_key) =
                                        this.pending_visible_bases.remove(&payload.key)
                                        && this.base_icons.hashes.get(&base_key)
                                            == Some(&icon_payload_hash(payload))
                                    {
                                        this.remember_negative_icon(payload.key.clone());
                                    } else {
                                        this.shell_icons.insert(&payload.key, texture);
                                    }
                                }
                                if let explorer_model::ExplorerEvent::ShellIconLoaded {
                                    payload,
                                    ..
                                } = &event
                                {
                                    this.pending_icon_keys.remove(&payload.key);
                                    this.pending_icon_contexts.remove(&payload.key);
                                }
                                if let explorer_model::ExplorerEvent::ShellIconFailed {
                                    key, ..
                                } = &event
                                {
                                    this.pending_icon_keys.remove(key);
                                    this.pending_icon_contexts.remove(key);
                                    if let Some(base_key) = this.pending_base_icons.remove(key) {
                                        this.failed_base_icons.insert(base_key);
                                    }
                                    this.pending_visible_bases.remove(key);
                                    if key.item_id.is_some() {
                                        this.remember_negative_icon(key.clone());
                                    }
                                }
                                if let explorer_model::ExplorerEvent::ThumbnailFinished {
                                    key,
                                    outcome,
                                    ..
                                } = &event
                                {
                                    this.pending_thumbnail_keys.remove(key);
                                    let _ = this.thumbnail_scheduler.complete(key);
                                    this.thumbnail_requests.remove(key);
                                    if let explorer_model::ThumbnailTerminal::Ready { pixels, .. } = outcome {
                                        let _ = this.thumbnail_memory_cache.insert(
                                            key.clone(),
                                            Arc::new(pixels.clone()),
                                        );
                                    }
                                    let current_preview = this.preview_thumbnail_key.as_ref() == Some(key)
                                        && this.state.view_settings().preview_pane
                                        && this.state.tabs().active_tab().selection.len() == 1
                                        && this.state.tabs().active_tab().selection.contains(&key.item_id)
                                        && this.state.tabs().active_tab().generation.value()
                                            == key.source_generation;
                                    if current_preview {
                                        match outcome {
                                            explorer_model::ThumbnailTerminal::Ready { pixels, .. } => {
                                                this.preview_texture = thumbnail_texture(pixels);
                                                this.preview_thumbnail_failed =
                                                    this.preview_texture.is_none();
                                            }
                                            explorer_model::ThumbnailTerminal::Fallback(_)
                                            | explorer_model::ThumbnailTerminal::Failed(_) => {
                                                this.preview_texture = None;
                                                this.preview_thumbnail_failed = true;
                                            }
                                        }
                                    }
                                    let presentation = this.thumbnail_presentations.remove(key);
                                    if let (
                                        Some(presentation),
                                        explorer_model::ThumbnailTerminal::Ready { pixels, .. },
                                    ) = (presentation, outcome)
                                        && let Some(texture) = thumbnail_texture(pixels)
                                    {
                                        this.shell_icons.insert(&presentation, texture);
                                    }
                                    let scheduler = this.thumbnail_scheduler.stats();
                                    let cache = this.thumbnail_memory_cache.stats();
                                    let icons = this.shell_icons.stats();
                                    tracing::debug!(
                                        queued = scheduler.queued_unique,
                                        queue_capacity = scheduler.queue_capacity,
                                        active = scheduler.active_unique,
                                        concurrency_limit = scheduler.concurrency_limit,
                                        consumers = scheduler.consumers,
                                        decoded_in_flight_bytes = scheduler.decoded_in_flight_bytes,
                                        decoded_byte_limit = scheduler.decoded_byte_limit,
                                        cancellations = scheduler.cancellations,
                                        cache_entries = cache.entries,
                                        cache_entry_budget = cache.entry_budget,
                                        cache_bytes = cache.current_bytes,
                                        cache_byte_budget = cache.byte_budget,
                                        cache_hits = cache.hits,
                                        cache_misses = cache.misses,
                                        cache_evictions = cache.evictions,
                                        icon_entries = icons.entries,
                                        icon_entry_budget = icons.entry_budget,
                                        icon_bytes = icons.current_bytes,
                                        icon_byte_budget = icons.byte_budget,
                                        icon_hits = icons.hits,
                                        icon_misses = icons.misses,
                                        icon_negative_hits = icons.negative_hits,
                                        icon_evictions = icons.evictions,
                                        icon_stale_rejections = icons.stale_rejections,
                                        "thumbnail performance snapshot"
                                    );
                                    this.pump_thumbnail_scheduler();
                                }
                                if let explorer_model::ExplorerEvent::PreviewHostFinished {
                                    outcome,
                                    ..
                                } = &event
                                {
                                    this.apply_preview_host_terminal(outcome);
                                }
                                let ancestry = match &event {
                                    explorer_model::ExplorerEvent::LocationResolved {
                                        context,
                                        metadata,
                                    } => Some((context.clone(), metadata.descriptor.clone())),
                                    _ => None,
                                };
                                let folder_transition = match &event {
                                    explorer_model::ExplorerEvent::LocationResolved {
                                        context,
                                        metadata,
                                    } => Some((
                                        context.tab_id,
                                        metadata.descriptor.path().map(std::path::Path::to_path_buf),
                                    )),
                                    _ => None,
                                };
                                let active_search_scope_changed = matches!(
                                    &event,
                                    explorer_model::ExplorerEvent::LocationResolved {
                                        context,
                                        ..
                                    } if context.tab_id == this.state.tabs().active_tab_id()
                                );
                                // Capture the affected navigation parents before watcher recovery
                                // advances the tab generation. The replacement tree requests are
                                // created afterwards so they share the refreshed generation.
                                let navigation_reconciliation =
                                    this.state.navigation_reconciliation_targets(&event);
                                let recovery = this.state.watcher_recovery_command(&event);
                                let navigation_recovery = this
                                    .state
                                    .begin_navigation_reconciliation(navigation_reconciliation);
                                let refresh_after_action =
                                    this.state.service_event_requires_active_refresh(&event);
                                let outcome = this.state.apply_service_event(event);
                                if outcome == explorer_model::WindowEventOutcome::IgnoredStale {
                                    this.service_qos
                                        .observations_mut()
                                        .record_stale_result();
                                }
                                if outcome == explorer_model::WindowEventOutcome::Applied
                                    && active_search_scope_changed
                                    && matches!(
                                        this.state.tabs().active_tab().search,
                                        explorer_model::TabSearchState::Idle
                                    )
                                {
                                    // GPUI's editable text element retains its original
                                    // placeholder with the input entity. Recreate an empty idle
                                    // input when the committed location changes so the visible
                                    // Explorer search scope follows the new folder immediately.
                                    this.reset_search_input(String::new(), cx);
                                }
                                if outcome == explorer_model::WindowEventOutcome::Applied
                                    && directory_enrichment_terminal
                                {
                                    // The directory reducer now owns the complete sorted snapshot.
                                    // Seed its visible icon/thumbnail pipeline immediately instead
                                    // of waiting for a later ScrollHandle layout callback to cause
                                    // another render. This is especially important when the first
                                    // rows are folders and lower visible file rows need extension
                                    // icons or thumbnails.
                                    this.resume_visual_refinement();
                                }
                                if outcome == explorer_model::WindowEventOutcome::Applied
                                    && let Some(action) = delegated_action
                                {
                                    delegated_actions.push(action);
                                }
                                if outcome == explorer_model::WindowEventOutcome::Applied
                                    && navigation_children
                                {
                                    this.submit_navigation_icon_loads();
                                }
                                tracing::debug!(
                                    ?context,
                                    ?outcome,
                                    terminal,
                                    "Explorer UI applied service event"
                                );
                                if outcome == explorer_model::WindowEventOutcome::Applied
                                    && ancestry.is_some()
                                {
                                    this.notify_durable_state();
                                }
                                if outcome == explorer_model::WindowEventOutcome::Applied
                                    && let Some((tab_id, directory)) = folder_transition
                                    && let Some(handle) = &folder_scripts
                                    && let Err(error) =
                                        handle.enter_directory(tab_id, directory.as_deref())
                                {
                                    tracing::warn!(%error, ?tab_id, "folder automation transition failed");
                                }
                                if let Some(command) = recovery {
                                    this.submit_command(command);
                                }
                                if let Some(command) = this.state.take_pending_lock_recovery_command() {
                                    this.submit_command(command);
                                }
                                if let Some(command) = this.state.take_pending_context_menu_command() {
                                    this.submit_command(command);
                                }
                                // Reconciliation targets were validated before applying the event.
                                // DirectoryChanged is intentionally `IgnoredUnrelated` by the tab
                                // reducer, but its navigation-tree reload must still be submitted.
                                for command in navigation_recovery {
                                    this.submit_command(command);
                                }
                                if outcome == explorer_model::WindowEventOutcome::Applied
                                    && refresh_after_action
                                    && let Some(command) = this.state.begin_refresh_navigation()
                                {
                                    this.submit_command(command);
                                }
                                if outcome == explorer_model::WindowEventOutcome::Applied
                                    && let Some((source, location)) = ancestry
                                    && let Some(command) =
                                        this.state.begin_ancestry_request(&source, location)
                                {
                                    this.submit_command(command);
                                }
                            }
                            let deferred = this.pending_service_event_count();
                            let exhausted = deferred > 0
                                && budget.admit_next(integrated, started.elapsed()).is_err();
                            this.service_delivery
                                .record_drain(integrated, deferred, exhausted);
                            match this.service_qos.observe_pressure(
                                explorer_jobs::PressureSample::new(
                                    deferred,
                                    budget.item_limit(),
                                    exhausted,
                                ),
                            ) {
                                explorer_jobs::DegradationTransition::Recovered { from, to } => {
                                    tracing::debug!(?from, ?to, deferred, "UI result-delivery degradation recovery");
                                    this.recover_discarded_enrichment();
                                    if from.sheds(explorer_jobs::QosWorkClass::VisualRefinement)
                                        && !to.sheds(explorer_jobs::QosWorkClass::VisualRefinement)
                                    {
                                        this.resume_visual_refinement();
                                    }
                                }
                                explorer_jobs::DegradationTransition::Degraded { from, to } => {
                                    tracing::debug!(?from, ?to, deferred, "UI result-delivery degradation transition");
                                }
                                explorer_jobs::DegradationTransition::Unchanged(_) => {}
                            }
                            if deferred == 0 && this.enrichment_retry_needed {
                                this.recover_discarded_enrichment();
                            }
                            if integrated > 0 {
                                cx.notify();
                            }
                        })
                    .is_err()
                {
                    break;
                }
                if !delegated_actions.is_empty() {
                    let Some(root) = window_handle.downcast::<ExplorerRoot>() else {
                        break;
                    };
                    for (command, target) in delegated_actions {
                        if root
                            .update(cx, |this, window, cx| {
                                if command
                                    == explorer_model::ContextMenuHostCommand::Properties
                                {
                                    let (owner_window, _, _) = chrome::context_menu_coordinates(
                                        gpui::point(px(0.0), px(0.0)),
                                        window,
                                    );
                                    if let Some(command) = this
                                        .state
                                        .begin_properties_request_for_target(owner_window, target)
                                    {
                                        this.submit_command(command);
                                    }
                                } else if this.state.restore_context_target_selection(&target) {
                                    this.handle_action(
                                        action_for_host_context_command(command),
                                        ActionSource::Programmatic,
                                        window,
                                        cx,
                                    );
                                }
                            })
                            .is_err()
                        {
                            break;
                        }
                    }
                }
                let _ = this.update(cx, |this, cx| {
                    let has_external_cue = matches!(
                        this.state.drag_session().state(),
                        explorer_model::DragSessionState::Dragging {
                            target: Some(_),
                            ..
                        }
                    );
                    if has_external_cue && !cx.has_active_drag() {
                        this.state.clear_external_drag();
                        cx.notify();
                    }
                });
                if disconnected {
                    break;
                }
            }
        })
        .detach();
    }

    /// Connects the production GPUI key-dispatch tree to the native window focus owner.
    pub fn attach_focus_handle(&mut self, focus_handle: gpui::FocusHandle) {
        self.focus_handle = Some(focus_handle);
    }

    pub fn attach_text_inputs(&mut self, cx: &mut Context<Self>) {
        self.breadcrumb_menu_focus = Some(cx.focus_handle());
        self.command_menu_focus = Some(cx.focus_handle());
        self.reset_address_input(self.state.address_draft().to_owned(), cx);
        self.reset_search_input(String::new(), cx);
    }

    fn reset_search_input(&mut self, value: String, cx: &mut Context<Self>) {
        let search = cx.new(|cx| EditableTextState::new(StringStorage::from(value), cx));
        cx.subscribe(&search, |this, input, _: &TextChanged, cx| {
            if this.state.focused_surface() != focus::FocusSurface::Search {
                this.state.begin_search_editing();
                this.state.focus(focus::FocusSurface::Search);
            }
            let value = input.read(cx).as_str().to_owned();
            tracing::info!(
                input_surface = "Search",
                utf8_bytes = value.len(),
                characters = value.chars().count(),
                contains_cjk = value
                    .chars()
                    .any(|character| ('\u{3400}'..='\u{9fff}').contains(&character)),
                "Editable input changed"
            );
            if let Some(command) = this.state.update_active_search_text(value) {
                this.submit_command(command);
            }
            cx.notify();
        })
        .detach();
        self.search_input = Some(search);
    }

    fn reset_address_input(&mut self, value: String, cx: &mut Context<Self>) {
        let address = cx.new(|cx| EditableTextState::new(StringStorage::from(value), cx));
        cx.subscribe(&address, |this, input, _: &TextChanged, cx| {
            if this.state.focused_surface() == focus::FocusSurface::AddressBar {
                let value = input.read(cx).as_str().to_owned();
                let _ = this.state.update_address_edit_input(value);
                cx.notify();
            }
        })
        .detach();
        self.address_input = Some(address);
    }

    fn reset_rename_input(&mut self, value: String, cx: &mut Context<Self>) {
        let rename = cx.new(|cx| EditableTextState::new(StringStorage::from(value), cx));
        cx.subscribe(&rename, |this, input, _: &TextChanged, cx| {
            if this.state.rename_editor().is_some() {
                let _ = this
                    .state
                    .update_inline_rename(input.read(cx).as_str().to_owned());
                cx.notify();
            }
        })
        .detach();
        rename.update(cx, EditableTextState::select_document);
        self.rename_input = Some(rename);
    }

    fn submit_active_location_load(&mut self) {
        let Some(command) = self.state.begin_active_location_load() else {
            return;
        };
        self.submit_command(command);
    }

    fn submit_navigation_icon_loads(&mut self) {
        let tab = self.state.tabs().active_tab();
        let theme = match self.tokens.theme.mode {
            ThemeMode::Light => explorer_model::ShellIconTheme::Light,
            ThemeMode::Dark => explorer_model::ShellIconTheme::Dark,
        };
        let tab_id = tab.id;
        let generation = tab.generation;
        let allow_prefetch = self.optional_work_allowed(explorer_jobs::QosWorkClass::Prefetch);
        let keys = navigation_pane::windows_navigation_items_with_pins(
            self.state.quick_access_navigation_pins(),
        )
        .into_iter()
        .filter_map(|item| item.icon_location)
        .chain(self.state.navigation_icon_locations())
        .chain(
            self.state
                .tabs()
                .tabs()
                .iter()
                .filter(|candidate| allow_prefetch || candidate.id == tab_id)
                .filter_map(|tab| tab.history.current().map(|entry| entry.location.clone())),
        )
        .map(|location| navigation_pane::shell_icon_key(&location, theme, self.shell_icon_dpi))
        .chain(std::iter::once(
            navigation_pane::generic_breadcrumb_folder_icon_key(
                theme,
                self.shell_icon_dpi,
                self.icon_epochs.association(),
            ),
        ))
        .collect::<HashSet<_>>();
        for key in keys {
            if self.shell_icons.entries.contains_key(&key)
                || !self.pending_icon_keys.insert(key.clone())
            {
                continue;
            }
            let context = explorer_model::RequestContext::new(tab_id, generation);
            self.pending_icon_contexts
                .insert(key.clone(), context.clone());
            let submitted = self.submit_command(explorer_model::ExplorerCommand::LoadShellIcon {
                context,
                key: key.clone(),
            });
            if !submitted {
                self.pending_icon_keys.remove(&key);
                self.pending_icon_contexts.remove(&key);
                self.request_enrichment_retry();
            }
        }
    }

    fn submit_file_icon_loads(
        &mut self,
        directory_context: &explorer_model::RequestContext,
        entries: &[explorer_model::FileEntry],
    ) {
        let view_settings = self
            .state
            .tabs()
            .tabs()
            .iter()
            .find(|tab| tab.id == directory_context.tab_id)
            .map_or_else(explorer_model::ViewSettings::default, |tab| {
                tab.view.settings.clone()
            });
        let always_show_icons = self
            .state
            .tabs()
            .tabs()
            .iter()
            .find(|tab| tab.id == directory_context.tab_id)
            .is_some_and(|tab| tab.view.settings.always_show_icons);
        let logical_size = navigation_pane::view_icon_logical_size_for_settings(&view_settings);
        let theme = match self.tokens.theme.mode {
            ThemeMode::Light => explorer_model::ShellIconTheme::Light,
            ThemeMode::Dark => explorer_model::ShellIconTheme::Dark,
        };
        let generation = directory_context.generation.value();
        let association_epoch = self.icon_epochs.association();
        let visible_ids = entries
            .iter()
            .map(|entry| entry.id.clone())
            .collect::<HashSet<_>>();
        self.pending_icon_keys.retain(|key| {
            key.item_id
                .as_ref()
                .is_none_or(|item_id| visible_ids.contains(item_id))
        });
        self.pending_icon_contexts
            .retain(|key, _| self.pending_icon_keys.contains(key));
        self.pending_visible_bases.retain(|key, _| {
            key.item_id
                .as_ref()
                .is_some_and(|item_id| visible_ids.contains(item_id))
        });
        // Navigation/breadcrumb icons share the cache but not the visible file viewport budget.
        // Counting their item-less keys here can permanently starve the first on-screen file
        // icons after an expanded drive submits a large child tree.
        let pending_visible_files = self
            .pending_icon_keys
            .iter()
            .filter(|key| {
                key.item_id
                    .as_ref()
                    .is_some_and(|item_id| visible_ids.contains(item_id))
            })
            .count();
        let remaining = FILE_VIEWPORT_ICON_REQUEST_CAP.saturating_sub(pending_visible_files);
        let stale = self
            .thumbnail_requests
            .iter()
            .filter(|(key, (_, _, consumer))| {
                consumer.tab_id == directory_context.tab_id
                    && (consumer.generation != directory_context.generation
                        || key.source_generation != generation
                        || (!visible_ids.contains(&key.item_id)
                            && self.preview_thumbnail_key.as_ref() != Some(*key)))
            })
            .map(|(key, (_, _, consumer))| (key.clone(), *consumer))
            .collect::<Vec<_>>();
        for (key, consumer) in stale {
            let _ = self.thumbnail_scheduler.cancel_consumer(&key, consumer);
            self.thumbnail_requests.remove(&key);
            self.pending_thumbnail_keys.remove(&key);
            self.thumbnail_presentations.remove(&key);
        }
        let mut pending_base_classes = self
            .pending_base_icons
            .values()
            .cloned()
            .collect::<HashSet<_>>();
        for entry in entries.iter().take(FILE_VIEWPORT_ICON_REQUEST_CAP) {
            let representative = navigation_pane::file_icon_key_for_size(
                entry,
                theme,
                self.shell_icon_dpi,
                logical_size,
            );
            let base_key = explorer_model::base_icon_key(
                entry,
                representative.size_bucket,
                self.shell_icon_dpi,
                theme,
                association_epoch,
            );
            if !uses_shared_base_icon(&base_key.class) {
                continue;
            }
            if self.base_icons.entries.contains_key(&base_key)
                || self.failed_base_icons.contains(&base_key)
                || !pending_base_classes.insert(base_key.clone())
            {
                continue;
            }
            let Some(base_location) = base_icon_request_location(&base_key.class) else {
                continue;
            };
            let mut request_key = representative;
            request_key.item_id = None;
            request_key.location = base_location;
            request_key.association_generation = association_epoch;
            request_key.overlay_generation = 0;
            if self.submit_command(explorer_model::ExplorerCommand::LoadShellIcon {
                context: directory_context.clone(),
                key: request_key.clone(),
            }) {
                self.pending_icon_contexts
                    .insert(request_key.clone(), directory_context.clone());
                self.pending_base_icons.insert(request_key, base_key);
            } else {
                self.request_enrichment_retry();
            }
        }
        let mut keys = Vec::with_capacity(remaining);
        for entry in entries.iter().take(remaining) {
            let presentation = navigation_pane::file_icon_key_for_size(
                entry,
                theme,
                self.shell_icon_dpi,
                logical_size,
            );
            let base_key = explorer_model::base_icon_key(
                entry,
                presentation.size_bucket,
                self.shell_icon_dpi,
                theme,
                association_epoch,
            );
            let shared_base = uses_shared_base_icon(&base_key.class);
            if shared_base
                && !self.base_icons.entries.contains_key(&base_key)
                && !self.failed_base_icons.contains(&base_key)
            {
                continue;
            }
            let mut key = file_icon_cache_key(
                entry,
                theme,
                self.shell_icon_dpi,
                logical_size,
                association_epoch,
            );
            key.overlay_generation = self
                .item_overlay_epochs
                .get(&entry.id)
                .copied()
                .unwrap_or_else(|| self.icon_epochs.overlay());
            if self.negative_icon_keys.contains(&key) {
                self.shell_icons.record_negative_hit();
                continue;
            }
            keys.push((
                key,
                (shared_base && self.base_icons.entries.contains_key(&base_key))
                    .then_some(base_key),
            ));
        }
        for (key, base_key) in keys {
            if self.shell_icons.entries.contains_key(&key)
                || !self.pending_icon_keys.insert(key.clone())
            {
                continue;
            }
            let context = explorer_model::RequestContext::new(
                directory_context.tab_id,
                directory_context.generation,
            );
            self.pending_icon_contexts
                .insert(key.clone(), context.clone());
            let submitted = self.submit_command(explorer_model::ExplorerCommand::LoadShellIcon {
                context,
                key: key.clone(),
            });
            if !submitted {
                self.pending_icon_keys.remove(&key);
                self.pending_icon_contexts.remove(&key);
                self.request_enrichment_retry();
            } else if let Some(base_key) = base_key {
                self.pending_visible_bases.insert(key, base_key);
            }
        }
        let (thumbnail_mode, thumbnail_logical_size) =
            explorer_model::view_mode_thumbnail_policy(view_settings.mode);
        if thumbnail_mode == explorer_model::ThumbnailMode::Thumbnail && !always_show_icons {
            let physical = u32::from(thumbnail_logical_size)
                .saturating_mul(u32::from(self.shell_icon_dpi))
                .saturating_add(95)
                / 96;
            let physical_size = u16::try_from(physical).unwrap_or(u16::MAX).max(1);
            for entry in entries
                .iter()
                .filter(|entry| namespace_thumbnail_supported(entry))
                .take(remaining)
            {
                let key = explorer_model::ThumbnailRequestKey {
                    item_id: entry.id.clone(),
                    physical_size,
                    dpi: self.shell_icon_dpi,
                    mode: thumbnail_mode,
                    source_generation: generation,
                    theme,
                    association_generation: association_epoch,
                    overlay_generation: self
                        .item_overlay_epochs
                        .get(&entry.id)
                        .copied()
                        .unwrap_or_else(|| self.icon_epochs.overlay()),
                };
                if !self.pending_thumbnail_keys.insert(key.clone()) {
                    continue;
                }
                let presentation = file_icon_cache_key(
                    entry,
                    theme,
                    self.shell_icon_dpi,
                    logical_size,
                    association_epoch,
                );
                let mut presentation = presentation;
                presentation.overlay_generation = self
                    .item_overlay_epochs
                    .get(&entry.id)
                    .copied()
                    .unwrap_or_else(|| self.icon_epochs.overlay());
                self.thumbnail_presentations
                    .insert(key.clone(), presentation);
                let consumer = explorer_model::ThumbnailConsumer {
                    tab_id: directory_context.tab_id,
                    generation: directory_context.generation,
                    size_generation: u64::from(physical_size),
                };
                let outcome = self.thumbnail_scheduler.submit(
                    key.clone(),
                    consumer,
                    explorer_model::ThumbnailPriority::ActiveVisible,
                );
                if outcome == explorer_jobs::ThumbnailScheduleOutcome::Overloaded {
                    self.pending_thumbnail_keys.remove(&key);
                    self.thumbnail_presentations.remove(&key);
                    self.request_enrichment_retry();
                    continue;
                }
                self.thumbnail_requests.entry(key).or_insert_with(|| {
                    (
                        explorer_model::RequestContext::new(
                            directory_context.tab_id,
                            directory_context.generation,
                        ),
                        entry.location.clone(),
                        consumer,
                    )
                });
            }
            self.pump_thumbnail_scheduler();
        }
    }

    /// Schedules a snapshot-wide refinement pass only while off-screen enrichment remains
    /// admitted. The visible viewport re-requests its own entries during rendering.
    fn submit_offscreen_file_icon_loads(
        &mut self,
        context: &explorer_model::RequestContext,
        entries: &[explorer_model::FileEntry],
    ) {
        if self.optional_work_allowed(explorer_jobs::QosWorkClass::OffscreenEnrichment) {
            self.submit_file_icon_loads(context, entries);
        }
    }

    fn synchronize_preview_thumbnail(&mut self, entry: Option<&explorer_model::FileEntry>) {
        let tab = self.state.tabs().active_tab();
        let desired = entry
            .filter(|entry| {
                self.state.view_settings().preview_pane
                    && tab.selection.len() == 1
                    && tab.selection.contains(&entry.id)
                    && previewable_image(&entry.location)
            })
            .map(|entry| {
                let logical = self.state.view_settings().preview_pane_width.clamp(96, 768);
                let physical = u32::from(logical)
                    .saturating_mul(u32::from(self.shell_icon_dpi))
                    .saturating_add(95)
                    / 96;
                explorer_model::ThumbnailRequestKey {
                    item_id: entry.id.clone(),
                    physical_size: u16::try_from(physical).unwrap_or(768).clamp(96, 768),
                    dpi: self.shell_icon_dpi,
                    mode: explorer_model::ThumbnailMode::Thumbnail,
                    source_generation: tab.generation.value(),
                    theme: match self.tokens.theme.mode {
                        ThemeMode::Light => explorer_model::ShellIconTheme::Light,
                        ThemeMode::Dark => explorer_model::ShellIconTheme::Dark,
                    },
                    association_generation: self.icon_epochs.association(),
                    overlay_generation: 0,
                }
            });
        if self.preview_thumbnail_key == desired {
            return;
        }
        if let Some(previous) = self.preview_thumbnail_key.take()
            && let Some((_, _, consumer)) = self.thumbnail_requests.remove(&previous)
        {
            let _ = self
                .thumbnail_scheduler
                .cancel_consumer(&previous, consumer);
            self.pending_thumbnail_keys.remove(&previous);
        }
        self.preview_texture = None;
        self.preview_thumbnail_failed = false;
        let Some(key) = desired else {
            return;
        };
        self.preview_thumbnail_key = Some(key.clone());
        if let Some(pixels) = self.thumbnail_memory_cache.get(&key) {
            self.preview_texture = thumbnail_texture(&pixels);
            self.preview_thumbnail_failed = self.preview_texture.is_none();
            return;
        }
        let Some(entry) = entry else {
            return;
        };
        let consumer = explorer_model::ThumbnailConsumer {
            tab_id: tab.id,
            generation: tab.generation,
            size_generation: u64::from(key.physical_size),
        };
        if self.thumbnail_scheduler.submit(
            key.clone(),
            consumer,
            explorer_model::ThumbnailPriority::ActiveVisible,
        ) == explorer_jobs::ThumbnailScheduleOutcome::Overloaded
        {
            self.preview_thumbnail_failed = true;
            self.request_enrichment_retry();
            return;
        }
        self.pending_thumbnail_keys.insert(key.clone());
        self.thumbnail_requests.insert(
            key,
            (
                explorer_model::RequestContext::new(tab.id, tab.generation),
                entry.location.clone(),
                consumer,
            ),
        );
        self.pump_thumbnail_scheduler();
    }

    fn submit_preview_coordinator_action(
        &mut self,
        action: explorer_jobs::PreviewCoordinatorAction,
    ) -> bool {
        let tab = self.state.tabs().active_tab();
        let context = explorer_model::RequestContext::new(tab.id, tab.generation);
        let command = match action {
            explorer_jobs::PreviewCoordinatorAction::Start {
                generation,
                selection,
            } => {
                let Some((parent_window, left, top, width, height, dpi)) =
                    self.preview_host_boundary
                else {
                    return false;
                };
                explorer_model::PreviewHostCommand::Start {
                    selection,
                    parent_window,
                    bounds: explorer_model::PreviewHostBounds {
                        generation,
                        left_physical: left,
                        top_physical: top,
                        width_physical: width,
                        height_physical: height,
                        dpi,
                    },
                }
            }
            explorer_jobs::PreviewCoordinatorAction::Unload { generation } => {
                explorer_model::PreviewHostCommand::Unload { generation }
            }
        };
        self.submit_command(explorer_model::ExplorerCommand::PreviewHost { context, command })
    }

    fn synchronize_preview_handler(&mut self, entry: Option<&explorer_model::FileEntry>) {
        if !self.visual_refinement_allowed() {
            return;
        }
        let tab = self.state.tabs().active_tab();
        let pane_open = self.state.view_settings().preview_pane;
        let candidate = pane_open
            .then_some(entry)
            .flatten()
            .filter(|entry| tab.selection.len() == 1 && tab.selection.contains(&entry.id));
        let handler_candidate = candidate.filter(|entry| !previewable_image(&entry.location));
        let signature = Some((
            tab.id,
            handler_candidate.map(|entry| entry.id.clone()),
            pane_open,
        ));
        if self.preview_selection_signature == signature {
            return;
        }
        self.preview_selection_signature = signature;

        if !pane_open {
            if let Ok(Some(action)) = self.preview_coordinator.close() {
                let _ = self.submit_preview_coordinator_action(action);
            }
            return;
        }
        if matches!(
            self.preview_coordinator.lifecycle(),
            explorer_model::PreviewLifecycle::Closed
        ) {
            let _ = self.preview_coordinator.open();
        }
        let eligibility = match candidate {
            None => explorer_model::PreviewEligibility::None,
            Some(entry) if entry.is_container => explorer_model::PreviewEligibility::Folder,
            Some(entry) if previewable_image(&entry.location) => {
                explorer_model::PreviewEligibility::Unsupported
            }
            Some(entry) => explorer_model::PreviewEligibility::SingleEligible(
                explorer_model::PreviewSelection {
                    item_id: entry.id.clone(),
                    location: entry.location.clone(),
                    display_name: entry.display_name.clone(),
                },
            ),
        };
        if let Ok(Some(action)) = self
            .preview_coordinator
            .select(&eligibility, self.preview_clock.elapsed())
        {
            let _ = self.submit_preview_coordinator_action(action);
        }
    }

    fn poll_preview_handler(&mut self) {
        if !self.optional_work_allowed(explorer_jobs::QosWorkClass::OptionalAnimation) {
            return;
        }
        if self.preview_host_boundary.is_none() {
            return;
        }
        if let Ok(Some(action)) = self.preview_coordinator.poll(self.preview_clock.elapsed())
            && !self.submit_preview_coordinator_action(action)
            && let Some(generation) = self.preview_coordinator.lifecycle().generation()
        {
            let _ = self.preview_coordinator.finish(generation, false, true);
        }
    }

    fn apply_preview_host_terminal(&mut self, outcome: &explorer_model::PreviewHostTerminal) {
        let generation = outcome.generation();
        match outcome {
            explorer_model::PreviewHostTerminal::Ready { .. } => {
                if self.preview_coordinator.finish(generation, true, true) {
                    self.preview_thumbnail_failed = false;
                }
            }
            explorer_model::PreviewHostTerminal::Unloaded { .. } => {
                let _ = self.preview_coordinator.unloaded(generation);
            }
            explorer_model::PreviewHostTerminal::Failed { .. } => {
                if matches!(
                    self.preview_coordinator.lifecycle(),
                    explorer_model::PreviewLifecycle::Unloading { .. }
                ) {
                    let _ = self.preview_coordinator.unloaded(generation);
                } else if self.preview_coordinator.finish(generation, false, true) {
                    self.preview_thumbnail_failed = true;
                }
            }
            explorer_model::PreviewHostTerminal::Updated { .. } => {}
        }
    }

    fn forward_preview_accelerator(&mut self, event: &gpui::KeyDownEvent) -> bool {
        if self.state.focused_surface() != focus::FocusSurface::PreviewPane {
            return false;
        }
        let modifiers = event.keystroke.modifiers;
        let key = event.keystroke.key.as_str();
        let reserved_by_app = (modifiers.control && matches!(key, "l" | "f" | "t" | "w" | "tab"))
            || (modifiers.alt && matches!(key, "left" | "right" | "up" | "p" | "enter"))
            || modifiers.platform
            || key == "tab";
        if reserved_by_app {
            return false;
        }
        let Some(virtual_key) = preview_virtual_key(key) else {
            return false;
        };
        let Some(generation) = self.preview_coordinator.lifecycle().generation() else {
            return false;
        };
        if !matches!(
            self.preview_coordinator.lifecycle(),
            explorer_model::PreviewLifecycle::Visible { .. }
        ) {
            return false;
        }
        let mut modifier_bits = 0_u8;
        modifier_bits |= u8::from(modifiers.control);
        modifier_bits |= u8::from(modifiers.shift) << 1;
        modifier_bits |= u8::from(modifiers.alt) << 2;
        let tab = self.state.tabs().active_tab();
        self.submit_command(explorer_model::ExplorerCommand::PreviewHost {
            context: explorer_model::RequestContext::new(tab.id, tab.generation),
            command: explorer_model::PreviewHostCommand::Accelerator {
                generation,
                virtual_key,
                modifiers: modifier_bits,
            },
        })
    }

    fn pump_thumbnail_scheduler(&mut self) {
        while let Some(key) = self.thumbnail_scheduler.try_start(4 * 1024 * 1024) {
            let Some((context, location, _)) = self.thumbnail_requests.get(&key).cloned() else {
                let _ = self.thumbnail_scheduler.complete(&key);
                continue;
            };
            if let Some(pixels) = self.thumbnail_memory_cache.get(&key) {
                if self.preview_thumbnail_key.as_ref() == Some(&key) {
                    self.preview_texture = thumbnail_texture(&pixels);
                    self.preview_thumbnail_failed = self.preview_texture.is_none();
                }
                if let Some(presentation) = self.thumbnail_presentations.remove(&key)
                    && let Some(texture) = thumbnail_texture(&pixels)
                {
                    self.shell_icons.insert(&presentation, texture);
                }
                self.pending_thumbnail_keys.remove(&key);
                self.thumbnail_requests.remove(&key);
                let _ = self.thumbnail_scheduler.complete(&key);
                continue;
            }
            if !self.submit_command(explorer_model::ExplorerCommand::LoadThumbnail {
                context,
                key: key.clone(),
                location,
                cache_only: false,
            }) {
                self.pending_thumbnail_keys.remove(&key);
                self.thumbnail_presentations.remove(&key);
                self.thumbnail_requests.remove(&key);
                let _ = self.thumbnail_scheduler.complete(&key);
                self.request_enrichment_retry();
            }
        }
    }

    fn submit_location_icon_loads<'a>(
        &mut self,
        context: &explorer_model::RequestContext,
        locations: impl IntoIterator<Item = &'a explorer_model::LocationDescriptor>,
    ) {
        let theme = match self.tokens.theme.mode {
            ThemeMode::Light => explorer_model::ShellIconTheme::Light,
            ThemeMode::Dark => explorer_model::ShellIconTheme::Dark,
        };
        let keys = locations
            .into_iter()
            .map(|location| navigation_pane::shell_icon_key(location, theme, self.shell_icon_dpi))
            .collect::<HashSet<_>>();
        for key in keys {
            if self.shell_icons.entries.contains_key(&key)
                || !self.pending_icon_keys.insert(key.clone())
            {
                continue;
            }
            let request_context =
                explorer_model::RequestContext::new(context.tab_id, context.generation);
            self.pending_icon_contexts
                .insert(key.clone(), request_context.clone());
            let submitted = self.submit_command(explorer_model::ExplorerCommand::LoadShellIcon {
                context: request_context,
                key: key.clone(),
            });
            if !submitted {
                self.pending_icon_keys.remove(&key);
                self.pending_icon_contexts.remove(&key);
                self.request_enrichment_retry();
            }
        }
    }

    fn navigation_icon_snapshot(
        &mut self,
        file_entries: &[explorer_model::FileEntry],
    ) -> HashMap<explorer_model::ShellIconKey, Arc<RenderImage>> {
        let theme = match self.tokens.theme.mode {
            ThemeMode::Light => explorer_model::ShellIconTheme::Light,
            ThemeMode::Dark => explorer_model::ShellIconTheme::Dark,
        };
        let static_locations = navigation_pane::windows_navigation_items_with_pins(
            self.state.quick_access_navigation_pins(),
        )
        .into_iter()
        .filter_map(|item| item.icon_location)
        .collect::<Vec<_>>();
        let mut snapshot = static_locations
            .iter()
            .filter_map(|location| {
                self.shell_icons.get_compatible_navigation_icon(
                    location,
                    theme,
                    self.shell_icon_dpi,
                )
            })
            .collect::<HashMap<_, _>>();
        let generic_breadcrumb_key = navigation_pane::generic_breadcrumb_folder_icon_key(
            theme,
            self.shell_icon_dpi,
            self.icon_epochs.association(),
        );
        if let Some(texture) = self.shell_icons.get(&generic_breadcrumb_key) {
            snapshot.insert(generic_breadcrumb_key, texture);
        }
        for location in self.state.navigation_icon_locations() {
            if let Some((key, texture)) = self.shell_icons.get_compatible_navigation_icon(
                &location,
                theme,
                self.shell_icon_dpi,
            ) {
                snapshot.insert(key, texture);
            }
        }
        for location in self
            .state
            .tabs()
            .tabs()
            .iter()
            .filter_map(|tab| tab.history.current().map(|entry| &entry.location))
        {
            if let Some((key, texture)) = self.shell_icons.get_compatible_navigation_icon(
                location,
                theme,
                self.shell_icon_dpi,
            ) {
                snapshot.insert(key, texture);
            }
        }
        let address = &self.state.tabs().active_tab().view.address;
        for location in address
            .resolved_ancestry
            .iter()
            .map(|segment| &segment.location)
            .chain(address.menu_children.iter().map(|child| &child.location))
        {
            if let Some((key, texture)) = self.shell_icons.get_compatible_navigation_icon(
                location,
                theme,
                self.shell_icon_dpi,
            ) {
                snapshot.insert(key, texture);
            }
        }
        let theme = match self.tokens.theme.mode {
            ThemeMode::Light => explorer_model::ShellIconTheme::Light,
            ThemeMode::Dark => explorer_model::ShellIconTheme::Dark,
        };
        let logical_size =
            navigation_pane::view_icon_logical_size_for_settings(&self.state.view_settings());
        let association_epoch = self.icon_epochs.association();
        for entry in file_entries {
            let presentation_key = navigation_pane::file_icon_key_for_size(
                entry,
                theme,
                self.shell_icon_dpi,
                logical_size,
            );
            let mut cache_key = file_icon_cache_key(
                entry,
                theme,
                self.shell_icon_dpi,
                logical_size,
                association_epoch,
            );
            cache_key.overlay_generation = self
                .item_overlay_epochs
                .get(&entry.id)
                .copied()
                .unwrap_or_else(|| self.icon_epochs.overlay());
            if let Some(texture) = self.shell_icons.get(&cache_key) {
                snapshot.insert(presentation_key, texture);
            } else {
                let base_key = explorer_model::base_icon_key(
                    entry,
                    presentation_key.size_bucket,
                    self.shell_icon_dpi,
                    theme,
                    association_epoch,
                );
                if let Some(texture) = self.base_icons.get(&base_key) {
                    snapshot.insert(presentation_key, texture);
                }
            }
        }
        snapshot
    }

    /// Starts a typed native file operation and registers its operation-center record before
    /// submitting work to the service boundary.
    pub fn execute_file_operation(&mut self, request: explorer_model::FileOperationRequest) {
        let command = self.state.begin_file_operation(request);
        self.submit_command(command);
    }

    /// Requests cooperative cancellation; the Shell STA flips the shared token immediately and
    /// the progress sink aborts at the next native callback boundary.
    pub fn cancel_file_operation(&mut self, request_id: explorer_common::RequestId) {
        self.submit_command(explorer_model::ExplorerCommand::Cancel { request_id });
    }

    /// Submits text from the dedicated search editor to the active tab's independent generation.
    pub fn submit_search(&mut self, input: impl Into<String>) -> bool {
        let Some(command) = self.state.begin_active_search(input.into()) else {
            return false;
        };
        self.submit_command(command);
        true
    }

    /// Leaves search and restores the directory snapshot and navigation history underneath it.
    pub fn leave_search(&mut self) {
        self.state.leave_active_search();
    }

    pub fn begin_inline_rename(&mut self, row_index: usize) -> bool {
        self.state.begin_inline_rename(row_index)
    }

    pub fn update_inline_rename(&mut self, value: String) -> bool {
        self.state.update_inline_rename(value)
    }

    /// Esc calls `cancel_inline_rename`; Enter and focus loss call this method with their explicit
    /// trigger. Validation errors leave the editor and selection state intact.
    ///
    /// # Errors
    ///
    /// Returns the input or collision error produced by the shared Windows name validator.
    pub fn commit_inline_rename(
        &mut self,
        trigger: explorer_model::RenameCommitTrigger,
    ) -> Result<bool, explorer_common::ExplorerError> {
        let Some(request) = self.state.commit_inline_rename(trigger)? else {
            return Ok(false);
        };
        self.execute_file_operation(request);
        Ok(true)
    }

    pub fn cancel_inline_rename(&mut self) -> bool {
        self.state.cancel_inline_rename()
    }

    pub fn begin_permanent_delete_confirmation(&mut self) -> bool {
        self.state.begin_permanent_delete_confirmation()
    }

    /// Submits permanent deletion only after a prior explicit confirmation request.
    pub fn confirm_permanent_delete(&mut self) -> bool {
        let Some(request) = self.state.confirm_permanent_delete() else {
            return false;
        };
        self.execute_file_operation(request);
        true
    }

    /// Cancelling confirmation clears UI state and deliberately submits no Shell command.
    pub fn cancel_permanent_delete_confirmation(&mut self) -> bool {
        self.state.cancel_permanent_delete_confirmation()
    }

    /// Submits Paste with an explicit collision decision. Conflict UI can call this again after a
    /// `Prompt` terminal without rebuilding or unmarshalling the clipboard object on the UI thread.
    pub fn paste_with_conflict(&mut self, decision: explorer_model::ConflictDecision) -> bool {
        let Some(command) = self.state.begin_paste_request(decision) else {
            return false;
        };
        self.submit_command(command);
        true
    }

    fn submit_command(&mut self, command: explorer_model::ExplorerCommand) -> bool {
        let is_cancellation = matches!(&command, explorer_model::ExplorerCommand::Cancel { .. });
        if let explorer_model::ExplorerCommand::Navigate { context, location }
        | explorer_model::ExplorerCommand::Refresh { context, location } = &command
            && let Some(root) = location.synthetic_root()
        {
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_or(0, |value| value.as_secs());
            let entries = self.state.synthetic_root_entries(root, now);
            let title = match root {
                explorer_model::SyntheticRoot::Home => "Home",
                explorer_model::SyntheticRoot::QuickAccess => "Quick access",
            };
            for event in [
                explorer_model::ExplorerEvent::LocationResolved {
                    context: context.clone(),
                    metadata: explorer_model::LocationMetadata {
                        descriptor: location.clone(),
                        display_title: title.to_owned(),
                        can_go_up: false,
                        can_write: false,
                    },
                },
                explorer_model::ExplorerEvent::DirectoryBatch {
                    context: context.clone(),
                    entries,
                },
                explorer_model::ExplorerEvent::DirectoryFinished {
                    context: context.clone(),
                },
            ] {
                let _ = self.state.apply_service_event(event);
            }
            return true;
        }
        let Some(service) = &self.service else {
            return false;
        };
        let failed_command = command.clone();
        let context = command.context().cloned();
        let tracked_operation = context.as_ref().is_some_and(|context| {
            self.state
                .operation_center()
                .get(context.request_id)
                .is_some()
        });
        if let Some(context) = &context {
            self.navigation_started
                .insert(context.request_id, Instant::now());
        }
        if let Err(error) = service.submit(command) {
            if matches!(error, ExplorerServiceError::Overloaded) {
                self.service_qos.observations_mut().record_overload();
            }
            tracing::error!(?context, ?error, "Explorer command submission failed");
            explorer_common::record_process_error_message(
                explorer_common::ErrorSeverity::Error,
                "ui",
                "submit_command",
                &format!("context={context:?}; service endpoint: {error:?}"),
                Some(file!()),
            );
            if let Some(context) = &context
                && let Some(started) = self.navigation_started.remove(&context.request_id)
            {
                self.service_qos
                    .observations_mut()
                    .record_latency(started.elapsed());
            }
            if self.synthesize_special_submission_failure(&failed_command, &error) {
                return false;
            }
            if let Some(context) = context {
                if tracked_operation {
                    let kind = match error {
                        ExplorerServiceError::Overloaded | ExplorerServiceError::Disconnected => {
                            explorer_common::ExplorerErrorKind::Availability
                        }
                        ExplorerServiceError::Internal => {
                            explorer_common::ExplorerErrorKind::Internal
                        }
                    };
                    let _ = self.state.apply_service_event(
                        explorer_model::ExplorerEvent::OperationFinished {
                            context,
                            outcome: explorer_model::OperationTerminal::Failed(
                                explorer_common::ExplorerError::new(
                                    kind,
                                    "submit Explorer command",
                                    true,
                                    "The operation could not be queued, but Explorer can continue.",
                                    format!("service endpoint: {error:?}"),
                                ),
                            ),
                        },
                    );
                    return false;
                }
                let kind = match error {
                    ExplorerServiceError::Overloaded | ExplorerServiceError::Disconnected => {
                        explorer_common::ExplorerErrorKind::Availability
                    }
                    ExplorerServiceError::Internal => explorer_common::ExplorerErrorKind::Internal,
                };
                let _ = self
                    .state
                    .apply_service_event(explorer_model::ExplorerEvent::Failed {
                        context,
                        error: explorer_common::ExplorerError::new(
                            kind,
                            "submit Explorer command",
                            true,
                            "無法載入資料夾，請再試一次。",
                            format!("service endpoint: {error:?}"),
                        ),
                    });
            }
            return false;
        }
        if is_cancellation {
            self.service_qos.observations_mut().record_cancellation();
        }
        true
    }

    /// Completes request-specific UI state when the bounded service endpoint rejects admission.
    /// These paths cannot wait for the Shell worker because no request reached it.
    fn synthesize_special_submission_failure(
        &mut self,
        command: &explorer_model::ExplorerCommand,
        endpoint_error: &ExplorerServiceError,
    ) -> bool {
        let kind = match endpoint_error {
            ExplorerServiceError::Overloaded | ExplorerServiceError::Disconnected => {
                explorer_common::ExplorerErrorKind::Availability
            }
            ExplorerServiceError::Internal => explorer_common::ExplorerErrorKind::Internal,
        };
        let error = || {
            explorer_common::ExplorerError::new(
                kind,
                "submit Explorer command",
                true,
                "The request could not be queued, but Explorer can continue.",
                format!("service endpoint: {endpoint_error:?}"),
            )
        };
        match command {
            explorer_model::ExplorerCommand::ShowContextMenu { context, .. } => {
                let _ = self.state.apply_service_event(
                    explorer_model::ExplorerEvent::ContextMenuFinished {
                        context: context.clone(),
                        outcome: explorer_model::ContextMenuOutcome::Failed { error: error() },
                    },
                );
                if let Some(next) = self.state.take_pending_context_menu_command() {
                    self.submit_command(next);
                }
                true
            }
            explorer_model::ExplorerCommand::PreviewHost { command, .. } => {
                let generation = match command {
                    explorer_model::PreviewHostCommand::Start { bounds, .. }
                    | explorer_model::PreviewHostCommand::SetBounds(bounds) => bounds.generation,
                    explorer_model::PreviewHostCommand::SetFocus { generation }
                    | explorer_model::PreviewHostCommand::Accelerator { generation, .. }
                    | explorer_model::PreviewHostCommand::Unload { generation } => *generation,
                };
                self.apply_preview_host_terminal(&explorer_model::PreviewHostTerminal::Failed {
                    generation,
                    error: explorer_model::PreviewHostError::Disconnected,
                });
                true
            }
            explorer_model::ExplorerCommand::DiscoverLockOwners { context, .. } => {
                let _ = self.state.apply_service_event(
                    explorer_model::ExplorerEvent::LockOwnersDiscovered {
                        context: context.clone(),
                        outcome: explorer_model::LockOwnerDiscoveryTerminal::Failed(error()),
                    },
                );
                true
            }
            explorer_model::ExplorerCommand::CloseLockOwners { context, .. } => {
                let _ = self.state.apply_service_event(
                    explorer_model::ExplorerEvent::LockOwnersClosed {
                        context: context.clone(),
                        outcome: explorer_model::LockOwnerCloseTerminal::Failed(error()),
                    },
                );
                true
            }
            _ => false,
        }
    }

    fn observe_service_event(&mut self, event: &explorer_model::ExplorerEvent) {
        let Some(context) = event.context() else {
            return;
        };
        if matches!(event, explorer_model::ExplorerEvent::DirectoryBatch { .. })
            && self.first_batch_seen.insert(context.request_id)
            && let Some(started) = self.navigation_started.get(&context.request_id)
        {
            tracing::info!(
                request_id = ?context.request_id,
                tab_id = ?context.tab_id,
                generation = context.generation.value(),
                first_item_micros = started.elapsed().as_micros(),
                first_viewport_micros = started.elapsed().as_micros(),
                "Explorer first viewport is ready"
            );
        }
        if matches!(
            event,
            explorer_model::ExplorerEvent::OperationProgress { .. }
        ) && let Some(started) = self.navigation_started.get(&context.request_id)
        {
            tracing::debug!(
                request_id = ?context.request_id,
                progress_to_render_micros = started.elapsed().as_micros(),
                "operation progress reached UI render state"
            );
        }
        if event.is_terminal() {
            self.first_batch_seen.remove(&context.request_id);
            if let Some(started) = self.navigation_started.remove(&context.request_id) {
                self.service_qos
                    .observations_mut()
                    .record_latency(started.elapsed());
                tracing::info!(
                    request_id = ?context.request_id,
                    tab_id = ?context.tab_id,
                    generation = context.generation.value(),
                    terminal_micros = started.elapsed().as_micros(),
                    "Explorer request reached terminal event"
                );
            }
        }
    }

    pub const fn tokens(&self) -> &UiTokens {
        &self.tokens
    }

    pub const fn state(&self) -> &AppViewState {
        &self.state
    }

    fn terminate_scrollbar_drag(
        &mut self,
        reason: interaction::ScrollbarTerminal,
        source: ActionSource,
    ) {
        if self.state.scrollbar_drag_session().is_some() {
            dispatch_action(
                &mut self.state,
                ExplorerAction::EndScrollbarDrag { reason },
                source,
            );
        }
        self.pointer_capture.take();
    }

    fn terminate_details_column_resize(&mut self) {
        self.state.end_details_column_resize();
        self.pointer_capture.take();
    }

    fn update_scrollbar_drag(&mut self, pointer_axis: f32) -> bool {
        let Some(session) = self.state.scrollbar_drag_session() else {
            return false;
        };
        let handle = match session.kind {
            interaction::ScrollbarKind::Navigation => &self.navigation_scroll,
            interaction::ScrollbarKind::FileView
            | interaction::ScrollbarKind::FileViewHorizontal => &self.file_scroll,
        };
        let bounds = handle.bounds();
        let horizontal = session.kind == interaction::ScrollbarKind::FileViewHorizontal;
        let viewport = if horizontal {
            f32::from(bounds.size.width).max(0.0)
        } else {
            f32::from(bounds.size.height).max(0.0)
        };
        let maximum = if horizontal {
            let settings = self.state.view_settings();
            chrome::details_horizontal_maximum(&settings, self.file_viewport_width)
        } else {
            f32::from(handle.max_offset().y).max(0.0)
        };
        let pointer_local = pointer_axis
            - if horizontal {
                f32::from(bounds.left())
            } else {
                f32::from(bounds.top())
            };
        let Some(target) = interaction::scrollbar_target_offset(
            viewport,
            maximum,
            self.tokens.layout.minimum_hit_target.value(),
            pointer_local,
            session.grab_offset_y,
        ) else {
            return false;
        };
        let offset = handle.offset();
        handle.set_offset(if horizontal {
            gpui::point(px(-target), offset.y)
        } else {
            gpui::point(offset.x, px(-target))
        });
        true
    }

    #[allow(
        clippy::needless_pass_by_value,
        clippy::too_many_lines,
        reason = "the root owns each event and may move external path payloads into the service command"
    )]
    fn handle_action(
        &mut self,
        action: ExplorerAction,
        source: ActionSource,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if action == ExplorerAction::ToggleFolderSizeProportionalBar {
            if self.code_lines_runtime.is_some() {
                let current = self.code_lines_visuals.as_ref().map_or(
                    code_lines_column::CodeLinesDisplayMode::default(),
                    |visuals| visuals.config.display,
                );
                let next = match current {
                    code_lines_column::CodeLinesDisplayMode::CodeOnly => {
                        code_lines_column::CodeLinesDisplayMode::WithCommentAndBlank
                    }
                    code_lines_column::CodeLinesDisplayMode::WithCommentAndBlank => {
                        code_lines_column::CodeLinesDisplayMode::CodeOnly
                    }
                };
                self.code_lines_display_override = Some(next);
                if let Some(visuals) = self.code_lines_visuals.as_mut() {
                    visuals.config.display = next;
                }
                cx.notify();
                return;
            }
            let current = self.folder_size_visuals.as_ref().map_or(
                folder_size_column::FolderSizeDisplayMode::default(),
                |visuals| visuals.config.folder_size_display,
            );
            let next = match current {
                folder_size_column::FolderSizeDisplayMode::BarAndText => {
                    folder_size_column::FolderSizeDisplayMode::TextOnly
                }
                folder_size_column::FolderSizeDisplayMode::TextOnly => {
                    folder_size_column::FolderSizeDisplayMode::BarAndText
                }
            };
            self.folder_size_display_override = Some(next);
            if let Some(visuals) = self.folder_size_visuals.as_mut() {
                visuals.config.folder_size_display = next;
            }
            cx.notify();
            return;
        }
        self.capture_durable_window_placement(window, cx);
        if matches!(
            action,
            ExplorerAction::OpenNavigationHistory { .. }
                | ExplorerAction::OpenDetailsColumnMenu { .. }
        ) {
            self.navigation_history_release_deadline =
                Some(Instant::now() + Duration::from_millis(250));
        } else if matches!(action, ExplorerAction::ShowContextMenu { .. }) {
            let suppress_release = self
                .navigation_history_release_deadline
                .is_some_and(|deadline| Instant::now() <= deadline);
            self.navigation_history_release_deadline = None;
            if suppress_release {
                // Opening a popup on right-button down can retarget the matching release to the
                // file view after rerender. Scope suppression to this one gesture; later item
                // context menus are never gated by potentially stale popup state.
                tracing::debug!("ignoring popup release retargeted into the file view");
                return;
            }
        }
        if let ExplorerAction::SelectItem { row_index } = &action {
            tracing::debug!(row_index, "selecting exact presentation row");
        }
        if let ExplorerAction::ShowContextMenu { item_id, .. } = &action {
            tracing::debug!(
                item_identity = ?item_id.as_ref().map(explorer_model::ShellItemId::provider_bytes),
                "opening context menu for exact hit identity"
            );
        }
        let durable_action = is_durable_action(&action);
        let address_is_editing = matches!(
            self.state.tabs().active_tab().view.address.mode,
            explorer_model::AddressBarMode::Editing
                | explorer_model::AddressBarMode::NavigationError
        );
        if address_is_editing && should_end_address_edit(&action, source) {
            self.state.cancel_address_edit();
        }
        if self.state.rename_editor().is_some() && should_end_inline_rename(&action, source) {
            match self
                .state
                .commit_inline_rename(explorer_model::RenameCommitTrigger::Blur)
            {
                Ok(Some(request)) => {
                    self.execute_file_operation(request);
                    self.rename_input = None;
                }
                Ok(None) => self.rename_input = None,
                Err(_) => {}
            }
        }
        let implicit_scrollbar_terminal = match &action {
            ExplorerAction::NewTab
            | ExplorerAction::CloseActiveTab
            | ExplorerAction::ActivateTab { .. }
            | ExplorerAction::CloseTab { .. }
            | ExplorerAction::NextTab
            | ExplorerAction::PreviousTab => Some(interaction::ScrollbarTerminal::TabSwitch),
            ExplorerAction::SetViewMode(_) => Some(interaction::ScrollbarTerminal::ViewSwitch),
            ExplorerAction::CloseWindow => Some(interaction::ScrollbarTerminal::WindowClose),
            _ => None,
        };
        if let Some(reason) = implicit_scrollbar_terminal {
            self.terminate_scrollbar_drag(reason, source);
            self.terminate_details_column_resize();
            if self.state.end_marquee() {
                self.pointer_capture.take();
            }
        }
        let focused_before = self.state.focused_surface();
        let closing_tab = match &action {
            ExplorerAction::CloseActiveTab => Some(self.state.tabs().active_tab_id()),
            ExplorerAction::CloseTab { tab_id } => Some(*tab_id),
            _ => None,
        };
        let ((), measurement) = measure_callback(action.name(), || {
            dispatch_action(&mut self.state, action.clone(), source);
        });
        self.invalidate_size_map_after_action(&action);
        if matches!(
            &action,
            ExplorerAction::NewTab
                | ExplorerAction::CloseActiveTab
                | ExplorerAction::ActivateTab { .. }
                | ExplorerAction::CloseTab { .. }
                | ExplorerAction::NextTab
                | ExplorerAction::PreviousTab
        ) && matches!(
            self.state.tabs().active_tab().search,
            explorer_model::TabSearchState::Idle
        ) {
            // Each tab owns an independent search scope. A fresh empty input makes GPUI consume
            // the newly active tab's current-folder placeholder without touching real queries.
            self.reset_search_input(String::new(), cx);
        }
        if let ExplorerAction::UpdatePreviewHostBoundary {
            parent_window,
            left_physical,
            top_physical,
            width_physical,
            height_physical,
            dpi,
        } = &action
        {
            let boundary = (
                *parent_window,
                *left_physical,
                *top_physical,
                *width_physical,
                *height_physical,
                *dpi,
            );
            if self.preview_host_boundary != Some(boundary) {
                self.preview_host_boundary = Some(boundary);
                if let Some(generation) = self.preview_coordinator.lifecycle().generation()
                    && matches!(
                        self.preview_coordinator.lifecycle(),
                        explorer_model::PreviewLifecycle::Loading { .. }
                            | explorer_model::PreviewLifecycle::Visible { .. }
                    )
                {
                    let tab = self.state.tabs().active_tab();
                    self.submit_command(explorer_model::ExplorerCommand::PreviewHost {
                        context: explorer_model::RequestContext::new(tab.id, tab.generation),
                        command: explorer_model::PreviewHostCommand::SetBounds(
                            explorer_model::PreviewHostBounds {
                                generation,
                                left_physical: *left_physical,
                                top_physical: *top_physical,
                                width_physical: *width_physical,
                                height_physical: *height_physical,
                                dpi: *dpi,
                            },
                        ),
                    });
                }
            }
        }
        if action == ExplorerAction::CloseWindow
            && let Ok(Some(unload)) = self.preview_coordinator.close()
        {
            let _ = self.submit_preview_coordinator_action(unload);
        }
        if matches!(
            action,
            ExplorerAction::FocusNext | ExplorerAction::FocusPrevious
        ) && self.state.focused_surface() == focus::FocusSurface::PreviewPane
            && let Some(generation) = self.preview_coordinator.lifecycle().generation()
            && matches!(
                self.preview_coordinator.lifecycle(),
                explorer_model::PreviewLifecycle::Visible { .. }
            )
        {
            let tab = self.state.tabs().active_tab();
            self.submit_command(explorer_model::ExplorerCommand::PreviewHost {
                context: explorer_model::RequestContext::new(tab.id, tab.generation),
                command: explorer_model::PreviewHostCommand::SetFocus { generation },
            });
        }
        if action == ExplorerAction::RetryExtensionBroker {
            let health = self
                .broker_retry_observer
                .as_ref()
                .map_or(state::BrokerUiHealth::Unavailable, |retry| retry());
            self.state.set_broker_health(health);
            // Force the active selection through the preview coordinator again. Keeping the
            // failed key would make `synchronize_preview_thumbnail` treat Retry as a no-op.
            self.preview_thumbnail_key = None;
            self.preview_texture = None;
            self.preview_thumbnail_failed = false;
        }
        if durable_action {
            self.notify_durable_state();
        }
        if let Some(scope) = self.state.take_confirmed_session_reset() {
            let accepted = self
                .session_reset_observer
                .as_ref()
                .is_some_and(|observer| observer(scope));
            self.state.finish_session_reset_submission(scope, accepted);
        }
        if action == ExplorerAction::ClearThumbnailCache {
            self.thumbnail_scheduler.clear();
            self.thumbnail_memory_cache.clear();
            self.pending_thumbnail_keys.clear();
            self.thumbnail_requests.clear();
            self.thumbnail_presentations.clear();
            let tab = self.state.tabs().active_tab();
            self.submit_command(explorer_model::ExplorerCommand::ClearThumbnailCache {
                context: explorer_model::RequestContext::new(tab.id, tab.generation),
            });
        }
        if action == ExplorerAction::RefreshTortoiseGitStatus {
            let _ = self.refresh_tortoise_git_status();
        }
        if let Some(tab_id) = closing_tab
            && !self.state.tabs().tabs().iter().any(|tab| tab.id == tab_id)
            && let Some(handle) = &self.folder_scripts
            && let Err(error) = handle.close_tab(tab_id)
        {
            tracing::warn!(%error, ?tab_id, "folder automation tab cleanup failed");
        }
        if let ExplorerAction::SelectItem { row_index } = &action {
            self.file_scroll.scroll_to_item(*row_index);
        }
        match action {
            ExplorerAction::BeginContextItemGesture { .. } => {
                // Selecting an unselected row rerenders that row between secondary-button down
                // and up. Native capture keeps the release routed to this root even when the
                // window is not topmost and the original GPUI element has been replaced.
                self.pointer_capture.take();
                self.pointer_capture = self.acquire_pointer_capture(window);
                if self.pointer_capture.is_none() {
                    tracing::debug!(
                        "native context-item gesture capture unavailable; using client capture"
                    );
                }
            }
            ExplorerAction::ShowContextMenu { .. } | ExplorerAction::CancelFileDrag => {
                // TrackPopupMenuEx and OLE must begin without stale GPUI capture ownership.
                self.pointer_capture.take();
            }
            ExplorerAction::UpdateFileDrag { x, y }
                if matches!(
                    self.state.drag_session().state(),
                    explorer_model::DragSessionState::Candidate {
                        button: explorer_model::DragButton::Right,
                        ..
                    }
                ) && self
                    .pointer_capture
                    .as_ref()
                    .is_some_and(|capture| !capture.secondary_button_pressed()) =>
            {
                // GPUI can turn the physical release into MouseMove when selecting the pressed
                // row rerenders it. Win32 button state is the bounded terminal oracle for that
                // one captured candidate; a real right-drag has already left Candidate state.
                let item_id = self.state.pending_context_item_id();
                let extended_verbs = self.state.pending_context_extended_verbs();
                let (owner_window, menu_x, menu_y) =
                    chrome::context_menu_coordinates(gpui::point(px(x), px(y)), window);
                self.pointer_capture.take();
                if let Some(command) = self.state.begin_context_menu_request(
                    item_id,
                    owner_window,
                    menu_x,
                    menu_y,
                    false,
                    extended_verbs,
                ) {
                    cx.on_next_frame(window, move |this, _, cx| {
                        this.submit_command(command);
                        cx.notify();
                    });
                }
                let _ = self.state.cancel_drag();
            }
            ExplorerAction::BeginScrollbarDrag { .. } => {
                self.state.end_details_column_resize();
                self.pointer_capture.take();
                self.pointer_capture = self.acquire_pointer_capture(window);
                if self.pointer_capture.is_none() {
                    tracing::debug!("native scrollbar capture unavailable; using client capture");
                }
            }
            ExplorerAction::BeginMarquee { .. } if self.state.marquee_session().is_some() => {
                self.pointer_capture.take();
                self.pointer_capture = self.acquire_pointer_capture(window);
            }
            ExplorerAction::UpdateMarquee { y, .. } => {
                let layout = self.tokens.layout;
                let body_height = f32::from(window.viewport_size().height)
                    - layout.title_tab_height.value()
                    - layout.address_bar_height.value()
                    - layout.command_bar_height.value()
                    - layout.status_bar_height.value();
                if y < 28.0 {
                    self.file_scroll
                        .scroll_to_item(self.file_scroll.top_item().saturating_sub(1));
                } else if y > body_height - 28.0 {
                    self.file_scroll
                        .scroll_to_item(self.file_scroll.bottom_item().saturating_add(1));
                }
            }
            ExplorerAction::BeginRenameFocused => {
                if let Some(editor) = self.state.rename_editor() {
                    self.reset_rename_input(editor.buffer.clone(), cx);
                    // The editor element is introduced by the render triggered below. Focusing
                    // its handle synchronously is too early because the handle is not yet in the
                    // window dispatch tree; repeat focus at the end of this effect cycle.
                    if let Some(input) = self.rename_input.clone() {
                        window.defer(cx, move |window, cx| {
                            input.read(cx).focus_handle(cx).focus(window, cx);
                        });
                    }
                }
            }
            ExplorerAction::CommitInlineRename => {
                match self
                    .state
                    .commit_inline_rename(explorer_model::RenameCommitTrigger::Enter)
                {
                    Ok(Some(request)) => {
                        self.execute_file_operation(request);
                        self.rename_input = None;
                    }
                    Ok(None) => self.rename_input = None,
                    Err(_) => {}
                }
            }
            ExplorerAction::CancelInlineRename => {
                self.rename_input = None;
            }
            ExplorerAction::UpdateScrollbarDrag { pointer_y } => {
                if self
                    .pointer_capture
                    .as_ref()
                    .is_some_and(|capture| !capture.is_owned())
                {
                    self.terminate_scrollbar_drag(
                        interaction::ScrollbarTerminal::CaptureLost,
                        source,
                    );
                } else {
                    let kind = self
                        .state
                        .scrollbar_drag_session()
                        .map(|session| session.kind);
                    let scale_factor = window.scale_factor();
                    let pointer_axis = self
                        .pointer_capture
                        .as_ref()
                        .and_then(|capture| capture.cursor_client_position())
                        .and_then(|position| {
                            kind.and_then(|kind| {
                                captured_scrollbar_axis_to_logical(kind, position, scale_factor)
                            })
                        })
                        .unwrap_or(pointer_y);
                    if self.update_scrollbar_drag(pointer_axis) {
                        cx.notify();
                        cx.refresh_windows();
                    }
                }
            }
            ExplorerAction::EndMarquee
            | ExplorerAction::EndScrollbarDrag { .. }
            | ExplorerAction::EndDetailsColumnResize
            | ExplorerAction::AutoSizeDetailsColumn { .. } => {
                self.pointer_capture.take();
            }
            ExplorerAction::BeginDetailsColumnResize {
                ref column,
                pointer_x,
            } => {
                self.terminate_scrollbar_drag(interaction::ScrollbarTerminal::ViewSwitch, source);
                self.pointer_capture = self.acquire_pointer_capture(window);
                let scale_factor = window.scale_factor();
                if let Some(client_x) = self
                    .pointer_capture
                    .as_ref()
                    .and_then(|capture| capture.cursor_client_position().map(|position| position.0))
                    .and_then(|client_x| physical_client_to_logical(client_x, scale_factor))
                {
                    // GPUI events are logical pixels while Win32 ScreenToClient is physical.
                    // Normalize the captured sample once so the reducer always sees logical px.
                    if (client_x - pointer_x).abs() > f32::EPSILON {
                        self.state
                            .begin_details_column_resize(column.clone(), client_x);
                    }
                }
            }
            ExplorerAction::UpdateDetailsColumnResize { pointer_x } => {
                if self
                    .pointer_capture
                    .as_ref()
                    .is_some_and(|capture| !capture.is_owned())
                {
                    self.terminate_details_column_resize();
                } else {
                    let scale_factor = window.scale_factor();
                    let client_x = self
                        .pointer_capture
                        .as_ref()
                        .and_then(|capture| capture.cursor_client_position())
                        .and_then(|position| physical_client_to_logical(position.0, scale_factor));
                    // The reducer already processed GPUI's event coordinate. Re-apply the same
                    // action using the normalized capture sample outside the HWND, where GPUI may
                    // report a clipped or rebased value.
                    if let Some(client_x) = client_x
                        && (client_x - pointer_x).abs() > f32::EPSILON
                    {
                        self.state.update_details_column_resize(client_x);
                    }
                }
            }
            _ => {}
        }
        if matches!(
            action,
            ExplorerAction::FocusAddress | ExplorerAction::EnterAddressEdit
        ) {
            if !address_is_editing {
                self.reset_address_input(self.state.address_draft().to_owned(), cx);
            }
            if let Some(input) = &self.address_input {
                if !address_is_editing || source == ActionSource::Keyboard {
                    input.update(cx, EditableTextState::select_document);
                }
                // Entering edit mode replaces the breadcrumb tree with a new text input. Its
                // focus handle is not attached to the window dispatch tree until the next effect
                // cycle, so focusing it synchronously can leave the address bar visibly editing
                // but unable to receive keyboard or IME input (notably after namespace/ZIP
                // navigation). Repeat the focus after the new element has been mounted.
                let input = input.clone();
                window.defer(cx, move |window, cx| {
                    input.read(cx).focus_handle(cx).focus(window, cx);
                    let input = input.clone();
                    window.defer(cx, move |window, cx| {
                        input.read(cx).focus_handle(cx).focus(window, cx);
                    });
                });
            }
        }
        if action == ExplorerAction::SubmitFocusedInput {
            self.submit_focused_input(focused_before, cx);
        }
        if action == ExplorerAction::ClearSearch {
            self.reset_search_input(String::new(), cx);
            if let Some(input) = &self.search_input {
                input.read(cx).focus_handle(cx).focus(window, cx);
            }
        }
        if action == ExplorerAction::CancelFocusedInput
            && focused_before == focus::FocusSurface::Search
        {
            self.reset_search_input(String::new(), cx);
        }
        if matches!(
            action,
            ExplorerAction::OpenBreadcrumbChildren { .. }
                | ExplorerAction::RetryBreadcrumbChildren { .. }
        ) && let Some(command) = self.state.begin_child_container_request()
        {
            self.submit_command(command);
        }
        if let ExplorerAction::ToggleNavigationNode { location } = &action
            && self.state.navigation_node_expanded(location)
            && let Some(command) = self.state.begin_navigation_node_request(location.clone())
        {
            self.submit_command(command);
        }
        if matches!(action, ExplorerAction::ToggleNavigationNode { .. }) {
            self.submit_navigation_icon_loads();
        }
        let navigation_command = match &action {
            ExplorerAction::Back => self.state.begin_back_navigation(),
            ExplorerAction::Forward => self.state.begin_forward_navigation(),
            ExplorerAction::ActivateNavigationHistory { direction, steps } => {
                self.state.begin_history_navigation(*direction, *steps)
            }
            ExplorerAction::Up => self.state.begin_up_navigation(),
            ExplorerAction::Refresh => self.state.begin_refresh_navigation(),
            ExplorerAction::SubmitAddress(value) => self.state.begin_address_submission(value),
            ExplorerAction::ActivateBreadcrumbSegment { location }
            | ExplorerAction::ActivateBreadcrumbChild { location }
            | ExplorerAction::ActivateNavigationItem { location } => {
                self.state.begin_active_navigation(location.clone(), false)
            }
            _ => None,
        };
        if let Some(command) = navigation_command {
            self.submit_command(command);
        }
        if action == ExplorerAction::NewTab
            && let Some(command) = self.state.take_pending_new_tab_command()
        {
            self.submit_command(command);
        }
        if action == ExplorerAction::ToggleTheme {
            let theme = match self.tokens.theme.mode {
                ThemeMode::Light => explorer_model::ShellIconTheme::Light,
                ThemeMode::Dark => explorer_model::ShellIconTheme::Dark,
            };
            self.shell_icons
                .invalidate_environment(self.shell_icon_dpi, theme);
            self.icon_epochs.advance_association();
            self.base_icons
                .invalidate_environment(self.shell_icon_dpi, theme);
            self.pending_base_icons.clear();
            self.pending_visible_bases.clear();
            self.failed_base_icons.clear();
            self.submit_navigation_icon_loads();
        }
        if matches!(
            &action,
            ExplorerAction::SetViewMode(_)
                | ExplorerAction::SetExtensionView { .. }
                | ExplorerAction::ZoomView { .. }
        ) {
            let tab = self.state.tabs().active_tab();
            let context = explorer_model::RequestContext::new(tab.id, tab.generation);
            let entries = tab
                .visible_snapshot()
                .map(|snapshot| snapshot.entries().to_vec())
                .unwrap_or_default();
            self.submit_offscreen_file_icon_loads(&context, &entries);
        }
        if let ExplorerAction::OpenItem { row_index, new_tab } = action {
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_or(0, |value| value.as_secs());
            let _ = self.state.record_recent_row(row_index, now);
            if let Some(command) = self.state.open_row_command(row_index, new_tab) {
                self.submit_command(command);
            }
        }
        if action == ExplorerAction::OpenFocused
            && let Some(row_index) = self.state.focused_row_index()
        {
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_or(0, |value| value.as_secs());
            let _ = self.state.record_recent_row(row_index, now);
            if let Some(command) = self.state.open_row_command(row_index, false) {
                self.submit_command(command);
            }
        }
        if action == ExplorerAction::CreateFolder
            && let Some(request) = self.state.create_folder_request()
        {
            self.execute_file_operation(request);
        }
        if let ExplorerAction::CreateNewItem { index } = action
            && let Some(request) = self.state.create_new_item_request(index)
        {
            self.execute_file_operation(request);
        }
        if action == ExplorerAction::RecycleDeleteSelected
            && let Some(request) = self.state.recycle_selected_request()
        {
            self.execute_file_operation(request);
        }
        if action == ExplorerAction::CreateShortcutSelected
            && let Some(request) = self.state.create_shortcut_selected_request()
        {
            self.execute_file_operation(request);
        }
        if action == ExplorerAction::ConfirmPermanentDelete
            && let Some(request) = self.state.confirm_permanent_delete()
        {
            self.execute_file_operation(request);
        }
        if action == ExplorerAction::RetryLockedDelete
            && let Some(command) = self.state.retry_locked_delete()
        {
            self.submit_command(command);
        }
        if action == ExplorerAction::CloseLockOwnersAndRetry
            && let Some(command) = self.state.close_lock_owners_and_retry()
        {
            self.submit_command(command);
        }
        if let ExplorerAction::CancelOperation { request_id } = action {
            self.cancel_file_operation(request_id);
        }
        let clipboard_mode = match &action {
            ExplorerAction::CopySelected => Some(explorer_model::ClipboardMode::Copy),
            ExplorerAction::CutSelected => Some(explorer_model::ClipboardMode::Cut),
            _ => None,
        };
        if let Some(mode) = clipboard_mode
            && let Some(command) = self.state.begin_clipboard_request(mode)
        {
            self.submit_command(command);
        }
        if action == ExplorerAction::Paste
            && let Some(command) = self
                .state
                .begin_paste_request(explorer_model::ConflictDecision::Prompt)
        {
            self.submit_command(command);
        }
        if let Some(command) = self.state.take_pending_drag_command() {
            // OLE owns capture for the modal drag loop. Release the short GPUI gesture capture
            // before crossing into the Shell so right-drag and left-drag do not compete for it.
            self.pointer_capture.take();
            self.submit_command(command);
        }
        if action == ExplorerAction::ShareSelected {
            let (owner_window, _, _) =
                chrome::context_menu_coordinates(gpui::point(px(0.0), px(0.0)), window);
            if let Some(command) = self.state.begin_share_request(owner_window) {
                self.submit_command(command);
            }
        }
        if action == ExplorerAction::PinSelectedToStart {
            let (owner_window, _, _) =
                chrome::context_menu_coordinates(gpui::point(px(0.0), px(0.0)), window);
            if let Some(command) = self.state.begin_pin_to_start_request(owner_window) {
                self.submit_command(command);
            }
        }
        if action == ExplorerAction::ShowPropertiesSelected {
            let (owner_window, _, _) =
                chrome::context_menu_coordinates(gpui::point(px(0.0), px(0.0)), window);
            if let Some(command) = self.state.begin_properties_request(owner_window) {
                self.submit_command(command);
            }
        }
        if action == ExplorerAction::AddSelectedToFavorites
            && let Some(previous) = self.state.toggle_selected_quick_access()
            && !self.notify_durable_state()
        {
            self.state.rollback_quick_access(previous);
        }
        if matches!(
            action,
            ExplorerAction::UndoCurrentFolder
                | ExplorerAction::CompressSelectedToZip
                | ExplorerAction::RestoreSelected
                | ExplorerAction::EmptyRecycleBin
        ) {
            let (owner_window, _, _) =
                chrome::context_menu_coordinates(gpui::point(px(0.0), px(0.0)), window);
            let command = match action {
                ExplorerAction::UndoCurrentFolder => self.state.begin_undo_request(owner_window),
                ExplorerAction::CompressSelectedToZip => {
                    self.state.begin_compress_to_zip_request(owner_window)
                }
                ExplorerAction::RestoreSelected => self.state.begin_restore_request(owner_window),
                ExplorerAction::EmptyRecycleBin => {
                    self.state.begin_empty_recycle_bin_request(owner_window)
                }
                _ => None,
            };
            if let Some(command) = command {
                self.submit_command(command);
            }
        }
        if action == ExplorerAction::CopySelectedPaths
            && let Some(text) = self.state.selected_paths_clipboard_text()
        {
            cx.write_to_clipboard(ClipboardItem::new_string(text));
        }
        if let ExplorerAction::ShowContextMenu {
            item_id,
            owner_window,
            x,
            y,
            keyboard_invoked,
            extended_verbs,
        } = &action
            && let Some(command) = self.state.begin_context_menu_request(
                item_id.clone(),
                *owner_window,
                *x,
                *y,
                *keyboard_invoked,
                *extended_verbs,
            )
        {
            cx.on_next_frame(window, move |this, _, cx| {
                this.submit_command(command);
                cx.notify();
            });
        }
        if matches!(action, ExplorerAction::UpdateExternalDrag { .. }) {
            match self.state.drag_session().state() {
                explorer_model::DragSessionState::Dragging {
                    auto_scroll: Some(explorer_model::AutoScrollDirection::Up),
                    ..
                } => self
                    .file_scroll
                    .scroll_to_item(self.file_scroll.top_item().saturating_sub(1)),
                explorer_model::DragSessionState::Dragging {
                    auto_scroll: Some(explorer_model::AutoScrollDirection::Down),
                    ..
                } => self
                    .file_scroll
                    .scroll_to_item(self.file_scroll.bottom_item().saturating_add(1)),
                _ => {}
            }
        }
        self.file_performance.record_input(measurement.elapsed);
        measurement.record();
        synchronize_theme(&mut self.tokens, &self.state);
        self.synchronize_native_focus(window, cx);
        if self.state.close_requested() {
            window.remove_window();
        }
        cx.notify();
    }

    fn breadcrumb_key_action(&self, event: &gpui::KeyDownEvent) -> Option<ExplorerAction> {
        if self.state.focused_surface() != focus::FocusSurface::AddressBar {
            return None;
        }
        let address = &self.state.tabs().active_tab().view.address;
        match address.mode {
            explorer_model::AddressBarMode::Editing
            | explorer_model::AddressBarMode::NavigationError => None,
            explorer_model::AddressBarMode::Browsing => match event.keystroke.key.as_str() {
                "left" => Some(ExplorerAction::MoveBreadcrumbSegmentFocus { direction: -1 }),
                "right" => Some(ExplorerAction::MoveBreadcrumbSegmentFocus { direction: 1 }),
                "home" => Some(ExplorerAction::MoveBreadcrumbSegmentFocus { direction: -127 }),
                "end" => Some(ExplorerAction::MoveBreadcrumbSegmentFocus { direction: 127 }),
                "down" => self
                    .state
                    .focused_breadcrumb_segment_id()
                    .map(|segment_id| ExplorerAction::OpenBreadcrumbChildren { segment_id }),
                "enter" | "space" => self
                    .state
                    .focused_breadcrumb_location()
                    .map(|location| ExplorerAction::ActivateBreadcrumbSegment { location }),
                _ => None,
            },
            explorer_model::AddressBarMode::EnumeratingMenu { .. } => {
                let movement = match event.keystroke.key.as_str() {
                    "up" => Some(explorer_model::MenuFocusMovement::Previous),
                    "down" => Some(explorer_model::MenuFocusMovement::Next),
                    "home" => Some(explorer_model::MenuFocusMovement::First),
                    "end" => Some(explorer_model::MenuFocusMovement::Last),
                    "pageup" => Some(explorer_model::MenuFocusMovement::PagePrevious),
                    "pagedown" => Some(explorer_model::MenuFocusMovement::PageNext),
                    _ => None,
                };
                if let Some(movement) = movement {
                    return Some(ExplorerAction::MoveBreadcrumbMenuFocus { movement });
                }
                match event.keystroke.key.as_str() {
                    "left" | "right" => Some(ExplorerAction::CloseBreadcrumbMenu),
                    "enter" | "space" => self
                        .state
                        .focused_breadcrumb_menu_location()
                        .map(|location| ExplorerAction::ActivateBreadcrumbChild { location }),
                    _ if !event.keystroke.modifiers.control
                        && !event.keystroke.modifiers.alt
                        && !event.keystroke.modifiers.platform =>
                    {
                        event
                            .keystroke
                            .key_char
                            .as_ref()
                            .filter(|text| !text.chars().all(char::is_whitespace))
                            .map(|text| ExplorerAction::TypeAheadBreadcrumbMenu {
                                text: text.clone(),
                            })
                    }
                    _ => None,
                }
            }
        }
    }

    fn file_view_key_action(
        &self,
        event: &gpui::KeyDownEvent,
        window: &Window,
    ) -> Option<ExplorerAction> {
        if self.state.focused_surface() != focus::FocusSurface::FileView
            || (event.keystroke.modifiers.alt && event.keystroke.key != "enter")
            || event.keystroke.modifiers.platform
        {
            return None;
        }
        if self.state.rename_editor().is_some() {
            return match event.keystroke.key.as_str() {
                "enter" => Some(ExplorerAction::CommitInlineRename),
                "escape" => Some(ExplorerAction::CancelInlineRename),
                _ => None,
            };
        }
        if self.state.marquee_session().is_some() && event.keystroke.key == "escape" {
            return Some(ExplorerAction::EndMarquee);
        }
        if let Some(action) = file_view_global_command_action(event) {
            return Some(action);
        }
        let count = self.state.visible_row_count();
        if count == 0 {
            return None;
        }
        let current = self.state.focused_row_index().unwrap_or(0).min(count - 1);
        let current_position = f32::from(u16::try_from(current).unwrap_or(u16::MAX));
        if let Some(action) = file_view_item_command_action(event, current) {
            return Some(action);
        }
        match event.keystroke.key.as_str() {
            "menu" | "f10" if event.keystroke.key == "menu" || event.keystroke.modifiers.shift => {
                let layout = self.tokens.layout;
                let position = gpui::point(
                    px(self.state.navigation_pane_width().value() + 96.0),
                    px(layout.title_tab_height.value()
                        + layout.address_bar_height.value()
                        + layout.command_bar_height.value()
                        + layout.details_header_height.value()
                        + layout.file_row_height.value() * (current_position + 0.5)),
                );
                let (owner_window, x, y) = chrome::context_menu_coordinates(position, window);
                return Some(ExplorerAction::ShowContextMenu {
                    item_id: self.state.presentation_item_id(current),
                    owner_window,
                    x,
                    y,
                    keyboard_invoked: true,
                    extended_verbs: false,
                });
            }
            _ => {}
        }
        if event.keystroke.key == "escape" {
            return Some(ExplorerAction::ClearSelection);
        }
        let target = file_view_navigation_target(
            &self.state.view_settings(),
            self.tokens.layout,
            self.file_viewport_width,
            current,
            count,
            event.keystroke.key.as_str(),
        )?;
        Some(if event.keystroke.modifiers.shift {
            ExplorerAction::SelectRange {
                row_index: target,
                additive: event.keystroke.modifiers.control,
            }
        } else if event.keystroke.modifiers.control {
            ExplorerAction::FocusItem { row_index: target }
        } else {
            ExplorerAction::SelectItem { row_index: target }
        })
    }

    fn navigation_history_key_action(&self, event: &gpui::KeyDownEvent) -> Option<ExplorerAction> {
        let direction = self.state.navigation_history_menu_direction()?;
        match event.keystroke.key.as_str() {
            "up" => Some(ExplorerAction::MoveNavigationHistoryFocus { direction: -1 }),
            "down" => Some(ExplorerAction::MoveNavigationHistoryFocus { direction: 1 }),
            "home" => Some(ExplorerAction::MoveNavigationHistoryFocus { direction: i8::MIN }),
            "end" => Some(ExplorerAction::MoveNavigationHistoryFocus { direction: i8::MAX }),
            "escape" => Some(ExplorerAction::CloseNavigationHistory),
            "enter" | "space" => Some(ExplorerAction::ActivateNavigationHistory {
                direction,
                steps: self.state.navigation_history_menu_index() + 1,
            }),
            _ => None,
        }
    }

    fn command_more_key_action(&self, event: &gpui::KeyDownEvent) -> Option<ExplorerAction> {
        if self.state.focused_surface() != focus::FocusSurface::CommandBar
            || !self.state.more_menu_open()
        {
            return None;
        }
        match event.keystroke.key.as_str() {
            "up" => Some(ExplorerAction::MoveMoreMenuFocus { direction: -1 }),
            "down" => Some(ExplorerAction::MoveMoreMenuFocus { direction: 1 }),
            "home" => Some(ExplorerAction::MoveMoreMenuFocus { direction: i8::MIN }),
            "end" => Some(ExplorerAction::MoveMoreMenuFocus { direction: i8::MAX }),
            "escape" | "left" | "right" => Some(ExplorerAction::CloseMoreMenu),
            "enter" | "space" => Some(match self.state.more_menu_index() {
                0 => ExplorerAction::UndoCurrentFolder,
                1 => ExplorerAction::CompressSelectedToZip,
                2 => ExplorerAction::AddSelectedToFavorites,
                3 => ExplorerAction::CopySelectedPaths,
                4 => ExplorerAction::SelectAllItems,
                5 => ExplorerAction::ClearSelection,
                6 => ExplorerAction::InvertSelection,
                7 => ExplorerAction::ShowPropertiesSelected,
                8 => ExplorerAction::RestoreSelected,
                9 => ExplorerAction::EmptyRecycleBin,
                _ => ExplorerAction::OpenFolderOptions,
            }),
            _ => None,
        }
    }

    fn command_new_key_action(&self, event: &gpui::KeyDownEvent) -> Option<ExplorerAction> {
        if self.state.focused_surface() != focus::FocusSurface::CommandBar
            || !self.state.new_menu_open()
        {
            return None;
        }
        match event.keystroke.key.as_str() {
            "up" => Some(ExplorerAction::MoveNewMenuFocus { direction: -1 }),
            "down" => Some(ExplorerAction::MoveNewMenuFocus { direction: 1 }),
            "home" => Some(ExplorerAction::MoveNewMenuFocus { direction: i8::MIN }),
            "end" => Some(ExplorerAction::MoveNewMenuFocus { direction: i8::MAX }),
            "escape" | "left" | "right" => Some(ExplorerAction::CloseNewMenu),
            "enter" | "space" => Some(ExplorerAction::CreateNewItem {
                index: self.state.new_menu_index(),
            }),
            _ => None,
        }
    }

    fn command_sort_key_action(&self, event: &gpui::KeyDownEvent) -> Option<ExplorerAction> {
        if self.state.focused_surface() != focus::FocusSurface::CommandBar
            || !self.state.sort_menu_open()
        {
            return None;
        }
        match event.keystroke.key.as_str() {
            "up" => Some(ExplorerAction::MoveSortMenuFocus { direction: -1 }),
            "down" => Some(ExplorerAction::MoveSortMenuFocus { direction: 1 }),
            "home" => Some(ExplorerAction::MoveSortMenuFocus { direction: i8::MIN }),
            "end" => Some(ExplorerAction::MoveSortMenuFocus { direction: i8::MAX }),
            "escape" | "left" | "right" => Some(ExplorerAction::CloseSortMenu),
            "enter" | "space" => Some(match self.state.sort_menu_index() {
                0 => ExplorerAction::SetColumnId(explorer_model::ColumnId::Name),
                1 => ExplorerAction::SetColumnId(explorer_model::ColumnId::DateModified),
                2 => ExplorerAction::SetColumnId(explorer_model::ColumnId::Type),
                3 => ExplorerAction::SetColumnId(explorer_model::ColumnId::Size),
                4 => ExplorerAction::SetSortDirection(explorer_model::SortDirection::Ascending),
                _ => ExplorerAction::SetSortDirection(explorer_model::SortDirection::Descending),
            }),
            _ => None,
        }
    }

    fn command_view_key_action(&self, event: &gpui::KeyDownEvent) -> Option<ExplorerAction> {
        if self.state.focused_surface() != focus::FocusSurface::CommandBar
            || !self.state.view_menu_open()
        {
            return None;
        }
        match event.keystroke.key.as_str() {
            "up" => Some(ExplorerAction::MoveViewMenuFocus { direction: -1 }),
            "down" => Some(ExplorerAction::MoveViewMenuFocus { direction: 1 }),
            "home" => Some(ExplorerAction::MoveViewMenuFocus { direction: i8::MIN }),
            "end" => Some(ExplorerAction::MoveViewMenuFocus { direction: i8::MAX }),
            "escape" | "left" => Some(ExplorerAction::CloseViewMenu),
            "right" if self.state.view_menu_index() == 10 => {
                Some(ExplorerAction::ToggleViewShowSubmenu)
            }
            "enter" | "space" => Some(match self.state.view_menu_index() {
                0 => ExplorerAction::SetViewMode(explorer_model::ViewMode::ExtraLargeIcons),
                1 => ExplorerAction::SetViewMode(explorer_model::ViewMode::LargeIcons),
                2 => ExplorerAction::SetViewMode(explorer_model::ViewMode::MediumIcons),
                3 => ExplorerAction::SetViewMode(explorer_model::ViewMode::SmallIcons),
                4 => ExplorerAction::SetViewMode(explorer_model::ViewMode::List),
                5 => ExplorerAction::SetViewMode(explorer_model::ViewMode::Details),
                6 => ExplorerAction::SetViewMode(explorer_model::ViewMode::Tiles),
                7 => ExplorerAction::SetViewMode(explorer_model::ViewMode::Content),
                8 => ExplorerAction::ToggleDetailsPane,
                9 => ExplorerAction::TogglePreviewPane,
                10 => ExplorerAction::ToggleViewShowSubmenu,
                11 => self
                    .size_map_runtime
                    .as_ref()
                    .map(|runtime| runtime.config())
                    .filter(size_map_view::is_supported_size_map_config)
                    .map_or(ExplorerAction::ToggleViewShowSubmenu, |config| {
                        ExplorerAction::SetExtensionView {
                            view_id: config.view_id,
                        }
                    }),
                _ => ExplorerAction::ToggleViewShowSubmenu,
            }),
            _ => None,
        }
    }

    fn command_extensions_key_action(&self, event: &gpui::KeyDownEvent) -> Option<ExplorerAction> {
        if self.state.focused_surface() != focus::FocusSurface::CommandBar
            || !self.state.extensions_menu_open()
        {
            return None;
        }
        match event.keystroke.key.as_str() {
            "escape" | "left" | "right" => Some(ExplorerAction::CloseExtensionsMenu),
            "enter" | "space" if self.state.tortoise_git_available() => {
                Some(ExplorerAction::RefreshTortoiseGitStatus)
            }
            _ => None,
        }
    }

    fn details_column_menu_key_action(&self, event: &gpui::KeyDownEvent) -> Option<ExplorerAction> {
        self.state.details_column_menu()?;
        match event.keystroke.key.as_str() {
            "escape" | "left" => Some(ExplorerAction::CloseDetailsColumnMenu),
            _ => None,
        }
    }

    fn navigation_tree_key_action(&self, event: &gpui::KeyDownEvent) -> Option<ExplorerAction> {
        if self.state.focused_surface() != focus::FocusSurface::NavigationPane {
            return None;
        }
        let location = self.state.focused_navigation_location()?.clone();
        match event.keystroke.key.as_str() {
            "right" if !self.state.navigation_node_expanded(&location) => {
                Some(ExplorerAction::ToggleNavigationNode { location })
            }
            "left" if self.state.navigation_node_expanded(&location) => {
                Some(ExplorerAction::ToggleNavigationNode { location })
            }
            "left" => location
                .path()
                .and_then(std::path::Path::parent)
                .map(|parent| ExplorerAction::ActivateNavigationItem {
                    location: explorer_model::LocationDescriptor::file_system(parent),
                }),
            "enter" | "space" => Some(ExplorerAction::ActivateNavigationItem { location }),
            _ => None,
        }
    }

    fn submit_focused_input(&mut self, focused: focus::FocusSurface, cx: &mut Context<Self>) {
        match focused {
            focus::FocusSurface::Search => {
                let input = self
                    .search_input
                    .as_ref()
                    .map(|input| input.read(cx).as_str().to_owned())
                    .unwrap_or_default();
                let _ = self.submit_search(input);
            }
            focus::FocusSurface::AddressBar => {
                let input = self
                    .address_input
                    .as_ref()
                    .map(|input| input.read(cx).as_str().trim().to_owned())
                    .unwrap_or_default();
                if !input.is_empty()
                    && let Some(command) = self.state.begin_address_submission(&input)
                {
                    self.submit_command(command);
                }
            }
            _ => {}
        }
    }

    fn synchronize_native_focus(&self, window: &mut Window, cx: &mut Context<Self>) {
        if self.state.rename_editor().is_some()
            && let Some(input) = &self.rename_input
        {
            input.read(cx).focus_handle(cx).focus(window, cx);
            return;
        }
        if (self.state.sort_menu_open()
            || self.state.view_menu_open()
            || self.state.more_menu_open()
            || self.state.extensions_menu_open()
            || self.state.new_menu_open()
            || self.state.details_column_menu().is_some())
            && let Some(menu) = &self.command_menu_focus
        {
            menu.focus(window, cx);
            return;
        }
        if matches!(
            self.state.tabs().active_tab().view.address.mode,
            explorer_model::AddressBarMode::EnumeratingMenu { .. }
        ) || self.state.navigation_history_menu_direction().is_some()
        {
            if let Some(menu) = &self.breadcrumb_menu_focus {
                menu.focus(window, cx);
            }
            return;
        }
        let input = match self.state.focused_surface() {
            focus::FocusSurface::AddressBar
                if matches!(
                    self.state.tabs().active_tab().view.address.mode,
                    explorer_model::AddressBarMode::Editing
                        | explorer_model::AddressBarMode::NavigationError
                ) =>
            {
                self.address_input.as_ref()
            }
            focus::FocusSurface::Search => self.search_input.as_ref(),
            _ => None,
        };
        if let Some(input) = input {
            input.read(cx).focus_handle(cx).focus(window, cx);
        } else if let Some(root) = &self.focus_handle {
            root.focus(window, cx);
        }
    }
}

fn file_view_navigation_target(
    settings: &explorer_model::ViewSettings,
    layout: LayoutTokens,
    viewport_width: f32,
    current: usize,
    count: usize,
    key: &str,
) -> Option<usize> {
    if count == 0 {
        return None;
    }
    let current = current.min(count - 1);
    let columns = chrome::spatial_grid_columns(
        chrome::spatial_grid_metrics(settings, layout),
        viewport_width,
        count,
    );
    let page = columns.saturating_mul(8).max(1);
    Some(match key {
        "left" => current.saturating_sub(1),
        "right" => current.saturating_add(1).min(count - 1),
        "up" => current.saturating_sub(columns),
        "down" => current.saturating_add(columns).min(count - 1),
        "home" => 0,
        "end" => count - 1,
        "pageup" => current.saturating_sub(page),
        "pagedown" => current.saturating_add(page).min(count - 1),
        _ => return None,
    })
}

fn synchronize_theme(tokens: &mut UiTokens, state: &AppViewState) {
    tokens.theme = match state.current_theme() {
        ThemeMode::Light => ThemeTokens::light(),
        ThemeMode::Dark => ThemeTokens::dark(),
    };
}

impl Default for ExplorerRoot {
    fn default() -> Self {
        Self::new(UiTokens::default())
    }
}

fn seed_visual_directory(state: &mut AppViewState, fixture: VisualFixtureState) {
    let populated = !matches!(
        fixture,
        VisualFixtureState::Empty | VisualFixtureState::Error
    );
    seed_active_visual_tab(
        state,
        "Visual Fixture",
        populated,
        matches!(fixture, VisualFixtureState::Error),
    );

    match fixture {
        VisualFixtureState::Empty | VisualFixtureState::Populated | VisualFixtureState::Error => {}
        VisualFixtureState::MultiTab => {
            let _ = state.new_tab();
            seed_active_visual_tab(state, "Downloads", true, false);
        }
        VisualFixtureState::Operation => {
            let location = explorer_model::LocationDescriptor::file_system(r"C:\VisualFixture");
            let request = explorer_model::FileOperationRequest {
                kind: explorer_model::FileOperationKind::CreateFolder {
                    parent: location,
                    name: "Quarterly Reports".to_owned(),
                },
                flags: explorer_model::FileOperationFlags::default(),
            };
            let command = state.begin_file_operation(request);
            if let Some(context) = command.context().cloned() {
                let _ =
                    state.apply_service_event(explorer_model::ExplorerEvent::OperationProgress {
                        context,
                        progress: explorer_model::OperationProgress {
                            completed_items: 0,
                            total_items: 1,
                            completed_bytes: 6_291_456,
                            total_bytes: Some(12_582_912),
                        },
                    });
            }
        }
        VisualFixtureState::DragCue => {
            let _ = state.select_row(0);
            state.update_external_drag_target(
                Some(1),
                explorer_model::DropTargetKind::FolderItem,
                240.0,
                180.0,
                620.0,
                explorer_model::DragEffect::Copy,
            );
        }
        VisualFixtureState::Search => {
            if let Some(command) = state.begin_active_search("type:txt quarterly".to_owned())
                && let Some(context) = command.context().cloned()
            {
                let entries = visual_entries("Search result");
                let _ = state.apply_service_event(explorer_model::ExplorerEvent::SearchBatch {
                    context: context.clone(),
                    source: explorer_model::SearchBackend::FileSystemFallback,
                    entries,
                });
                let _ = state.apply_service_event(explorer_model::ExplorerEvent::SearchStatus {
                    context: context.clone(),
                    status: explorer_model::SearchSourceStatus {
                        backend: explorer_model::SearchBackend::WindowsIndex,
                        phase: explorer_model::SearchSourcePhase::Unavailable,
                        diagnostic: Some(
                            "Index unavailable; filesystem fallback active".to_owned(),
                        ),
                    },
                });
                let error = explorer_common::ExplorerError::new(
                    explorer_common::ExplorerErrorKind::Availability,
                    "visual search fixture",
                    true,
                    "Some locations could not be searched.",
                    "deterministic partial fixture",
                );
                let _ = state.apply_service_event(explorer_model::ExplorerEvent::SearchFinished {
                    context,
                    outcome: explorer_model::SearchTerminal::Partial(error),
                });
            }
        }
        VisualFixtureState::Focused => {
            state.focus(focus::FocusSurface::Search);
        }
    }
}

fn seed_active_visual_tab(state: &mut AppViewState, title: &str, populated: bool, failed: bool) {
    let Some(command) = state.begin_active_location_load() else {
        return;
    };
    let Some(context) = command.context().cloned() else {
        return;
    };
    let descriptor = explorer_model::LocationDescriptor::file_system(r"C:\VisualFixture");
    let _ = state.apply_service_event(explorer_model::ExplorerEvent::LocationResolved {
        context: context.clone(),
        metadata: explorer_model::LocationMetadata {
            descriptor,
            display_title: title.to_owned(),
            can_go_up: true,
            can_write: true,
        },
    });
    if failed {
        let error = explorer_common::ExplorerError::new(
            explorer_common::ExplorerErrorKind::Authorization,
            "visual directory fixture",
            true,
            "This folder cannot be opened. Check your permissions and try again.",
            "deterministic access-denied fixture",
        );
        let _ = state.apply_service_event(explorer_model::ExplorerEvent::Failed { context, error });
        return;
    }
    if populated {
        let _ = state.apply_service_event(explorer_model::ExplorerEvent::DirectoryBatch {
            context: context.clone(),
            entries: visual_entries(title),
        });
    }
    let _ = state.apply_service_event(explorer_model::ExplorerEvent::DirectoryFinished { context });
}

fn visual_entries(prefix: &str) -> Vec<explorer_model::FileEntry> {
    [
        (1_u8, format!("{prefix} folder"), true),
        (2_u8, format!("{prefix} notes.txt"), false),
        (3_u8, "專案摘要.docx".to_owned(), false),
        (4_u8, "Archive".to_owned(), true),
    ]
    .into_iter()
    .filter_map(|(identity, display_name, is_container)| {
        let id = explorer_model::ShellItemId::from_provider_bytes([identity])?;
        Some(explorer_model::FileEntry {
            id,
            location: explorer_model::LocationDescriptor::file_system(format!(
                r"C:\VisualFixture\{display_name}"
            )),
            display_name,
            is_container,
            metadata: explorer_model::FileEntryMetadata::default(),
        })
    })
    .collect()
}

fn shell_icon_texture(payload: &explorer_model::ShellIconPayload) -> Option<Arc<RenderImage>> {
    let tight_stride = usize::from(payload.width) * 4;
    let mut pixels = if payload.stride as usize == tight_stride {
        payload.rgba.clone()
    } else {
        let mut tight = Vec::with_capacity(tight_stride * usize::from(payload.height));
        for row in payload.rgba.chunks_exact(payload.stride as usize) {
            tight.extend_from_slice(&row[..tight_stride]);
        }
        tight
    };
    prepare_shell_texture_pixels(&mut pixels);
    let buffer =
        image::RgbaImage::from_raw(u32::from(payload.width), u32::from(payload.height), pixels)?;
    let frame = image::Frame::new(buffer);
    Some(Arc::new(RenderImage::new(smallvec::smallvec![frame])))
}

fn thumbnail_texture(pixels: &explorer_model::ThumbnailPixels) -> Option<Arc<RenderImage>> {
    let width = usize::try_from(pixels.width).ok()?;
    let height = usize::try_from(pixels.height).ok()?;
    let tight_stride = width.checked_mul(4)?;
    let source_stride = usize::try_from(pixels.stride).ok()?;
    let mut owned = if source_stride == tight_stride {
        pixels.bytes.clone()
    } else {
        let capacity = tight_stride.checked_mul(height)?;
        let mut tight = Vec::with_capacity(capacity);
        for row in pixels.bytes.chunks_exact(source_stride).take(height) {
            tight.extend_from_slice(row.get(..tight_stride)?);
        }
        tight
    };
    prepare_shell_texture_pixels(&mut owned);
    let buffer = image::RgbaImage::from_raw(pixels.width, pixels.height, owned)?;
    Some(Arc::new(RenderImage::new(smallvec::smallvec![
        image::Frame::new(buffer)
    ])))
}

fn previewable_image(location: &explorer_model::LocationDescriptor) -> bool {
    location
        .path()
        .and_then(std::path::Path::extension)
        .and_then(std::ffi::OsStr::to_str)
        .is_some_and(|extension| {
            matches!(
                extension.to_ascii_lowercase().as_str(),
                "jpg" | "jpeg" | "png" | "bmp" | "gif" | "webp" | "tif" | "tiff"
            )
        })
}

fn namespace_thumbnail_supported(entry: &explorer_model::FileEntry) -> bool {
    explorer_model::namespace_command_enabled(
        &explorer_model::NamespaceAvailability::Available,
        entry.metadata.namespace_capabilities,
        explorer_model::NamespaceCommand::Thumbnail,
    )
}

/// GPUI CE's Windows image upload path consumes the first and third byte as B and R even though
/// `image::RgbaImage` names them R and B. Keep the Shell/cache boundary in portable straight RGBA
/// and adapt only at the renderer boundary; otherwise Explorer's yellow folder becomes cyan.
fn prepare_shell_texture_pixels(pixels: &mut [u8]) {
    for pixel in pixels.chunks_exact_mut(4) {
        pixel.swap(0, 2);
    }
}

impl Render for ExplorerRoot {
    #[allow(
        clippy::too_many_lines,
        reason = "the root registers the complete, auditable keyboard action scope in one place"
    )]
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let window_title = active_window_title(self.state.tabs());
        if self.last_window_title.as_deref() != Some(window_title.as_str()) {
            window.set_window_title(&window_title);
            self.last_window_title = Some(window_title);
        }
        if !window.is_window_active() && self.state.rename_editor().is_some() {
            match self
                .state
                .commit_inline_rename(explorer_model::RenameCommitTrigger::Blur)
            {
                Ok(Some(request)) => {
                    self.execute_file_operation(request);
                    self.rename_input = None;
                }
                Ok(None) => self.rename_input = None,
                Err(_) => {}
            }
        }
        if !window.is_window_active()
            && (matches!(
                self.state.tabs().active_tab().view.address.mode,
                explorer_model::AddressBarMode::EnumeratingMenu { .. }
            ) || self.state.tabs().active_tab().view.address.overflow_open)
        {
            self.state.close_address_menu();
        }
        if !window.is_window_active()
            && matches!(
                self.state.tabs().active_tab().view.address.mode,
                explorer_model::AddressBarMode::Editing
                    | explorer_model::AddressBarMode::NavigationError
            )
        {
            self.state.cancel_address_edit();
        }
        if !window.is_window_active() {
            self.state.close_navigation_history_menu();
            self.state.close_sort_menu();
            self.state.close_view_menu();
            self.state.close_more_menu();
            self.state.close_extensions_menu();
            self.state.close_new_menu();
            self.state.cancel_permanent_delete_confirmation();
        }
        if self.state.scrollbar_drag_session().is_some() && !window.is_window_active() {
            self.terminate_scrollbar_drag(
                interaction::ScrollbarTerminal::WindowBlur,
                ActionSource::Programmatic,
            );
        }
        if self.state.details_column_resize_active() && !window.is_window_active() {
            self.terminate_details_column_resize();
        }
        if self.state.marquee_session().is_some() && !window.is_window_active() {
            self.state.end_marquee();
            self.pointer_capture.take();
        }
        if (self.state.scrollbar_drag_session().is_some()
            || self.state.details_column_resize_active()
            || self.state.marquee_session().is_some())
            && self
                .pointer_capture
                .as_ref()
                .is_some_and(|capture| !capture.is_owned())
        {
            if self.state.marquee_session().is_some() {
                self.state.end_marquee();
                self.pointer_capture.take();
            } else if self.state.scrollbar_drag_session().is_some() {
                self.terminate_scrollbar_drag(
                    interaction::ScrollbarTerminal::CaptureLost,
                    ActionSource::Programmatic,
                );
            } else {
                self.terminate_details_column_resize();
            }
        }
        self.file_viewport_width =
            chrome::explorer_file_viewport_width(window, &self.state, self.tokens);
        let view_settings = self.state.view_settings();
        let rebuilds_before = self.state.presentation_rebuilds();
        let file_presentation = self.state.directory_presentation();
        if self.state.presentation_rebuilds() != rebuilds_before {
            self.file_performance.record_presentation_rebuild();
        }
        if let Some(presentation) = file_presentation.as_ref() {
            self.file_performance
                .record_directory_revision(presentation.revision());
        }
        let realized_entries = file_presentation
            .as_ref()
            .map(|presentation| {
                let metrics = chrome::spatial_grid_layout(
                    chrome::spatial_grid_metrics(&view_settings, self.tokens.layout),
                    self.file_viewport_width,
                    presentation.len(),
                )
                .metrics;
                let scroll_offset = (-f32::from(self.file_scroll.offset().y)).max(0.0);
                let measured_viewport_height = f32::from(self.file_scroll.bounds().size.height);
                let fallback_viewport_height =
                    chrome::explorer_file_viewport_height(window, self.tokens);
                let layout_ready = measured_viewport_height > metrics.cell_height
                    || fallback_viewport_height > metrics.cell_height;
                let viewport_height = if measured_viewport_height > 0.0 {
                    measured_viewport_height
                } else {
                    fallback_viewport_height
                }
                .max(metrics.cell_height);
                let range = if let Some(range) =
                    prelayout_icon_range(presentation.len(), layout_ready)
                {
                    range
                } else if metrics.wrapped {
                    file_view::fixed_grid_virtual_range(
                        presentation.len(),
                        metrics.cell_width,
                        metrics.cell_height,
                        self.file_viewport_width.max(metrics.cell_width),
                        viewport_height,
                        scroll_offset,
                        2,
                    )
                    .items
                } else {
                    let header_height = if view_settings.mode == explorer_model::ViewMode::Details {
                        self.tokens.layout.details_header_height.value()
                    } else {
                        0.0
                    };
                    file_view::fixed_virtual_range(
                        presentation.len(),
                        metrics.cell_height,
                        (viewport_height - header_height).max(metrics.cell_height),
                        (scroll_offset - header_height).max(0.0),
                        2,
                    )
                    .items
                };
                prime_top_icon_range(presentation.len(), scroll_offset, range)
                    .filter_map(|ordinal| {
                        presentation.entry(ordinal).map(|(_, entry)| entry.clone())
                    })
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        if !realized_entries.is_empty() {
            let tab = self.state.tabs().active_tab();
            let context = explorer_model::RequestContext::new(tab.id, tab.generation);
            self.submit_file_icon_loads(&context, &realized_entries);
        }
        let preview_entry = {
            let tab = self.state.tabs().active_tab();
            (tab.selection.len() == 1)
                .then(|| tab.visible_snapshot())
                .flatten()
                .and_then(|snapshot| {
                    snapshot
                        .entries()
                        .iter()
                        .find(|entry| tab.selection.contains(&entry.id))
                        .cloned()
                })
        };
        self.synchronize_preview_thumbnail(preview_entry.as_ref());
        self.synchronize_preview_handler(preview_entry.as_ref());
        self.submit_folder_size_requests();
        self.submit_code_lines_requests();
        self.submit_size_map_requests();
        let size_map_context = {
            let tab = self.state.tabs().active_tab();
            self.size_map_visual_context
                .as_ref()
                .filter(|context| context.tab_id == tab.id && context.generation == tab.generation)
                .cloned()
        };
        let shell_icons = self.navigation_icon_snapshot(&realized_entries);
        let safe_mode_offer = self.safe_mode_offers.first().cloned();
        let safe_mode_error = self.safe_mode_confirmation_error.clone();
        let content = chrome::ExplorerWindow::new(self.tokens, self.state.clone())
            .with_shell_icons(shell_icons, self.shell_icon_dpi)
            .with_file_presentation(file_presentation)
            .with_file_performance(Arc::clone(&self.file_performance))
            .with_navigation_scroll(self.navigation_scroll.clone())
            .with_file_scroll(self.file_scroll.clone())
            .with_text_inputs(
                self.address_input.as_ref().map(gpui::Entity::downgrade),
                self.search_input.as_ref().map(gpui::Entity::downgrade),
                self.rename_input.as_ref().map(gpui::Entity::downgrade),
            )
            .with_breadcrumb_menu_focus(self.breadcrumb_menu_focus.clone())
            .with_command_menu_focus(self.command_menu_focus.clone())
            .with_preview_thumbnail(self.preview_texture.clone(), self.preview_thumbnail_failed)
            .with_folder_size_visuals(self.folder_size_visuals.clone())
            .with_visual_column_runtime(self.visual_column_runtime.clone())
            .with_code_lines_visuals(self.code_lines_visuals.clone())
            .with_code_lines_runtime(self.code_lines_runtime.clone())
            .with_size_map(
                self.size_map_is_active(),
                size_map_context
                    .as_ref()
                    .and_then(|_| self.size_map_visuals.clone()),
                self.size_map_runtime.clone(),
                size_map_context,
            )
            .on_action(std::rc::Rc::new(cx.listener(
                |this, action: &ExplorerAction, window, cx| {
                    this.handle_action(action.clone(), ActionSource::Mouse, window, cx);
                },
            )));
        div()
            .id("explorer-action-scope")
            .size_full()
            .when_some(self.focus_handle.clone(), |element, focus_handle| {
                element.track_focus(&focus_handle)
            })
            .on_action(
                cx.listener(|this, _: &actions::NewExplorerTab, window, cx| {
                    this.handle_action(ExplorerAction::NewTab, ActionSource::Keyboard, window, cx);
                }),
            )
            .on_action(
                cx.listener(|this, _: &actions::CloseExplorerTab, window, cx| {
                    this.handle_action(
                        ExplorerAction::CloseActiveTab,
                        ActionSource::Keyboard,
                        window,
                        cx,
                    );
                }),
            )
            .on_action(
                cx.listener(|this, _: &actions::NextExplorerTab, window, cx| {
                    this.handle_action(ExplorerAction::NextTab, ActionSource::Keyboard, window, cx);
                }),
            )
            .on_action(
                cx.listener(|this, _: &actions::PreviousExplorerTab, window, cx| {
                    this.handle_action(
                        ExplorerAction::PreviousTab,
                        ActionSource::Keyboard,
                        window,
                        cx,
                    );
                }),
            )
            .on_key_down(cx.listener(|this, event: &gpui::KeyDownEvent, window, cx| {
                if let Some(presentation_token) = this
                    .safe_mode_offers
                    .first()
                    .map(|offer| offer.presentation_token)
                {
                    cx.stop_propagation();
                    if matches!(event.keystroke.key.as_str(), "enter" | "space") {
                        this.confirm_safe_mode_offer(presentation_token);
                    }
                    return;
                }
                let lock_modal_action = this.state.lock_recovery().and_then(|recovery| match event
                    .keystroke
                    .key
                    .as_str()
                {
                    "enter" | "space" => Some(match recovery.focused_target() {
                        state::LockRecoveryFocusTarget::CloseAndRetry => {
                            ExplorerAction::CloseLockOwnersAndRetry
                        }
                        state::LockRecoveryFocusTarget::Retry => ExplorerAction::RetryLockedDelete,
                        state::LockRecoveryFocusTarget::Cancel => {
                            ExplorerAction::CancelLockedDeleteRecovery
                        }
                    }),
                    "tab" => Some(ExplorerAction::MoveLockedDeleteDialogFocus {
                        direction: if event.keystroke.modifiers.shift {
                            -1
                        } else {
                            1
                        },
                    }),
                    "escape" => Some(ExplorerAction::CancelLockedDeleteRecovery),
                    _ => None,
                });
                if this.state.lock_recovery().is_some() && lock_modal_action.is_none() {
                    cx.stop_propagation();
                    return;
                }
                let modal_action = lock_modal_action.or_else(|| {
                    this.state
                        .permanent_delete_confirmation_focus()
                        .and_then(|focused| match event.keystroke.key.as_str() {
                            "enter" | "space" => Some(match focused {
                                actions::PermanentDeleteDialogTarget::Cancel => {
                                    ExplorerAction::CancelPermanentDelete
                                }
                                actions::PermanentDeleteDialogTarget::Delete => {
                                    ExplorerAction::ConfirmPermanentDelete
                                }
                            }),
                            "tab" => Some(ExplorerAction::MovePermanentDeleteDialogFocus {
                                direction: if event.keystroke.modifiers.shift {
                                    -1
                                } else {
                                    1
                                },
                            }),
                            "escape" => Some(ExplorerAction::CancelPermanentDelete),
                            _ => None,
                        })
                });
                if this.state.permanent_delete_confirmation_count().is_some()
                    && modal_action.is_none()
                {
                    cx.stop_propagation();
                    return;
                }
                if let Some(action) = modal_action.or_else(|| {
                    this.navigation_history_key_action(event)
                        .or_else(|| this.breadcrumb_key_action(event))
                        .or_else(|| this.command_extensions_key_action(event))
                        .or_else(|| this.details_column_menu_key_action(event))
                        .or_else(|| this.command_new_key_action(event))
                        .or_else(|| this.command_sort_key_action(event))
                        .or_else(|| this.command_view_key_action(event))
                        .or_else(|| this.command_more_key_action(event))
                        .or_else(|| this.navigation_tree_key_action(event))
                        .or_else(|| this.file_view_key_action(event, window))
                }) {
                    cx.stop_propagation();
                    this.handle_action(action, ActionSource::Keyboard, window, cx);
                } else if this.forward_preview_accelerator(event) {
                    cx.stop_propagation();
                }
            }))
            .on_action(cx.listener(|this, _: &actions::NavigateBack, window, cx| {
                this.handle_action(ExplorerAction::Back, ActionSource::Keyboard, window, cx);
            }))
            .on_action(
                cx.listener(|this, _: &actions::NavigateForward, window, cx| {
                    this.handle_action(ExplorerAction::Forward, ActionSource::Keyboard, window, cx);
                }),
            )
            .on_action(cx.listener(|this, _: &actions::NavigateUp, window, cx| {
                this.handle_action(ExplorerAction::Up, ActionSource::Keyboard, window, cx);
            }))
            .on_action(
                cx.listener(|this, _: &actions::RefreshExplorer, window, cx| {
                    this.handle_action(ExplorerAction::Refresh, ActionSource::Keyboard, window, cx);
                }),
            )
            .on_action(
                cx.listener(|this, _: &actions::RenameFocusedItem, window, cx| {
                    tracing::info!(
                        focused_surface = ?this.state.focused_surface(),
                        rename_editor_active = this.state.rename_editor().is_some(),
                        focused_row = ?this.state.focused_row_index(),
                        "F2 rename binding received"
                    );
                    if this.state.focused_surface() == focus::FocusSurface::FileView
                        && this.state.rename_editor().is_none()
                    {
                        this.handle_action(
                            ExplorerAction::BeginRenameFocused,
                            ActionSource::Keyboard,
                            window,
                            cx,
                        );
                    }
                }),
            )
            .on_action(
                cx.listener(|this, _: &actions::FocusAddressBar, window, cx| {
                    this.handle_action(
                        ExplorerAction::FocusAddress,
                        ActionSource::Keyboard,
                        window,
                        cx,
                    );
                }),
            )
            .on_action(
                cx.listener(|this, _: &actions::FocusSearchBox, window, cx| {
                    this.handle_action(
                        ExplorerAction::FocusSearch,
                        ActionSource::Keyboard,
                        window,
                        cx,
                    );
                }),
            )
            .on_action(
                cx.listener(|this, _: &actions::FocusNextSurface, window, cx| {
                    this.handle_action(
                        ExplorerAction::FocusNext,
                        ActionSource::Keyboard,
                        window,
                        cx,
                    );
                }),
            )
            .on_action(
                cx.listener(|this, _: &actions::FocusPreviousSurface, window, cx| {
                    this.handle_action(
                        ExplorerAction::FocusPrevious,
                        ActionSource::Keyboard,
                        window,
                        cx,
                    );
                }),
            )
            .on_action(
                cx.listener(|this, _: &actions::SubmitFocusedInput, window, cx| {
                    let action = if this.state.rename_editor().is_some() {
                        ExplorerAction::CommitInlineRename
                    } else {
                        ExplorerAction::SubmitFocusedInput
                    };
                    this.handle_action(action, ActionSource::Keyboard, window, cx);
                }),
            )
            .on_action(
                cx.listener(|this, _: &actions::CancelFocusedInput, window, cx| {
                    let action = if this.state.rename_editor().is_some() {
                        ExplorerAction::CancelInlineRename
                    } else {
                        ExplorerAction::CancelFocusedInput
                    };
                    this.handle_action(action, ActionSource::Keyboard, window, cx);
                }),
            )
            .on_action(
                cx.listener(|this, _: &actions::CancelScrollbarDrag, window, cx| {
                    if this.state.context_menu_pending() {
                        if let Some(command) = this.state.cancel_pending_context_menu() {
                            this.submit_command(command);
                        }
                        return;
                    }
                    let menu_action = if this.state.permanent_delete_confirmation_count().is_some()
                    {
                        Some(ExplorerAction::CancelPermanentDelete)
                    } else if this.state.lock_recovery().is_some() {
                        Some(ExplorerAction::CancelLockedDeleteRecovery)
                    } else if this.state.navigation_history_menu_direction().is_some() {
                        Some(ExplorerAction::CloseNavigationHistory)
                    } else if this.state.details_column_menu().is_some() {
                        Some(ExplorerAction::CloseDetailsColumnMenu)
                    } else if this.state.extensions_menu_open() {
                        Some(ExplorerAction::CloseExtensionsMenu)
                    } else if this.state.new_menu_open() {
                        Some(ExplorerAction::CloseNewMenu)
                    } else if this.state.sort_menu_open() {
                        Some(ExplorerAction::CloseSortMenu)
                    } else if this.state.view_menu_open() {
                        Some(ExplorerAction::CloseViewMenu)
                    } else if this.state.more_menu_open() {
                        Some(ExplorerAction::CloseMoreMenu)
                    } else {
                        None
                    };
                    if let Some(action) = menu_action {
                        this.handle_action(action, ActionSource::Keyboard, window, cx);
                    } else if matches!(
                        this.state.focused_surface(),
                        focus::FocusSurface::Search | focus::FocusSurface::AddressBar
                    ) || this.state.rename_editor().is_some()
                    {
                        let action = if this.state.rename_editor().is_some() {
                            ExplorerAction::CancelInlineRename
                        } else {
                            ExplorerAction::CancelFocusedInput
                        };
                        this.handle_action(action, ActionSource::Keyboard, window, cx);
                    } else if matches!(
                        this.state.tabs().active_tab().view.address.mode,
                        explorer_model::AddressBarMode::EnumeratingMenu { .. }
                    ) || this.state.tabs().active_tab().view.address.overflow_open
                    {
                        this.handle_action(
                            ExplorerAction::CloseBreadcrumbMenu,
                            ActionSource::Keyboard,
                            window,
                            cx,
                        );
                    } else if this.state.details_column_resize_active() {
                        this.handle_action(
                            ExplorerAction::EndDetailsColumnResize,
                            ActionSource::Keyboard,
                            window,
                            cx,
                        );
                    } else {
                        this.handle_action(
                            ExplorerAction::EndScrollbarDrag {
                                reason: interaction::ScrollbarTerminal::Escape,
                            },
                            ActionSource::Keyboard,
                            window,
                            cx,
                        );
                    }
                }),
            )
            .on_action(
                cx.listener(|this, _: &actions::ToggleExplorerTheme, window, cx| {
                    this.handle_action(
                        ExplorerAction::ToggleTheme,
                        ActionSource::Keyboard,
                        window,
                        cx,
                    );
                }),
            )
            .on_action(
                cx.listener(|this, _: &actions::ToggleExplorerPreview, window, cx| {
                    this.handle_action(
                        ExplorerAction::TogglePreviewPane,
                        ActionSource::Keyboard,
                        window,
                        cx,
                    );
                }),
            )
            .on_action(
                cx.listener(|this, _: &actions::CloseExplorerWindow, window, cx| {
                    this.handle_action(
                        ExplorerAction::CloseWindow,
                        ActionSource::Keyboard,
                        window,
                        cx,
                    );
                }),
            )
            .on_action(
                cx.listener(|this, _: &actions::ShrinkNavigationPane, window, cx| {
                    this.handle_action(
                        ExplorerAction::AdjustNavigationPaneWidth { direction: -1 },
                        ActionSource::Keyboard,
                        window,
                        cx,
                    );
                }),
            )
            .on_action(
                cx.listener(|this, _: &actions::GrowNavigationPane, window, cx| {
                    this.handle_action(
                        ExplorerAction::AdjustNavigationPaneWidth { direction: 1 },
                        ActionSource::Keyboard,
                        window,
                        cx,
                    );
                }),
            )
            .on_action(
                cx.listener(|this, _: &actions::ShrinkSidePane, window, cx| {
                    this.handle_action(
                        ExplorerAction::AdjustSidePaneWidth { direction: -1 },
                        ActionSource::Keyboard,
                        window,
                        cx,
                    );
                }),
            )
            .on_action(cx.listener(|this, _: &actions::GrowSidePane, window, cx| {
                this.handle_action(
                    ExplorerAction::AdjustSidePaneWidth { direction: 1 },
                    ActionSource::Keyboard,
                    window,
                    cx,
                );
            }))
            .on_action(
                cx.listener(|this, _: &actions::ResetNavigationPane, window, cx| {
                    this.handle_action(
                        ExplorerAction::ResetNavigationPaneWidth,
                        ActionSource::Keyboard,
                        window,
                        cx,
                    );
                }),
            )
            .child(content)
            .when_some(safe_mode_offer, |element, offer| {
                let package = offer.package_id.as_deref().unwrap_or("unknown package");
                let interface = offer
                    .primary_interface_namespace
                    .zip(offer.primary_interface_value)
                    .map_or_else(
                        || "unavailable".to_owned(),
                        |(namespace, value)| format!("{namespace:08x}:{value}"),
                    );
                element.child(
                    div()
                        .id("safe-mode-offer-overlay")
                        .absolute()
                        .top_0()
                        .right_0()
                        .bottom_0()
                        .left_0()
                        .occlude()
                        .flex()
                        .items_center()
                        .justify_center()
                        .bg(theme::MODAL_BACKDROP.to_gpui())
                        .child(
                            div()
                                .id("safe-mode-offer-dialog")
                                .role(Role::Dialog)
                                .aria_label(format!(
                                    "Safe Mode confirmation required; Suspect package: {package}"
                                ))
                                .w(px(480.0))
                                .p(px(20.0))
                                .rounded(px(8.0))
                                .bg(self.tokens.theme.colors.surface.to_gpui())
                                .flex()
                                .flex_col()
                                .gap(px(12.0))
                                .child(
                                    div()
                                        .text_size(px(20.0))
                                        .child("Safe Mode requires confirmation"),
                                )
                                .child(
                                    div()
                                        .id("safe-mode-suspect-package")
                                        .role(Role::Label)
                                        .aria_label(format!("Suspect package: {package}"))
                                        .child(format!("Suspect package: {package}")),
                                )
                                .child(div().child(format!("Interface: {interface}")))
                                .child(div().child(format!("Operation: {}", offer.operation)))
                                .when_some(safe_mode_error, |dialog, error| {
                                    dialog.child(
                                        div()
                                            .id("safe-mode-confirmation-error")
                                            .text_color(self.tokens.theme.colors.danger.to_gpui())
                                            .child(error),
                                    )
                                })
                                .child(
                                    div()
                                        .id("safe-mode-confirm")
                                        .role(Role::Button)
                                        .aria_label("Confirm and re-enable")
                                        .p(px(8.0))
                                        .rounded(px(4.0))
                                        .bg(self.tokens.theme.colors.accent.to_gpui())
                                        .text_color(
                                            self.tokens.theme.colors.selected_text.to_gpui(),
                                        )
                                        .child("Confirm and re-enable")
                                        .on_click(cx.listener(move |this, _, _, _| {
                                            this.confirm_safe_mode_offer(offer.presentation_token);
                                        })),
                                ),
                        ),
                )
            })
    }
}

/// Creates the baseline resizable Explorer window configuration.
pub fn initial_window_options(cx: &App) -> WindowOptions {
    window_options_with_size(cx, INITIAL_WINDOW_WIDTH, INITIAL_WINDOW_HEIGHT)
}

/// Creates deterministic window options for visual fixtures and tests.
pub fn window_options_with_size(cx: &App, width: f32, height: f32) -> WindowOptions {
    debug_assert!(width >= MINIMUM_WINDOW_WIDTH && width.is_finite());
    debug_assert!(height >= MINIMUM_WINDOW_HEIGHT && height.is_finite());
    WindowOptions {
        window_bounds: Some(WindowBounds::Windowed(Bounds::centered(
            None,
            size(px(width), px(height)),
            cx,
        ))),
        titlebar: Some(gpui::TitlebarOptions {
            title: Some(SharedString::from(PRODUCT_NAME)),
            appears_transparent: true,
            ..Default::default()
        }),
        is_resizable: true,
        window_min_size: Some(size(px(MINIMUM_WINDOW_WIDTH), px(MINIMUM_WINDOW_HEIGHT))),
        ..Default::default()
    }
}

/// Creates window options from monitor-fitted persisted normal bounds and maximized state.
#[allow(
    clippy::cast_precision_loss,
    reason = "validated window coordinates are converted to GPUI's logical f32 pixel representation"
)]
pub fn window_options_with_placement(
    placement: explorer_model::PersistedWindowPlacement,
) -> WindowOptions {
    let bounds = Bounds {
        origin: gpui::point(
            px(placement.normal_bounds.left as f32),
            px(placement.normal_bounds.top as f32),
        ),
        size: size(
            px(placement.normal_bounds.width as f32),
            px(placement.normal_bounds.height as f32),
        ),
    };
    WindowOptions {
        window_bounds: Some(if placement.maximized {
            WindowBounds::Maximized(bounds)
        } else {
            WindowBounds::Windowed(bounds)
        }),
        titlebar: Some(gpui::TitlebarOptions {
            title: Some(SharedString::from(PRODUCT_NAME)),
            appears_transparent: true,
            ..Default::default()
        }),
        is_resizable: true,
        window_min_size: Some(size(px(MINIMUM_WINDOW_WIDTH), px(MINIMUM_WINDOW_HEIGHT))),
        ..Default::default()
    }
}

#[cfg(test)]
mod tests {
    use std::{
        sync::{Arc, Mutex},
        time::{Duration, Instant},
    };

    use super::{
        BaseIconCache, ENRICHMENT_SERVICE_EVENT_CAPACITY, ExplorerRoot, ExtensionUiPumpPortV1,
        SafeModeOfferV1, UiTokens, VisibleItemIconCache, VisualFixtureState,
        action_for_host_context_command, active_window_title, advance_item_overlay_epoch,
        captured_scrollbar_axis_to_logical, coalesce_directory_events, extension_ui_pump_due,
        file_view_global_command_action, file_view_item_command_action,
        file_view_navigation_target, folder_size_result_is_current, is_passive_pointer_action,
        physical_client_to_logical, prepare_shell_texture_pixels, should_end_address_edit,
        should_end_inline_rename, synchronize_theme, thumbnail_texture,
        window_title_for_history_entry,
    };

    struct SequenceExtensionPumpV1 {
        due: std::collections::VecDeque<bool>,
    }

    impl ExtensionUiPumpPortV1 for SequenceExtensionPumpV1 {
        fn poll_due(&mut self, _: Instant) -> bool {
            self.due.pop_front().unwrap_or(false)
        }
    }

    #[test]
    fn size_map_view_or_tab_switch_clears_requests_and_rejects_the_previous_session() {
        use crate::actions::ExplorerAction;

        let mut root = ExplorerRoot::new(UiTokens::default());
        let (tab_id, generation) = {
            let tab = root.state.tabs().active_tab();
            (tab.id, tab.generation)
        };
        let prior = explorer_model::RequestContext::new(tab_id, generation);
        let resumed = explorer_model::RequestContext::new(tab_id, generation);
        let item_id =
            explorer_model::ShellItemId::from_provider_bytes([0x51]).expect("fixture identity");
        root.size_map_visual_context = Some(prior.clone());
        root.size_map_visuals = Some(super::size_map_view::SizeMapVisualsV1 {
            values: std::collections::HashMap::from([(
                item_id.clone(),
                super::size_map_view::SizeMapMeasuredValueV1 {
                    exact_bytes: Some(1),
                    partial: false,
                    error: None,
                },
            )]),
        });
        root.size_map_requested
            .insert((tab_id, generation, item_id.clone()));

        root.invalidate_size_map_after_action(&ExplorerAction::SetViewMode(
            explorer_model::ViewMode::Details,
        ));
        assert!(root.size_map_visual_context.is_none());
        assert!(root.size_map_requested.is_empty());
        assert!(
            root.size_map_visuals
                .as_ref()
                .expect("Size Map visuals")
                .values
                .is_empty()
        );

        root.size_map_visual_context = Some(prior.clone());
        root.size_map_requested
            .insert((tab_id, generation, item_id.clone()));
        root.invalidate_size_map_after_action(&ExplorerAction::ActivateTab { tab_id });
        assert!(root.size_map_visual_context.is_none());
        assert!(root.size_map_requested.is_empty());

        let result = super::size_map_view::SizeMapMeasureResultV1 {
            context: prior,
            item_id,
            exact_bytes: Some(1),
            partial: false,
            error: None,
        };
        assert_ne!(result.context, resumed);
        assert!(!ExplorerRoot::size_map_result_is_current(&result, &resumed));
    }

    #[test]
    fn code_lines_context_is_cleared_before_the_next_render_snapshot() {
        let mut root = ExplorerRoot::new(UiTokens::default());
        let tab = root.state.tabs().active_tab();
        let previous = explorer_model::RequestContext::new(tab.id, tab.generation);
        let current = explorer_model::RequestContext::new(
            tab.id,
            explorer_model::Generation::new(tab.generation.value().saturating_add(1)),
        );
        let item =
            explorer_model::ShellItemId::from_provider_bytes([0x63]).expect("fixture identity");
        root.code_lines_visuals = Some(super::code_lines_column::CodeLinesColumnVisuals {
            config: super::code_lines_column::CodeLinesColumnConfigV1::default(),
            context: Some(previous.clone()),
            values: std::collections::HashMap::from([(
                item.clone(),
                super::code_lines_column::CodeLinesValueV1 {
                    language: "Rust".to_owned(),
                    code: 12,
                    comments: 1,
                    blanks: 1,
                    total: 14,
                },
            )]),
            errors: std::collections::HashMap::from([(item.clone(), "old error".to_owned())]),
        });
        root.code_lines_requested
            .insert((previous.tab_id, previous.generation, item));

        assert!(root.begin_code_lines_context(current.clone()));
        let visuals = root
            .code_lines_visuals
            .as_ref()
            .expect("Code lines visuals");
        assert_eq!(visuals.context.as_ref(), Some(&current));
        assert!(visuals.values.is_empty());
        assert!(visuals.errors.is_empty());
        assert!(root.code_lines_requested.is_empty());
    }

    #[test]
    fn extension_pump_requests_one_notification_only_on_its_due_tick() {
        let mut pump: Box<dyn ExtensionUiPumpPortV1> = Box::new(SequenceExtensionPumpV1 {
            due: std::collections::VecDeque::from([false, false, true, false]),
        });
        let now = Instant::now();
        let decisions = (0..4)
            .map(|_| extension_ui_pump_due(Some(&mut pump), now))
            .collect::<Vec<_>>();
        assert_eq!(decisions, vec![false, false, true, false]);
    }

    #[test]
    fn folder_size_result_rejects_a_superseded_ui_generation() {
        let tab = explorer_model::TabId::new();
        let current = explorer_model::RequestContext::new(tab, explorer_model::Generation::new(2));
        let item_id = explorer_model::ShellItemId::from_provider_bytes(1_u64.to_le_bytes())
            .expect("stable item ID");
        let result = super::folder_size_column::FolderSizeResultV1 {
            context: explorer_model::RequestContext::new(
                current.tab_id,
                explorer_model::Generation::new(1),
            ),
            item_id,
            exact_bytes: Some(42),
            partial: false,
            error: None,
        };
        assert!(!folder_size_result_is_current(&result, &current));
        let current_result = super::folder_size_column::FolderSizeResultV1 {
            context: current.clone(),
            ..result.clone()
        };
        assert!(folder_size_result_is_current(&current_result, &current));

        let mut visuals = super::folder_size_column::FolderSizeColumnVisuals {
            config: super::folder_size_column::VisualColumnConfigV1::default(),
            context: Some(result.context.clone()),
            values: std::collections::HashMap::from([(
                result.item_id,
                super::folder_size_column::FolderSizeValueV1 {
                    exact_bytes: Some(42),
                    partial: false,
                    error: None,
                },
            )]),
        };
        assert!(visuals.begin_context(&current));
        assert!(visuals.values.is_empty());
        assert!(!visuals.begin_context(&current));
    }

    #[test]
    fn host_context_command_routes_to_existing_explorer_actions() {
        use explorer_model::ContextMenuHostCommand as Command;
        assert_eq!(
            action_for_host_context_command(Command::Open),
            ExplorerAction::OpenFocused
        );
        assert_eq!(
            action_for_host_context_command(Command::Cut),
            ExplorerAction::CutSelected
        );
        assert_eq!(
            action_for_host_context_command(Command::Copy),
            ExplorerAction::CopySelected
        );
        assert_eq!(
            action_for_host_context_command(Command::CopyPath),
            ExplorerAction::CopySelectedPaths
        );
        assert_eq!(
            action_for_host_context_command(Command::CreateShortcut),
            ExplorerAction::CreateShortcutSelected
        );
        assert_eq!(
            action_for_host_context_command(Command::Delete),
            ExplorerAction::RecycleDeleteSelected
        );
        assert_eq!(
            action_for_host_context_command(Command::Rename),
            ExplorerAction::BeginRenameFocused
        );
        assert_eq!(
            action_for_host_context_command(Command::Share),
            ExplorerAction::ShareSelected
        );
        assert_eq!(
            action_for_host_context_command(Command::PinToStart),
            ExplorerAction::PinSelectedToStart
        );
        assert_eq!(
            action_for_host_context_command(Command::ToggleQuickAccess),
            ExplorerAction::AddSelectedToFavorites
        );
        assert_eq!(
            action_for_host_context_command(Command::Properties),
            ExplorerAction::ShowPropertiesSelected
        );
    }

    #[test]
    fn qos_presentation_boundary_accepts_current_generation_and_rejects_superseded_results() {
        let mut root = ExplorerRoot::new(UiTokens::default());
        let command = root
            .state
            .begin_active_location_load()
            .expect("directory load context");
        let current = command.context().expect("directory context").clone();
        let stale = explorer_model::RequestContext::new(
            current.tab_id,
            explorer_model::Generation::new(current.generation.value().saturating_add(1)),
        );
        let mut wrong_request = current.clone();
        wrong_request.request_id = explorer_common::RequestId::new();
        let current_event = explorer_model::ExplorerEvent::DirectoryFinished {
            context: current.clone(),
        };
        let stale_event = explorer_model::ExplorerEvent::DirectoryFinished { context: stale };
        let wrong_request_event = explorer_model::ExplorerEvent::DirectoryFinished {
            context: wrong_request,
        };

        assert!(root.accepts_presentation_event(&current_event));
        assert!(!root.accepts_presentation_event(&stale_event));
        assert!(!root.accepts_presentation_event(&wrong_request_event));
    }

    #[test]
    fn qos_presentation_boundary_rejects_unknown_operation_before_global_side_effects() {
        let mut root = ExplorerRoot::new(UiTokens::default());
        let tab = root.state().tabs().active_tab();
        let unknown = explorer_model::RequestContext::new(tab.id, tab.generation);
        let unknown_terminal = explorer_model::ExplorerEvent::OperationFinished {
            context: unknown,
            outcome: explorer_model::OperationTerminal::Finished,
        };
        assert!(!root.accepts_presentation_event(&unknown_terminal));

        let command = root
            .state
            .begin_file_operation(explorer_model::FileOperationRequest {
                kind: explorer_model::FileOperationKind::Copy {
                    items: Vec::new(),
                    destination: explorer_model::LocationDescriptor::file_system(r"C:\"),
                },
                flags: explorer_model::FileOperationFlags::default(),
            });
        let context = command.context().expect("operation context").clone();
        let correlated_terminal = explorer_model::ExplorerEvent::OperationFinished {
            context,
            outcome: explorer_model::OperationTerminal::Finished,
        };
        assert!(root.accepts_presentation_event(&correlated_terminal));
    }

    #[test]
    fn qos_enrichment_backlog_is_bounded_and_never_delays_foreground_delivery() {
        let mut root = ExplorerRoot::new(UiTokens::default());
        let tab = root.state().tabs().active_tab();
        let context = explorer_model::RequestContext::new(tab.id, tab.generation);
        let key = explorer_model::ShellIconKey {
            item_id: None,
            location: explorer_model::LocationDescriptor::file_system(r"C:\"),
            size_bucket: 16,
            dpi: 96,
            theme: explorer_model::ShellIconTheme::Light,
            association_generation: 0,
            overlay_generation: 0,
        };
        for _ in 0..=ENRICHMENT_SERVICE_EVENT_CAPACITY {
            root.enqueue_service_event(explorer_model::ExplorerEvent::ShellIconFailed {
                context: context.clone(),
                key: key.clone(),
                reason: explorer_model::ShellIconFallbackReason::ShellUnavailable,
            });
        }
        assert_eq!(
            root.pending_enrichment_events.len(),
            ENRICHMENT_SERVICE_EVENT_CAPACITY
        );
        assert_eq!(
            root.service_qos_snapshot_for_test().observations.overloads,
            1
        );
        assert!(root.enrichment_retry_needed);
        root.recover_discarded_enrichment();
        assert!(!root.enrichment_retry_needed);

        root.enqueue_service_event(explorer_model::ExplorerEvent::ClipboardChanged {
            state: explorer_model::ClipboardState::default(),
        });
        assert!(matches!(
            root.pop_next_service_event(),
            Some(explorer_model::ExplorerEvent::ClipboardChanged { .. })
        ));
    }

    #[test]
    fn qos_same_icon_key_rejects_a_superseded_request_context() {
        let mut root = ExplorerRoot::new(UiTokens::default());
        let tab = root.state().tabs().active_tab();
        let expected = explorer_model::RequestContext::new(tab.id, tab.generation);
        let superseded = explorer_model::RequestContext::new(tab.id, tab.generation);
        let key = explorer_model::ShellIconKey {
            item_id: None,
            location: explorer_model::LocationDescriptor::file_system(r"C:\"),
            size_bucket: 16,
            dpi: 96,
            theme: explorer_model::ShellIconTheme::Light,
            association_generation: 0,
            overlay_generation: 0,
        };
        root.pending_icon_keys.insert(key.clone());
        root.pending_icon_contexts
            .insert(key.clone(), expected.clone());

        let stale = explorer_model::ExplorerEvent::ShellIconFailed {
            context: superseded,
            key: key.clone(),
            reason: explorer_model::ShellIconFallbackReason::ShellUnavailable,
        };
        let current = explorer_model::ExplorerEvent::ShellIconFailed {
            context: expected,
            key,
            reason: explorer_model::ShellIconFallbackReason::ShellUnavailable,
        };
        assert!(!root.accepts_presentation_event(&stale));
        assert!(root.accepts_presentation_event(&current));
    }

    #[test]
    fn prelayout_icon_range_primes_one_bounded_first_viewport() {
        assert_eq!(
            super::prelayout_icon_range(100_000, false),
            Some(0..super::FILE_VIEWPORT_ICON_REQUEST_CAP)
        );
        assert_eq!(super::prelayout_icon_range(32, false), Some(0..32));
        assert_eq!(super::prelayout_icon_range(100_000, true), None);
        assert_eq!(
            super::prime_top_icon_range(100_000, 0.0, 0..5),
            0..super::FILE_VIEWPORT_ICON_REQUEST_CAP
        );
        assert_eq!(super::prime_top_icon_range(100_000, 400.0, 8..24), 8..24);
    }

    #[test]
    fn file_extensions_use_real_visible_items_instead_of_blocking_fake_base_paths() {
        assert!(!super::uses_shared_base_icon(
            &explorer_model::BaseIconClass::Extension("mp4".to_owned())
        ));
        assert!(!super::uses_shared_base_icon(
            &explorer_model::BaseIconClass::Extension("jpg".to_owned())
        ));
        assert!(!super::uses_shared_base_icon(
            &explorer_model::BaseIconClass::ExtensionlessFile
        ));
        assert_eq!(
            super::base_icon_request_location(&explorer_model::BaseIconClass::ExtensionlessFile),
            None
        );
        assert!(super::uses_shared_base_icon(
            &explorer_model::BaseIconClass::Folder
        ));
    }

    #[test]
    fn qos_visible_icons_and_thumbnails_remain_admitted_when_visual_refinement_is_shed() {
        let entry = explorer_model::FileEntry {
            id: explorer_model::ShellItemId::from_provider_bytes([91]).expect("identity"),
            location: explorer_model::LocationDescriptor::file_system(
                r"E:\av_out\visible-preview.jpg",
            ),
            display_name: "visible-preview.jpg".to_owned(),
            is_container: false,
            metadata: explorer_model::FileEntryMetadata {
                namespace_capabilities: explorer_model::NamespaceCapabilities::from_public_bits(
                    explorer_model::NamespaceCapabilities::THUMBNAIL,
                ),
                ..explorer_model::FileEntryMetadata::default()
            },
        };
        let mut root = ExplorerRoot::for_directory_fixture(
            UiTokens::default(),
            vec![entry.clone()],
            explorer_model::ViewMode::LargeIcons,
        );
        let service = Arc::new(RecordingService::default());
        root.service = Some(service.clone());
        for _ in 0..8 {
            let _ = root
                .service_qos
                .observe_pressure(explorer_jobs::PressureSample::new(1, 1, true));
        }
        assert!(
            root.service_qos
                .should_shed(explorer_jobs::QosWorkClass::VisualRefinement)
        );
        let tab = root.state.tabs().active_tab();
        let context = explorer_model::RequestContext::new(tab.id, tab.generation);

        root.submit_offscreen_file_icon_loads(&context, std::slice::from_ref(&entry));
        assert!(service.0.lock().unwrap().is_empty());

        // An expanded navigation tree may already own the global item-less icon keys. Those
        // requests must not consume the visible file viewport's independent capacity.
        for index in 0..super::FILE_VIEWPORT_ICON_REQUEST_CAP {
            root.pending_icon_keys.insert(explorer_model::ShellIconKey {
                item_id: None,
                location: explorer_model::LocationDescriptor::file_system(format!(
                    r"E:\navigation-child-{index}"
                )),
                size_bucket: 16,
                dpi: 96,
                theme: explorer_model::ShellIconTheme::Light,
                association_generation: 0,
                overlay_generation: 0,
            });
        }

        root.submit_file_icon_loads(&context, std::slice::from_ref(&entry));
        let commands = service.0.lock().unwrap();
        assert!(commands.iter().any(|command| matches!(
            command,
            explorer_model::ExplorerCommand::LoadShellIcon { key, .. }
                if key.item_id.as_ref() == Some(&entry.id)
        )));
        assert!(commands.iter().any(|command| matches!(
            command,
            explorer_model::ExplorerCommand::LoadThumbnail { .. }
        )));
    }

    #[test]
    fn qos_visible_icon_overload_retries_without_another_navigation() {
        let entry = explorer_model::FileEntry {
            id: explorer_model::ShellItemId::from_provider_bytes([92]).expect("identity"),
            location: explorer_model::LocationDescriptor::file_system(
                r"E:\av_out\retry-visible.exe",
            ),
            display_name: "retry-visible.exe".to_owned(),
            is_container: false,
            metadata: explorer_model::FileEntryMetadata::default(),
        };
        let mut root = ExplorerRoot::for_directory_fixture(
            UiTokens::default(),
            vec![entry.clone()],
            explorer_model::ViewMode::Details,
        );
        root.service = Some(Arc::new(OverloadedService));
        let tab = root.state.tabs().active_tab();
        let context = explorer_model::RequestContext::new(tab.id, tab.generation);

        root.submit_file_icon_loads(&context, std::slice::from_ref(&entry));
        assert!(root.enrichment_retry_needed);

        let recovered = Arc::new(RecordingService::default());
        root.service = Some(recovered.clone());
        root.recover_discarded_enrichment();
        assert!(!root.enrichment_retry_needed);
        assert!(recovered.0.lock().unwrap().iter().any(|command| matches!(
            command,
            explorer_model::ExplorerCommand::LoadShellIcon { .. }
        )));
    }

    #[test]
    fn qos_command_overload_finishes_a_tracked_operation_instead_of_wedging_it() {
        let mut root = ExplorerRoot {
            service: Some(Arc::new(OverloadedService)),
            ..ExplorerRoot::default()
        };
        let command = root
            .state
            .begin_file_operation(explorer_model::FileOperationRequest {
                kind: explorer_model::FileOperationKind::Copy {
                    items: Vec::new(),
                    destination: explorer_model::LocationDescriptor::file_system(r"C:\"),
                },
                flags: explorer_model::FileOperationFlags::default(),
            });
        let request_id = command.context().expect("operation context").request_id;

        assert!(!root.submit_command(command));
        let record = root
            .state
            .operation_center()
            .get(request_id)
            .expect("tracked operation remains observable");
        assert_eq!(record.phase, explorer_model::OperationPhase::Failed);
        assert!(!root.navigation_started.contains_key(&request_id));
        assert_eq!(
            root.service_qos_snapshot_for_test().observations.overloads,
            1
        );
    }

    #[test]
    fn qos_command_overload_completes_context_menu_and_preview_lifecycles() {
        let mut root = ExplorerRoot {
            service: Some(Arc::new(OverloadedService)),
            ..ExplorerRoot::default()
        };
        let menu = root
            .state
            .begin_context_menu_request(None, 1, 0, 0, true, false)
            .expect("background context menu request");
        assert!(root.state.context_menu_pending());
        assert!(!root.submit_command(menu));
        assert!(!root.state.context_menu_pending());
        assert!(root.state.context_menu_error().is_some());

        let selection = explorer_model::PreviewSelection {
            item_id: explorer_model::ShellItemId::from_provider_bytes([1])
                .expect("preview identity"),
            location: explorer_model::LocationDescriptor::file_system(r"C:\preview.txt"),
            display_name: "preview.txt".to_owned(),
        };
        root.preview_coordinator.open().expect("open preview");
        root.preview_coordinator
            .select(
                &explorer_model::PreviewEligibility::SingleEligible(selection.clone()),
                Duration::ZERO,
            )
            .expect("schedule preview");
        let explorer_jobs::PreviewCoordinatorAction::Start { generation, .. } = root
            .preview_coordinator
            .poll(Duration::from_secs(1))
            .expect("poll preview")
            .expect("start preview")
        else {
            panic!("preview debounce must start the selected generation");
        };
        let tab = root.state.tabs().active_tab();
        let preview = explorer_model::ExplorerCommand::PreviewHost {
            context: explorer_model::RequestContext::new(tab.id, tab.generation),
            command: explorer_model::PreviewHostCommand::Start {
                selection,
                parent_window: 1,
                bounds: explorer_model::PreviewHostBounds {
                    generation,
                    left_physical: 0,
                    top_physical: 0,
                    width_physical: 100,
                    height_physical: 100,
                    dpi: 96,
                },
            },
        };
        assert!(!root.submit_command(preview));
        assert!(root.preview_thumbnail_failed);
        assert!(matches!(
            root.preview_coordinator.lifecycle(),
            explorer_model::PreviewLifecycle::Failed { generation: current, .. } if *current == generation
        ));
    }

    #[test]
    fn resolved_history_entries_project_native_window_titles() {
        for (location, display_title, expected) in [
            (
                explorer_model::LocationDescriptor::file_system(r"C:\"),
                "This PC",
                r"C:\",
            ),
            (
                explorer_model::LocationDescriptor::file_system(r"D:\test\資料夾"),
                "ignored",
                r"D:\test\資料夾",
            ),
            (
                explorer_model::LocationDescriptor::file_system(r"\\server\share\資料"),
                "ignored",
                r"\\server\share\資料",
            ),
            (
                explorer_model::LocationDescriptor::ParsingName(
                    "shell:MyComputerFolder".to_owned(),
                ),
                "本機",
                "本機",
            ),
        ] {
            let entry = explorer_model::HistoryEntry::new(location, display_title);
            assert_eq!(window_title_for_history_entry(Some(&entry)), expected);
        }
    }

    #[test]
    fn virtual_window_title_never_exposes_empty_or_internal_identity() {
        let empty = explorer_model::HistoryEntry::new(
            explorer_model::LocationDescriptor::ShellNamespace(vec![1]),
            "   ",
        );
        let internal = explorer_model::HistoryEntry::new(
            explorer_model::LocationDescriptor::ParsingName("super-explorer:home".to_owned()),
            "super-explorer:home",
        );
        assert_eq!(
            window_title_for_history_entry(Some(&empty)),
            "SuperExplorer"
        );
        assert_eq!(
            window_title_for_history_entry(Some(&internal)),
            "SuperExplorer"
        );
        assert_eq!(window_title_for_history_entry(None), "SuperExplorer");
    }

    #[test]
    fn active_window_title_ignores_address_draft_and_background_tab() {
        let fallback = explorer_model::HistoryEntry::new(
            explorer_model::LocationDescriptor::file_system(r"C:\fallback"),
            "fallback",
        );
        let mut first = explorer_model::TabState::new(explorer_model::HistoryEntry::new(
            explorer_model::LocationDescriptor::file_system(r"C:\background"),
            "background",
        ));
        first.view.address.enter_editing();
        first
            .view
            .address
            .update_draft(r"E:\not-resolved".to_owned());
        let second = explorer_model::TabState::new(explorer_model::HistoryEntry::new(
            explorer_model::LocationDescriptor::file_system(r"D:\active"),
            "active",
        ));
        let active = second.id;
        let mut tabs = explorer_model::ExplorerWindowState::from_restored_tabs(
            vec![first, second],
            active,
            fallback,
        )
        .expect("valid two-tab window");
        assert_eq!(active_window_title(&tabs), r"D:\active");

        tabs.tab_mut(tabs.tabs()[0].id)
            .expect("background tab")
            .view
            .address
            .navigation_failed("background failure".to_owned());
        assert_eq!(active_window_title(&tabs), r"D:\active");

        assert_eq!(tabs.close(active), explorer_model::TabCloseOutcome::Closed);
        assert_eq!(active_window_title(&tabs), r"C:\background");
    }

    #[test]
    fn correlated_directory_batches_are_coalesced_per_ui_transaction() {
        let context = explorer_model::RequestContext::new(
            explorer_model::TabId::new(),
            explorer_model::Generation::default(),
        );
        let make_entry = |id| explorer_model::FileEntry {
            id: explorer_model::ShellItemId::from_provider_bytes([id]).expect("identity"),
            display_name: format!("{id}.txt"),
            location: explorer_model::LocationDescriptor::file_system(format!(
                r"C:\fixture\{id}.txt"
            )),
            is_container: false,
            metadata: explorer_model::FileEntryMetadata::default(),
        };
        let events = coalesce_directory_events(vec![
            explorer_model::ExplorerEvent::DirectoryBatch {
                context: context.clone(),
                entries: vec![make_entry(1)],
            },
            explorer_model::ExplorerEvent::DirectoryBatch {
                context: context.clone(),
                entries: vec![make_entry(2)],
            },
        ]);
        assert_eq!(events.len(), 1);
        assert!(matches!(
            &events[0],
            explorer_model::ExplorerEvent::DirectoryBatch { entries, .. } if entries.len() == 2
        ));
    }

    #[test]
    fn item_overlay_epoch_invalidation_is_scoped_by_stable_identity() {
        let first = explorer_model::ShellItemId::from_provider_bytes([1]).expect("first");
        let second = explorer_model::ShellItemId::from_provider_bytes([2]).expect("second");
        let mut epochs = std::collections::HashMap::new();
        assert_eq!(advance_item_overlay_epoch(&mut epochs, &first), 1);
        assert_eq!(epochs.get(&first), Some(&1));
        assert_eq!(epochs.get(&second), None);
    }
    use crate::{
        actions::{ActionOutcome, ActionSource, ExplorerAction, dispatch_action},
        focus::FocusSurface,
        layout::LayoutTokens,
        state::AppViewState,
        theme::{SemanticColorSlot, ThemeMode, ThemeTokens},
    };

    #[test]
    fn details_column_capture_coordinates_preserve_one_to_one_logical_drag_at_every_dpi() {
        for scale_factor in [1.0_f32, 1.25, 1.5, 1.75, 2.0] {
            let origin = physical_client_to_logical(200.0 * scale_factor, scale_factor).unwrap();
            let current = physical_client_to_logical(240.0 * scale_factor, scale_factor).unwrap();
            assert!(
                (current - origin - 40.0).abs() <= f32::EPSILON,
                "scale factor {scale_factor}"
            );

            let mut state = AppViewState::default();
            state.begin_details_column_resize(explorer_model::ColumnId::Name, origin);
            state.update_details_column_resize(current);
            assert_eq!(
                state
                    .view_settings()
                    .details_column_width(&explorer_model::ColumnId::Name),
                320,
                "scale factor {scale_factor} must not multiply the logical delta"
            );
        }
        assert_eq!(physical_client_to_logical(-150.0, 1.5), Some(-100.0));
        assert_eq!(physical_client_to_logical(20.0, 0.0), None);
        assert_eq!(physical_client_to_logical(20.0, f32::NAN), None);
        assert_eq!(physical_client_to_logical(f32::NAN, 2.0), None);
    }

    #[test]
    fn scrollbar_capture_coordinates_preserve_one_to_one_drag_at_every_dpi() {
        use crate::interaction::ScrollbarKind;

        for scale_factor in [1.0_f32, 1.25, 1.5, 1.75, 2.0] {
            let physical = (240.0 * scale_factor, 360.0 * scale_factor);
            assert_eq!(
                captured_scrollbar_axis_to_logical(
                    ScrollbarKind::FileViewHorizontal,
                    physical,
                    scale_factor,
                ),
                Some(240.0),
                "horizontal scale factor {scale_factor}"
            );
            for kind in [ScrollbarKind::FileView, ScrollbarKind::Navigation] {
                assert_eq!(
                    captured_scrollbar_axis_to_logical(kind, physical, scale_factor),
                    Some(360.0),
                    "vertical {kind:?} scale factor {scale_factor}"
                );
            }
        }
        assert_eq!(
            captured_scrollbar_axis_to_logical(ScrollbarKind::FileView, (20.0, 40.0), 0.0,),
            None
        );
        assert_eq!(
            captured_scrollbar_axis_to_logical(
                ScrollbarKind::FileViewHorizontal,
                (f32::NAN, 40.0),
                1.5,
            ),
            None
        );
    }

    #[test]
    fn root_owns_the_single_theme_and_layout_source() {
        let root = ExplorerRoot::default();
        assert_eq!(root.tokens().theme, ThemeTokens::light());
        assert_eq!(root.tokens().layout, LayoutTokens::WINDOWS_11);
        assert_eq!(
            root.state().current_theme(),
            AppViewState::default().current_theme()
        );
        assert_eq!(root.state().tabs().tabs().len(), 1);
        assert_eq!(root.state().focused_surface(), FocusSurface::FileView);
    }

    #[test]
    fn pointer_motion_updates_do_not_end_active_text_editors() {
        let passive_actions = [
            ExplorerAction::UpdateMarquee {
                x: 1.0,
                y: 2.0,
                scroll_y: 0.0,
                viewport_width: 800.0,
            },
            ExplorerAction::UpdateFileDrag { x: 1.0, y: 2.0 },
            ExplorerAction::UpdateExternalDrag {
                destination_row: None,
                target: explorer_model::DropTargetKind::FileView,
                pointer_y: 2.0,
                top: 0.0,
                bottom: 400.0,
                effect: explorer_model::DragEffect::Copy,
            },
            ExplorerAction::UpdateDetailsColumnResize { pointer_x: 10.0 },
            ExplorerAction::UpdateSidePaneResize { pointer_x: 10.0 },
            ExplorerAction::UpdateScrollbarDrag { pointer_y: 10.0 },
            ExplorerAction::UpdateNavigationPaneResize { pointer_x: 10.0 },
            ExplorerAction::CancelFileDrag,
            ExplorerAction::EndDetailsColumnResize,
            ExplorerAction::EndSidePaneResize,
            ExplorerAction::EndScrollbarDrag {
                reason: crate::interaction::ScrollbarTerminal::PointerUp,
            },
            ExplorerAction::EndNavigationPaneResize,
        ];
        assert!(passive_actions.iter().all(is_passive_pointer_action));
        assert!(passive_actions.iter().all(|action| {
            !should_end_address_edit(action, ActionSource::Mouse)
                && !should_end_inline_rename(action, ActionSource::Mouse)
        }));

        let pointer_down_actions = [
            ExplorerAction::SelectItem { row_index: 0 },
            ExplorerAction::BeginMarquee {
                x: 1.0,
                y: 2.0,
                additive: false,
            },
            ExplorerAction::BeginFileDrag {
                x: 1.0,
                y: 2.0,
                button: explorer_model::DragButton::Left,
            },
            ExplorerAction::BeginScrollbarDrag {
                kind: crate::interaction::ScrollbarKind::FileView,
                grab_offset_y: 0.0,
            },
        ];
        assert!(
            pointer_down_actions
                .iter()
                .all(|action| !is_passive_pointer_action(action))
        );
        assert!(pointer_down_actions.iter().all(|action| {
            should_end_address_edit(action, ActionSource::Mouse)
                && should_end_inline_rename(action, ActionSource::Mouse)
        }));
        assert!(!should_end_address_edit(
            &ExplorerAction::UpdateAddressDraft("C:\\draft".to_owned()),
            ActionSource::Mouse,
        ));
        assert!(!should_end_inline_rename(
            &ExplorerAction::CommitInlineRename,
            ActionSource::Mouse,
        ));
    }

    fn key_event(key: &str, control: bool, alt: bool, shift: bool) -> gpui::KeyDownEvent {
        gpui::KeyDownEvent {
            keystroke: gpui::Keystroke {
                modifiers: gpui::Modifiers {
                    control,
                    alt,
                    shift,
                    ..gpui::Modifiers::default()
                },
                key: key.to_owned(),
                key_char: None,
            },
            is_held: false,
            prefer_character_input: false,
        }
    }

    #[test]
    fn explorer_file_shortcuts_map_to_typed_actions_even_for_empty_folders() {
        let global = [
            ("a", true, ExplorerAction::SelectAllItems),
            ("i", true, ExplorerAction::InvertSelection),
            ("c", true, ExplorerAction::CopySelected),
            ("x", true, ExplorerAction::CutSelected),
            ("v", true, ExplorerAction::Paste),
            ("backspace", false, ExplorerAction::Back),
            ("f3", false, ExplorerAction::FocusSearch),
        ];
        for (key, control, expected) in global {
            assert_eq!(
                file_view_global_command_action(&key_event(key, control, false, false)),
                Some(expected)
            );
        }

        let current = 7;
        assert_eq!(
            file_view_item_command_action(&key_event("enter", false, true, false), current),
            Some(ExplorerAction::ShowPropertiesSelected)
        );
        assert_eq!(
            file_view_item_command_action(&key_event("enter", true, false, false), current),
            Some(ExplorerAction::OpenItem {
                row_index: current,
                new_tab: true,
            })
        );
        assert_eq!(
            file_view_item_command_action(&key_event("space", true, false, false), current),
            Some(ExplorerAction::SelectAdditionalItem { row_index: current })
        );
        assert_eq!(
            file_view_item_command_action(&key_event("f2", false, false, false), current),
            Some(ExplorerAction::BeginRenameFocused)
        );
        assert_eq!(
            file_view_item_command_action(&key_event("delete", false, false, true), current),
            Some(ExplorerAction::RequestPermanentDelete)
        );
    }

    #[test]
    fn safe_mode_offer_is_removed_only_after_confirmation_observer_succeeds() {
        let attempts = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let failing_attempts = Arc::clone(&attempts);
        let mut root = ExplorerRoot::default();
        root.configure_safe_mode_offers(
            vec![SafeModeOfferV1 {
                presentation_token: 1,
                package_id: Some("example.plugin".to_owned()),
                primary_interface_namespace: Some(0x5345_0001),
                primary_interface_value: Some(7),
                operation: "RegistrarInProgress".to_owned(),
            }],
            Arc::new(move |_| {
                failing_attempts.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                Err("confirmation failed".to_owned())
            }),
        );
        root.confirm_safe_mode_offer(1);
        assert_eq!(root.safe_mode_offer_count(), 1);
        assert_eq!(attempts.load(std::sync::atomic::Ordering::SeqCst), 1);

        root.configure_safe_mode_offers(root.safe_mode_offers.clone(), Arc::new(|_| Ok(())));
        root.confirm_safe_mode_offer(1);
        assert_eq!(root.safe_mode_offer_count(), 0);
    }

    #[test]
    fn all_view_modes_have_clamped_spatial_keyboard_navigation() {
        for mode in explorer_model::ViewMode::ALL {
            let settings = explorer_model::ViewSettings {
                mode,
                ..explorer_model::ViewSettings::default()
            };
            assert_eq!(
                file_view_navigation_target(
                    &settings,
                    LayoutTokens::WINDOWS_11,
                    600.0,
                    4,
                    20,
                    "home"
                ),
                Some(0)
            );
            assert_eq!(
                file_view_navigation_target(
                    &settings,
                    LayoutTokens::WINDOWS_11,
                    600.0,
                    4,
                    20,
                    "end"
                ),
                Some(19)
            );
            let down = file_view_navigation_target(
                &settings,
                LayoutTokens::WINDOWS_11,
                600.0,
                4,
                20,
                "down",
            )
            .expect("down target");
            assert!(down > 4 && down < 20, "mode={mode:?} down={down}");
            assert_eq!(
                file_view_navigation_target(
                    &settings,
                    LayoutTokens::WINDOWS_11,
                    600.0,
                    19,
                    20,
                    "pagedown"
                ),
                Some(19)
            );
        }
        assert_eq!(
            file_view_navigation_target(
                &explorer_model::ViewSettings::default(),
                LayoutTokens::WINDOWS_11,
                600.0,
                0,
                0,
                "down"
            ),
            None
        );
    }

    #[test]
    fn icon_mode_down_arrow_uses_the_rendered_column_count() {
        let target = |mode, viewport_width| {
            let settings = explorer_model::ViewSettings {
                mode,
                ..explorer_model::ViewSettings::default()
            };
            file_view_navigation_target(
                &settings,
                LayoutTokens::WINDOWS_11,
                viewport_width,
                0,
                20,
                "down",
            )
        };
        assert_eq!(target(explorer_model::ViewMode::SmallIcons, 480.0), Some(2));
        assert_eq!(
            target(explorer_model::ViewMode::MediumIcons, 520.0),
            Some(4)
        );
        assert_eq!(target(explorer_model::ViewMode::LargeIcons, 600.0), Some(4));
    }

    #[test]
    fn windows_shell_texture_adapter_swaps_only_red_and_blue() {
        let mut pixels = vec![253, 212, 100, 255, 20, 40, 60, 80];
        prepare_shell_texture_pixels(&mut pixels);
        assert_eq!(pixels, [100, 212, 253, 255, 60, 40, 20, 80]);
    }

    #[test]
    fn texture_cache_is_lru_bounded_and_rejects_stale_association_or_overlay_results() {
        let mut cache = VisibleItemIconCache {
            capacity: 2,
            ..Default::default()
        };
        let texture = || {
            Arc::new(gpui::RenderImage::new(smallvec::SmallVec::<
                [image::Frame; 1],
            >::new()))
        };
        let key = |name: &str, generation| explorer_model::ShellIconKey {
            item_id: None,
            location: explorer_model::LocationDescriptor::file_system(format!(r"C:\{name}")),
            size_bucket: 20,
            dpi: 96,
            theme: explorer_model::ShellIconTheme::Light,
            association_generation: generation,
            overlay_generation: generation,
        };
        let first = key("first", 0);
        let second = key("second", 0);
        let third = key("third", 0);
        assert!(cache.insert(&first, texture()));
        assert!(cache.insert(&second, texture()));
        assert!(cache.get(&first).is_some());
        assert!(cache.insert(&third, texture()));
        assert!(cache.entries.contains_key(&first));
        assert!(!cache.entries.contains_key(&second));
        assert!(cache.entries.contains_key(&third));

        let current = key("first", 2);
        let stale = key("first", 1);
        assert!(cache.insert(&current, texture()));
        assert!(!cache.insert(&stale, texture()));
        let stale_overlay = explorer_model::ShellIconKey {
            association_generation: 3,
            overlay_generation: 1,
            ..current.clone()
        };
        assert!(!cache.insert(&stale_overlay, texture()));
        assert!(cache.entries.contains_key(&current));
        assert!(!cache.entries.contains_key(&first));

        cache.invalidate_environment(144, explorer_model::ShellIconTheme::Dark);
        assert!(cache.entries.is_empty());
    }

    #[test]
    fn navigation_icon_lookup_survives_newer_file_view_key_replacing_exact_drive_key() {
        let mut cache = VisibleItemIconCache::default();
        let location = explorer_model::LocationDescriptor::file_system(r"C:\");
        let exact = crate::navigation_pane::shell_icon_key(
            &location,
            explorer_model::ShellIconTheme::Light,
            96,
        );
        let newer = explorer_model::ShellIconKey {
            item_id: Some(
                explorer_model::ShellItemId::from_provider_bytes([0x43])
                    .expect("stable C drive identity"),
            ),
            association_generation: 4,
            overlay_generation: 2,
            ..exact.clone()
        };
        let texture = Arc::new(gpui::RenderImage::new(smallvec::SmallVec::<
            [image::Frame; 1],
        >::new()));
        assert!(cache.insert(&exact, Arc::clone(&texture)));
        assert!(cache.insert(&newer, Arc::clone(&texture)));
        assert!(!cache.entries.contains_key(&exact));

        let (resolved_key, resolved_texture) = cache
            .get_compatible_navigation_icon(&location, explorer_model::ShellIconTheme::Light, 96)
            .expect("newer compatible drive icon remains available to navigation");
        assert_eq!(resolved_key, newer);
        assert!(Arc::ptr_eq(&resolved_texture, &texture));
    }

    #[test]
    fn shared_base_cache_is_bounded_and_returns_the_same_arc_texture() {
        let mut cache = BaseIconCache::default();
        let texture = Arc::new(gpui::RenderImage::new(smallvec::SmallVec::<
            [image::Frame; 1],
        >::new()));
        let key = explorer_model::BaseIconKey {
            class: explorer_model::BaseIconClass::Extension("jpg".to_owned()),
            size_bucket: 20,
            dpi: 96,
            theme: explorer_model::ShellIconTheme::Light,
            association_epoch: 1,
        };
        cache.insert(key.clone(), Arc::clone(&texture), 7);
        let first = cache.get(&key).expect("shared texture");
        let second = cache.get(&key).expect("shared texture");
        assert!(Arc::ptr_eq(&first, &second));
        assert!(cache.entries.len() <= super::BASE_ICON_CACHE_CAPACITY);
        assert!(cache.current_bytes <= super::BASE_ICON_CACHE_BYTE_BUDGET);
    }

    #[derive(Default)]
    struct RecordingService(Mutex<Vec<explorer_model::ExplorerCommand>>);

    struct OverloadedService;

    impl explorer_model::ExplorerService for OverloadedService {
        fn submit(
            &self,
            _command: explorer_model::ExplorerCommand,
        ) -> Result<(), explorer_model::ExplorerServiceError> {
            Err(explorer_model::ExplorerServiceError::Overloaded)
        }

        fn try_recv(
            &self,
        ) -> Result<Option<explorer_model::ExplorerEvent>, explorer_model::ExplorerServiceError>
        {
            Ok(None)
        }
    }

    fn preview_fixture_root() -> (ExplorerRoot, Vec<explorer_model::FileEntry>) {
        let service = Arc::new(RecordingService::default());
        let mut root = ExplorerRoot {
            service: Some(service),
            ..ExplorerRoot::default()
        };
        let command = root
            .state
            .begin_active_location_load()
            .expect("preview fixture load");
        let context = command.context().expect("preview context").clone();
        assert_eq!(
            root.state
                .apply_service_event(explorer_model::ExplorerEvent::LocationResolved {
                    context: context.clone(),
                    metadata: explorer_model::LocationMetadata {
                        descriptor: explorer_model::LocationDescriptor::file_system(r"C:\preview"),
                        display_title: "preview".to_owned(),
                        can_go_up: true,
                        can_write: true,
                    },
                }),
            explorer_model::WindowEventOutcome::Applied
        );
        let entries = [(1, "first.jpg"), (2, "second.png"), (3, "unsupported.txt")]
            .into_iter()
            .map(|(id, name)| explorer_model::FileEntry {
                id: explorer_model::ShellItemId::from_provider_bytes([id]).expect("preview id"),
                display_name: name.to_owned(),
                location: explorer_model::LocationDescriptor::file_system(format!(
                    r"C:\preview\{name}"
                )),
                is_container: false,
                metadata: explorer_model::FileEntryMetadata::default(),
            })
            .collect::<Vec<_>>();
        assert_eq!(
            root.state
                .apply_service_event(explorer_model::ExplorerEvent::DirectoryBatch {
                    context: context.clone(),
                    entries: entries.clone(),
                }),
            explorer_model::WindowEventOutcome::Applied
        );
        let _ = root
            .state
            .apply_service_event(explorer_model::ExplorerEvent::DirectoryFinished { context });
        root.state.toggle_preview_pane();
        (root, entries)
    }

    #[test]
    fn selected_image_preview_handles_loading_cache_corrupt_unsupported_multiple_and_stale_selection()
     {
        let (mut root, entries) = preview_fixture_root();
        assert!(root.state.select_row(0));
        root.synchronize_preview_thumbnail(Some(&entries[0]));
        let first_key = root.preview_thumbnail_key.clone().expect("loading key");
        assert!(root.preview_texture.is_none());
        assert!(!root.preview_thumbnail_failed);
        assert!(root.pending_thumbnail_keys.contains(&first_key));

        assert!(root.state.select_row_additive(1));
        root.synchronize_preview_thumbnail(None);
        assert!(root.preview_thumbnail_key.is_none());
        assert!(!root.pending_thumbnail_keys.contains(&first_key));

        root.state.clear_selection();
        assert!(root.state.select_row(2));
        root.synchronize_preview_thumbnail(Some(&entries[2]));
        assert!(root.preview_thumbnail_key.is_none());
        assert!(root.preview_texture.is_none());

        let corrupt = explorer_model::ThumbnailPixels {
            width: 2,
            height: 2,
            stride: 8,
            bytes: vec![0; 3],
        };
        assert!(thumbnail_texture(&corrupt).is_none());

        root.state.clear_selection();
        assert!(root.state.select_row(0));
        root.synchronize_preview_thumbnail(Some(&entries[0]));
        let cached_key = root.preview_thumbnail_key.clone().expect("cache key");
        root.synchronize_preview_thumbnail(None);
        let pixels = explorer_model::ThumbnailPixels {
            width: 2,
            height: 1,
            stride: 8,
            bytes: vec![0, 0, 255, 255, 0, 255, 0, 255],
        };
        assert_eq!(
            root.thumbnail_memory_cache
                .insert(cached_key.clone(), Arc::new(pixels)),
            explorer_jobs::CacheInsertOutcome::Inserted
        );
        root.synchronize_preview_thumbnail(Some(&entries[0]));
        assert!(root.preview_texture.is_some());
        assert!(!root.preview_thumbnail_failed);

        root.state.clear_selection();
        assert!(root.state.select_row(1));
        root.synchronize_preview_thumbnail(Some(&entries[1]));
        let second_key = root.preview_thumbnail_key.clone().expect("replacement key");
        assert_ne!(first_key.item_id, second_key.item_id);
        assert_eq!(second_key.item_id, entries[1].id);
        assert!(root.preview_texture.is_none());
    }

    #[test]
    fn preview_host_selection_boundary_resize_and_unload_use_typed_generation_commands() {
        let (mut root, entries) = preview_fixture_root();
        let service = Arc::new(RecordingService::default());
        root.service = Some(service.clone());
        root.preview_coordinator = explorer_jobs::PreviewCoordinator::new(Duration::ZERO);
        root.preview_host_boundary = Some((101, 20, 30, 320, 240, 144));
        root.state.clear_selection();
        assert!(root.state.select_row(2));
        root.synchronize_preview_handler(Some(&entries[2]));
        root.poll_preview_handler();

        let generation = service
            .0
            .lock()
            .expect("commands")
            .iter()
            .find_map(|command| match command {
                explorer_model::ExplorerCommand::PreviewHost {
                    command:
                        explorer_model::PreviewHostCommand::Start {
                            parent_window,
                            bounds,
                            ..
                        },
                    ..
                } => {
                    assert_eq!(*parent_window, 101);
                    assert_eq!(bounds.width_physical, 320);
                    assert_eq!(bounds.dpi, 144);
                    Some(bounds.generation)
                }
                _ => None,
            })
            .expect("typed preview start");
        root.apply_preview_host_terminal(&explorer_model::PreviewHostTerminal::Ready {
            generation,
            mode: explorer_model::PreviewInitializationMode::File,
        });

        root.state.clear_selection();
        assert!(root.state.select_row(0));
        root.synchronize_preview_handler(Some(&entries[0]));
        root.apply_preview_host_terminal(&explorer_model::PreviewHostTerminal::Failed {
            generation,
            error: explorer_model::PreviewHostError::Crash,
        });
        assert!(
            !root.preview_thumbnail_failed,
            "a superseded preview terminal cannot mark the replacement selection failed"
        );
        assert!(
            service
                .0
                .lock()
                .expect("commands")
                .iter()
                .any(|command| matches!(
                    command,
                    explorer_model::ExplorerCommand::PreviewHost {
                        command: explorer_model::PreviewHostCommand::Unload { generation: value },
                        ..
                    } if *value == generation
                ))
        );
    }

    #[test]
    fn preview_host_accelerator_key_mapping_is_bounded_to_win32_virtual_keys() {
        assert_eq!(super::preview_virtual_key("a"), Some(u32::from(b'A')));
        assert_eq!(super::preview_virtual_key("left"), Some(0x25));
        assert_eq!(super::preview_virtual_key("f12"), Some(0x7B));
        assert_eq!(super::preview_virtual_key("unknown"), None);
    }

    #[test]
    fn namespace_thumbnail_requests_require_the_public_shell_capability() {
        let entry = |id, capabilities| explorer_model::FileEntry {
            id: explorer_model::ShellItemId::from_provider_bytes([id]).expect("identity"),
            display_name: format!("namespace-{id}"),
            location: explorer_model::LocationDescriptor::ParsingName(format!("shell:test-{id}")),
            is_container: false,
            metadata: explorer_model::FileEntryMetadata {
                namespace_capabilities: explorer_model::NamespaceCapabilities::from_public_bits(
                    capabilities,
                ),
                ..explorer_model::FileEntryMetadata::default()
            },
        };
        assert!(!super::namespace_thumbnail_supported(&entry(
            1,
            explorer_model::NamespaceCapabilities::OPEN
        )));
        assert!(super::namespace_thumbnail_supported(&entry(
            2,
            explorer_model::NamespaceCapabilities::OPEN
                | explorer_model::NamespaceCapabilities::THUMBNAIL
        )));
    }

    #[test]
    fn negative_visible_result_prevents_repeat_shell_work_for_same_overlay_epoch() {
        let service = Arc::new(RecordingService::default());
        let mut root = ExplorerRoot {
            service: Some(service.clone()),
            ..ExplorerRoot::default()
        };
        let entry = explorer_model::FileEntry {
            id: explorer_model::ShellItemId::from_provider_bytes([91]).expect("identity"),
            location: explorer_model::LocationDescriptor::file_system(r"C:\fixture\one.txt"),
            display_name: "one.txt".to_owned(),
            is_container: false,
            metadata: explorer_model::FileEntryMetadata::default(),
        };
        let tab = root.state.tabs().active_tab();
        let context = explorer_model::RequestContext::new(tab.id, tab.generation);
        let presentation = crate::navigation_pane::file_icon_key_for_size(
            &entry,
            explorer_model::ShellIconTheme::Light,
            96,
            20,
        );
        let base_key = explorer_model::base_icon_key(
            &entry,
            presentation.size_bucket,
            96,
            explorer_model::ShellIconTheme::Light,
            root.icon_epochs.association(),
        );
        root.failed_base_icons.insert(base_key);
        let mut item_key = super::file_icon_cache_key(
            &entry,
            explorer_model::ShellIconTheme::Light,
            96,
            20,
            root.icon_epochs.association(),
        );
        item_key.overlay_generation = root.icon_epochs.overlay();
        root.remember_negative_icon(item_key);
        root.submit_file_icon_loads(&context, &[entry]);
        assert!(service.0.lock().unwrap().is_empty());
        assert_eq!(root.shell_icons.stats().negative_hits, 1);
    }

    #[test]
    fn file_icon_service_key_carries_generation_while_presentation_key_stays_stable() {
        let entry = explorer_model::FileEntry {
            id: explorer_model::ShellItemId::from_provider_bytes([7]).unwrap(),
            location: explorer_model::LocationDescriptor::file_system(r"D:\test\fixture.txt"),
            display_name: "fixture.txt".to_owned(),
            is_container: false,
            metadata: explorer_model::FileEntryMetadata::default(),
        };
        let presentation = crate::navigation_pane::file_icon_key_for_size(
            &entry,
            explorer_model::ShellIconTheme::Light,
            168,
            20,
        );
        let cache =
            super::file_icon_cache_key(&entry, explorer_model::ShellIconTheme::Light, 168, 20, 9);
        assert_eq!(cache.location, presentation.location);
        assert_eq!(cache.item_id, presentation.item_id);
        assert_eq!(cache.size_bucket, presentation.size_bucket);
        assert_eq!(cache.association_generation, 9);
        assert_eq!(cache.overlay_generation, 9);
        assert_eq!(presentation.association_generation, 0);
        assert_eq!(presentation.overlay_generation, 0);
    }

    impl explorer_model::ExplorerService for RecordingService {
        fn submit(
            &self,
            command: explorer_model::ExplorerCommand,
        ) -> Result<(), explorer_model::ExplorerServiceError> {
            self.0.lock().unwrap().push(command);
            Ok(())
        }

        fn try_recv(
            &self,
        ) -> Result<Option<explorer_model::ExplorerEvent>, explorer_model::ExplorerServiceError>
        {
            Ok(None)
        }
    }

    #[test]
    fn one_hundred_thousand_same_extension_rows_request_one_bounded_real_viewport() {
        let service = Arc::new(RecordingService::default());
        let mut root = ExplorerRoot {
            service: Some(service.clone()),
            ..ExplorerRoot::default()
        };
        let tab = root.state.tabs().active_tab();
        let context = explorer_model::RequestContext::new(tab.id, tab.generation);
        let entries = (0_u64..100_000)
            .map(|index| explorer_model::FileEntry {
                id: explorer_model::ShellItemId::from_provider_bytes(index.to_le_bytes()).unwrap(),
                location: explorer_model::LocationDescriptor::file_system(format!(
                    r"C:\fixture\{index}.txt"
                )),
                display_name: format!("{index}.txt"),
                is_container: false,
                metadata: explorer_model::FileEntryMetadata::default(),
            })
            .collect::<Vec<_>>();
        root.submit_file_icon_loads(&context, &entries);
        root.submit_file_icon_loads(&context, &entries);
        let commands = service.0.lock().unwrap();
        let icon_commands = commands
            .iter()
            .filter_map(|command| match command {
                explorer_model::ExplorerCommand::LoadShellIcon { key, .. } => Some(key),
                _ => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(icon_commands.len(), 64);
        assert!(icon_commands.iter().all(|key| key.item_id.is_some()));
        assert!(icon_commands.iter().enumerate().all(|(index, key)| {
            key.location
                == explorer_model::LocationDescriptor::file_system(format!(
                    r"C:\fixture\{index}.txt"
                ))
        }));
    }

    #[test]
    fn dotfile_icon_request_uses_the_real_file_instead_of_a_synthetic_extensionless_path() {
        let service = Arc::new(RecordingService::default());
        let mut root = ExplorerRoot {
            service: Some(service.clone()),
            ..ExplorerRoot::default()
        };
        let tab = root.state.tabs().active_tab();
        let context = explorer_model::RequestContext::new(tab.id, tab.generation);
        let entry = explorer_model::FileEntry {
            id: explorer_model::ShellItemId::from_provider_bytes([0x47, 0x49, 0x54])
                .expect("identity"),
            location: explorer_model::LocationDescriptor::file_system(r"D:\UE_5.7\.gitignore"),
            display_name: ".gitignore".to_owned(),
            is_container: false,
            metadata: explorer_model::FileEntryMetadata::default(),
        };

        root.submit_file_icon_loads(&context, std::slice::from_ref(&entry));

        let commands = service.0.lock().unwrap();
        let icon_keys = commands
            .iter()
            .filter_map(|command| match command {
                explorer_model::ExplorerCommand::LoadShellIcon { key, .. } => Some(key),
                _ => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(icon_keys.len(), 1);
        assert_eq!(icon_keys[0].item_id.as_ref(), Some(&entry.id));
        assert_eq!(icon_keys[0].location, entry.location);
        assert_ne!(
            icon_keys[0].location,
            explorer_model::LocationDescriptor::file_system(r"C:\__super_explorer_base_file__.")
        );
    }

    #[test]
    fn fast_scroll_replaces_visible_icon_consumers_without_growing_pending_work() {
        let service = Arc::new(RecordingService::default());
        let mut root = ExplorerRoot {
            service: Some(service.clone()),
            ..ExplorerRoot::default()
        };
        let tab = root.state.tabs().active_tab();
        let context = explorer_model::RequestContext::new(tab.id, tab.generation);
        let entries = (0_u64..128)
            .map(|index| explorer_model::FileEntry {
                id: explorer_model::ShellItemId::from_provider_bytes(index.to_le_bytes())
                    .expect("identity"),
                location: explorer_model::LocationDescriptor::file_system(format!(
                    r"C:\fixture\{index}.exe"
                )),
                display_name: format!("{index}.exe"),
                is_container: false,
                metadata: explorer_model::FileEntryMetadata::default(),
            })
            .collect::<Vec<_>>();
        root.submit_file_icon_loads(&context, &entries[..64]);
        assert_eq!(
            root.pending_icon_keys.len(),
            super::FILE_VIEWPORT_ICON_REQUEST_CAP
        );
        root.submit_file_icon_loads(&context, &entries[64..]);
        assert_eq!(
            root.pending_icon_keys.len(),
            super::FILE_VIEWPORT_ICON_REQUEST_CAP
        );
        assert_eq!(root.pending_visible_bases.len(), 0);
    }

    #[test]
    fn tortoise_git_refresh_invalidates_only_overlay_presentations_and_preserves_view_state() {
        let entry = explorer_model::FileEntry {
            id: explorer_model::ShellItemId::from_provider_bytes([44]).expect("identity"),
            location: explorer_model::LocationDescriptor::file_system(r"C:\fixture\one.txt"),
            display_name: "one.txt".to_owned(),
            is_container: false,
            metadata: explorer_model::FileEntryMetadata::default(),
        };
        let mut root = ExplorerRoot::for_directory_fixture(
            UiTokens::default(),
            vec![entry.clone()],
            explorer_model::ViewMode::Details,
        );
        let service = Arc::new(RecordingService::default());
        root.service = Some(service.clone());
        root.configure_tortoise_git_available(true);
        let _ = dispatch_action(
            &mut root.state,
            ExplorerAction::SelectItem { row_index: 0 },
            ActionSource::Mouse,
        );
        root.set_file_scroll_offset_for_test(123.0);

        let association_epoch = root.icon_epochs.association();
        let base_key = explorer_model::base_icon_key(
            &entry,
            20,
            96,
            explorer_model::ShellIconTheme::Light,
            association_epoch,
        );
        let base_texture = Arc::new(gpui::RenderImage::new(smallvec::SmallVec::<
            [image::Frame; 1],
        >::new()));
        root.base_icons
            .insert(base_key.clone(), Arc::clone(&base_texture), 1);

        let mut old_icon_key = super::file_icon_cache_key(
            &entry,
            explorer_model::ShellIconTheme::Light,
            96,
            20,
            association_epoch,
        );
        old_icon_key.overlay_generation = 8;
        assert!(root.shell_icons.insert(&old_icon_key, base_texture));
        root.item_overlay_epochs.insert(entry.id.clone(), 17);
        root.negative_icon_keys.insert(old_icon_key.clone());
        root.negative_icon_order.push_back(old_icon_key.clone());
        root.pending_icon_keys.insert(old_icon_key.clone());

        let thumbnail_key = explorer_model::ThumbnailRequestKey {
            item_id: entry.id.clone(),
            physical_size: 96,
            dpi: 96,
            mode: explorer_model::ThumbnailMode::Thumbnail,
            source_generation: 1,
            theme: explorer_model::ShellIconTheme::Light,
            association_generation: association_epoch,
            overlay_generation: 17,
        };
        root.pending_thumbnail_keys.insert(thumbnail_key.clone());
        root.thumbnail_presentations
            .insert(thumbnail_key, old_icon_key);

        let selected_before = root.state.tabs().active_tab().selection.clone();
        let history_before = root.state.tabs().active_tab().history.current().cloned();
        let view_before = root.state.view_settings();
        let scroll_before = root.file_scroll.offset();

        assert!(root.refresh_tortoise_git_status());
        assert_eq!(root.icon_epochs.association(), association_epoch);
        assert!(root.icon_epochs.overlay() > 17);
        assert!(root.item_overlay_epochs.is_empty());
        assert!(root.shell_icons.entries.is_empty());
        assert!(root.negative_icon_keys.is_empty());
        assert!(root.negative_icon_order.is_empty());
        assert!(root.pending_thumbnail_keys.is_empty());
        assert!(root.thumbnail_presentations.is_empty());
        assert!(root.base_icons.get(&base_key).is_some());
        assert_eq!(root.state.tabs().active_tab().selection, selected_before);
        assert_eq!(
            root.state.tabs().active_tab().history.current(),
            history_before.as_ref()
        );
        assert_eq!(root.state.view_settings(), view_before);
        assert_eq!(root.file_scroll.offset(), scroll_before);
        assert!(service.0.lock().unwrap().iter().any(|command| matches!(
            command,
            explorer_model::ExplorerCommand::LoadShellIcon { .. }
        )));
    }

    #[test]
    fn breadcrumb_locations_request_each_real_shell_icon_once() {
        let service = Arc::new(RecordingService::default());
        let mut root = ExplorerRoot {
            service: Some(service.clone()),
            ..ExplorerRoot::default()
        };
        let tab = root.state.tabs().active_tab();
        let context = explorer_model::RequestContext::new(tab.id, tab.generation);
        let locations = [
            explorer_model::LocationDescriptor::ParsingName("shell:MyComputerFolder".to_owned()),
            explorer_model::LocationDescriptor::file_system(r"D:\"),
            explorer_model::LocationDescriptor::file_system(r"D:\fixture"),
        ];
        root.submit_location_icon_loads(&context, locations.iter());
        root.submit_location_icon_loads(&context, locations.iter());
        let commands = service.0.lock().unwrap();
        assert_eq!(
            commands
                .iter()
                .filter(|command| matches!(
                    command,
                    explorer_model::ExplorerCommand::LoadShellIcon { .. }
                ))
                .count(),
            locations.len()
        );
    }

    #[test]
    fn navigation_initialization_requests_each_open_tab_location_icon_once() {
        let service = Arc::new(RecordingService::default());
        let first_location = explorer_model::LocationDescriptor::file_system(r"C:\first");
        let second_location = explorer_model::LocationDescriptor::file_system(r"D:\second");
        let first = explorer_model::TabState::new(explorer_model::HistoryEntry::new(
            first_location.clone(),
            "first",
        ));
        let second = explorer_model::TabState::new(explorer_model::HistoryEntry::new(
            second_location.clone(),
            "second",
        ));
        let active = second.id;
        let tabs = explorer_model::ExplorerWindowState::from_restored_tabs(
            vec![first, second],
            active,
            explorer_model::HistoryEntry::new(
                explorer_model::LocationDescriptor::file_system(r"C:\fallback"),
                "fallback",
            ),
        )
        .expect("valid two-tab window");
        let mut root = ExplorerRoot {
            state: AppViewState::with_restored_window_and_drag_threshold(tabs, (4.0, 4.0)),
            service: Some(service.clone()),
            ..ExplorerRoot::default()
        };

        root.submit_navigation_icon_loads();
        root.submit_navigation_icon_loads();

        let expected = [first_location, second_location].map(|location| {
            crate::navigation_pane::shell_icon_key(
                &location,
                explorer_model::ShellIconTheme::Light,
                root.shell_icon_dpi,
            )
        });
        let commands = service.0.lock().unwrap();
        for key in expected {
            assert_eq!(
                commands
                    .iter()
                    .filter(|command| matches!(
                        command,
                        explorer_model::ExplorerCommand::LoadShellIcon {
                            key: requested,
                            ..
                        } if requested == &key
                    ))
                    .count(),
                1,
                "each visible tab location is requested and deduplicated"
            );
        }
    }

    #[test]
    fn navigation_initialization_requests_one_generic_shell_folder_icon() {
        let service = Arc::new(RecordingService::default());
        let mut root = ExplorerRoot {
            service: Some(service.clone()),
            ..ExplorerRoot::default()
        };
        root.submit_navigation_icon_loads();
        root.submit_navigation_icon_loads();

        let generic_key = crate::navigation_pane::generic_breadcrumb_folder_icon_key(
            explorer_model::ShellIconTheme::Light,
            root.shell_icon_dpi,
            root.icon_epochs.association(),
        );
        let commands = service.0.lock().unwrap();
        assert_eq!(
            commands
                .iter()
                .filter(|command| matches!(
                    command,
                    explorer_model::ExplorerCommand::LoadShellIcon { key, .. }
                        if key == &generic_key
                ))
                .count(),
            1
        );
        assert!(root.pending_icon_keys.contains(&generic_key));
    }

    #[test]
    fn navigation_snapshot_exposes_the_current_generic_breadcrumb_texture() {
        let mut root = ExplorerRoot::default();
        let generic_key = crate::navigation_pane::generic_breadcrumb_folder_icon_key(
            explorer_model::ShellIconTheme::Light,
            root.shell_icon_dpi,
            root.icon_epochs.association(),
        );
        let texture = Arc::new(gpui::RenderImage::new(smallvec::SmallVec::<
            [image::Frame; 1],
        >::new()));
        assert!(root.shell_icons.insert(&generic_key, Arc::clone(&texture)));

        let snapshot = root.navigation_icon_snapshot(&[]);
        let captured = snapshot
            .get(&generic_key)
            .expect("generic breadcrumb texture is included");
        assert!(Arc::ptr_eq(captured, &texture));
    }

    #[test]
    fn navigation_context_regression_reconciles_and_retries_dynamic_icons() {
        let service = Arc::new(RecordingService::default());
        let mut root = ExplorerRoot {
            service: Some(service.clone()),
            ..ExplorerRoot::default()
        };
        let parent = explorer_model::LocationDescriptor::file_system(r"D:\");
        assert!(root.state.toggle_navigation_node(parent.clone()));
        let command = root
            .state
            .begin_navigation_node_request(parent.clone())
            .expect("child enumeration request");
        let explorer_model::ExplorerCommand::EnumerateChildContainers {
            context,
            segment_id,
            menu_generation,
            ..
        } = command
        else {
            panic!("navigation command");
        };
        let child = explorer_model::LocationDescriptor::file_system(r"D:\AI_Pic");
        assert_eq!(
            root.state
                .apply_service_event(explorer_model::ExplorerEvent::ChildContainersBatch {
                    context,
                    segment_id,
                    menu_generation,
                    children: vec![explorer_model::BreadcrumbMenuItem {
                        display_name: "AI_Pic".to_owned(),
                        location: child.clone(),
                    }],
                }),
            explorer_model::WindowEventOutcome::Applied
        );
        root.submit_navigation_icon_loads();
        let child_key = crate::navigation_pane::shell_icon_key(
            &child,
            explorer_model::ShellIconTheme::Light,
            96,
        );
        assert!(service.0.lock().unwrap().iter().any(|command| matches!(
            command,
            explorer_model::ExplorerCommand::LoadShellIcon { key, .. } if key == &child_key
        )));

        root.pending_icon_keys.remove(&child_key);
        root.submit_navigation_icon_loads();
        assert_eq!(
            service
                .0
                .lock()
                .unwrap()
                .iter()
                .filter(|command| matches!(
                    command,
                    explorer_model::ExplorerCommand::LoadShellIcon { key, .. } if key == &child_key
                ))
                .count(),
            2,
            "a missed or failed dynamic icon is retried on reconciliation"
        );
    }

    #[test]
    fn theme_action_synchronizes_every_root_semantic_provider_once() {
        let mut root = ExplorerRoot::default();
        let trace = dispatch_action(
            &mut root.state,
            ExplorerAction::ToggleTheme,
            ActionSource::Keyboard,
        );
        synchronize_theme(&mut root.tokens, &root.state);

        assert_eq!(trace.outcome, ActionOutcome::Handled);
        assert_eq!(root.state.current_theme(), ThemeMode::Dark);
        assert_eq!(root.tokens.theme.mode, ThemeMode::Dark);
        for slot in SemanticColorSlot::ALL {
            assert_eq!(
                root.tokens.theme.colors.get(slot),
                ThemeTokens::dark().colors.get(slot)
            );
        }
    }

    #[test]
    fn every_visual_fixture_state_has_distinct_deterministic_model_evidence() {
        let empty =
            ExplorerRoot::for_visual_fixture(UiTokens::default(), VisualFixtureState::Empty);
        assert!(
            empty
                .state
                .tabs()
                .active_tab()
                .visible_snapshot()
                .is_some_and(|snapshot| snapshot.entries().is_empty())
        );

        let populated =
            ExplorerRoot::for_visual_fixture(UiTokens::default(), VisualFixtureState::Populated);
        assert_eq!(
            populated
                .state
                .tabs()
                .active_tab()
                .visible_snapshot()
                .map(|snapshot| snapshot.entries().len()),
            Some(4)
        );

        let error =
            ExplorerRoot::for_visual_fixture(UiTokens::default(), VisualFixtureState::Error);
        assert!(matches!(
            error.state.tabs().active_tab().directory,
            explorer_model::DirectoryState::Error { .. }
        ));

        let multi =
            ExplorerRoot::for_visual_fixture(UiTokens::default(), VisualFixtureState::MultiTab);
        assert_eq!(multi.state.tabs().tabs().len(), 2);

        let operation =
            ExplorerRoot::for_visual_fixture(UiTokens::default(), VisualFixtureState::Operation);
        assert!(operation.state.operation_center().latest().is_some());

        let drag =
            ExplorerRoot::for_visual_fixture(UiTokens::default(), VisualFixtureState::DragCue);
        assert!(matches!(
            drag.state.drag_session().state(),
            explorer_model::DragSessionState::Dragging { .. }
        ));

        let search =
            ExplorerRoot::for_visual_fixture(UiTokens::default(), VisualFixtureState::Search);
        assert!(matches!(
            search.state.tabs().active_tab().search,
            explorer_model::TabSearchState::Partial { .. }
        ));

        let focused =
            ExplorerRoot::for_visual_fixture(UiTokens::default(), VisualFixtureState::Focused);
        assert_eq!(focused.state.focused_surface(), FocusSurface::Search);
    }

    #[test]
    fn synthetic_home_navigation_is_service_independent_and_bounded() {
        let mut root = ExplorerRoot::new(UiTokens::default());
        root.configure_quick_access(vec![explorer_model::PersistedQuickAccessPin {
            location: explorer_model::LocationDescriptor::file_system(r"C:\fixture"),
            display_name: "fixture".to_owned(),
            order: 0,
        }]);
        let command = root
            .state
            .begin_active_navigation(
                explorer_model::LocationDescriptor::synthetic(explorer_model::SyntheticRoot::Home),
                false,
            )
            .expect("home navigation");
        assert!(root.submit_command(command));
        let tab = root.state.tabs().active_tab();
        assert_eq!(
            tab.history.current().map(|entry| &entry.location),
            Some(&explorer_model::LocationDescriptor::synthetic(
                explorer_model::SyntheticRoot::Home,
            ))
        );
        assert_eq!(
            tab.visible_snapshot().map(|value| value.entries().len()),
            Some(1)
        );
    }
}
