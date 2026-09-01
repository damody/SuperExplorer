//! Stateless Explorer chrome components for the M1 visual checkpoint.

use std::{
    collections::{HashMap, HashSet},
    hash::{Hash, Hasher},
    rc::Rc,
    sync::Arc,
    time::{Duration, Instant},
};

fn extension_render_item_id(
    item_id: &explorer_model::ShellItemId,
) -> explorer_extension_ui_api::StableIdV1 {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    item_id.provider_bytes().hash(&mut hasher);
    explorer_extension_ui_api::StableIdV1::new(
        explorer_extension_ui_api::EXTENSION_ID_NAMESPACE_V1,
        hasher.finish().max(1),
    )
}

fn extension_render_generation(item_id: &explorer_model::ShellItemId, snapshot: impl Hash) -> u64 {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    item_id.provider_bytes().hash(&mut hasher);
    snapshot.hash(&mut hasher);
    hasher.finish().max(1)
}

use abi_stable::std_types::{ROption, RString};
use explorer_model::{DirectoryState, TabId, TabSearchState};
use gpui::{
    AccessibleAction, Anchor, AnchoredPositionMode, App, BoxShadow, Context, DispatchPhase,
    Focusable, FontWeight, IntoElement, MouseButton, MouseMoveEvent, MouseUpEvent, ObjectFit,
    Render, RenderImage, RenderOnce, Role, SharedString, Window, WindowControlArea, anchored,
    canvas, deferred, div, hsla, img, point, prelude::*, px, relative, svg,
};
use gpui_elements::editable_text::{EditableTextState, text_input};
use raw_window_handle::{HasWindowHandle, RawWindowHandle};

use crate::{
    UiTokens,
    actions::{BookmarkPathKind, ExplorerAction, NavigationHistoryDirection},
    diagnostics::{TypographyObservation, region_probe, typography_probe},
    extension_commands::{ExifRenamePreset, ExtensionCommandPanel},
    focus::FocusSurface,
    icons::{
        ExplorerIcon, chrome_icon, navigation_history_icon, navigation_icon,
        unavailable_navigation_icon,
    },
    navigation_pane::{
        NavigationItem, NavigationItemAvailability, NavigationItemKind,
        is_generic_breadcrumb_folder_icon_key, is_selected, shell_icon_key,
        windows_navigation_items_with_pins,
    },
    state::{AppViewState, CommandKind, LockRecoveryPhase, LockRecoveryUiState},
    typography::TypographyStyle,
};

fn typography_diagnostic(tokens: UiTokens, style: TypographyStyle) -> TypographyObservation {
    TypographyObservation {
        profile: tokens.typography.reference_profile.to_owned(),
        family: tokens.typography.family.primary.to_owned(),
        size: style.size.value(),
        weight: style.weight,
        line_height: style.line_height.value(),
        baseline: style.baseline.value(),
    }
}

fn visual_column_color(color: crate::theme::Rgba8) -> explorer_extension_ui_api::CellColorV1 {
    explorer_extension_ui_api::CellColorV1 {
        red: color.red,
        green: color.green,
        blue: color.blue,
        alpha: color.alpha,
    }
}

fn shared_visual_column_theme(
    colors: crate::theme::SemanticColors,
) -> explorer_extension_ui_api::CellThemeV1 {
    explorer_extension_ui_api::CellThemeV1 {
        foreground: visual_column_color(colors.text_primary),
        muted_foreground: visual_column_color(colors.text_secondary),
        background: visual_column_color(colors.surface),
        selection_background: visual_column_color(colors.selected_active),
        accent: visual_column_color(colors.accent),
    }
}

pub const EXPLORER_WINDOW_ID: &str = "explorer-window";

#[derive(Clone)]
struct DetailsColumnDrag {
    column: explorer_model::ColumnId,
    label: String,
}

struct DetailsColumnDragPreview {
    label: String,
}

impl Render for DetailsColumnDragPreview {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .px(px(10.0))
            .py(px(6.0))
            .rounded(px(4.0))
            .border(px(1.0))
            .child(self.label.clone())
    }
}

#[derive(Clone)]
struct BookmarkDrag {
    id: explorer_model::BookmarkId,
    label: String,
}

struct BookmarkDragPreview {
    label: String,
}

fn bookmark_icon(target: &explorer_model::BookmarkTarget) -> &'static str {
    use explorer_model::{BookmarkTarget, FileSystemKind};
    match target {
        BookmarkTarget::LuaScript { .. } => "⚡",
        BookmarkTarget::Folder { location } | BookmarkTarget::File { location } => {
            match location.file_system_kind() {
                Some(FileSystemKind::Adb) => "📱",
                Some(FileSystemKind::Sftp) => "🖥",
                Some(FileSystemKind::Local) | None => "🔖",
            }
        }
        BookmarkTarget::FolderPath { path } | BookmarkTarget::FilePath { path } => {
            let path = path.trim_start();
            if path
                .get(..6)
                .is_some_and(|prefix| prefix.eq_ignore_ascii_case("adb://"))
            {
                "📱"
            } else if path
                .get(..7)
                .is_some_and(|prefix| prefix.eq_ignore_ascii_case("sftp://"))
            {
                "🖥"
            } else {
                "🔖"
            }
        }
    }
}

fn bookmark_icon_element(target: &explorer_model::BookmarkTarget) -> gpui::AnyElement {
    if matches!(target, explorer_model::BookmarkTarget::LuaScript { .. }) {
        div()
            .w(px(18.0))
            .h(px(18.0))
            .flex_none()
            .child(
                img("lua/logo.gif")
                    .size_full()
                    .object_fit(ObjectFit::Contain),
            )
            .into_any_element()
    } else {
        div()
            .w(px(18.0))
            .flex_none()
            .child(bookmark_icon(target))
            .into_any_element()
    }
}

fn bookmark_label(
    target: &explorer_model::BookmarkTarget,
    label: impl Into<SharedString>,
) -> gpui::AnyElement {
    div()
        .flex()
        .items_center()
        .gap(px(6.0))
        .child(bookmark_icon_element(target))
        .child(label.into())
        .into_any_element()
}

fn bookmark_toolbar_display_name(name: &str, parent_name: Option<&str>) -> String {
    parent_name.map_or_else(
        || name.to_owned(),
        |parent_name| format!("{name}（{parent_name}）"),
    )
}

impl Render for BookmarkDragPreview {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .px(px(10.0))
            .py(px(6.0))
            .rounded(px(4.0))
            .border(px(1.0))
            .child(self.label.clone())
    }
}
pub const WINDOW_CHROME_ID: &str = "window-chrome";
pub const WINDOW_DRAG_REGION_ID: &str = "window-drag-region";
pub const TAB_STRIP_ID: &str = "tab-strip";
pub const ACTIVE_TAB_ID: &str = "active-tab";
pub const NEW_TAB_BUTTON_ID: &str = "new-tab-button";
pub const CAPTION_MINIMIZE_ID: &str = "caption-minimize";
pub const CAPTION_MAXIMIZE_ID: &str = "caption-maximize";
pub const CAPTION_CLOSE_ID: &str = "caption-close";
pub const COMMAND_BAR_ID: &str = "command-bar";
pub const NAVIGATION_BAR_ID: &str = "navigation-bar";
pub const ADDRESS_EDITOR_ID: &str = "breadcrumb-address-editor";
pub const SEARCH_BOX_ID: &str = "search-box";
pub const NAVIGATION_PANE_ID: &str = "navigation-pane";
pub const NAVIGATION_DIVIDER_ID: &str = "navigation-divider";
pub const FILE_VIEW_HOST_ID: &str = "file-view-host";
pub const DETAILS_HEADER_ID: &str = "details-header";
pub const OPERATION_CENTER_ID: &str = "operation-center";
pub const STATUS_BAR_ID: &str = "status-bar";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PlaceholderInputMode {
    FocusOnly,
    Editable,
}

impl PlaceholderInputMode {
    pub const fn accepts_ime(self) -> bool {
        matches!(self, Self::Editable)
    }
}

pub const M1_ADDRESS_INPUT_MODE: PlaceholderInputMode = PlaceholderInputMode::Editable;
pub const M1_SEARCH_INPUT_MODE: PlaceholderInputMode = PlaceholderInputMode::Editable;

pub type ActionCallback = Rc<dyn Fn(&ExplorerAction, &mut Window, &mut App)>;

/// Cheap render-only state handles. Reducer ownership remains in `ExplorerRoot`; component clones
/// share one immutable frame snapshot and therefore cannot copy a directory or selection set.
pub type WindowChromeViewModel = Arc<AppViewState>;
pub type NavigationAddressViewModel = Arc<AppViewState>;
pub type CommandBarViewModel = Arc<AppViewState>;
pub type NavigationPaneViewModel = Arc<AppViewState>;
pub type OperationCenterViewModel = Arc<AppViewState>;
pub type StatusBarViewModel = Arc<AppViewState>;

/// Native non-client regions required for Windows caption and Snap behavior.
pub const CAPTION_CONTROL_AREAS: [WindowControlArea; 4] = [
    WindowControlArea::Drag,
    WindowControlArea::Min,
    WindowControlArea::Max,
    WindowControlArea::Close,
];

pub(crate) fn explorer_file_viewport_width(
    window: &Window,
    state: &AppViewState,
    tokens: UiTokens,
) -> f32 {
    let settings = state.view_settings();
    let side_pane_width = if settings.details_pane {
        f32::from(settings.details_pane_width)
    } else if settings.preview_pane {
        f32::from(settings.preview_pane_width)
    } else {
        0.0
    };
    let side_divider = if side_pane_width > 0.0 {
        tokens.layout.divider_width.value()
    } else {
        0.0
    };
    (f32::from(window.viewport_size().width)
        - state.navigation_pane_width().value()
        - tokens.layout.divider_width.value()
        - side_pane_width
        - side_divider
        - tokens.layout.content_spacing.value() * 1.5
        - tokens.layout.focus_stroke.value())
    .max(0.0)
}

pub(crate) fn explorer_file_origin_y(tokens: UiTokens) -> f32 {
    tokens.layout.title_tab_height.value()
        + tokens.layout.address_bar_height.value()
        + BOOKMARK_BAR_HEIGHT
        + tokens.layout.command_bar_height.value()
}

const BOOKMARK_BAR_HEIGHT: f32 = 32.0;

pub(crate) fn explorer_file_viewport_height(window: &Window, tokens: UiTokens) -> f32 {
    explorer_file_viewport_height_for_window(f32::from(window.viewport_size().height), tokens)
}

pub(crate) fn explorer_file_viewport_height_for_window(
    window_height: f32,
    tokens: UiTokens,
) -> f32 {
    (window_height - explorer_file_origin_y(tokens)).max(0.0)
}

/// Top-level presentation component. It receives copied view state and tokens
/// and deliberately has no access to Shell or filesystem services.
#[derive(IntoElement)]
pub struct ExplorerWindow {
    tokens: UiTokens,
    state: Arc<AppViewState>,
    on_action: Option<ActionCallback>,
    navigation_scroll: Option<gpui::ScrollHandle>,
    file_scroll: Option<gpui::ScrollHandle>,
    address_input: Option<gpui::WeakEntity<EditableTextState>>,
    search_input: Option<gpui::WeakEntity<EditableTextState>>,
    rename_input: Option<gpui::WeakEntity<EditableTextState>>,
    breadcrumb_menu_focus: Option<gpui::FocusHandle>,
    command_menu_focus: Option<gpui::FocusHandle>,
    shell_icons: HashMap<explorer_model::ShellIconKey, Arc<RenderImage>>,
    thumbnail_icon_keys: HashSet<explorer_model::ShellIconKey>,
    shell_icon_dpi: u16,
    file_presentation: Option<crate::file_view::DirectoryPresentation>,
    file_performance: Option<Arc<crate::performance::FileViewPerformanceCounters>>,
    preview_texture: Option<Arc<RenderImage>>,
    preview_thumbnail_failed: bool,
    folder_size_visuals: Option<crate::folder_size_column::FolderSizeColumnVisuals>,
    visual_column_runtime: Option<crate::folder_size_column::VisualColumnRuntimeHandleV1>,
    code_lines_visuals: Vec<crate::code_lines_column::CodeLinesColumnVisuals>,
    code_lines_runtimes: Vec<crate::code_lines_column::CodeLinesRuntimeHandleV1>,
    size_map_active: bool,
    size_map_visuals: Option<crate::size_map_view::SizeMapVisualsV1>,
    size_map_runtime: Option<crate::size_map_view::SizeMapRuntimeHandleV1>,
    size_map_context: Option<explorer_model::RequestContext>,
}

impl ExplorerWindow {
    pub fn new(tokens: UiTokens, state: AppViewState) -> Self {
        Self {
            tokens,
            state: Arc::new(state),
            on_action: None,
            navigation_scroll: None,
            file_scroll: None,
            address_input: None,
            search_input: None,
            rename_input: None,
            breadcrumb_menu_focus: None,
            command_menu_focus: None,
            shell_icons: HashMap::new(),
            thumbnail_icon_keys: HashSet::new(),
            shell_icon_dpi: 96,
            file_presentation: None,
            file_performance: None,
            preview_texture: None,
            preview_thumbnail_failed: false,
            folder_size_visuals: None,
            visual_column_runtime: None,
            code_lines_visuals: Vec::new(),
            code_lines_runtimes: Vec::new(),
            size_map_active: false,
            size_map_visuals: None,
            size_map_runtime: None,
            size_map_context: None,
        }
    }

    #[must_use]
    pub fn on_action(mut self, callback: ActionCallback) -> Self {
        self.on_action = Some(callback);
        self
    }

    #[must_use]
    pub fn with_file_scroll(mut self, handle: gpui::ScrollHandle) -> Self {
        self.file_scroll = Some(handle);
        self
    }

    #[must_use]
    pub fn with_navigation_scroll(mut self, handle: gpui::ScrollHandle) -> Self {
        self.navigation_scroll = Some(handle);
        self
    }

    #[must_use]
    pub fn with_text_inputs(
        mut self,
        address: Option<gpui::WeakEntity<EditableTextState>>,
        search: Option<gpui::WeakEntity<EditableTextState>>,
        rename: Option<gpui::WeakEntity<EditableTextState>>,
    ) -> Self {
        self.address_input = address;
        self.search_input = search;
        self.rename_input = rename;
        self
    }

    #[must_use]
    pub fn with_breadcrumb_menu_focus(mut self, handle: Option<gpui::FocusHandle>) -> Self {
        self.breadcrumb_menu_focus = handle;
        self
    }

    #[must_use]
    pub fn with_command_menu_focus(mut self, handle: Option<gpui::FocusHandle>) -> Self {
        self.command_menu_focus = handle;
        self
    }

    #[must_use]
    pub fn with_shell_icons(
        mut self,
        shell_icons: HashMap<explorer_model::ShellIconKey, Arc<RenderImage>>,
        thumbnail_icon_keys: HashSet<explorer_model::ShellIconKey>,
        shell_icon_dpi: u16,
    ) -> Self {
        self.shell_icons = shell_icons;
        self.thumbnail_icon_keys = thumbnail_icon_keys;
        self.shell_icon_dpi = shell_icon_dpi;
        self
    }

    #[must_use]
    pub fn with_file_presentation(
        mut self,
        presentation: Option<crate::file_view::DirectoryPresentation>,
    ) -> Self {
        self.file_presentation = presentation;
        self
    }

    #[must_use]
    pub fn with_file_performance(
        mut self,
        performance: Arc<crate::performance::FileViewPerformanceCounters>,
    ) -> Self {
        self.file_performance = Some(performance);
        self
    }

    #[must_use]
    pub fn with_preview_thumbnail(
        mut self,
        texture: Option<Arc<RenderImage>>,
        failed: bool,
    ) -> Self {
        self.preview_texture = texture;
        self.preview_thumbnail_failed = failed;
        self
    }

    #[must_use]
    pub fn with_folder_size_visuals(
        mut self,
        visuals: Option<crate::folder_size_column::FolderSizeColumnVisuals>,
    ) -> Self {
        self.folder_size_visuals = visuals;
        self
    }

    #[must_use]
    pub fn with_visual_column_runtime(
        mut self,
        runtime: Option<crate::folder_size_column::VisualColumnRuntimeHandleV1>,
    ) -> Self {
        self.visual_column_runtime = runtime;
        self
    }

    #[must_use]
    pub fn with_code_lines_columns(
        mut self,
        visuals: Vec<crate::code_lines_column::CodeLinesColumnVisuals>,
        runtimes: Vec<crate::code_lines_column::CodeLinesRuntimeHandleV1>,
    ) -> Self {
        self.code_lines_visuals = visuals;
        self.code_lines_runtimes = runtimes;
        self
    }

    #[must_use]
    pub fn with_size_map(
        mut self,
        active: bool,
        visuals: Option<crate::size_map_view::SizeMapVisualsV1>,
        runtime: Option<crate::size_map_view::SizeMapRuntimeHandleV1>,
        context: Option<explorer_model::RequestContext>,
    ) -> Self {
        self.size_map_active = active;
        self.size_map_visuals = visuals;
        self.size_map_runtime = runtime;
        self.size_map_context = context;
        self
    }
}

impl RenderOnce for ExplorerWindow {
    fn render(self, window: &mut Window, _cx: &mut App) -> impl IntoElement {
        let window_active = window.is_window_active();
        let file_icons = self.shell_icons.clone();
        let thumbnail_icon_keys = self.thumbnail_icon_keys;
        let navigation_icons = self.shell_icons.clone();
        let tab_icons = self.shell_icons.clone();
        let view_settings = self.state.view_settings();
        let column_registry = self.state.column_registry().clone();
        let size_map_menu_view = self
            .size_map_runtime
            .as_ref()
            .map(|runtime| runtime.config())
            .filter(crate::size_map_view::is_supported_size_map_config)
            .filter(|_| {
                self.state.extensions().iter().any(|extension| {
                    extension.package_id == "rust-folder-size-map-view" && extension.enabled
                })
            });
        let show_side_pane = f32::from(window.bounds().size.width)
            >= self.tokens.layout.compact_window_width.value()
            && (view_settings.details_pane || view_settings.preview_pane);
        let scrollbar_dragging = self
            .state
            .scrollbar_drag_session()
            .map(|session| session.kind);
        let details_column_resizing = self.state.details_column_resize_active();
        let side_pane_resizing = self.state.side_pane_resize_active();
        let marquee = self.state.marquee_session().cloned();
        let marquee_active = marquee.is_some();
        let about_dialog_info = self.state.about_dialog().cloned();
        let session_reset_confirmation = self.state.session_reset_confirmation();
        let permanent_delete_count = self.state.permanent_delete_confirmation_count();
        let permanent_delete_focus = self.state.permanent_delete_confirmation_focus();
        let lock_recovery = self.state.lock_recovery().cloned();
        let bookmark_folder_dismiss = self
            .state
            .bookmark_folder_menu()
            .map(|id| (id, self.on_action.clone()));
        let file_viewport_width = explorer_file_viewport_width(window, &self.state, self.tokens);
        let file_origin_x =
            self.state.navigation_pane_width().value() + self.tokens.layout.divider_width.value();
        let file_origin_y = explorer_file_origin_y(self.tokens);
        let scrollbar_capture_action = self.on_action.clone();
        let folder_size_backend_status = self.visual_column_runtime.as_ref().and_then(|runtime| {
            let (status, active) = runtime.backend_status();
            status.label(active).map(str::to_owned)
        });
        div()
            .id(EXPLORER_WINDOW_ID)
            .debug_selector(|| EXPLORER_WINDOW_ID.to_owned())
            .size_full()
            .flex()
            .flex_col()
            .relative()
            .bg(self.tokens.theme.colors.surface.to_gpui())
            .text_color(self.tokens.theme.colors.text_primary.to_gpui())
            .font_family(self.tokens.typography.family.primary)
            .text_size(px(self.tokens.typography.file_row.size.value()))
            .line_height(px(self.tokens.typography.file_row.line_height.value()))
            .child(region_probe(EXPLORER_WINDOW_ID, None, "normal"))
            .child(
                WindowChrome::new(
                    self.tokens,
                    self.state.clone(),
                    window_active,
                    self.on_action.clone(),
                )
                .with_shell_icons(tab_icons, self.shell_icon_dpi),
            )
            .child(NavigationBar::new(
                self.tokens,
                self.state.clone(),
                self.address_input,
                self.search_input,
                self.breadcrumb_menu_focus,
                navigation_icons,
                self.shell_icon_dpi,
                self.on_action.clone(),
            ))
            .child(bookmark_bar(
                self.tokens,
                &self.state,
                f32::from(window.bounds().size.width),
                self.on_action.clone(),
            ))
            .when_some(bookmark_folder_dismiss, |element, (id, callback)| {
                let dismiss = ExplorerAction::ToggleBookmarkFolderMenu { id };
                let right_callback = callback.clone();
                let right_dismiss = dismiss.clone();
                element.child(
                    deferred(
                        div()
                            .id("bookmark-folder-menu-dismiss-overlay")
                            .absolute()
                            .inset_0()
                            .when_some(callback, move |overlay, cb| {
                                overlay.on_mouse_down(MouseButton::Left, move |_, window, cx| {
                                    cb(&dismiss, window, cx)
                                })
                            })
                            .when_some(right_callback, move |overlay, cb| {
                                overlay.on_mouse_down(MouseButton::Right, move |_, window, cx| {
                                    cb(&right_dismiss, window, cx)
                                })
                            }),
                    )
                    .with_priority(155),
                )
            })
            .child(
                CommandBar::new(self.tokens, self.state.clone(), self.on_action.clone())
                    .with_menu_focus(self.command_menu_focus)
                    .with_extension_view(size_map_menu_view),
            )
            .child(
                div()
                    .flex_1()
                    .flex()
                    .overflow_hidden()
                    .child(
                        div()
                            .relative()
                            .h_full()
                            .w(px(self.state.navigation_pane_width().value()))
                            .flex_none()
                            .child(NavigationPane::new(
                                self.tokens,
                                self.state.clone(),
                                self.navigation_scroll.clone(),
                                self.shell_icons,
                                self.shell_icon_dpi,
                                self.on_action.clone(),
                            ))
                            .when_some(self.navigation_scroll.clone(), |element, handle| {
                                element.child(explorer_vertical_scrollbar(
                                    "navigation-scrollbar",
                                    crate::interaction::ScrollbarKind::Navigation,
                                    &handle,
                                    self.tokens,
                                    self.on_action.clone(),
                                ))
                            }),
                    )
                    .child(NavigationDivider::new(
                        self.tokens,
                        self.state.clone(),
                        self.on_action.clone(),
                    ))
                    .child(
                        div()
                            .relative()
                            .h_full()
                            .flex_1()
                            .overflow_hidden()
                            .child(FileViewHost::new(
                                self.tokens,
                                FileViewStatus::from_tab(self.state.tabs().active_tab()),
                                self.file_presentation,
                                self.file_performance,
                                self.state.tabs().active_tab().selection.clone(),
                                self.state.rename_editor().cloned(),
                                self.state.provisional_new_folder_entry(),
                                self.rename_input,
                                self.state.clipboard().clone(),
                                self.state.drag_session().state().clone(),
                                self.state.drop_target_row(),
                                self.state.active_presentation().can_write,
                                self.state
                                    .tabs()
                                    .active_tab()
                                    .history
                                    .current()
                                    .map(|entry| entry.location.clone()),
                                self.state.context_menu_pending(),
                                marquee,
                                file_origin_x,
                                file_origin_y,
                                self.state.view_settings(),
                                column_registry.clone(),
                                file_viewport_width,
                                self.file_scroll.clone(),
                                file_icons,
                                thumbnail_icon_keys,
                                self.shell_icon_dpi,
                                self.state.details_column_menu(),
                                self.state.details_filter_menu(),
                                self.state.active_details_filters(),
                                explorer_model::ColumnId::BUILT_INS
                                    .into_iter()
                                    .map(|column| {
                                        (column.clone(), self.state.details_filter_options(column))
                                    })
                                    .collect(),
                                self.folder_size_visuals,
                                self.visual_column_runtime,
                                self.code_lines_visuals,
                                self.code_lines_runtimes,
                                self.size_map_active,
                                self.size_map_visuals,
                                self.size_map_runtime,
                                self.size_map_context,
                                explorer_model::RequestContext::new(
                                    self.state.tabs().active_tab().id,
                                    self.state.tabs().active_tab().generation,
                                ),
                                self.on_action.clone(),
                            ))
                            .when_some(self.file_scroll.clone(), |element, handle| {
                                element
                                    .child(explorer_vertical_scrollbar(
                                        "file-view-scrollbar",
                                        crate::interaction::ScrollbarKind::FileView,
                                        &handle,
                                        self.tokens,
                                        self.on_action.clone(),
                                    ))
                                    .when(
                                        view_settings.mode == explorer_model::ViewMode::Details,
                                        |element| {
                                            element.child(explorer_horizontal_scrollbar(
                                                &handle,
                                                view_settings.clone(),
                                                column_registry.clone(),
                                                file_viewport_width,
                                                self.tokens,
                                                self.on_action.clone(),
                                            ))
                                        },
                                    )
                            }),
                    )
                    .when(show_side_pane, |element| {
                        element.child(side_pane_divider(
                            self.tokens,
                            &self.state,
                            self.on_action.clone(),
                        ))
                    })
                    .when(show_side_pane && view_settings.details_pane, |element| {
                        element.child(details_side_pane(self.tokens, &self.state))
                    })
                    .when(show_side_pane && view_settings.preview_pane, |element| {
                        element.child(preview_side_pane(
                            self.tokens,
                            &self.state,
                            self.preview_texture,
                            self.preview_thumbnail_failed,
                            self.on_action.clone(),
                        ))
                    }),
            )
            .when_some(
                self.state.pending_right_drop().cloned(),
                |element, pending| {
                    element.child(right_drag_terminal_menu(
                        self.tokens,
                        pending.allowed,
                        self.on_action.clone(),
                    ))
                },
            )
            .child(OperationCenter::new(
                self.tokens,
                self.state.clone(),
                self.on_action.clone(),
            ))
            .child(StatusBar::new(
                self.tokens,
                self.state.clone(),
                folder_size_backend_status,
                self.on_action.clone(),
            ))
            .when_some(self.state.bookmark_context_menu(), |element, menu| {
                element.child(bookmark_context_menu(
                    self.tokens,
                    &self.state,
                    menu,
                    f32::from(window.bounds().size.width),
                    f32::from(window.bounds().size.height),
                    self.on_action.clone(),
                ))
            })
            .when_some(
                self.state.bookmark_toolbar_context_menu(),
                |element, menu| {
                    element.child(bookmark_toolbar_context_menu(
                        self.tokens,
                        menu,
                        f32::from(window.bounds().size.width),
                        f32::from(window.bounds().size.height),
                        self.on_action.clone(),
                    ))
                },
            )
            .when_some(self.state.remote_context_menu(), |element, menu| {
                element.child(remote_context_menu(
                    self.tokens,
                    menu,
                    f32::from(window.bounds().size.width),
                    f32::from(window.bounds().size.height),
                    self.on_action.clone(),
                ))
            })
            .when_some(about_dialog_info, |element, info| {
                element.child(about_dialog(self.tokens, info, self.on_action.clone()))
            })
            .when_some(session_reset_confirmation, |element, scope| {
                element.child(session_reset_confirmation_dialog(
                    self.tokens,
                    scope,
                    self.on_action.clone(),
                ))
            })
            .when_some(permanent_delete_count, |element, count| {
                element.child(permanent_delete_confirmation_dialog(
                    self.tokens,
                    count,
                    permanent_delete_focus
                        .unwrap_or(crate::actions::PermanentDeleteDialogTarget::Delete),
                    self.on_action.clone(),
                ))
            })
            .when_some(lock_recovery, |element, recovery| {
                element.child(lock_recovery_dialog(
                    self.tokens,
                    recovery,
                    self.on_action.clone(),
                ))
            })
            .when(
                scrollbar_dragging.is_some()
                    || details_column_resizing
                    || side_pane_resizing
                    || marquee_active,
                |element| {
                    element.child(pointer_drag_capture_listener(
                        scrollbar_capture_action,
                        scrollbar_dragging,
                        details_column_resizing,
                        side_pane_resizing,
                        marquee_active,
                        file_origin_x,
                        file_origin_y,
                        self.file_scroll.clone(),
                        file_viewport_width,
                    ))
                },
            )
    }
}

fn bookmark_bar(
    tokens: UiTokens,
    state: &AppViewState,
    width: f32,
    callback: Option<ActionCallback>,
) -> impl IntoElement {
    let entries = state
        .bookmarks()
        .root_entries()
        .cloned()
        .collect::<Vec<_>>();
    let root_folders = state
        .bookmarks()
        .child_folders(None)
        .cloned()
        .collect::<Vec<_>>();
    let active_folder_menu = state.bookmark_folder_menu().and_then(|id| {
        state
            .bookmarks()
            .folder(id)
            .cloned()
            .map(|folder| (folder, bookmark_folder_menu_items(state.bookmarks(), id)))
    });
    let visible_limit = bookmark_visible_count(entries.len(), width);
    let visible = entries
        .iter()
        .take(visible_limit)
        .cloned()
        .collect::<Vec<_>>();
    let overflow_entries = entries
        .iter()
        .skip(visible.len())
        .cloned()
        .collect::<Vec<_>>();
    let overflow = overflow_entries.len();
    let toolbar_context_cb = callback.clone();
    let root_drop_cb = callback.clone();
    div()
        .id("bookmark-toolbar")
        .debug_selector(|| "bookmark-toolbar".to_owned())
        .relative()
        .h(px(BOOKMARK_BAR_HEIGHT))
        .flex_none()
        .flex()
        .items_center()
        .gap(px(4.0))
        .px(px(tokens.layout.control_padding_horizontal.value()))
        .border_b(px(tokens.layout.focus_stroke.value()))
        .border_color(tokens.theme.colors.divider.to_gpui())
        .bg(tokens.theme.colors.surface.to_gpui())
        .when_some(toolbar_context_cb, |element, cb| {
            element
                .on_mouse_down(MouseButton::Right, move |event, window, cx| {
                    cb(
                        &ExplorerAction::OpenBookmarkToolbarContextMenu {
                            parent_id: None,
                            x: f32::from(event.position.x),
                            y: f32::from(event.position.y),
                        },
                        window,
                        cx,
                    );
                })
                .on_mouse_up(MouseButton::Right, |_, _, cx| cx.stop_propagation())
        })
        .when_some(root_drop_cb, |element, cb| {
            element.on_drop(move |drag: &BookmarkDrag, window, cx| {
                cb(
                    &ExplorerAction::MoveBookmarkToFolder {
                        id: drag.id,
                        parent_id: None,
                    },
                    window,
                    cx,
                );
            })
        })
        .child({
            let current_folder = state.current_folder_bookmark_target_and_id();
            let enabled = current_folder.is_some();
            let bookmarked = current_folder.is_some_and(|(_, id)| id.is_some());
            let label = if bookmarked {
                "Edit or remove current folder bookmark"
            } else if enabled {
                "Add current folder bookmark and choose a folder"
            } else {
                "Current location cannot be bookmarked"
            };
            div()
                .id("bookmark-star-toggle")
                .role(Role::Button)
                .aria_label(label)
                .flex_none()
                .px(px(8.0))
                .py(px(4.0))
                .text_size(px(20.0))
                .rounded(px(4.0))
                .text_color(if bookmarked {
                    tokens.theme.colors.focus.to_gpui()
                } else if enabled {
                    tokens.theme.colors.text_primary.to_gpui()
                } else {
                    tokens.theme.colors.text_disabled.to_gpui()
                })
                .child(if bookmarked { "★" } else { "☆" })
                .when(enabled, |element| {
                    element
                        .cursor_pointer()
                        .hover(|style| style.bg(tokens.theme.colors.control_hover.to_gpui()))
                        .when_some(callback.clone(), move |element, callback| {
                            element.on_click(move |event, window, cx| {
                                let (_, screen_x, screen_y) =
                                    context_menu_coordinates(event.position(), window);
                                callback(
                                    &ExplorerAction::ToggleCurrentFolderBookmark {
                                        screen_x: screen_x.saturating_sub(18),
                                        screen_y: screen_y.saturating_add(18),
                                    },
                                    window,
                                    cx,
                                );
                            })
                        })
                })
        })
        .children(root_folders.into_iter().map(|folder| {
            let action = ExplorerAction::ToggleBookmarkFolderMenu { id: folder.id };
            let callback = callback.clone();
            let context_callback = callback.clone();
            let drop_callback = callback.clone();
            let folder_id = folder.id;
            div()
                .id(("bookmark-folder", folder.id.as_u128() as u64))
                .role(Role::Button)
                .aria_label(format!("Bookmark folder {}", folder.name))
                .cursor_pointer()
                .px(px(8.0))
                .py(px(4.0))
                .rounded(px(4.0))
                .hover(|style| style.bg(tokens.theme.colors.control_hover.to_gpui()))
                .child(format!("📁 {} ▾", folder.name))
                .when_some(callback, move |element, callback| {
                    element.on_click(move |_, window, cx| callback(&action, window, cx))
                })
                .when_some(context_callback, move |element, cb| {
                    element.on_mouse_down(MouseButton::Right, move |event, window, cx| {
                        cx.stop_propagation();
                        cb(
                            &ExplorerAction::OpenBookmarkToolbarContextMenu {
                                parent_id: Some(folder_id),
                                x: f32::from(event.position.x),
                                y: f32::from(event.position.y),
                            },
                            window,
                            cx,
                        );
                    })
                })
                .when_some(drop_callback, move |element, cb| {
                    element.on_drop(move |drag: &BookmarkDrag, window, cx| {
                        cb(
                            &ExplorerAction::MoveBookmarkToFolder {
                                id: drag.id,
                                parent_id: Some(folder_id),
                            },
                            window,
                            cx,
                        );
                        cx.stop_propagation();
                    })
                })
                .on_mouse_up(MouseButton::Right, |_, _, cx| cx.stop_propagation())
        }))
        .children(visible.into_iter().map(|bookmark| {
            let id = bookmark.id;
            let icon = bookmark_icon(&bookmark.target);
            let display_name = bookmark_toolbar_display_name(
                &bookmark.name,
                bookmark
                    .parent_id
                    .and_then(|id| state.bookmarks().folder(id))
                    .map(|folder| folder.name.as_str()),
            );
            let action = ExplorerAction::ActivateBookmark { id };
            let callback = callback.clone();
            let context_callback = callback.clone();
            let drag_label = bookmark.name.clone();
            div()
                .id(("bookmark", id.as_u128() as u64))
                .role(Role::Button)
                .aria_label(format!("{icon} Bookmark: {}", bookmark.name))
                .cursor_pointer()
                .px(px(8.0))
                .py(px(4.0))
                .rounded(px(4.0))
                .hover(|style| style.bg(tokens.theme.colors.control_hover.to_gpui()))
                .on_drag(
                    BookmarkDrag {
                        id,
                        label: drag_label,
                    },
                    |drag, _, _, cx| {
                        cx.new(|_| BookmarkDragPreview {
                            label: drag.label.clone(),
                        })
                    },
                )
                .child(bookmark_label(&bookmark.target, display_name))
                .when_some(callback, move |element, callback| {
                    element.on_click(move |_, window, cx| callback(&action, window, cx))
                })
                .when_some(context_callback, move |element, cb| {
                    element.on_mouse_down(MouseButton::Right, move |event, window, cx| {
                        cx.stop_propagation();
                        cb(
                            &ExplorerAction::OpenBookmarkContextMenu {
                                id,
                                x: f32::from(event.position.x),
                                y: f32::from(event.position.y),
                            },
                            window,
                            cx,
                        );
                    })
                })
                .on_mouse_up(MouseButton::Right, |_, _, cx| cx.stop_propagation())
        }))
        .when(overflow > 0, |element| {
            let toggle = ExplorerAction::ToggleBookmarkOverflow;
            let toggle_callback = callback.clone();
            element.child(
                div()
                    .id("bookmark-overflow-toggle")
                    .role(Role::Button)
                    .aria_label(format!("More Bookmarks, {overflow} items"))
                    .cursor_pointer()
                    .px(px(8.0))
                    .py(px(4.0))
                    .child(format!("More Bookmarks ({overflow})"))
                    .when_some(toggle_callback, move |element, callback| {
                        element.on_click(move |_, window, cx| callback(&toggle, window, cx))
                    }),
            )
        })
        .child({
            let callback = callback.clone();
            let action = ExplorerAction::AddLuaBookmark;
            div()
                .id("bookmark-add-lua")
                .role(Role::Button)
                .aria_label("Add Lua bookmark")
                .cursor_pointer()
                .px(px(8.0))
                .child("＋")
                .when_some(callback, move |element, callback| {
                    element.on_click(move |_, window, cx| callback(&action, window, cx))
                })
        })
        .child({
            let callback = callback.clone();
            let action = ExplorerAction::ToggleBookmarkManager;
            div()
                .id("bookmark-manage")
                .role(Role::Button)
                .aria_label("Manage bookmarks")
                .cursor_pointer()
                .px(px(8.0))
                .child("管理")
                .when_some(callback, move |element, callback| {
                    element.on_click(move |_, window, cx| callback(&action, window, cx))
                })
        })
        .when(state.bookmark_overflow_open() && overflow > 0, |element| {
            element.child(
                deferred(
                    div()
                        .id("bookmark-overflow-menu")
                        .absolute()
                        .top(px(BOOKMARK_BAR_HEIGHT - 1.0))
                        .right(px(80.0))
                        .min_w(px(260.0))
                        .max_w(px(520.0))
                        .p(px(6.0))
                        .rounded(px(6.0))
                        .border(px(1.0))
                        .border_color(tokens.theme.colors.divider.to_gpui())
                        .bg(tokens.theme.colors.menu_fill.to_gpui())
                        .children(overflow_entries.into_iter().map(|bookmark| {
                            let id = bookmark.id;
                            let icon = bookmark_icon(&bookmark.target);
                            let action = ExplorerAction::ActivateBookmark { id };
                            let callback = callback.clone();
                            let context_callback = callback.clone();
                            div()
                                .id(("bookmark-overflow", id.as_u128() as u64))
                                .role(Role::Button)
                                .aria_label(format!("{icon} Bookmark: {}", bookmark.name))
                                .cursor_pointer()
                                .px(px(8.0))
                                .py(px(6.0))
                                .rounded(px(4.0))
                                .hover(|style| {
                                    style.bg(tokens.theme.colors.control_hover.to_gpui())
                                })
                                .child(bookmark_label(&bookmark.target, bookmark.name.clone()))
                                .when_some(callback, move |element, callback| {
                                    element.on_click(move |_, window, cx| {
                                        callback(&action, window, cx)
                                    })
                                })
                                .when_some(context_callback, move |element, cb| {
                                    element.on_mouse_down(
                                        MouseButton::Right,
                                        move |event, window, cx| {
                                            cx.stop_propagation();
                                            cb(
                                                &ExplorerAction::OpenBookmarkContextMenu {
                                                    id,
                                                    x: f32::from(event.position.x),
                                                    y: f32::from(event.position.y),
                                                },
                                                window,
                                                cx,
                                            );
                                        },
                                    )
                                })
                                .on_mouse_up(MouseButton::Right, |_, _, cx| cx.stop_propagation())
                        })),
                )
                .with_priority(150),
            )
        })
        .when_some(active_folder_menu, |element, (_folder, entries)| {
            element.child(
                deferred(
                    div()
                        .id("bookmark-folder-menu")
                        .role(Role::Menu)
                        .aria_label("Bookmark folder menu")
                        .absolute()
                        .top(px(BOOKMARK_BAR_HEIGHT - 1.0))
                        .left(px(52.0))
                        .min_w(px(280.0))
                        .max_h(px(420.0))
                        .overflow_y_scroll()
                        .p(px(6.0))
                        .rounded(px(6.0))
                        .border(px(1.0))
                        .border_color(tokens.theme.colors.divider.to_gpui())
                        .bg(tokens.theme.colors.menu_fill.to_gpui())
                        .children(entries.into_iter().map(|entry| match entry {
                            BookmarkFolderMenuItem::Folder(folder) => {
                                let folder_id = folder.id;
                                let action =
                                    ExplorerAction::ToggleBookmarkFolderMenu { id: folder_id };
                                let callback = callback.clone();
                                let context_callback = callback.clone();
                                div()
                                    .id(("bookmark-folder-child", folder_id.as_u128() as u64))
                                    .role(Role::MenuItem)
                                    .cursor_pointer()
                                    .px(px(8.0))
                                    .py(px(5.0))
                                    .flex()
                                    .justify_between()
                                    .child(format!("📁 {}", folder.name))
                                    .child("›")
                                    .when_some(callback, move |item, cb| {
                                        item.on_click(move |_, window, cx| cb(&action, window, cx))
                                    })
                                    .when_some(context_callback, move |item, cb| {
                                        item.on_mouse_down(
                                            MouseButton::Right,
                                            move |event, window, cx| {
                                                cx.stop_propagation();
                                                cb(&ExplorerAction::OpenBookmarkToolbarContextMenu {
                                                parent_id: Some(folder_id),
                                                x: f32::from(event.position.x),
                                                y: f32::from(event.position.y),
                                            }, window, cx);
                                            },
                                        )
                                    })
                                    .on_mouse_up(MouseButton::Right, |_, _, cx| {
                                        cx.stop_propagation()
                                    })
                                    .into_any_element()
                            }
                            BookmarkFolderMenuItem::Bookmark(bookmark) => {
                                let id = bookmark.id;
                                let action = ExplorerAction::ActivateBookmark { id };
                                let callback = callback.clone();
                                let context_callback = callback.clone();
                                div()
                                    .id(("bookmark-folder-entry", id.as_u128() as u64))
                                    .role(Role::MenuItem)
                                    .aria_label(format!("Bookmark {}", bookmark.name))
                                    .cursor_pointer()
                                    .px(px(8.0))
                                    .py(px(5.0))
                                    .child(bookmark_label(&bookmark.target, bookmark.name.clone()))
                                    .when_some(callback, move |item, cb| {
                                        item.on_click(move |_, window, cx| cb(&action, window, cx))
                                    })
                                    .when_some(context_callback, move |item, cb| {
                                        item.on_mouse_down(
                                            MouseButton::Right,
                                            move |event, window, cx| {
                                                cx.stop_propagation();
                                                cb(
                                                    &ExplorerAction::OpenBookmarkContextMenu {
                                                        id,
                                                        x: f32::from(event.position.x),
                                                        y: f32::from(event.position.y),
                                                    },
                                                    window,
                                                    cx,
                                                );
                                            },
                                        )
                                    })
                                    .on_mouse_up(MouseButton::Right, |_, _, cx| {
                                        cx.stop_propagation()
                                    })
                                    .into_any_element()
                            }
                        })),
                )
                .with_priority(160),
            )
        })
}

enum BookmarkFolderMenuItem {
    Folder(explorer_model::BookmarkFolder),
    Bookmark(explorer_model::Bookmark),
}

fn bookmark_folder_menu_items(
    bookmarks: &explorer_model::Bookmarks,
    root: explorer_model::BookmarkFolderId,
) -> Vec<BookmarkFolderMenuItem> {
    let mut output = bookmarks
        .child_folders(Some(root))
        .cloned()
        .map(BookmarkFolderMenuItem::Folder)
        .chain(
            bookmarks
                .child_entries(Some(root))
                .cloned()
                .map(BookmarkFolderMenuItem::Bookmark),
        )
        .collect::<Vec<_>>();
    output.sort_by_key(|item| match item {
        BookmarkFolderMenuItem::Folder(folder) => folder.order,
        BookmarkFolderMenuItem::Bookmark(bookmark) => bookmark.order,
    });
    output
}

fn bookmark_visible_count(entry_count: usize, width: f32) -> usize {
    entry_count.min(((width - 120.0) / 150.0).floor().max(1.0) as usize)
}

fn bookmark_manager_folder_depth(
    bookmarks: &explorer_model::Bookmarks,
    folder: &explorer_model::BookmarkFolder,
    expanded: &HashSet<explorer_model::BookmarkFolderId>,
) -> Option<usize> {
    let mut depth = 0usize;
    let mut parent = folder.parent_id;
    while let Some(id) = parent {
        if !expanded.contains(&id) {
            return None;
        }
        let ancestor = bookmarks.folder(id)?;
        depth = depth.saturating_add(1);
        parent = ancestor.parent_id;
    }
    Some(depth)
}

pub(crate) fn bookmark_manager(
    tokens: UiTokens,
    state: &AppViewState,
    ui: &crate::bookmark_manager_window::BookmarkManagerUiState,
    search_input: Option<gpui::WeakEntity<EditableTextState>>,
    detail_input: Option<gpui::WeakEntity<EditableTextState>>,
    detail_location_input: Option<gpui::WeakEntity<EditableTextState>>,
    search_query: &str,
    callback: Option<ActionCallback>,
    ui_callback: Option<crate::bookmark_manager_window::BookmarkManagerUiCallback>,
) -> impl IntoElement {
    let search_query = search_query.trim().to_lowercase();
    let manager_row_height = if ui.compact { 24.0 } else { 32.0 };
    let (input_text, input_selection, input_selection_text, input_caret) =
        editable_input_colors(tokens);
    let folder_rows = state
        .bookmarks()
        .folders()
        .iter()
        .cloned()
        .filter_map(|folder| {
            let depth =
                bookmark_manager_folder_depth(state.bookmarks(), &folder, &ui.expanded_folders)?;
            (search_query.is_empty() || folder.name.to_lowercase().contains(&search_query))
                .then_some((folder, depth))
        })
        .map(|(folder, depth)| {
            let edit = ExplorerAction::EditBookmarkFolder { id: folder.id };
            let edit_cb = callback.clone();
            let select_cb = ui_callback.clone();
            let toggle_cb = ui_callback.clone();
            let toggle =
                crate::bookmark_manager_window::BookmarkManagerUiAction::ToggleFolder(folder.id);
            let select = crate::bookmark_manager_window::BookmarkManagerUiAction::Navigate(
                crate::bookmark_manager_window::BookmarkManagerLocation::Folder(folder.id),
            );
            let selected = ui.selected_folder == Some(folder.id);
            div()
                .id(("bookmark-folder-row", folder.id.as_u128() as u64))
                .role(Role::ListItem)
                .aria_label(format!("Bookmark folder {}", folder.name))
                .flex()
                .items_center()
                .h(px(manager_row_height))
                .pl(px(28.0 + depth as f32 * 20.0))
                .pr(px(8.0))
                .gap(px(7.0))
                .cursor_pointer()
                .when(selected, |row| {
                    row.bg(tokens.theme.colors.control_pressed.to_gpui())
                })
                .hover(|style| style.bg(tokens.theme.colors.control_hover.to_gpui()))
                .child(
                    div()
                        .id(("bookmark-manager-folder-toggle", folder.id.as_u128() as u64))
                        .role(Role::Button)
                        .child(if ui.expanded_folders.contains(&folder.id) {
                            "⌄"
                        } else {
                            "▸"
                        })
                        .when_some(toggle_cb, move |arrow, cb| {
                            arrow.on_mouse_down(MouseButton::Left, move |_, window, cx| {
                                cb(&toggle, window, cx)
                            })
                        }),
                )
                .child(format!("📁 {}", folder.name))
                .when_some(select_cb, move |element, cb| {
                    element.on_mouse_down(MouseButton::Left, move |_, window, cx| {
                        cb(&select, window, cx)
                    })
                })
                .when_some(edit_cb, move |element, cb| {
                    element.on_click(move |event, window, cx| {
                        if event.click_count() == 2 {
                            cb(&edit, window, cx);
                        }
                    })
                })
        })
        .collect::<Vec<_>>();
    let mut presented_bookmarks = state
        .bookmarks()
        .entries()
        .iter()
        .cloned()
        .filter(|bookmark| match ui.location {
            crate::bookmark_manager_window::BookmarkManagerLocation::AllBookmarks => true,
            crate::bookmark_manager_window::BookmarkManagerLocation::Root => {
                bookmark.parent_id.is_none()
            }
            crate::bookmark_manager_window::BookmarkManagerLocation::Folder(id) => {
                bookmark.parent_id == Some(id)
            }
        })
        .filter(|bookmark| {
            search_query.is_empty()
                || bookmark.name.to_lowercase().contains(&search_query)
                || bookmark
                    .target
                    .editable_payload()
                    .to_lowercase()
                    .contains(&search_query)
        })
        .collect::<Vec<_>>();
    presented_bookmarks.sort_by(|left, right| {
        let ordering = match ui.sort_column {
            crate::bookmark_manager_window::BookmarkManagerSortColumn::Name => {
                left.name.to_lowercase().cmp(&right.name.to_lowercase())
            }
            crate::bookmark_manager_window::BookmarkManagerSortColumn::Tags => {
                std::cmp::Ordering::Equal
            }
            crate::bookmark_manager_window::BookmarkManagerSortColumn::Location => left
                .target
                .editable_payload()
                .to_lowercase()
                .cmp(&right.target.editable_payload().to_lowercase()),
        };
        ordering.then_with(|| left.order.cmp(&right.order))
    });
    if ui.descending {
        presented_bookmarks.reverse();
    }
    let rows = presented_bookmarks
        .into_iter()
        .map(|bookmark| {
            let sibling_index = state
                .bookmarks()
                .child_entries(bookmark.parent_id)
                .position(|entry| entry.id == bookmark.id)
                .unwrap_or(0);
            let id = bookmark.id;
            let edit = ExplorerAction::EditBookmark { id };
            let drag_label = bookmark.name.clone();
            let location = bookmark.target.editable_payload();
            let drop_cb = callback.clone();
            let edit_cb = callback.clone();
            let context_cb = callback.clone();
            let select_cb = ui_callback.clone();
            let select =
                crate::bookmark_manager_window::BookmarkManagerUiAction::SelectBookmark(id);
            let selected = ui.selected_bookmark == Some(id);
            div()
                .id(("bookmark-row", id.as_u128() as u64))
                .role(Role::ListItem)
                .aria_label(format!("Bookmark {}", bookmark.name))
                .flex()
                .items_center()
                .h(px(manager_row_height))
                .px(px(8.0))
                .cursor_move()
                .border_b(px(1.0))
                .border_color(tokens.theme.colors.divider.to_gpui())
                .when(selected, |row| {
                    row.bg(tokens.theme.colors.file_row_selected_active.to_gpui())
                        .text_color(tokens.theme.colors.file_row_selected_text.to_gpui())
                })
                .hover(|style| style.bg(tokens.theme.colors.control_hover.to_gpui()))
                .on_drag(
                    BookmarkDrag {
                        id,
                        label: drag_label,
                    },
                    |drag, _, _, cx| {
                        cx.new(|_| BookmarkDragPreview {
                            label: drag.label.clone(),
                        })
                    },
                )
                .when_some(drop_cb, move |element, cb| {
                    element.on_drop(move |drag: &BookmarkDrag, window, cx| {
                        cb(
                            &ExplorerAction::MoveBookmark {
                                id: drag.id,
                                destination: sibling_index,
                            },
                            window,
                            cx,
                        );
                        cx.stop_propagation();
                    })
                })
                .when_some(context_cb, move |element, cb| {
                    element.on_mouse_down(MouseButton::Right, move |event, window, cx| {
                        cx.stop_propagation();
                        cb(
                            &ExplorerAction::OpenBookmarkContextMenu {
                                id,
                                x: f32::from(event.position.x),
                                y: f32::from(event.position.y),
                            },
                            window,
                            cx,
                        );
                    })
                })
                .on_mouse_up(MouseButton::Right, |_, _, cx| cx.stop_propagation())
                .when_some(select_cb, move |element, cb| {
                    element.on_mouse_down(MouseButton::Left, move |_, window, cx| {
                        cb(&select, window, cx)
                    })
                })
                .child(
                    div()
                        .w_2_5()
                        .overflow_hidden()
                        .child(bookmark_label(&bookmark.target, bookmark.name.clone())),
                )
                .child(div().w_1_5().child(""))
                .child(div().flex_1().overflow_hidden().child(location))
                .when_some(edit_cb, move |element, cb| {
                    element.on_click(move |event, window, cx| {
                        if event.click_count() == 2 {
                            cb(&edit, window, cx);
                        }
                    })
                })
        })
        .collect::<Vec<_>>();
    let back_cb = ui_callback.clone();
    let forward_cb = ui_callback.clone();
    let manage_cb = ui_callback.clone();
    let view_cb = ui_callback.clone();
    let can_back = ui.history_index > 0;
    let can_forward = ui.history_index + 1 < ui.history.len();
    let import = ExplorerAction::ImportBookmarksFromClipboard;
    let import_cb = callback.clone();
    let backup = ExplorerAction::BackupBookmarksToClipboard;
    let backup_cb = callback.clone();
    let transfer_cb = ui_callback.clone();
    let manage_add_cb = callback.clone();
    let manage_parent_id = ui.selected_folder;
    let manage_edit_cb = callback.clone();
    let manage_remove_cb = callback.clone();
    let view_name_cb = ui_callback.clone();
    let view_location_cb = ui_callback.clone();
    let view_density_cb = ui_callback.clone();
    let all_bookmarks_cb = ui_callback.clone();
    let root_bookmarks_cb = ui_callback.clone();
    let toolbar_bookmarks_cb = ui_callback.clone();
    let menu_bookmarks_cb = ui_callback.clone();
    let column_name_cb = ui_callback.clone();
    let column_tags_cb = ui_callback.clone();
    let column_location_cb = ui_callback.clone();
    let dismiss_menu_cb = ui_callback.clone();
    div()
        .id("bookmark-manager-content")
        .size_full()
        .relative()
        .overflow_hidden()
        .flex()
        .flex_col()
        .font_family(tokens.typography.family.primary)
        .text_size(px(14.0))
        .bg(tokens.theme.colors.surface.to_gpui())
        .child(
            div()
                .id("bookmark-manager-toolbar")
                .w_full()
                .h(px(54.0))
                .flex_none()
                .flex()
                .items_center()
                .gap(px(14.0))
                .px(px(14.0))
                .border_b(px(1.0))
                .border_color(tokens.theme.colors.divider.to_gpui())
                .bg(tokens.theme.colors.subtle_surface.to_gpui())
                .child(
                    div()
                        .id("bookmark-manager-back")
                        .role(Role::Button)
                        .text_size(px(22.0))
                        .text_color(if can_back { tokens.theme.colors.text_primary.to_gpui() } else { tokens.theme.colors.text_disabled.to_gpui() })
                        .when(can_back, |button| button.cursor_pointer())
                        .child("‹")
                        .when_some(back_cb, |button, cb| button.on_click(move |_, window, cx| cb(&crate::bookmark_manager_window::BookmarkManagerUiAction::Back, window, cx))),
                )
                .child(
                    div()
                        .id("bookmark-manager-forward")
                        .role(Role::Button)
                        .text_size(px(22.0))
                        .text_color(if can_forward { tokens.theme.colors.text_primary.to_gpui() } else { tokens.theme.colors.text_disabled.to_gpui() })
                        .when(can_forward, |button| button.cursor_pointer())
                        .child("›")
                        .when_some(forward_cb, |button, cb| button.on_click(move |_, window, cx| cb(&crate::bookmark_manager_window::BookmarkManagerUiAction::Forward, window, cx))),
                )
                .child(
                    div()
                        .id("bookmark-manager-manage-menu")
                        .role(Role::Button)
                        .cursor_pointer()
                        .flex()
                        .items_center()
                        .gap(px(6.0))
                        .px(px(6.0))
                        .py(px(5.0))
                        .rounded(px(4.0))
                        .hover(|style| style.bg(tokens.theme.colors.control_hover.to_gpui()))
                        .child("⚙")
                        .child("管理 (O)⌄")
                        .when_some(manage_cb, move |e, cb| {
                            e.on_click(move |_, w, cx| cb(&crate::bookmark_manager_window::BookmarkManagerUiAction::ToggleMenu(crate::bookmark_manager_window::BookmarkManagerMenu::Manage), w, cx))
                        }),
                )
                .child(
                    div()
                        .id("bookmark-manager-view-toggle")
                        .role(Role::Button)
                        .aria_label("Toggle compact bookmark view")
                        .cursor_pointer()
                        .child("☷  檢視 (V)⌄")
                        .when_some(view_cb, move |e, cb| {
                            e.on_click(move |_, w, cx| cb(&crate::bookmark_manager_window::BookmarkManagerUiAction::ToggleMenu(crate::bookmark_manager_window::BookmarkManagerMenu::View), w, cx))
                        }),
                )
                .child(
                    div()
                        .id("bookmark-manager-transfer-menu")
                        .role(Role::Button)
                        .aria_label("Import and backup bookmarks")
                        .cursor_pointer()
                        .child("↕  匯入及備份 (I)⌄")
                        .when_some(transfer_cb, move |e, cb| {
                            e.on_click(move |_, w, cx| cb(&crate::bookmark_manager_window::BookmarkManagerUiAction::ToggleMenu(crate::bookmark_manager_window::BookmarkManagerMenu::Transfer), w, cx))
                        }),
                )
                .child(div().flex_1())
                .child(
                    div()
                        .id("bookmark-manager-search")
                        .w(px(300.0))
                        .h(px(34.0))
                        .flex()
                        .items_center()
                        .gap(px(8.0))
                        .px(px(12.0))
                        .rounded(px(16.0))
                        .border(px(1.0))
                        .border_color(tokens.theme.colors.divider.to_gpui())
                        .text_color(tokens.theme.colors.text_disabled.to_gpui())
                        .child("⌕")
                        .when_some(search_input, |search, input| {
                            search.child(
                                text_input("bookmark-manager-search-input")
                                    .state(input)
                                    .multiline(false)
                                    .placeholder("搜尋書籤")
                                    .caret_blink_interval_500ms()
                                    .flex_1()
                                    .h_full()
                                    .px(px(2.0))
                                    .text_size(px(14.0))
                                    .text_color(input_text)
                                    .selection_color(input_selection.into())
                                    .selection_text_color(input_selection_text.into())
                                    .caret_color(input_caret.into()),
                            )
                        }),
                ),
        )
        .when_some(state.bookmark_notice(), |element, notice| {
            element.child(
                div()
                    .id("bookmark-manager-notice")
                    .flex_none()
                    .px(px(12.0))
                    .py(px(5.0))
                    .bg(tokens.theme.colors.subtle_surface.to_gpui())
                    .child(notice.to_owned()),
            )
        })
        .when(ui.open_menu.is_some(), |element| {
            element.child(
                deferred(
                    div()
                        .id("bookmark-manager-menu-dismiss-overlay")
                        .absolute()
                        .inset_0()
                        .when_some(dismiss_menu_cb, |overlay, cb| {
                            overlay.on_mouse_down(MouseButton::Left, move |_, window, cx| {
                                cb(&crate::bookmark_manager_window::BookmarkManagerUiAction::DismissMenu, window, cx)
                            })
                        }),
                )
                .with_priority(210),
            )
        })
        .when(
            ui.open_menu == Some(crate::bookmark_manager_window::BookmarkManagerMenu::Manage),
            |element| {
                element.child(
                    deferred(
                        div()
                            .id("bookmark-manager-manage-popup")
                            .absolute()
                            .top(px(48.0))
                            .left(px(70.0))
                            .w(px(230.0))
                            .p(px(6.0))
                            .rounded(px(6.0))
                            .border(px(1.0))
                            .border_color(tokens.theme.colors.divider.to_gpui())
                            .bg(tokens.theme.colors.menu_fill.to_gpui())
                            .child(
                                div()
                                    .id("bookmark-manager-command-add-folder")
                                    .role(Role::MenuItem)
                                    .cursor_pointer()
                                    .px(px(10.0))
                                    .py(px(6.0))
                                    .child("新增資料夾")
                                    .when_some(manage_add_cb, |row, cb| {
                                        row.on_click(move |_, window, cx| {
                                            cb(&ExplorerAction::AddBookmarkFolder { parent_id: manage_parent_id }, window, cx)
                                        })
                                    }),
                            )
                            .when_some(ui.selected_bookmark, |menu, id| {
                                let edit = ExplorerAction::EditBookmark { id };
                                let remove = ExplorerAction::RequestRemoveBookmark { id };
                                menu.child(
                                    div()
                                        .id("bookmark-manager-command-edit")
                                        .role(Role::MenuItem)
                                        .cursor_pointer()
                                        .px(px(10.0))
                                        .py(px(6.0))
                                        .child("編輯書籤")
                                        .when_some(manage_edit_cb.clone(), |row, cb| row.on_click(move |_, window, cx| cb(&edit, window, cx))),
                                )
                                .child(
                                    div()
                                        .id("bookmark-manager-command-remove")
                                        .role(Role::MenuItem)
                                        .cursor_pointer()
                                        .px(px(10.0))
                                        .py(px(6.0))
                                        .text_color(tokens.theme.colors.danger.to_gpui())
                                        .child("刪除書籤")
                                        .when_some(manage_remove_cb.clone(), |row, cb| row.on_click(move |_, window, cx| cb(&remove, window, cx))),
                                )
                            })
                            .when_some(ui.selected_folder, |menu, id| {
                                let edit = ExplorerAction::EditBookmarkFolder { id };
                                let remove = ExplorerAction::RemoveBookmarkFolder { id };
                                menu.child(
                                    div().id("bookmark-manager-command-edit-folder").role(Role::MenuItem).cursor_pointer().px(px(10.0)).py(px(6.0)).child("重新命名資料夾").when_some(manage_edit_cb.clone(), |row, cb| row.on_click(move |_, window, cx| cb(&edit, window, cx))),
                                ).child(
                                    div().id("bookmark-manager-command-remove-folder").role(Role::MenuItem).cursor_pointer().px(px(10.0)).py(px(6.0)).text_color(tokens.theme.colors.danger.to_gpui()).child("刪除資料夾").when_some(manage_remove_cb.clone(), |row, cb| row.on_click(move |_, window, cx| cb(&remove, window, cx))),
                                )
                            }),
                    )
                    .with_priority(220),
                )
            },
        )
        .when(
            ui.open_menu == Some(crate::bookmark_manager_window::BookmarkManagerMenu::View),
            |element| {
                element.child(
                    deferred(
                        div()
                            .id("bookmark-manager-view-popup")
                            .absolute()
                            .top(px(48.0))
                            .left(px(190.0))
                            .w(px(210.0))
                            .p(px(6.0))
                            .rounded(px(6.0))
                            .border(px(1.0))
                            .border_color(tokens.theme.colors.divider.to_gpui())
                            .bg(tokens.theme.colors.menu_fill.to_gpui())
                            .child(bookmark_manager_ui_menu_row("bookmark-manager-sort-name", "依名稱排序", crate::bookmark_manager_window::BookmarkManagerUiAction::Sort(crate::bookmark_manager_window::BookmarkManagerSortColumn::Name), view_name_cb))
                            .child(bookmark_manager_ui_menu_row("bookmark-manager-sort-location", "依網址排序", crate::bookmark_manager_window::BookmarkManagerUiAction::Sort(crate::bookmark_manager_window::BookmarkManagerSortColumn::Location), view_location_cb))
                            .child(bookmark_manager_ui_menu_row("bookmark-manager-toggle-density", "切換緊湊檢視", crate::bookmark_manager_window::BookmarkManagerUiAction::ToggleDensity, view_density_cb)),
                    )
                    .with_priority(220),
                )
            },
        )
        .when(
            ui.open_menu == Some(crate::bookmark_manager_window::BookmarkManagerMenu::Transfer),
            |element| {
                element.child(
                    deferred(
                        div()
                            .id("bookmark-manager-transfer-popup")
                            .absolute()
                            .top(px(48.0))
                            .left(px(300.0))
                            .w(px(260.0))
                            .p(px(6.0))
                            .rounded(px(6.0))
                            .border(px(1.0))
                            .border_color(tokens.theme.colors.divider.to_gpui())
                            .bg(tokens.theme.colors.menu_fill.to_gpui())
                            .child(
                                div().id("bookmark-manager-import").role(Role::MenuItem).cursor_pointer().px(px(10.0)).py(px(6.0)).child("從剪貼簿匯入").when_some(import_cb, |row, cb| row.on_click(move |_, window, cx| cb(&import, window, cx))),
                            )
                            .child(
                                div().id("bookmark-manager-backup").role(Role::MenuItem).cursor_pointer().px(px(10.0)).py(px(6.0)).child("備份到剪貼簿").when_some(backup_cb, |row, cb| row.on_click(move |_, window, cx| cb(&backup, window, cx))),
                            ),
                    )
                    .with_priority(220),
                )
            },
        )
        .child(
            div()
                .id("bookmark-manager")
                .flex_1()
                .min_h_0()
                .flex()
                .child(
                    div()
                        .id("bookmark-manager-tree")
                        .w(px(260.0))
                        .h_full()
                        .flex_none()
                        .overflow_y_scroll()
                        .py(px(8.0))
                        .border_r(px(1.0))
                        .border_color(tokens.theme.colors.divider.to_gpui())
                        .bg(tokens.theme.colors.subtle_surface.to_gpui())
                        .child(
                            div()
                                .id("bookmark-manager-all-bookmarks")
                                .role(Role::Button)
                                .cursor_pointer()
                                .h(px(32.0))
                                .px(px(12.0))
                                .flex()
                                .items_center()
                                .gap(px(8.0))
                                .child("⌄  ◷  瀏覽紀錄"),
                        )
                        .child(
                            div()
                                .id("bookmark-manager-toolbar-bookmarks")
                                .role(Role::Button)
                                .cursor_pointer()
                                .h(px(32.0))
                                .px(px(12.0))
                                .flex()
                                .items_center()
                                .gap(px(8.0))
                                .child("   ⇩  下載項目"),
                        )
                        .child(
                            div()
                                .h(px(32.0))
                                .px(px(12.0))
                                .flex()
                                .items_center()
                                .gap(px(8.0))
                                .child("▸  ◇  標籤"),
                        )
                        .child(
                            div()
                                .h(px(32.0))
                                .px(px(12.0))
                                .flex()
                                .items_center()
                                .gap(px(8.0))
                                .font_weight(FontWeight::SEMIBOLD)
                                .when(matches!(ui.location, crate::bookmark_manager_window::BookmarkManagerLocation::AllBookmarks), |row| row.bg(tokens.theme.colors.control_pressed.to_gpui()))
                                .child("⌄  ★  所有書籤")
                                .when_some(all_bookmarks_cb, |row, cb| row.on_mouse_down(MouseButton::Left, move |_, window, cx| cb(&crate::bookmark_manager_window::BookmarkManagerUiAction::Navigate(crate::bookmark_manager_window::BookmarkManagerLocation::AllBookmarks), window, cx))),
                        )
                        .child(
                            div()
                                .id("bookmark-manager-root-bookmarks")
                                .role(Role::Button)
                                .cursor_pointer()
                                .h(px(32.0))
                                .pl(px(38.0))
                                .flex()
                                .items_center()
                                .child("▸  ▣  書籤工具列")
                                .when_some(toolbar_bookmarks_cb, |row, cb| row.on_mouse_down(MouseButton::Left, move |_, window, cx| cb(&crate::bookmark_manager_window::BookmarkManagerUiAction::Navigate(crate::bookmark_manager_window::BookmarkManagerLocation::Root), window, cx))),
                        )
                        .child(
                            div()
                                .id("bookmark-manager-menu-bookmarks")
                                .role(Role::Button)
                                .cursor_pointer()
                                .h(px(32.0))
                                .pl(px(38.0))
                                .flex()
                                .items_center()
                                .child("▸  ▤  書籤選單")
                                .when_some(menu_bookmarks_cb, |row, cb| row.on_mouse_down(MouseButton::Left, move |_, window, cx| cb(&crate::bookmark_manager_window::BookmarkManagerUiAction::Navigate(crate::bookmark_manager_window::BookmarkManagerLocation::Root), window, cx))),
                        )
                        .child(
                            div()
                                .h(px(32.0))
                                .pl(px(38.0))
                                .flex()
                                .items_center()
                                .when(matches!(ui.location, crate::bookmark_manager_window::BookmarkManagerLocation::Root), |row| row.bg(tokens.theme.colors.control_pressed.to_gpui()))
                                .child("⌄  📁  其他書籤")
                                .when_some(root_bookmarks_cb, |row, cb| row.on_mouse_down(MouseButton::Left, move |_, window, cx| cb(&crate::bookmark_manager_window::BookmarkManagerUiAction::Navigate(crate::bookmark_manager_window::BookmarkManagerLocation::Root), window, cx))),
                        )
                        .children(folder_rows),
                )
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .h_full()
                        .flex()
                        .flex_col()
                        .child(
                            div()
                                .id("bookmark-manager-columns")
                                .h(px(32.0))
                                .flex_none()
                                .flex()
                                .items_center()
                                .border_b(px(1.0))
                                .border_color(tokens.theme.colors.divider.to_gpui())
                                .child(div().w_2_5().px(px(8.0)).cursor_pointer().child("名稱").when_some(column_name_cb, |header, cb| header.on_mouse_down(MouseButton::Left, move |_, window, cx| cb(&crate::bookmark_manager_window::BookmarkManagerUiAction::Sort(crate::bookmark_manager_window::BookmarkManagerSortColumn::Name), window, cx))))
                                .child(
                                    div()
                                        .w_1_5()
                                        .px(px(8.0))
                                        .border_l(px(1.0))
                                        .border_color(tokens.theme.colors.divider.to_gpui())
                                        .cursor_pointer()
                                        .child("標籤")
                                        .when_some(column_tags_cb, |header, cb| header.on_mouse_down(MouseButton::Left, move |_, window, cx| cb(&crate::bookmark_manager_window::BookmarkManagerUiAction::Sort(crate::bookmark_manager_window::BookmarkManagerSortColumn::Tags), window, cx))),
                                )
                                .child(
                                    div()
                                        .flex_1()
                                        .px(px(8.0))
                                        .border_l(px(1.0))
                                        .border_color(tokens.theme.colors.divider.to_gpui())
                                        .cursor_pointer()
                                        .child("網址")
                                        .when_some(column_location_cb, |header, cb| header.on_mouse_down(MouseButton::Left, move |_, window, cx| cb(&crate::bookmark_manager_window::BookmarkManagerUiAction::Sort(crate::bookmark_manager_window::BookmarkManagerSortColumn::Location), window, cx))),
                                ),
                        )
                        .child(
                            div()
                                .id("bookmark-manager-list")
                                .flex_1()
                                .min_h_0()
                                .overflow_y_scroll()
                                .children(rows),
                        ),
                ),
        )
        .child(
            div()
                .id("bookmark-manager-details")
                .h(px(96.0))
                .flex_none()
                .flex()
                .flex_col()
                .justify_center()
                .gap(px(10.0))
                .px(px(10.0))
                .border_t(px(1.0))
                .border_color(tokens.theme.colors.divider.to_gpui())
                .bg(tokens.theme.colors.subtle_surface.to_gpui())
                .child(
                    div().w_full().flex().items_center().gap(px(10.0)).child("名稱 (N)").when_some(detail_input, |row, input| {
                    row.child(
                        text_input("bookmark-manager-detail-name-input")
                            .state(input)
                            .multiline(false)
                            .caret_blink_interval_500ms()
                            .flex_1()
                            .h(px(34.0))
                            .px(px(10.0))
                            .rounded(px(15.0))
                            .border(px(1.0))
                            .border_color(tokens.theme.colors.divider.to_gpui())
                            .bg(tokens.theme.colors.surface.to_gpui())
                            .text_color(input_text)
                            .selection_color(input_selection.into())
                            .selection_text_color(input_selection_text.into())
                            .caret_color(input_caret.into()),
                    )
                }))
                .child(
                    div().w_full().flex().items_center().gap(px(10.0)).child("網址 (L)").when_some(detail_location_input, |row, input| {
                    row.child(
                        text_input("bookmark-manager-detail-location-input")
                            .state(input)
                            .multiline(false)
                            .caret_blink_interval_500ms()
                            .flex_1()
                            .h(px(34.0))
                            .px(px(10.0))
                            .rounded(px(15.0))
                            .border(px(1.0))
                            .border_color(tokens.theme.colors.divider.to_gpui())
                            .bg(tokens.theme.colors.surface.to_gpui())
                            .text_color(input_text)
                            .selection_color(input_selection.into())
                            .selection_text_color(input_selection_text.into())
                            .caret_color(input_caret.into()),
                    )
                })),
        )
}

fn bookmark_manager_ui_menu_row(
    id: &'static str,
    label: &'static str,
    action: crate::bookmark_manager_window::BookmarkManagerUiAction,
    callback: Option<crate::bookmark_manager_window::BookmarkManagerUiCallback>,
) -> impl IntoElement {
    div()
        .id(id)
        .role(Role::MenuItem)
        .cursor_pointer()
        .px(px(10.0))
        .py(px(6.0))
        .child(label)
        .when_some(callback, move |row, cb| {
            row.on_click(move |_, window, cx| cb(&action, window, cx))
        })
}

fn bookmark_toolbar_context_menu(
    tokens: UiTokens,
    menu: crate::state::BookmarkToolbarContextMenuState,
    window_width: f32,
    window_height: f32,
    callback: Option<ActionCallback>,
) -> impl IntoElement {
    let close = ExplorerAction::CloseBookmarkToolbarContextMenu;
    let close_cb = callback.clone();
    let close_right = close.clone();
    let close_right_cb = callback.clone();
    let mut commands = vec![
        (
            "新增資料夾",
            ExplorerAction::AddBookmarkFolder {
                parent_id: menu.parent_id,
            },
            false,
        ),
        (
            "新增資料夾路徑書籤…",
            ExplorerAction::AddPathBookmark {
                parent_id: menu.parent_id,
                kind: BookmarkPathKind::Folder,
            },
            false,
        ),
        (
            "新增檔案路徑書籤…",
            ExplorerAction::AddPathBookmark {
                parent_id: menu.parent_id,
                kind: BookmarkPathKind::File,
            },
            false,
        ),
        ("書籤管理員…", ExplorerAction::ToggleBookmarkManager, false),
    ];
    if let Some(id) = menu.parent_id {
        commands.extend([
            (
                "重新命名資料夾…",
                ExplorerAction::EditBookmarkFolder { id },
                false,
            ),
            (
                "刪除資料夾",
                ExplorerAction::RemoveBookmarkFolder { id },
                true,
            ),
        ]);
    }
    let rows = commands
        .into_iter()
        .enumerate()
        .map(|(index, (label, action, danger))| {
            let callback = callback.clone();
            div()
                .id(format!("bookmark-toolbar-context-command-{index}"))
                .role(Role::MenuItem)
                .aria_label(label)
                .cursor_pointer()
                .px(px(12.0))
                .py(px(7.0))
                .rounded(px(4.0))
                .text_color(if danger {
                    tokens.theme.colors.danger.to_gpui()
                } else {
                    tokens.theme.colors.text_primary.to_gpui()
                })
                .hover(|style| style.bg(tokens.theme.colors.control_hover.to_gpui()))
                .child(label)
                .when_some(callback, move |element, cb| {
                    element.on_click(move |_, window, cx| cb(&action, window, cx))
                })
        });
    let left = menu.x.min((window_width - 260.0).max(0.0));
    let top = menu.y.min((window_height - 300.0).max(0.0));
    div()
        .id("bookmark-toolbar-context-overlay")
        .absolute()
        .inset_0()
        .on_mouse_down(MouseButton::Left, move |_, window, cx| {
            if let Some(cb) = close_cb.as_ref() {
                cb(&close, window, cx);
            }
        })
        .on_mouse_down(MouseButton::Right, move |_, window, cx| {
            if let Some(cb) = close_right_cb.as_ref() {
                cb(&close_right, window, cx);
            }
        })
        .child(
            deferred(
                div()
                    .id("bookmark-toolbar-context-menu")
                    .role(Role::Menu)
                    .aria_label("Bookmark toolbar context menu")
                    .absolute()
                    .left(px(left))
                    .top(px(top))
                    .min_w(px(252.0))
                    .p(px(6.0))
                    .rounded(px(6.0))
                    .border(px(1.0))
                    .border_color(tokens.theme.colors.divider.to_gpui())
                    .bg(tokens.theme.colors.menu_fill.to_gpui())
                    .shadow_lg()
                    .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                    .children(rows),
            )
            .with_priority(310),
        )
}

fn bookmark_context_menu(
    tokens: UiTokens,
    state: &AppViewState,
    menu: crate::state::BookmarkContextMenuState,
    window_width: f32,
    window_height: f32,
    callback: Option<ActionCallback>,
) -> impl IntoElement {
    let close = ExplorerAction::CloseBookmarkContextMenu;
    let close_cb = callback.clone();
    let close_right = close.clone();
    let close_right_cb = callback.clone();
    let is_folder = state
        .bookmarks()
        .entries()
        .iter()
        .find(|bookmark| bookmark.id == menu.id)
        .is_some_and(|bookmark| bookmark.target.is_folder());
    let mut commands = vec![(
        "開啟",
        ExplorerAction::ActivateBookmark { id: menu.id },
        false,
    )];
    if is_folder {
        commands.push((
            "在新分頁開啟",
            ExplorerAction::OpenBookmarkInNewTab { id: menu.id },
            false,
        ));
    }
    commands.extend([
        (
            "編輯名稱與路徑…",
            ExplorerAction::EditBookmark { id: menu.id },
            false,
        ),
        (
            "刪除書籤",
            ExplorerAction::RequestRemoveBookmark { id: menu.id },
            true,
        ),
    ]);
    let rows = commands
        .into_iter()
        .enumerate()
        .map(|(index, (label, action, danger))| {
            let callback = callback.clone();
            div()
                .id(format!("bookmark-context-command-{index}"))
                .role(Role::MenuItem)
                .aria_label(label)
                .cursor_pointer()
                .px(px(12.0))
                .py(px(7.0))
                .rounded(px(4.0))
                .text_color(if danger {
                    tokens.theme.colors.danger.to_gpui()
                } else {
                    tokens.theme.colors.text_primary.to_gpui()
                })
                .hover(|style| style.bg(tokens.theme.colors.control_hover.to_gpui()))
                .child(label)
                .when_some(callback, move |element, cb| {
                    element.on_click(move |_, window, cx| cb(&action, window, cx))
                })
        });
    let left = menu.x.min((window_width - 260.0).max(0.0));
    let top = menu.y.min((window_height - 260.0).max(0.0));
    div()
        .id("bookmark-context-overlay")
        .absolute()
        .inset_0()
        .on_mouse_down(MouseButton::Left, move |_, window, cx| {
            if let Some(cb) = close_cb.as_ref() {
                cb(&close, window, cx);
            }
        })
        .on_mouse_down(MouseButton::Right, move |_, window, cx| {
            if let Some(cb) = close_right_cb.as_ref() {
                cb(&close_right, window, cx);
            }
        })
        .child(
            deferred(
                div()
                    .id("bookmark-context-menu")
                    .role(Role::Menu)
                    .aria_label("Bookmark context menu")
                    .absolute()
                    .left(px(left))
                    .top(px(top))
                    .min_w(px(252.0))
                    .p(px(6.0))
                    .rounded(px(6.0))
                    .border(px(1.0))
                    .border_color(tokens.theme.colors.divider.to_gpui())
                    .bg(tokens.theme.colors.menu_fill.to_gpui())
                    .shadow_lg()
                    .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                    .children(rows),
            )
            .with_priority(311),
        )
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum RemoteMenuPlacement {
    CommandStrip,
    Text,
}

#[derive(Clone, Debug, PartialEq)]
struct RemoteMenuCommand {
    label: &'static str,
    action: ExplorerAction,
    icon: Option<ExplorerIcon>,
    placement: RemoteMenuPlacement,
    danger: bool,
}

fn remote_menu_commands(
    background: bool,
    paste_available: bool,
    item_row_index: Option<usize>,
    item_is_container: bool,
) -> Vec<RemoteMenuCommand> {
    let mut commands = Vec::new();
    if background {
        commands.push(RemoteMenuCommand {
            label: "新增資料夾",
            action: ExplorerAction::CreateFolder,
            icon: Some(ExplorerIcon::New),
            placement: RemoteMenuPlacement::Text,
            danger: false,
        });
        commands.push(RemoteMenuCommand {
            label: "新增捷徑",
            action: ExplorerAction::CreateRemoteSymlink,
            icon: Some(ExplorerIcon::Add),
            placement: RemoteMenuPlacement::Text,
            danger: false,
        });
        commands.push(RemoteMenuCommand {
            label: "內容",
            action: ExplorerAction::ShowRemoteBackgroundProperties,
            icon: Some(ExplorerIcon::Details),
            placement: RemoteMenuPlacement::Text,
            danger: false,
        });
        if paste_available {
            commands.push(RemoteMenuCommand {
                label: "貼上",
                action: ExplorerAction::Paste,
                icon: Some(ExplorerIcon::Paste),
                placement: RemoteMenuPlacement::Text,
                danger: false,
            });
        }
        return commands;
    }

    commands.push(RemoteMenuCommand {
        label: "開啟",
        action: ExplorerAction::OpenFocused,
        icon: None,
        placement: RemoteMenuPlacement::Text,
        danger: false,
    });
    if item_is_container && let Some(row_index) = item_row_index {
        commands.push(RemoteMenuCommand {
            label: "在新分頁開啟",
            action: ExplorerAction::OpenItem {
                row_index,
                new_tab: true,
            },
            icon: None,
            placement: RemoteMenuPlacement::Text,
            danger: false,
        });
        commands.push(RemoteMenuCommand {
            label: "新增捷徑",
            action: ExplorerAction::CreateRemoteSymlinkToFolder { row_index },
            icon: Some(ExplorerIcon::Add),
            placement: RemoteMenuPlacement::Text,
            danger: false,
        });
    }
    commands.extend([
        RemoteMenuCommand {
            label: "下載到下載資料夾",
            action: ExplorerAction::DownloadSelectedToDownloads,
            icon: Some(ExplorerIcon::Add),
            placement: RemoteMenuPlacement::CommandStrip,
            danger: false,
        },
        RemoteMenuCommand {
            label: "剪下",
            action: ExplorerAction::CutSelected,
            icon: Some(ExplorerIcon::Cut),
            placement: RemoteMenuPlacement::CommandStrip,
            danger: false,
        },
        RemoteMenuCommand {
            label: "複製",
            action: ExplorerAction::CopySelected,
            icon: Some(ExplorerIcon::Copy),
            placement: RemoteMenuPlacement::CommandStrip,
            danger: false,
        },
        RemoteMenuCommand {
            label: "重新命名",
            action: ExplorerAction::BeginRenameFocused,
            icon: Some(ExplorerIcon::Rename),
            placement: RemoteMenuPlacement::CommandStrip,
            danger: false,
        },
        RemoteMenuCommand {
            label: "永久刪除…",
            action: ExplorerAction::RecycleDeleteSelected,
            icon: Some(ExplorerIcon::Delete),
            placement: RemoteMenuPlacement::CommandStrip,
            danger: true,
        },
    ]);
    if paste_available {
        commands.push(RemoteMenuCommand {
            label: "貼上",
            action: ExplorerAction::Paste,
            icon: Some(ExplorerIcon::Paste),
            placement: RemoteMenuPlacement::Text,
            danger: false,
        });
    }
    commands.extend([
        RemoteMenuCommand {
            label: "複製遠端路徑",
            action: ExplorerAction::CopySelectedPaths,
            icon: Some(ExplorerIcon::Copy),
            placement: RemoteMenuPlacement::CommandStrip,
            danger: false,
        },
        RemoteMenuCommand {
            label: "加入書籤",
            action: ExplorerAction::AddSelectedToBookmarks,
            icon: Some(ExplorerIcon::Pin),
            placement: RemoteMenuPlacement::CommandStrip,
            danger: false,
        },
        RemoteMenuCommand {
            label: "內容",
            action: ExplorerAction::ShowPropertiesSelected,
            icon: Some(ExplorerIcon::Details),
            placement: RemoteMenuPlacement::CommandStrip,
            danger: false,
        },
    ]);
    commands
}

fn remote_menu_icon(
    id: String,
    icon: ExplorerIcon,
    danger: bool,
    icon_size: f32,
    tokens: UiTokens,
) -> impl IntoElement {
    let color = if danger {
        tokens.theme.colors.danger
    } else {
        tokens.theme.colors.text_primary
    };
    div()
        .id(id)
        .w(px(icon_size))
        .h(px(icon_size))
        .flex_none()
        .child(
            svg()
                .path(icon.asset_path())
                .size_full()
                .text_color(color.to_gpui()),
        )
}

#[derive(Clone, Debug, PartialEq)]
struct ContextMenuVisualTokens {
    width: f32,
    maximum_height: f32,
    row_height: f32,
    font_size: f32,
    line_height: f32,
    font_weight: FontWeight,
    icon_gutter: f32,
    icon_size: f32,
    icon_left: f32,
    outer_padding: f32,
    divider_left_inset: f32,
    divider_right_inset: f32,
    surface: gpui::Rgba,
    border: gpui::Rgba,
    text: gpui::Rgba,
    danger: gpui::Rgba,
    hover: gpui::Rgba,
    pressed: gpui::Rgba,
    shadow: BoxShadow,
}

fn context_menu_visual_tokens(tokens: UiTokens) -> ContextMenuVisualTokens {
    let light = matches!(tokens.theme.mode, crate::theme::ThemeMode::Light);
    let high_contrast = tokens.theme.high_contrast_active;
    let metrics = explorer_model::WINDOWS_CONTEXT_MENU_VISUAL_METRICS;
    let palette = if light {
        explorer_model::WINDOWS_CONTEXT_MENU_LIGHT_PALETTE
    } else {
        explorer_model::WINDOWS_CONTEXT_MENU_DARK_PALETTE
    };
    let color = |rgb: [u8; 3]| crate::theme::Rgba8::opaque(rgb[0], rgb[1], rgb[2]).to_gpui();
    ContextMenuVisualTokens {
        width: f32::from(metrics.minimum_width),
        maximum_height: 420.0,
        row_height: f32::from(metrics.row_height),
        font_size: tokens.typography.menu.size.value(),
        line_height: tokens.typography.menu.line_height.value(),
        font_weight: FontWeight(f32::from(tokens.typography.menu.weight)),
        icon_gutter: f32::from(metrics.icon_gutter),
        icon_size: f32::from(metrics.icon_size),
        icon_left: f32::from(metrics.icon_left),
        outer_padding: f32::from(metrics.outer_padding),
        divider_left_inset: f32::from(metrics.icon_gutter),
        divider_right_inset: f32::from(metrics.divider_right_inset),
        surface: if !high_contrast {
            color(palette.surface)
        } else {
            tokens.theme.colors.menu_fill.to_gpui()
        },
        border: if !high_contrast {
            color(palette.divider)
        } else {
            tokens.theme.colors.divider.to_gpui()
        },
        text: if high_contrast {
            tokens.theme.colors.text_primary.to_gpui()
        } else {
            color(palette.text)
        },
        danger: tokens.theme.colors.danger.to_gpui(),
        hover: if high_contrast {
            tokens.theme.colors.control_hover.to_gpui()
        } else {
            color(palette.hover)
        },
        pressed: if high_contrast {
            tokens.theme.colors.control_pressed.to_gpui()
        } else {
            color(palette.hover)
        },
        // The native classic popup casts no visible top/left shadow. Its soft shadow is biased
        // down and slightly right, with a shorter right extent than bottom extent.
        shadow: BoxShadow {
            color: hsla(
                0.0,
                0.0,
                0.0,
                if high_contrast {
                    0.0
                } else if light {
                    0.2
                } else {
                    0.34
                },
            ),
            offset: point(px(3.0), px(9.0)),
            blur_radius: px(12.0),
            spread_radius: px(-4.0),
            inset: false,
        },
    }
}

fn remote_menu_text_command(
    index: usize,
    command: RemoteMenuCommand,
    tokens: UiTokens,
    callback: Option<ActionCallback>,
) -> gpui::AnyElement {
    let visual = context_menu_visual_tokens(tokens);
    let label = command.label;
    let action = command.action;
    let icon = command.icon;
    let danger = command.danger;
    div()
        .id(format!("remote-context-text-command-{index}"))
        .debug_selector(move || format!("remote-context-text-{label}"))
        .role(Role::MenuItem)
        .aria_label(label)
        .focusable()
        .tab_stop(true)
        .h(px(visual.row_height))
        .flex()
        .items_center()
        .text_size(px(visual.font_size))
        .line_height(px(visual.line_height))
        .font_weight(visual.font_weight)
        .text_color(if danger { visual.danger } else { visual.text })
        .cursor_pointer()
        .hover(move |style| style.bg(visual.hover))
        .active(move |style| style.bg(visual.pressed))
        .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
        .when_some(callback, move |element, cb| {
            let accessibility_cb = cb.clone();
            let accessibility_action = action.clone();
            let keyboard_cb = cb.clone();
            let keyboard_action = action.clone();
            element
                .on_key_down(move |event, window, cx| {
                    if matches!(event.keystroke.key.as_str(), "enter" | "space") {
                        cx.stop_propagation();
                        keyboard_cb(&keyboard_action, window, cx);
                    }
                })
                .on_click(move |_, window, cx| {
                    cx.stop_propagation();
                    cb(&action, window, cx);
                })
                .on_a11y_action(AccessibleAction::Click, move |_, window, cx| {
                    accessibility_cb(&accessibility_action, window, cx);
                })
        })
        .child(
            div()
                .w(px(visual.icon_gutter))
                .h(px(visual.icon_size))
                .flex_none()
                .when_some(icon, move |slot, icon| {
                    slot.child(div().ml(px(visual.icon_left)).child(remote_menu_icon(
                        format!("remote-context-text-icon-{index}"),
                        icon,
                        danger,
                        visual.icon_size,
                        tokens,
                    )))
                }),
        )
        .child(label)
        .into_any_element()
}

fn remote_context_menu(
    tokens: UiTokens,
    menu: crate::state::RemoteContextMenuState,
    window_width: f32,
    window_height: f32,
    callback: Option<ActionCallback>,
) -> impl IntoElement {
    let visual = context_menu_visual_tokens(tokens);
    let close = ExplorerAction::CloseRemoteContextMenu;
    let close_cb = callback.clone();
    let close_right = close.clone();
    let close_right_cb = callback.clone();
    let commands = remote_menu_commands(
        menu.background,
        menu.paste_available,
        menu.item_row_index,
        menu.item_is_container,
    );
    let primary_count = commands
        .iter()
        .take_while(|command| command.placement == RemoteMenuPlacement::Text)
        .count();
    let rows = commands
        .into_iter()
        .enumerate()
        .map(|(index, command)| {
            let row = remote_menu_text_command(index, command, tokens, callback.clone());
            div()
                .when(index == primary_count && primary_count > 0, |element| {
                    element.child(
                        div()
                            .id("remote-context-primary-separator")
                            .h(px(1.0))
                            .ml(px(visual.divider_left_inset))
                            .mr(px(visual.divider_right_inset))
                            .bg(visual.border),
                    )
                })
                .child(row)
        })
        .collect::<Vec<_>>();
    let (left, top) = remote_context_menu_position_with_size(
        menu.x,
        menu.y,
        window_width,
        window_height,
        visual.width,
        visual.maximum_height,
    );
    div()
        .id("remote-context-overlay")
        .absolute()
        .inset_0()
        .on_mouse_down(MouseButton::Left, move |_, window, cx| {
            if let Some(cb) = close_cb.as_ref() {
                cb(&close, window, cx);
            }
        })
        .on_mouse_down(MouseButton::Right, move |_, window, cx| {
            if let Some(cb) = close_right_cb.as_ref() {
                cb(&close_right, window, cx);
            }
        })
        .child(
            deferred(
                div()
                    .id("remote-context-menu")
                    .role(Role::Menu)
                    .aria_label("Remote file context menu")
                    .occlude()
                    .absolute()
                    .left(px(left))
                    .top(px(top))
                    .w(px(visual.width))
                    .py(px(visual.outer_padding))
                    .font_family(tokens.typography.family.primary)
                    .border(px(1.0))
                    .border_color(visual.border)
                    .bg(visual.surface)
                    .shadow(vec![visual.shadow])
                    .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                    .on_mouse_down(MouseButton::Right, |_, _, cx| cx.stop_propagation())
                    .children(rows),
            )
            .with_priority(300),
        )
}

#[allow(
    dead_code,
    reason = "kept as the deterministic default-size projection used by source-contract tests"
)]
fn remote_context_menu_position(
    client_x: f32,
    client_y: f32,
    window_width: f32,
    window_height: f32,
) -> (f32, f32) {
    remote_context_menu_position_with_size(
        client_x,
        client_y,
        window_width,
        window_height,
        296.0,
        420.0,
    )
}

fn remote_context_menu_position_with_size(
    client_x: f32,
    client_y: f32,
    window_width: f32,
    window_height: f32,
    menu_width: f32,
    menu_max_height: f32,
) -> (f32, f32) {
    (
        client_x.max(0.0).min((window_width - menu_width).max(0.0)),
        client_y
            .max(0.0)
            .min((window_height - menu_max_height).max(0.0)),
    )
}

pub(crate) fn bookmark_editor(
    tokens: UiTokens,
    state: &AppViewState,
    name_input: Option<gpui::WeakEntity<EditableTextState>>,
    payload_input: Option<gpui::WeakEntity<EditableTextState>>,
    callback: Option<ActionCallback>,
) -> impl IntoElement {
    let editor = state.bookmark_editor().expect("editor is open");
    let is_new = editor.id.is_none();
    let colors = tokens.theme.colors;
    let (input_text, input_selection, input_selection_text, input_caret) =
        editable_input_colors(tokens);
    let payload_label = match &editor.target {
        explorer_model::BookmarkTarget::LuaScript { .. } => {
            "Lua 原始碼（僅可使用唯讀 current_folder）"
        }
        explorer_model::BookmarkTarget::Folder { .. }
        | explorer_model::BookmarkTarget::FolderPath { .. } => "資料夾路徑（可編輯）",
        explorer_model::BookmarkTarget::File { .. }
        | explorer_model::BookmarkTarget::FilePath { .. } => "檔案路徑（可編輯）",
    };
    let payload_is_multiline = matches!(
        &editor.target,
        explorer_model::BookmarkTarget::LuaScript { .. }
    );
    let payload_height = if payload_is_multiline { 220.0 } else { 36.0 };
    let save = ExplorerAction::SaveBookmarkEditor;
    let cancel = ExplorerAction::CancelBookmarkEditor;
    let save_cb = callback.clone();
    let remove_cb = callback.clone();
    let overlay_cancel_cb = callback.clone();
    let overlay_cancel = cancel.clone();
    let destination_rows = std::iter::once((None, "根目錄".to_owned()))
        .chain(
            state
                .bookmarks()
                .folders()
                .iter()
                .map(|folder| (Some(folder.id), folder.name.clone())),
        )
        .map(|(parent_id, name)| {
            let selected = editor.parent_id == parent_id;
            let action = ExplorerAction::SelectBookmarkDestination { parent_id };
            let callback = callback.clone();
            div()
                .id(match parent_id {
                    Some(id) => format!("bookmark-destination-{}", id.as_u128()),
                    None => "bookmark-destination-root".to_owned(),
                })
                .role(Role::Button)
                .aria_label(format!("Save in {name}"))
                .cursor_pointer()
                .px(px(8.0))
                .py(px(5.0))
                .rounded(px(4.0))
                .bg(if selected {
                    colors.control_pressed.to_gpui()
                } else {
                    colors.control_fill.to_gpui()
                })
                .child(format!("{} {name}", if selected { "●" } else { "○" }))
                .when_some(callback, move |element, cb| {
                    element.on_click(move |_, window, cx| cb(&action, window, cx))
                })
        })
        .collect::<Vec<_>>();
    div()
        .id("bookmark-editor-overlay")
        .absolute()
        .inset_0()
        .flex()
        .items_center()
        .justify_center()
        .bg(tokens.theme.colors.surface.to_gpui())
        .when_some(overlay_cancel_cb, move |element, cb| {
            element.on_mouse_down(MouseButton::Left, move |_, window, cx| {
                cb(&overlay_cancel, window, cx)
            })
        })
        .child(
            div()
                .id("bookmark-editor")
                .role(Role::Dialog)
                .aria_label("Bookmark editor")
                .w_full()
                .p(px(20.0))
                .flex()
                .flex_col()
                .gap(px(9.0))
                .font_family(tokens.typography.family.primary)
                .text_size(px(tokens.typography.file_row.size.value()))
                .line_height(px(tokens.typography.file_row.line_height.value()))
                .rounded(px(8.0))
                .bg(tokens.theme.colors.surface.to_gpui())
                .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                .child(
                    div()
                        .text_size(px(20.0))
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_center()
                        .child(if is_new {
                            "新增書籤"
                        } else {
                            "編輯書籤"
                        }),
                )
                .child(div().h(px(1.0)).bg(colors.divider.to_gpui()))
                .child("名稱 (N)")
                .when_some(name_input, |e, input| {
                    e.child(
                        text_input("bookmark-name-input")
                            .state(input)
                            .multiline(false)
                            .caret_blink_interval_500ms()
                            .w_full()
                            .h(px(32.0))
                            .px(px(8.0))
                            .font_family(tokens.typography.family.primary)
                            .text_size(px(16.0))
                            .line_height(px(tokens.typography.address.line_height.value()))
                            .bg(colors.control_fill.to_gpui())
                            .text_color(input_text)
                            .selection_color(input_selection.into())
                            .selection_text_color(input_selection_text.into())
                            .caret_color(input_caret.into())
                            .rounded(px(4.0))
                            .border(px(1.0))
                            .border_color(colors.focus.to_gpui()),
                    )
                })
                .when(payload_is_multiline, |e| e.child(payload_label))
                .when(payload_is_multiline, |e| {
                    e.when_some(payload_input, |e, input| {
                        e.child(
                            text_input("bookmark-payload-input")
                                .state(input)
                                .multiline(payload_is_multiline)
                                .caret_blink_interval_500ms()
                                .w_full()
                                .h(px(payload_height))
                                .px(px(8.0))
                                .bg(colors.control_fill.to_gpui())
                                .text_color(input_text)
                                .selection_color(input_selection.into())
                                .selection_text_color(input_selection_text.into())
                                .caret_color(input_caret.into())
                                .border(px(1.0))
                                .border_color(colors.focus.to_gpui()),
                        )
                    })
                })
                .child("位置 (L)")
                .child(
                    div()
                        .id("bookmark-destination-picker")
                        .role(Role::List)
                        .max_h(px(110.0))
                        .overflow_y_scroll()
                        .flex()
                        .flex_col()
                        .gap(px(4.0))
                        .p(px(8.0))
                        .rounded(px(12.0))
                        .bg(colors.control_fill.to_gpui())
                        .children(destination_rows),
                )
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(8.0))
                        .child(
                            div()
                                .w(px(18.0))
                                .h(px(18.0))
                                .rounded(px(3.0))
                                .bg(colors.accent.to_gpui())
                                .child(div().size_full()),
                        )
                        .child("儲存時顯示編輯器 (S)"),
                )
                .child(
                    div()
                        .flex()
                        .justify_end()
                        .gap(px(8.0))
                        .when(!is_new, |row| {
                            row.child({
                                let remove = ExplorerAction::RemoveEditingBookmark;
                                div()
                                    .id("bookmark-editor-remove")
                                    .role(Role::Button)
                                    .aria_label("Remove bookmark")
                                    .cursor_pointer()
                                    .px(px(12.0))
                                    .py(px(6.0))
                                    .rounded(px(10.0))
                                    .text_color(colors.danger.to_gpui())
                                    .child("移除書籤")
                                    .when_some(remove_cb, move |e, cb| {
                                        e.on_click(move |_, w, cx| cb(&remove, w, cx))
                                    })
                            })
                        })
                        .child(
                            div()
                                .id("bookmark-editor-cancel")
                                .role(Role::Button)
                                .aria_label("Cancel bookmark edit")
                                .cursor_pointer()
                                .px(px(12.0))
                                .py(px(6.0))
                                .rounded(px(10.0))
                                .bg(colors.control_fill.to_gpui())
                                .child("取消")
                                .when_some(callback, move |e, cb| {
                                    e.on_click(move |_, w, cx| cb(&cancel, w, cx))
                                }),
                        )
                        .child(
                            div()
                                .id("bookmark-editor-save")
                                .role(Role::Button)
                                .aria_label("Save bookmark")
                                .cursor_pointer()
                                .px(px(12.0))
                                .py(px(6.0))
                                .rounded(px(10.0))
                                .bg(tokens.theme.colors.accent.to_gpui())
                                .text_color(colors.surface.to_gpui())
                                .child("儲存")
                                .when_some(save_cb, move |e, cb| {
                                    e.on_click(move |_, w, cx| cb(&save, w, cx))
                                }),
                        ),
                ),
        )
}

pub(crate) fn bookmark_folder_editor(
    tokens: UiTokens,
    input: Option<gpui::WeakEntity<EditableTextState>>,
    callback: Option<ActionCallback>,
) -> impl IntoElement {
    let colors = tokens.theme.colors;
    let (text, selection, selection_text, caret) = editable_input_colors(tokens);
    let save = ExplorerAction::SaveBookmarkFolderEditor;
    let cancel = ExplorerAction::CancelBookmarkFolderEditor;
    let save_cb = callback.clone();
    div()
        .id("bookmark-folder-editor-window-content")
        .absolute()
        .inset_0()
        .flex()
        .items_center()
        .justify_center()
        .bg(colors.subtle_surface.to_gpui())
        .child(
            div()
                .id("bookmark-folder-editor")
                .role(Role::Dialog)
                .aria_label("Rename bookmark folder")
                .w(px(420.0))
                .p(px(18.0))
                .flex()
                .flex_col()
                .gap(px(10.0))
                .rounded(px(8.0))
                .bg(colors.surface.to_gpui())
                .child("重新命名書籤資料夾")
                .when_some(input, |element, input| {
                    element.child(
                        text_input("bookmark-folder-name-input")
                            .state(input)
                            .multiline(false)
                            .caret_blink_interval_500ms()
                            .w_full()
                            .h(px(34.0))
                            .px(px(8.0))
                            .bg(colors.control_fill.to_gpui())
                            .text_color(text)
                            .selection_color(selection.into())
                            .selection_text_color(selection_text.into())
                            .caret_color(caret.into())
                            .border(px(1.0))
                            .border_color(colors.focus.to_gpui()),
                    )
                })
                .child(
                    div()
                        .flex()
                        .justify_end()
                        .gap(px(8.0))
                        .child(
                            div()
                                .id("bookmark-folder-editor-cancel")
                                .role(Role::Button)
                                .aria_label("Cancel bookmark folder edit")
                                .cursor_pointer()
                                .px(px(12.0))
                                .py(px(6.0))
                                .child("取消")
                                .when_some(callback, move |element, cb| {
                                    element.on_click(move |_, window, cx| cb(&cancel, window, cx))
                                }),
                        )
                        .child(
                            div()
                                .id("bookmark-folder-editor-save")
                                .role(Role::Button)
                                .aria_label("Save bookmark folder")
                                .cursor_pointer()
                                .px(px(12.0))
                                .py(px(6.0))
                                .bg(colors.accent.to_gpui())
                                .child("儲存")
                                .when_some(save_cb, move |element, cb| {
                                    element.on_click(move |_, window, cx| cb(&save, window, cx))
                                }),
                        ),
                ),
        )
}

fn session_reset_confirmation_dialog(
    tokens: UiTokens,
    scope: explorer_model::SessionResetScope,
    on_action: Option<ActionCallback>,
) -> impl IntoElement {
    let label = match scope {
        explorer_model::SessionResetScope::Session => "saved windows and tabs",
        explorer_model::SessionResetScope::ViewSettings => "saved view settings",
        explorer_model::SessionResetScope::QuickAccess => "Quick Access pins",
        explorer_model::SessionResetScope::AllRoadmapState => "all saved Explorer state",
    };
    div()
        .id("session-reset-confirmation-overlay")
        .absolute()
        .top_0()
        .right_0()
        .bottom_0()
        .left_0()
        .flex()
        .items_center()
        .justify_center()
        .bg(crate::theme::MODAL_BACKDROP.to_gpui())
        .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
        .on_mouse_down(MouseButton::Right, |_, _, cx| cx.stop_propagation())
        .on_mouse_down(MouseButton::Middle, |_, _, cx| cx.stop_propagation())
        .child(
            div()
                .id("session-reset-confirmation-dialog")
                .role(Role::Dialog)
                .aria_label(format!("Confirm reset of {label}"))
                .w(px(crate::layout::folder_options::DIALOG_WIDTH.value()))
                .p(px(crate::layout::folder_options::PAGE_PADDING.value()))
                .flex()
                .flex_col()
                .gap(px(tokens.layout.content_spacing.value()))
                .rounded(px(tokens.layout.corner_radius.value()))
                .border(px(1.0))
                .border_color(tokens.theme.colors.divider.to_gpui())
                .bg(tokens.theme.colors.menu_fill.to_gpui())
                .child(format!(
                    "Reset {label}? This removes only persisted state; current files are not changed."
                ))
                .child(
                    div()
                        .flex()
                        .justify_end()
                        .gap(px(tokens.layout.control_padding_horizontal.value()))
                        .child(folder_option_button(
                            "session-reset-cancel",
                            "Cancel",
                            ExplorerAction::CancelSavedStateReset,
                            tokens,
                            on_action.clone(),
                        ))
                        .child(folder_option_button(
                            "session-reset-confirm",
                            "Reset",
                            ExplorerAction::ConfirmSavedStateReset,
                            tokens,
                            on_action,
                        )),
                ),
        )
}

fn permanent_delete_confirmation_dialog(
    tokens: UiTokens,
    item_count: usize,
    focused_target: crate::actions::PermanentDeleteDialogTarget,
    on_action: Option<ActionCallback>,
) -> impl IntoElement {
    let item_label = if item_count == 1 { "item" } else { "items" };
    div()
        .id("permanent-delete-confirmation-overlay")
        .absolute()
        .top_0()
        .right_0()
        .bottom_0()
        .left_0()
        .occlude()
        .flex()
        .items_center()
        .justify_center()
        .bg(crate::theme::MODAL_BACKDROP.to_gpui())
        .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
        .child(
            div()
                .id("permanent-delete-confirmation-dialog")
                .role(Role::Dialog)
                .aria_label(format!("Permanently delete {item_count} {item_label}"))
                .w(px(crate::layout::folder_options::DIALOG_WIDTH.value()))
                .p(px(crate::layout::folder_options::PAGE_PADDING.value()))
                .flex()
                .flex_col()
                .gap(px(tokens.layout.content_spacing.value()))
                .rounded(px(tokens.layout.corner_radius.value()))
                .border(px(1.0))
                .border_color(tokens.theme.colors.divider.to_gpui())
                .bg(tokens.theme.colors.menu_fill.to_gpui())
                .child(format!(
                    "Permanently delete {item_count} {item_label}? This action cannot be undone."
                ))
                .child(
                    div()
                        .flex()
                        .justify_end()
                        .gap(px(tokens.layout.control_padding_horizontal.value()))
                        .child(permanent_delete_dialog_button(
                            "permanent-delete-cancel",
                            "Cancel",
                            ExplorerAction::CancelPermanentDelete,
                            crate::actions::PermanentDeleteDialogTarget::Cancel,
                            focused_target == crate::actions::PermanentDeleteDialogTarget::Cancel,
                            tokens,
                            on_action.clone(),
                        ))
                        .child(permanent_delete_dialog_button(
                            "permanent-delete-confirm",
                            "Delete",
                            ExplorerAction::ConfirmPermanentDelete,
                            crate::actions::PermanentDeleteDialogTarget::Delete,
                            focused_target == crate::actions::PermanentDeleteDialogTarget::Delete,
                            tokens,
                            on_action,
                        )),
                ),
        )
}

#[allow(
    dead_code,
    reason = "superseded by the dedicated native Properties window"
)]
fn remote_properties_dialog(
    tokens: UiTokens,
    properties: crate::state::RemotePropertiesState,
    on_action: Option<ActionCallback>,
) -> impl IntoElement {
    let entry = properties.entry;
    let location = match &entry.location {
        explorer_model::LocationDescriptor::Virtual(remote) => format!(
            "{}://{}/{}",
            remote.provider_id,
            remote.public_authority.as_deref().unwrap_or("remote"),
            remote.components.join("/")
        ),
        _ => "無法取得".to_owned(),
    };
    let type_name = entry.metadata.type_display.clone().unwrap_or_else(|| {
        if entry.is_container {
            "遠端資料夾"
        } else {
            "遠端檔案"
        }
        .to_owned()
    });
    let size = entry
        .metadata
        .size_bytes
        .map_or_else(|| "無法取得".to_owned(), |value| format!("{value} 位元組"));
    let changed = properties.mode != properties.original_mode;
    let mut grid = div().flex().flex_col().gap(px(4.0));
    for (label, read, write, execute) in [
        ("擁有者", 0o400, 0o200, 0o100),
        ("群組", 0o040, 0o020, 0o010),
        ("其他", 0o004, 0o002, 0o001),
    ] {
        grid = grid.child(
            div()
                .flex()
                .items_center()
                .gap(px(12.0))
                .child(div().w(px(70.0)).child(label))
                .child(remote_permission_toggle(
                    "讀取",
                    read,
                    properties.mode,
                    tokens,
                    on_action.clone(),
                ))
                .child(remote_permission_toggle(
                    "寫入",
                    write,
                    properties.mode,
                    tokens,
                    on_action.clone(),
                ))
                .child(remote_permission_toggle(
                    "執行",
                    execute,
                    properties.mode,
                    tokens,
                    on_action.clone(),
                )),
        );
    }
    div()
        .id("remote-properties-overlay")
        .absolute()
        .top_0()
        .right_0()
        .bottom_0()
        .left_0()
        .occlude()
        .flex()
        .items_center()
        .justify_center()
        .bg(crate::theme::MODAL_BACKDROP.to_gpui())
        .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
        .child(
            div()
                .id("remote-properties-dialog")
                .role(Role::Dialog)
                .aria_label(format!("{} 內容", entry.display_name))
                .w(px(560.0))
                .p(px(24.0))
                .flex()
                .flex_col()
                .gap(px(14.0))
                .rounded(px(tokens.layout.corner_radius.value()))
                .border(px(1.0))
                .border_color(tokens.theme.colors.divider.to_gpui())
                .bg(tokens.theme.colors.menu_fill.to_gpui())
                .child(
                    div()
                        .text_size(px(20.0))
                        .child(format!("{} - 內容", entry.display_name)),
                )
                .child("一般")
                .child(format!("檔案類型：{type_name}"))
                .child(format!("位置：{location}"))
                .child(format!("大小：{size}"))
                .when_some(entry.metadata.created_display.clone(), |dialog, value| {
                    dialog.child(format!("建立日期：{value}"))
                })
                .when_some(entry.metadata.modified_display.clone(), |dialog, value| {
                    dialog.child(format!("修改日期：{value}"))
                })
                .child(div().h(px(1.0)).bg(tokens.theme.colors.divider.to_gpui()))
                .child(format!("權限：{:04o}", properties.mode))
                .child(grid)
                .child(
                    div()
                        .flex()
                        .justify_end()
                        .gap(px(8.0))
                        .child(folder_option_button(
                            "remote-properties-ok",
                            "確定",
                            ExplorerAction::ApplyRemoteProperties,
                            tokens,
                            on_action.clone(),
                        ))
                        .child(folder_option_button(
                            "remote-properties-cancel",
                            "取消",
                            ExplorerAction::CloseRemoteProperties,
                            tokens,
                            on_action.clone(),
                        ))
                        .when(changed, |row| {
                            row.child(folder_option_button(
                                "remote-properties-apply",
                                "套用",
                                ExplorerAction::ApplyRemoteProperties,
                                tokens,
                                on_action,
                            ))
                        }),
                ),
        )
}

#[allow(
    dead_code,
    reason = "superseded by the dedicated native Properties window"
)]
fn remote_permission_toggle(
    label: &'static str,
    mask: u32,
    mode: u32,
    tokens: UiTokens,
    on_action: Option<ActionCallback>,
) -> impl IntoElement {
    let checked = mode & mask != 0;
    let action = on_action;
    div()
        .id(format!("remote-permission-{mask:o}"))
        .cursor_pointer()
        .flex()
        .items_center()
        .gap(px(4.0))
        .px(px(4.0))
        .aria_label(format!(
            "{label} {}",
            if checked { "已允許" } else { "未允許" }
        ))
        .when_some(action, move |element, callback| {
            element.on_click(move |_, window, cx| {
                callback(&ExplorerAction::ToggleRemotePermission { mask }, window, cx);
            })
        })
        .child(
            div()
                .w(px(14.0))
                .h(px(14.0))
                .border(px(1.0))
                .border_color(tokens.theme.colors.text_secondary.to_gpui())
                .when(checked, |box_element| {
                    box_element
                        .p(px(2.0))
                        .child(div().size_full().bg(tokens.theme.colors.accent.to_gpui()))
                }),
        )
        .child(label)
        .text_color(tokens.theme.colors.text_primary.to_gpui())
}

fn lock_recovery_dialog(
    tokens: UiTokens,
    recovery: LockRecoveryUiState,
    on_action: Option<ActionCallback>,
) -> impl IntoElement {
    let title = if recovery.item_count == 1 {
        "檔案正在使用中"
    } else {
        "部分項目正在使用中"
    };
    let owners = recovery.owners.clone();
    let result_owners = recovery.owners.clone();
    let close_outcomes = recovery.close_outcomes.clone();
    let close_enabled = recovery.can_close();
    let retry_enabled = recovery.can_retry();
    let busy = matches!(
        recovery.phase,
        LockRecoveryPhase::Discovering | LockRecoveryPhase::Closing | LockRecoveryPhase::Retrying
    );
    let focused_target = recovery.focused_target();
    div()
        .id("lock-recovery-overlay")
        .absolute()
        .top_0()
        .right_0()
        .bottom_0()
        .left_0()
        .occlude()
        .flex()
        .items_center()
        .justify_center()
        .bg(crate::theme::MODAL_BACKDROP.to_gpui())
        .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
        .on_mouse_down(MouseButton::Right, |_, _, cx| cx.stop_propagation())
        .on_mouse_down(MouseButton::Middle, |_, _, cx| cx.stop_propagation())
        .child(
            div()
                .id("lock-recovery-dialog")
                .role(Role::Dialog)
                .aria_label(title)
                .w(px(crate::layout::folder_options::DIALOG_WIDTH.value()))
                .max_h(px(crate::layout::lock_recovery::DIALOG_MAX_HEIGHT.value()))
                .p(px(crate::layout::folder_options::PAGE_PADDING.value()))
                .flex()
                .flex_col()
                .gap(px(tokens.layout.content_spacing.value()))
                .rounded(px(tokens.layout.corner_radius.value()))
                .border(px(1.0))
                .border_color(tokens.theme.colors.divider.to_gpui())
                .bg(tokens.theme.colors.menu_fill.to_gpui())
                .child(
                    div()
                        .text_size(px(tokens.typography.command.size.value()))
                        .child(title),
                )
                .child(format!(
                    "Windows 無法刪除選取的 {} 個項目，因為其他應用程式正在使用它。",
                    recovery.item_count
                ))
                .when(!owners.is_empty(), |dialog| {
                    dialog.child(
                        div()
                            .id("lock-owner-list")
                            .role(Role::List)
                            .aria_label("正在使用檔案的應用程式")
                            .max_h(px(
                                crate::layout::lock_recovery::OWNER_LIST_MAX_HEIGHT.value()
                            ))
                            .overflow_y_scroll()
                            .children(owners.into_iter().map(|owner| {
                                let owner_id = format!(
                                    "lock-owner-{}-{}",
                                    owner.identity.process_id, owner.identity.creation_time_100ns
                                );
                                let eligibility = match owner.eligibility {
                                    explorer_model::LockOwnerEligibility::Eligible => "可安全關閉",
                                    explorer_model::LockOwnerEligibility::ThisApplication => {
                                        "本程式不會被關閉"
                                    }
                                    explorer_model::LockOwnerEligibility::System
                                    | explorer_model::LockOwnerEligibility::Critical => {
                                        "系統程序不會被關閉"
                                    }
                                    explorer_model::LockOwnerEligibility::Service => {
                                        "服務不會被關閉"
                                    }
                                    explorer_model::LockOwnerEligibility::Protected
                                    | explorer_model::LockOwnerEligibility::Elevated => {
                                        "受保護的程序不會被關閉"
                                    }
                                    explorer_model::LockOwnerEligibility::IdentityUnavailable => {
                                        "無法確認程序身分"
                                    }
                                };
                                div()
                                    .id(owner_id)
                                    .role(Role::ListItem)
                                    .aria_label(format!("{}，{}", owner.display_name, eligibility))
                                    .min_h(px(tokens.layout.minimum_hit_target.value()))
                                    .flex()
                                    .flex_col()
                                    .justify_center()
                                    .border_b_1()
                                    .border_color(tokens.theme.colors.divider.to_gpui())
                                    .child(owner.display_name)
                                    .child(
                                        div()
                                            .text_size(px(tokens.typography.status.size.value()))
                                            .child(eligibility),
                                    )
                            })),
                    )
                })
                .when(!close_outcomes.is_empty(), |dialog| {
                    dialog.child(
                        div()
                            .id("lock-owner-results")
                            .role(Role::List)
                            .aria_label("關閉應用程式的結果")
                            .children(close_outcomes.into_iter().map(move |outcome| {
                                let name = result_owners
                                    .iter()
                                    .find(|owner| owner.identity == outcome.identity)
                                    .map_or_else(
                                        || format!("程序 {}", outcome.identity.process_id),
                                        |owner| owner.display_name.clone(),
                                    );
                                let result = match outcome.result {
                                    explorer_model::LockOwnerCloseResult::Closed => "已關閉",
                                    explorer_model::LockOwnerCloseResult::AlreadyExited => "已結束",
                                    explorer_model::LockOwnerCloseResult::StaleIdentity => {
                                        "程序身分已變更"
                                    }
                                    explorer_model::LockOwnerCloseResult::Denied => "拒絕存取",
                                    explorer_model::LockOwnerCloseResult::Protected => {
                                        "受保護，未關閉"
                                    }
                                    explorer_model::LockOwnerCloseResult::Refused => {
                                        "應用程式拒絕關閉"
                                    }
                                    explorer_model::LockOwnerCloseResult::Timeout => "等候關閉逾時",
                                };
                                let result_id = format!(
                                    "lock-owner-result-{}-{}",
                                    outcome.identity.process_id,
                                    outcome.identity.creation_time_100ns
                                );
                                div()
                                    .id(result_id)
                                    .role(Role::ListItem)
                                    .aria_label(format!("{name}，{result}"))
                                    .min_h(px(tokens.layout.minimum_hit_target.value()))
                                    .child(format!("{name} — {result}"))
                            })),
                    )
                })
                .child(
                    div()
                        .id("lock-recovery-status")
                        .role(Role::Status)
                        .aria_label(recovery.status.clone())
                        .child(recovery.status),
                )
                .child(
                    div()
                        .flex()
                        .justify_end()
                        .gap(px(tokens.layout.control_padding_horizontal.value()))
                        .child(lock_dialog_button(
                            "lock-recovery-close-retry",
                            "關閉程式並重試",
                            close_enabled,
                            focused_target == crate::state::LockRecoveryFocusTarget::CloseAndRetry,
                            ExplorerAction::CloseLockOwnersAndRetry,
                            tokens,
                            on_action.clone(),
                        ))
                        .child(lock_dialog_button(
                            "lock-recovery-retry",
                            if busy { "請稍候…" } else { "重試" },
                            retry_enabled,
                            focused_target == crate::state::LockRecoveryFocusTarget::Retry,
                            ExplorerAction::RetryLockedDelete,
                            tokens,
                            on_action.clone(),
                        ))
                        .child(lock_dialog_button(
                            "lock-recovery-cancel",
                            "取消",
                            true,
                            focused_target == crate::state::LockRecoveryFocusTarget::Cancel,
                            ExplorerAction::CancelLockedDeleteRecovery,
                            tokens,
                            on_action,
                        )),
                ),
        )
}

fn lock_dialog_button(
    id: &'static str,
    label: &'static str,
    enabled: bool,
    focused: bool,
    action: ExplorerAction,
    tokens: UiTokens,
    on_action: Option<ActionCallback>,
) -> impl IntoElement {
    div()
        .id(id)
        .role(Role::Button)
        .aria_label(label)
        .min_w(px(crate::layout::folder_options::BUTTON_MIN_WIDTH.value()))
        .h(px(tokens.layout.minimum_hit_target.value()))
        .flex()
        .items_center()
        .justify_center()
        .rounded(px(tokens.layout.corner_radius.value()))
        .border(px(1.0))
        .border_color(tokens.theme.colors.divider.to_gpui())
        .when(focused, |button| {
            button.bg(tokens.theme.colors.selected_active.to_gpui())
        })
        .when(enabled, |button| {
            button
                .hover(move |style| style.bg(tokens.theme.colors.control_hover.to_gpui()))
                .when_some(on_action, |button, callback| {
                    button.on_click(move |_, window, cx| callback(&action, window, cx))
                })
        })
        .child(label)
}

pub(crate) fn folder_options_window_content(
    tokens: UiTokens,
    draft: crate::state::FolderOptionsDraft,
    extensions: Vec<crate::state::ExtensionOptionV1>,
    scroll: gpui::ScrollHandle,
    scrollbar: gpui::AnyElement,
    on_action: Option<ActionCallback>,
    cache_budget_inputs: Vec<gpui::WeakEntity<EditableTextState>>,
    cache_usage: crate::folder_options_window::CacheUsageSnapshotV1,
) -> impl IntoElement {
    use crate::actions::FolderOptionsPage;

    let page = draft.page;
    let apply_error = draft.apply_error.clone();
    let settings = draft.settings;
    div()
        .id("folder-options-window-content")
        .absolute()
        .top_0()
        .right_0()
        .bottom_0()
        .left_0()
        .flex()
        .bg(tokens.theme.colors.menu_fill.to_gpui())
        .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
        .child(
            div()
                .id("folder-options-dialog")
                .role(Role::Dialog)
                .aria_label("資料夾選項")
                .w_full()
                .h_full()
                .flex()
                .flex_col()
                .rounded(px(tokens.layout.corner_radius.value()))
                .border(px(1.0))
                .border_color(tokens.theme.colors.divider.to_gpui())
                .bg(tokens.theme.colors.menu_fill.to_gpui())
                .child(
                    div()
                        .h(px(tokens.layout.address_bar_height.value()))
                        .flex_none()
                        .flex()
                        .items_center()
                        .justify_between()
                        .px(px(tokens.layout.divider_keyboard_step.value()))
                        .text_size(px(tokens.typography.address.size.value()))
                        .child("資料夾選項")
                        .child(folder_option_button(
                            "folder-options-close",
                            "關閉",
                            ExplorerAction::CloseFolderOptions,
                            tokens,
                            on_action.clone(),
                        )),
                )
                .child(
                    div()
                        .h(px(tokens.layout.title_tab_height.value()))
                        .flex_none()
                        .flex()
                        .px(px(tokens.layout.control_padding_horizontal.value()))
                        .child(folder_option_tab(
                            "folder-options-general-tab",
                            "一般",
                            page == FolderOptionsPage::General,
                            ExplorerAction::SetFolderOptionsPage(FolderOptionsPage::General),
                            tokens,
                            on_action.clone(),
                        ))
                        .child(folder_option_tab(
                            "folder-options-view-tab",
                            "檢視",
                            page == FolderOptionsPage::View,
                            ExplorerAction::SetFolderOptionsPage(FolderOptionsPage::View),
                            tokens,
                            on_action.clone(),
                        ))
                        .child(folder_option_tab(
                            "folder-options-extensions-tab",
                            "Extensions",
                            page == FolderOptionsPage::Extensions,
                            ExplorerAction::SetFolderOptionsPage(FolderOptionsPage::Extensions),
                            tokens,
                            on_action.clone(),
                        )),
                )
                .child(
                    div()
                        .id("folder-options-page")
                        .role(Role::List)
                        .aria_label(match page {
                            FolderOptionsPage::General => "一般",
                            FolderOptionsPage::View => "檢視",
                            FolderOptionsPage::Extensions => "Extensions",
                        })
                        .flex_1()
                        .min_h_0()
                        .overflow_y_scroll()
                        .track_scroll(&scroll)
                        .pr(px(tokens.layout.content_spacing.value() * 1.5))
                        .p(px(crate::layout::folder_options::PAGE_PADDING.value()))
                        .on_scroll_wheel(|_, window, cx| {
                            window.refresh();
                            cx.refresh_windows();
                        })
                        .when(page == FolderOptionsPage::General, |body| {
                            body.child(folder_options_general_page(
                                tokens,
                                draft.restore_previous_session,
                                on_action.clone(),
                            ))
                        })
                        .when(page == FolderOptionsPage::View, |body| {
                            body.child(folder_options_view_page(
                                tokens,
                                &settings,
                                on_action.clone(),
                                cache_budget_inputs.clone(),
                                cache_usage,
                            ))
                        })
                        .when(page == FolderOptionsPage::Extensions, |body| {
                            body.child(folder_options_extensions_page(
                                tokens,
                                &extensions,
                                &draft.extension_enabled,
                                on_action.clone(),
                            ))
                        }),
                )
                .child(
                    div()
                        .h(px(crate::layout::folder_options::FOOTER_HEIGHT.value()))
                        .flex_none()
                        .flex()
                        .items_center()
                        .justify_end()
                        .gap(px(tokens.layout.control_padding_horizontal.value()))
                        .px(px(tokens.layout.divider_keyboard_step.value()))
                        .border_t(px(1.0))
                        .border_color(tokens.theme.colors.divider.to_gpui())
                        .when_some(apply_error, |footer, error| {
                            footer.child(
                                div()
                                    .id("folder-options-apply-error")
                                    .role(Role::Alert)
                                    .flex_1()
                                    .text_color(tokens.theme.colors.danger.to_gpui())
                                    .child(error),
                            )
                        })
                        .child(folder_option_button(
                            "folder-options-ok",
                            "確定",
                            ExplorerAction::ConfirmFolderOptions,
                            tokens,
                            on_action.clone(),
                        ))
                        .child(folder_option_button(
                            "folder-options-cancel",
                            "取消",
                            ExplorerAction::CloseFolderOptions,
                            tokens,
                            on_action.clone(),
                        ))
                        .child(folder_option_button(
                            "folder-options-apply",
                            "套用",
                            ExplorerAction::ApplyFolderOptions,
                            tokens,
                            on_action,
                        )),
                ),
        )
        .child(scrollbar)
}

fn about_dialog(
    tokens: UiTokens,
    info: crate::state::AboutInfoV1,
    on_action: Option<ActionCallback>,
) -> impl IntoElement {
    let row = |label: &'static str, value: String| {
        div()
            .flex()
            .gap(px(tokens.layout.content_spacing.value() * 2.0))
            .child(
                div()
                    .w(px(100.0))
                    .flex_none()
                    .text_color(tokens.theme.colors.text_secondary.to_gpui())
                    .child(label),
            )
            .child(div().flex_1().child(value))
    };
    div()
        .id("about-overlay")
        .absolute()
        .top_0()
        .right_0()
        .bottom_0()
        .left_0()
        .flex()
        .items_center()
        .justify_center()
        .bg(crate::theme::MODAL_BACKDROP.to_gpui())
        .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
        .child(
            div()
                .id("about-dialog")
                .role(Role::Dialog)
                .aria_label("關於 SuperExplorer")
                .w(px(460.0))
                .flex()
                .flex_col()
                .gap(px(tokens.layout.content_spacing.value() * 2.0))
                .p(px(tokens.layout.divider_keyboard_step.value()))
                .rounded(px(tokens.layout.corner_radius.value()))
                .border(px(1.0))
                .border_color(tokens.theme.colors.divider.to_gpui())
                .bg(tokens.theme.colors.menu_fill.to_gpui())
                .child(
                    div()
                        .text_size(px(tokens.typography.address.size.value()))
                        .child("SuperExplorer"),
                )
                .child(row("版本", info.version))
                .child(row("編譯日期", info.build_date))
                .child(row("Git hash", info.git_hash))
                .child(row("作者", info.author))
                .child(div().flex().justify_end().child(folder_option_button(
                    "about-ok",
                    "OK",
                    ExplorerAction::CloseAboutDialog,
                    tokens,
                    on_action,
                ))),
        )
}

fn folder_options_extensions_page(
    tokens: UiTokens,
    extensions: &[crate::state::ExtensionOptionV1],
    enabled: &[bool],
    on_action: Option<ActionCallback>,
) -> impl IntoElement {
    div()
        .id("folder-options-extensions-page")
        .role(Role::List)
        .aria_label("Extensions")
        .flex()
        .flex_col()
        .gap(px(tokens.layout.maximum_visible_glyph.value()))
        .when(!enabled.is_empty() && enabled.iter().all(|enabled| !enabled), |page| {
            page.child(
                div()
                    .id("folder-options-extension-safe-mode")
                    .role(Role::Status)
                    .aria_label("Plugin Safe Mode")
                    .p(px(tokens.layout.control_padding_horizontal.value()))
                    .border(px(1.0))
                    .border_color(tokens.theme.colors.divider.to_gpui())
                    .rounded(px(tokens.layout.corner_radius.value()))
                    .child("Plugin Safe Mode 已啟用。勾選要重新啟用的 Plugin，按 Apply 或 OK，然後重新啟動 SuperExplorer。"),
            )
        })
        .children(extensions.iter().enumerate().map(|(index, extension)| {
            let website_action = ExplorerAction::OpenExtensionAuthorWebsite { index };
            let community_action = ExplorerAction::OpenExtensionCommunityWebsite { index };
            div()
                .id(SharedString::from(format!(
                    "folder-option-extension-{}",
                    extension.package_id
                )))
                .role(Role::ListItem)
                .aria_label(extension.display_name)
                .flex()
                .flex_col()
                .p(px(tokens.layout.control_padding_horizontal.value()))
                .border(px(1.0))
                .border_color(tokens.theme.colors.divider.to_gpui())
                .rounded(px(tokens.layout.corner_radius.value()))
                .child(folder_option_checkbox(
                    SharedString::from(format!(
                        "folder-option-extension-toggle-{}",
                        extension.package_id
                    )),
                    extension.display_name,
                    enabled.get(index).copied().unwrap_or(false),
                    ExplorerAction::ToggleFolderOptionExtension { index },
                    tokens,
                    on_action.clone(),
                ))
                .child(
                    div()
                        .ml(px(tokens.layout.minimum_hit_target.value()))
                        .text_size(px(tokens.typography.tooltip.size.value()))
                        .text_color(tokens.theme.colors.text_secondary.to_gpui())
                        .child(format!("用途：{}", extension.purpose)),
                )
                .child(
                    div()
                        .id(SharedString::from(format!(
                            "extension-author-{}",
                            extension.package_id
                        )))
                        .ml(px(tokens.layout.minimum_hit_target.value()))
                        .cursor_pointer()
                        .text_size(px(tokens.typography.tooltip.size.value()))
                        .text_color(tokens.theme.colors.accent.to_gpui())
                        .child(format!(
                            "作者：{} — {} · {}",
                            extension.author_name, extension.author_bio, extension.author_website
                        ))
                        .when_some(on_action.clone(), |author, callback| {
                            author.on_click(move |_, window, cx| {
                                callback(&website_action, window, cx)
                            })
                        }),
                )
                .child(
                    div()
                        .ml(px(tokens.layout.minimum_hit_target.value()))
                        .text_size(px(tokens.typography.tooltip.size.value()))
                        .text_color(tokens.theme.colors.text_secondary.to_gpui())
                        .child(format!("Release date：{}", extension.release_date)),
                )
                .child(
                    div()
                        .id(SharedString::from(format!(
                            "extension-community-{}",
                            extension.package_id
                        )))
                        .ml(px(tokens.layout.minimum_hit_target.value()))
                        .cursor_pointer()
                        .text_size(px(tokens.typography.tooltip.size.value()))
                        .text_color(tokens.theme.colors.accent.to_gpui())
                        .child(format!("社群：{}", extension.community_website))
                        .when_some(on_action.clone(), |community, callback| {
                            community.on_click(move |_, window, cx| {
                                callback(&community_action, window, cx)
                            })
                        }),
                )
        }))
}

fn folder_options_general_page(
    tokens: UiTokens,
    restore_previous_session: bool,
    on_action: Option<ActionCallback>,
) -> impl IntoElement {
    div()
        .flex()
        .flex_col()
        .gap(px(tokens.layout.maximum_visible_glyph.value()))
        .child(folder_option_group(
            "瀏覽資料夾",
            vec!["在同一個視窗中開啟每個資料夾", "在不同視窗中開啟每個資料夾"],
            tokens,
        ))
        .child(folder_option_checkbox(
            "folder-option-restore-session",
            "Restore previous windows and tabs at startup",
            restore_previous_session,
            ExplorerAction::ToggleRestorePreviousSession,
            tokens,
            on_action.clone(),
        ))
        .child(
            div()
                .flex()
                .gap(px(tokens.layout.content_spacing.value()))
                .child(folder_option_button(
                    "folder-options-reset-session",
                    "Reset session",
                    ExplorerAction::ResetSavedSession,
                    tokens,
                    on_action.clone(),
                ))
                .child(folder_option_button(
                    "folder-options-reset-view",
                    "Reset view settings",
                    ExplorerAction::ResetSavedViewSettings,
                    tokens,
                    on_action.clone(),
                ))
                .child(folder_option_button(
                    "folder-options-reset-all-state",
                    "Reset all Explorer state",
                    ExplorerAction::ResetAllSavedExplorerState,
                    tokens,
                    on_action,
                )),
        )
        .child(folder_option_group(
            "按一下項目的方式",
            vec![
                "按兩下以開啟項目（按一下以選取）",
                "按一下以開啟項目（指向以選取）",
            ],
            tokens,
        ))
}

fn folder_option_group(
    title: &'static str,
    options: Vec<&'static str>,
    tokens: UiTokens,
) -> impl IntoElement {
    div()
        .flex()
        .flex_col()
        .gap(px(tokens.layout.content_spacing.value()))
        .p(px(tokens.layout.divider_keyboard_step.value()))
        .border(px(1.0))
        .border_color(tokens.theme.colors.divider.to_gpui())
        .child(
            div()
                .text_size(px(tokens.typography.address.size.value()))
                .child(title),
        )
        .children(options.into_iter().enumerate().map(|(index, option)| {
            div()
                .h(px(tokens.layout.minimum_hit_target.value()))
                .flex()
                .items_center()
                .gap(px(tokens.layout.content_spacing.value()))
                .child(if index == 0 { "◉" } else { "○" })
                .child(option)
        }))
}

fn folder_options_view_page(
    tokens: UiTokens,
    settings: &explorer_model::ViewSettings,
    on_action: Option<ActionCallback>,
    cache_budget_inputs: Vec<gpui::WeakEntity<EditableTextState>>,
    cache_usage: crate::folder_options_window::CacheUsageSnapshotV1,
) -> impl IntoElement {
    div()
        .flex()
        .flex_col()
        .gap(px(tokens.layout.control_padding_horizontal.value()))
        .child(
            div()
                .flex()
                .items_center()
                .justify_between()
                .p(px(tokens.layout.control_padding_horizontal.value()))
                .border(px(1.0))
                .border_color(tokens.theme.colors.divider.to_gpui())
                .child("資料夾檢視：將目前檢視套用到此類型的所有資料夾")
                .child(folder_option_button(
                    "folder-options-reset",
                    "重設資料夾",
                    ExplorerAction::ResetFolderOptions,
                    tokens,
                    on_action.clone(),
                )),
        )
        .child(cache_budget_controls(
            tokens,
            settings.cache_budgets,
            cache_budget_inputs,
            cache_usage,
            on_action.clone(),
        ))
        .child(
            div()
                .text_size(px(tokens.typography.address.size.value()))
                .child("進階設定："),
        )
        .child(folder_option_checkbox(
            "folder-option-checkboxes",
            "使用核取方塊選取項目",
            settings.item_check_boxes,
            ExplorerAction::ToggleFolderOptionItemCheckBoxes,
            tokens,
            on_action.clone(),
        ))
        .child(folder_option_checkbox(
            "folder-option-extensions",
            "隱藏已知檔案類型的副檔名",
            !settings.file_name_extensions,
            ExplorerAction::ToggleFolderOptionFileNameExtensions,
            tokens,
            on_action.clone(),
        ))
        .child(folder_option_checkbox(
            "folder-option-hidden",
            "顯示隱藏的檔案、資料夾及磁碟機",
            settings.hidden_items,
            ExplorerAction::ToggleFolderOptionHiddenItems,
            tokens,
            on_action.clone(),
        ))
        .child(folder_option_checkbox(
            "folder-option-compact",
            "減少項目間的空白區域（精簡檢視）",
            settings.compact_view,
            ExplorerAction::ToggleFolderOptionCompactView,
            tokens,
            on_action.clone(),
        ))
        .child(folder_option_checkbox(
            "folder-option-always-icons",
            "一律顯示圖示，不顯示縮圖",
            settings.always_show_icons,
            ExplorerAction::ToggleFolderOptionAlwaysShowIcons,
            tokens,
            on_action.clone(),
        ))
        .child(folder_option_button(
            "folder-option-clear-thumbnail-cache",
            "清除縮圖快取",
            ExplorerAction::ClearThumbnailCache,
            tokens,
            on_action.clone(),
        ))
        .child(folder_option_checkbox(
            "folder-option-details-pane",
            "顯示詳細資料窗格",
            settings.details_pane,
            ExplorerAction::ToggleFolderOptionDetailsPane,
            tokens,
            on_action.clone(),
        ))
        .child(folder_option_checkbox(
            "folder-option-preview-pane",
            "顯示預覽窗格",
            settings.preview_pane,
            ExplorerAction::ToggleFolderOptionPreviewPane,
            tokens,
            on_action,
        ))
}

fn cache_budget_controls(
    tokens: UiTokens,
    budgets: explorer_model::CacheBudgetSettingsV1,
    inputs: Vec<gpui::WeakEntity<EditableTextState>>,
    usage: crate::folder_options_window::CacheUsageSnapshotV1,
    on_action: Option<ActionCallback>,
) -> impl IntoElement {
    let telemetry_id = |id| match id {
        explorer_model::CacheBudgetIdV1::ExtensionMemory => {
            Some(explorer_model::CacheTelemetryIdV1::ExtensionColumnsMemory)
        }
        explorer_model::CacheBudgetIdV1::IconDisk => {
            Some(explorer_model::CacheTelemetryIdV1::IconsDisk)
        }
        explorer_model::CacheBudgetIdV1::ThumbnailDisk => {
            Some(explorer_model::CacheTelemetryIdV1::ThumbnailsDisk)
        }
        explorer_model::CacheBudgetIdV1::ExtensionDisk => {
            Some(explorer_model::CacheTelemetryIdV1::ExtensionColumnsDisk)
        }
        explorer_model::CacheBudgetIdV1::MftPersistedIndex => {
            Some(explorer_model::CacheTelemetryIdV1::MftPersistedIndex)
        }
        explorer_model::CacheBudgetIdV1::MftVolumeIndex => {
            Some(explorer_model::CacheTelemetryIdV1::MftVolumeIndexMemory)
        }
        explorer_model::CacheBudgetIdV1::MftFileData => {
            Some(explorer_model::CacheTelemetryIdV1::MftFileDataMemory)
        }
        explorer_model::CacheBudgetIdV1::MftAggregates => {
            Some(explorer_model::CacheTelemetryIdV1::MftAggregateMemory)
        }
        explorer_model::CacheBudgetIdV1::MftLru => {
            Some(explorer_model::CacheTelemetryIdV1::MftServiceLru)
        }
        _ => None,
    };
    let labels = [
        "Icon memory",
        "Shared/base icon memory",
        "Thumbnail memory",
        "Extension data-column memory",
        "Icon GPU",
        "Thumbnail GPU",
        "Icon BC7 disk",
        "Thumbnail BC7 disk",
        "Extension data-column disk",
        "Persisted MFT index",
        "Volume index memory",
        "File data memory",
        "Folder aggregates memory",
        "MFT Service LRU",
        "Folder size cache TTL",
    ];
    let used_bytes = |id| match id {
        explorer_model::CacheBudgetIdV1::IconMemory => Some(usage.icon_memory_bytes),
        explorer_model::CacheBudgetIdV1::BaseIconMemory => Some(usage.base_icon_memory_bytes),
        explorer_model::CacheBudgetIdV1::ThumbnailMemory => Some(usage.thumbnail_memory_bytes),
        explorer_model::CacheBudgetIdV1::ExtensionMemory => usage.extension_memory_bytes,
        explorer_model::CacheBudgetIdV1::IconGpu => Some(usage.icon_gpu_bytes),
        explorer_model::CacheBudgetIdV1::ThumbnailGpu => Some(usage.thumbnail_gpu_bytes),
        explorer_model::CacheBudgetIdV1::IconDisk => usage.icon_disk_bytes,
        explorer_model::CacheBudgetIdV1::ThumbnailDisk => usage.thumbnail_disk_bytes,
        explorer_model::CacheBudgetIdV1::ExtensionDisk => usage.extension_disk_bytes,
        explorer_model::CacheBudgetIdV1::MftPersistedIndex => usage.mft_disk_bytes,
        explorer_model::CacheBudgetIdV1::MftVolumeIndex => usage.mft_volume_index_memory_bytes,
        explorer_model::CacheBudgetIdV1::MftFileData => usage.mft_file_data_memory_bytes,
        explorer_model::CacheBudgetIdV1::MftAggregates => usage.mft_aggregate_memory_bytes,
        explorer_model::CacheBudgetIdV1::MftLru => usage.mft_service_bytes,
        explorer_model::CacheBudgetIdV1::FolderSizeCacheTtlSeconds => None,
    };
    let effective_limit = |id| match id {
        explorer_model::CacheBudgetIdV1::IconMemory => Some(usage.icon_memory_limit),
        explorer_model::CacheBudgetIdV1::BaseIconMemory => Some(usage.base_icon_memory_limit),
        explorer_model::CacheBudgetIdV1::ThumbnailMemory => Some(usage.thumbnail_memory_limit),
        explorer_model::CacheBudgetIdV1::ExtensionMemory => usage.extension_memory_limit,
        explorer_model::CacheBudgetIdV1::IconGpu => Some(usage.icon_gpu_limit),
        explorer_model::CacheBudgetIdV1::ThumbnailGpu => Some(usage.thumbnail_gpu_limit),
        explorer_model::CacheBudgetIdV1::IconDisk => usage.icon_disk_limit,
        explorer_model::CacheBudgetIdV1::ThumbnailDisk => usage.thumbnail_disk_limit,
        explorer_model::CacheBudgetIdV1::ExtensionDisk => usage.extension_disk_limit,
        explorer_model::CacheBudgetIdV1::MftPersistedIndex => usage.mft_disk_limit,
        explorer_model::CacheBudgetIdV1::MftVolumeIndex => usage.mft_volume_index_memory_limit,
        explorer_model::CacheBudgetIdV1::MftFileData => usage.mft_file_data_memory_limit,
        explorer_model::CacheBudgetIdV1::MftAggregates => usage.mft_aggregate_memory_limit,
        explorer_model::CacheBudgetIdV1::MftLru => usage.mft_service_limit,
        explorer_model::CacheBudgetIdV1::FolderSizeCacheTtlSeconds => None,
    };
    div()
        .id("folder-options-cache-usage")
        .role(Role::Group)
        .aria_label("Cache usage")
        .flex()
        .flex_col()
        .gap(px(tokens.layout.control_padding_horizontal.value()))
        .child("Cache usage and limits (updates every second)")
        .child(
            div()
                .id("folder-options-cache-budget-controls")
                .flex()
                .flex_col()
                .gap(px(tokens.layout.control_padding_horizontal.value()))
                .children({
                    let mut rows = explorer_model::CACHE_BUDGET_DESCRIPTORS_V1
                        .into_iter()
                        .zip(labels)
                        .zip(inputs)
                        .map(|((descriptor, label), input)| {
                            let value = budgets.get(descriptor.id);
                            let is_ttl_row = descriptor.id
                                == explorer_model::CacheBudgetIdV1::FolderSizeCacheTtlSeconds;
                            let unit_label = if is_ttl_row { "seconds" } else { "MB" };
                            let unit_short = if is_ttl_row { "sec" } else { "MB" };
                            let configured_limit = u64::from(value) * 1024 * 1024;
                            let limit = effective_limit(descriptor.id).unwrap_or(configured_limit);
                            let availability = telemetry_id(descriptor.id)
                                .map(|id| usage.availability(id))
                                .unwrap_or(crate::folder_options_window::CacheUsageAvailabilityV1::Available);
                            let usage_text = cache_budget_usage_text(
                                availability,
                                used_bytes(descriptor.id),
                                limit,
                            );
                            let stops = descriptor.slider_stops();
                            let segment_width = 400.0 / stops.len().max(1) as f32;
                            let keyboard_stops = stops.clone();
                            div()
                                .id(SharedString::from(format!(
                                    "cache-budget-row-{:?}",
                                    descriptor.id
                                )))
                                .flex()
                                .flex_col()
                                .gap(px(tokens.layout.content_spacing.value()))
                                .child(
                                    div()
                                        .flex()
                                        .flex_wrap()
                                        .items_center()
                                        .gap(px(tokens.layout.content_spacing.value()))
                                        .child(label)
                                        .when(!is_ttl_row, |el| el.child(usage_text))
                                        .child(
                                            div()
                                                .id(SharedString::from(format!(
                                                    "cache-budget-input-{:?}",
                                                    descriptor.id
                                                )))
                                                .role(Role::TextInput)
                                                .aria_label(SharedString::from(format!(
                                                    "{label} limit, {value} {unit_label}"
                                                )))
                                                .w(px(112.0))
                                                .h(px(tokens.layout.minimum_hit_target.value()))
                                                .child(
                                                    text_input(SharedString::from(format!(
                                                        "cache-budget-editor-{:?}",
                                                        descriptor.id
                                                    )))
                                                    .state(input.clone())
                                                    .multiline(false)
                                                    .w_full()
                                                    .h_full()
                                                    .px(px(tokens.layout.content_spacing.value()))
                                                    .border(px(1.0))
                                                    .border_color(
                                                        tokens.theme.colors.divider.to_gpui(),
                                                    )
                                                    .rounded(px(tokens
                                                        .layout
                                                        .corner_radius
                                                        .value()))
                                                    .bg(tokens.theme.colors.control_fill.to_gpui()),
                                                ),
                                        )
                                        .child(unit_short),
                                )
                                .child(
                                    div()
                                        .id(SharedString::from(format!(
                                            "cache-budget-slider-{:?}",
                                            descriptor.id
                                        )))
                                        .role(Role::Slider)
                                        .tab_index(0)
                                        .aria_label(SharedString::from(format!("{label} limit")))
                                        .aria_numeric_value(f64::from(value))
                                        .aria_min_numeric_value(f64::from(descriptor.minimum_mb))
                                        .aria_max_numeric_value(f64::from(descriptor.maximum_mb))
                                        .w(px(400.0))
                                        .h(px(18.0))
                                        .flex()
                                        .rounded(px(9.0))
                                        .overflow_hidden()
                                        .border(px(1.0))
                                        .border_color(tokens.theme.colors.divider.to_gpui())
                                        .when_some(on_action.clone(), |bar, callback| {
                                            let keyboard_callback = callback.clone();
                                            let a11y_stops = keyboard_stops.clone();
                                            let keyboard_input = input.clone();
                                            let a11y_input = input.clone();
                                            bar.on_key_down(move |event, window, cx| {
                                                let nearest = keyboard_stops
                                                    .iter()
                                                    .enumerate()
                                                    .min_by_key(|(_, stop)| stop.abs_diff(value))
                                                    .map_or(0, |(index, _)| index);
                                                let target = match event.keystroke.key.as_str() {
                                                    "left" | "down" => nearest.saturating_sub(1),
                                                    "right" | "up" => (nearest + 1).min(
                                                        keyboard_stops.len().saturating_sub(1),
                                                    ),
                                                    "home" => 0,
                                                    "end" => keyboard_stops.len().saturating_sub(1),
                                                    _ => return,
                                                };
                                                if let Some(stop) = keyboard_stops.get(target) {
                                                    let text = stop.to_string();
                                                    let _ = keyboard_input.update(cx, |state, cx| {
                                                        state.emplace(&text, cx)
                                                    });
                                                    let mut next = budgets;
                                                    next.set(descriptor.id, *stop);
                                                    keyboard_callback(
                                                &ExplorerAction::SetFolderOptionCacheBudgets(next),
                                                window,
                                                cx,
                                            );
                                                    cx.stop_propagation();
                                                }
                                            })
                                            .on_a11y_action(AccessibleAction::SetValue, move |data, window, cx| {
                                                let Some(gpui::accesskit::ActionData::NumericValue(requested)) = data else {
                                                    return;
                                                };
                                                let requested = requested.clamp(
                                                    f64::from(descriptor.minimum_mb),
                                                    f64::from(descriptor.maximum_mb),
                                                ) as u32;
                                                let nearest = a11y_stops
                                                    .iter()
                                                    .min_by_key(|stop| stop.abs_diff(requested))
                                                    .copied()
                                                    .unwrap_or(descriptor.minimum_mb);
                                                let mut next = budgets;
                                                next.set(descriptor.id, nearest);
                                                let text = nearest.to_string();
                                                let _ = a11y_input.update(cx, |state, cx| {
                                                    state.emplace(&text, cx)
                                                });
                                                callback(
                                                    &ExplorerAction::SetFolderOptionCacheBudgets(next),
                                                    window,
                                                    cx,
                                                );
                                            })
                                        })
                                        .children(stops.into_iter().map(|stop| {
                                            let segment_input = input.clone();
                                            let mut next = budgets;
                                            next.set(descriptor.id, stop);
                                            let action =
                                                ExplorerAction::SetFolderOptionCacheBudgets(next);
                                            div()
                                                .w(px(segment_width))
                                                .h_full()
                                                .bg(if stop <= value {
                                                    tokens.theme.colors.accent.to_gpui()
                                                } else {
                                                    tokens.theme.colors.control_fill.to_gpui()
                                                })
                                                .when_some(
                                                    on_action.clone(),
                                                    |segment, callback| {
                                                        segment.on_mouse_down(
                                                            MouseButton::Left,
                                                            move |_, window, cx| {
                                                                let text = stop.to_string();
                                                                let _ = segment_input.update(
                                                                    cx,
                                                                    |state, cx| {
                                                                        state.emplace(&text, cx)
                                                                    },
                                                                );
                                                                callback(&action, window, cx);
                                                                cx.stop_propagation();
                                                            },
                                                        )
                                                    },
                                                )
                                        })),
                                )
                        });
                    let mut grouped_rows = rows
                        .by_ref()
                        .take(9)
                        .map(IntoElement::into_any_element)
                        .collect::<Vec<_>>();
                    let mft_rows = rows.by_ref().take(5).collect::<Vec<_>>();
                    grouped_rows.push(
                        div()
                            .id("folder-options-mft-service-resources")
                            .role(Role::Group)
                            .aria_label("MFT Service resources")
                            .flex()
                            .flex_col()
                            .gap(px(tokens.layout.control_padding_horizontal.value()))
                            .p(px(tokens.layout.control_padding_horizontal.value()))
                            .rounded(px(tokens.layout.corner_radius.value()))
                            .border(px(1.0))
                            .border_color(tokens.theme.colors.divider.to_gpui())
                            .child(
                                div()
                                    .text_size(px(tokens.typography.address.size.value()))
                                    .child("MFT Service 資源"),
                            )
                            .child(
                                div()
                                    .text_size(px(tokens.typography.tooltip.size.value()))
                                    .text_color(tokens.theme.colors.text_secondary.to_gpui())
                                    .child(
                                        "由所有 SuperExplorer 程序共用；磁碟索引會保留，記憶體快取會在 MFT Service 重新啟動後重建。",
                                    ),
                            )
                            .children(mft_rows)
                            .into_any_element(),
                    );
                    grouped_rows.extend(rows.map(IntoElement::into_any_element));
                    grouped_rows
                }),
        )
}

fn cache_budget_usage_text(
    availability: crate::folder_options_window::CacheUsageAvailabilityV1,
    used_bytes: Option<u64>,
    limit: u64,
) -> String {
    let formatted_limit = crate::formatting::format_file_size(limit);
    match (availability, used_bytes) {
        (crate::folder_options_window::CacheUsageAvailabilityV1::Unavailable, _) => {
            format!("Unavailable / {formatted_limit}")
        }
        (_, Some(bytes)) => format!(
            "{} / {formatted_limit}",
            crate::formatting::format_file_size(bytes),
        ),
        _ => format!("\u{2014} / {formatted_limit}"),
    }
}

fn folder_option_checkbox(
    id: impl Into<gpui::ElementId>,
    label: &'static str,
    checked: bool,
    action: ExplorerAction,
    tokens: UiTokens,
    on_action: Option<ActionCallback>,
) -> impl IntoElement {
    div()
        .id(id)
        .role(Role::CheckBox)
        .aria_label(label)
        .aria_selected(checked)
        .h(px(tokens.layout.minimum_hit_target.value()))
        .flex()
        .items_center()
        .gap(px(tokens.layout.content_spacing.value()))
        .px(px(tokens.layout.content_spacing.value()))
        .rounded(px(tokens.layout.corner_radius.value()))
        .hover(move |style| style.bg(tokens.theme.colors.control_hover.to_gpui()))
        .when_some(on_action, |row, callback| {
            row.on_click(move |_, window, cx| callback(&action, window, cx))
        })
        .child(if checked { "☑" } else { "☐" })
        .child(label)
}

fn folder_option_tab(
    id: &'static str,
    label: &'static str,
    selected: bool,
    action: ExplorerAction,
    tokens: UiTokens,
    on_action: Option<ActionCallback>,
) -> impl IntoElement {
    div()
        .id(id)
        .role(Role::Tab)
        .aria_label(label)
        .aria_selected(selected)
        .min_w(px(crate::layout::folder_options::TAB_MIN_WIDTH.value()))
        .h_full()
        .flex()
        .items_center()
        .justify_center()
        .when(selected, |tab| {
            tab.bg(tokens.theme.colors.selected_active.to_gpui())
        })
        .when_some(on_action, |tab, callback| {
            tab.on_click(move |_, window, cx| callback(&action, window, cx))
        })
        .child(label)
}

fn folder_option_button(
    id: &'static str,
    label: &'static str,
    action: ExplorerAction,
    tokens: UiTokens,
    on_action: Option<ActionCallback>,
) -> impl IntoElement {
    div()
        .id(id)
        .role(Role::Button)
        .aria_label(label)
        .min_w(px(crate::layout::folder_options::BUTTON_MIN_WIDTH.value()))
        .h(px(tokens.layout.minimum_hit_target.value()))
        .flex()
        .items_center()
        .justify_center()
        .rounded(px(tokens.layout.corner_radius.value()))
        .border(px(1.0))
        .border_color(tokens.theme.colors.divider.to_gpui())
        .hover(move |style| style.bg(tokens.theme.colors.control_hover.to_gpui()))
        .when_some(on_action, |button, callback| {
            button.on_click(move |_, window, cx| callback(&action, window, cx))
        })
        .child(label)
}

#[allow(
    clippy::too_many_arguments,
    reason = "the modal button keeps its action, focus target, state, and visual tokens explicit"
)]
fn permanent_delete_dialog_button(
    id: &'static str,
    label: &'static str,
    action: ExplorerAction,
    target: crate::actions::PermanentDeleteDialogTarget,
    focused: bool,
    tokens: UiTokens,
    on_action: Option<ActionCallback>,
) -> impl IntoElement {
    div()
        .id(id)
        .role(Role::Button)
        .aria_label(label)
        .aria_selected(focused)
        .min_w(px(crate::layout::folder_options::BUTTON_MIN_WIDTH.value()))
        .h(px(tokens.layout.minimum_hit_target.value()))
        .flex()
        .items_center()
        .justify_center()
        .rounded(px(tokens.layout.corner_radius.value()))
        .border(px(1.0))
        .border_color(tokens.theme.colors.divider.to_gpui())
        .when(focused, |button| {
            button.bg(tokens.theme.colors.selected_inactive.to_gpui())
        })
        .hover(move |style| style.bg(tokens.theme.colors.selected_inactive.to_gpui()))
        .active(move |style| style.bg(tokens.theme.colors.control_pressed.to_gpui()))
        .when_some(on_action, |button, callback| {
            let pointer_callback = callback.clone();
            button
                .on_mouse_move(move |_, window, cx| {
                    pointer_callback(
                        &ExplorerAction::SetPermanentDeleteDialogFocus { target },
                        window,
                        cx,
                    );
                })
                .on_click(move |_, window, cx| {
                    cx.stop_propagation();
                    callback(&action, window, cx);
                })
        })
        .child(label)
}

/// Installs window-wide drag listeners during GPUI's paint phase. Native `SetCapture` routes
/// pointer messages to this window after the cursor leaves both the thumb and the HWND; this
/// listener then keeps the typed scrollbar reducer as the sole owner of offset changes.
fn captured_left_mouse_up_action(
    marquee_active: bool,
    scrollbar_dragging: Option<crate::interaction::ScrollbarKind>,
    details_column_resizing: bool,
    side_pane_resizing: bool,
) -> Option<ExplorerAction> {
    if marquee_active {
        Some(ExplorerAction::EndMarquee)
    } else if scrollbar_dragging.is_some() {
        Some(ExplorerAction::EndScrollbarDrag {
            reason: crate::interaction::ScrollbarTerminal::PointerUp,
        })
    } else if side_pane_resizing {
        Some(ExplorerAction::EndSidePaneResize)
    } else if details_column_resizing {
        Some(ExplorerAction::EndDetailsColumnResize)
    } else {
        None
    }
}

fn pointer_drag_capture_listener(
    on_action: Option<ActionCallback>,
    scrollbar_dragging: Option<crate::interaction::ScrollbarKind>,
    details_column_resizing: bool,
    side_pane_resizing: bool,
    marquee_active: bool,
    file_origin_x: f32,
    file_origin_y: f32,
    file_scroll: Option<gpui::ScrollHandle>,
    file_viewport_width: f32,
) -> impl IntoElement {
    canvas(
        |_, _, _| (),
        move |_, (), window, _| {
            let move_action = on_action.clone();
            window.on_mouse_event(move |event: &MouseMoveEvent, phase, window, cx| {
                if phase != DispatchPhase::Capture {
                    return;
                }
                if let Some(on_action) = move_action.as_ref() {
                    let action = if marquee_active && event.dragging() {
                        let actual_origin = file_scroll.as_ref().map(|handle| {
                            let bounds = handle.bounds();
                            (f32::from(bounds.left()), f32::from(bounds.top()))
                        });
                        let (x, y) = file_view_local_pointer(
                            f32::from(event.position.x),
                            f32::from(event.position.y),
                            actual_origin,
                            (file_origin_x, file_origin_y),
                        );
                        ExplorerAction::UpdateMarquee {
                            x,
                            y,
                            scroll_y: file_scroll
                                .as_ref()
                                .map_or(0.0, |handle| -f32::from(handle.offset().y)),
                            viewport_width: file_viewport_width,
                        }
                    } else if marquee_active {
                        ExplorerAction::EndMarquee
                    } else if let Some(kind) = scrollbar_dragging.filter(|_| event.dragging()) {
                        ExplorerAction::UpdateScrollbarDrag {
                            pointer_y: if kind
                                == crate::interaction::ScrollbarKind::FileViewHorizontal
                            {
                                f32::from(event.position.x)
                            } else {
                                f32::from(event.position.y)
                            },
                        }
                    } else if scrollbar_dragging.is_some() {
                        ExplorerAction::EndScrollbarDrag {
                            reason: crate::interaction::ScrollbarTerminal::PointerUpOutside,
                        }
                    } else if details_column_resizing && event.dragging() {
                        ExplorerAction::UpdateDetailsColumnResize {
                            pointer_x: f32::from(event.position.x),
                        }
                    } else if side_pane_resizing && event.dragging() {
                        ExplorerAction::UpdateSidePaneResize {
                            pointer_x: f32::from(event.position.x),
                        }
                    } else if side_pane_resizing {
                        ExplorerAction::EndSidePaneResize
                    } else if details_column_resizing {
                        ExplorerAction::EndDetailsColumnResize
                    } else {
                        // External OLE drag-over is also represented as a pressed-button mouse
                        // move. An idle resize-capture listener must not consume that event.
                        return;
                    };
                    on_action(&action, window, cx);
                    cx.stop_propagation();
                }
            });

            let up_action = on_action;
            window.on_mouse_event(move |event: &MouseUpEvent, phase, window, cx| {
                if phase == DispatchPhase::Capture
                    && event.button == MouseButton::Left
                    && let Some(on_action) = up_action.as_ref()
                {
                    let Some(action) = captured_left_mouse_up_action(
                        marquee_active,
                        scrollbar_dragging,
                        details_column_resizing,
                        side_pane_resizing,
                    ) else {
                        // Windows OLE submits an external file drop to GPUI as a synthetic left
                        // MouseUp.  A capture listener that stops every idle MouseUp prevents the
                        // file-view's bubble-phase `on_drop` handler from ever receiving that
                        // terminal event.  Only an active capture-owned interaction may consume
                        // the release; ordinary clicks and external drops must keep propagating.
                        return;
                    };
                    on_action(&action, window, cx);
                    cx.stop_propagation();
                }
            });
        },
    )
    .absolute()
    .size_full()
}

fn side_pane_divider(
    tokens: UiTokens,
    state: &AppViewState,
    on_action: Option<ActionCallback>,
) -> impl IntoElement {
    let settings = state.view_settings();
    let width = if settings.details_pane {
        settings.details_pane_width
    } else {
        settings.preview_pane_width
    };
    let begin = on_action.clone();
    let reset = on_action;
    div()
        .id("side-pane-divider")
        .role(Role::Splitter)
        .aria_label("調整側邊窗格大小")
        .aria_numeric_value(f64::from(width))
        .aria_min_numeric_value(f64::from(tokens.layout.side_pane_min_width.value()))
        .aria_max_numeric_value(f64::from(tokens.layout.side_pane_max_width.value()))
        .w(px(tokens.layout.divider_width.value()))
        .h_full()
        .flex_none()
        .cursor_col_resize()
        .bg(tokens.theme.colors.divider.to_gpui())
        .hover(move |style| style.bg(tokens.theme.colors.focus.to_gpui()))
        .active(move |style| style.bg(tokens.theme.colors.accent.to_gpui()))
        .when_some(begin, |element, callback| {
            element.on_mouse_down(MouseButton::Left, move |event, window, cx| {
                callback(
                    &ExplorerAction::BeginSidePaneResize {
                        pointer_x: f32::from(event.position.x),
                    },
                    window,
                    cx,
                );
            })
        })
        .when_some(reset, |element, callback| {
            element.on_click(move |event, window, cx| {
                if event.click_count() == 2 {
                    callback(&ExplorerAction::ResetSidePaneWidth, window, cx);
                }
            })
        })
}

fn details_side_pane(tokens: UiTokens, state: &AppViewState) -> impl IntoElement {
    let colors = tokens.theme.colors;
    let selected = state
        .tabs()
        .active_tab()
        .visible_snapshot()
        .and_then(|snapshot| {
            snapshot
                .entries()
                .iter()
                .find(|entry| state.tabs().active_tab().selection.contains(&entry.id))
        });
    let selected_count = state.tabs().active_tab().selection.len();
    let title = match (selected_count, selected) {
        (0, _) => "未選取任何項目".to_owned(),
        (1, Some(entry)) => entry.display_name.clone(),
        (1, None) => "無法載入詳細資料".to_owned(),
        (count, _) => format!("{count} 個項目"),
    };
    let modified = selected
        .and_then(|entry| entry.metadata.modified_display.as_deref())
        .unwrap_or("");
    let kind = selected
        .and_then(|entry| entry.metadata.type_display.as_deref())
        .unwrap_or("");
    let size = selected
        .and_then(|entry| entry.metadata.size_bytes)
        .map(format_explorer_size)
        .unwrap_or_default();
    div()
        .id("details-side-pane")
        .role(Role::Complementary)
        .aria_label("詳細資料窗格")
        .w(px(f32::from(state.view_settings().details_pane_width)))
        .h_full()
        .flex_none()
        .flex()
        .flex_col()
        .gap(px(tokens.layout.content_spacing.value()))
        .p(px(tokens.layout.control_padding_horizontal.value()))
        .border_l(px(1.0))
        .border_color(colors.divider.to_gpui())
        .bg(colors.surface.to_gpui())
        .child(title)
        .child(modified.to_owned())
        .child(kind.to_owned())
        .child(size)
}

fn preview_side_pane(
    tokens: UiTokens,
    state: &AppViewState,
    preview_texture: Option<Arc<RenderImage>>,
    preview_failed: bool,
    on_action: Option<ActionCallback>,
) -> impl IntoElement {
    let colors = tokens.theme.colors;
    let selected_count = state.tabs().active_tab().selection.len();
    let selected = state
        .tabs()
        .active_tab()
        .visible_snapshot()
        .and_then(|snapshot| {
            snapshot
                .entries()
                .iter()
                .find(|entry| state.tabs().active_tab().selection.contains(&entry.id))
        });
    let message = match (selected_count, selected.is_some(), preview_failed) {
        (0, _, _) => "選取一個項目以預覽",
        (1, true, true) => "無法產生這個檔案的預覽",
        (1, true, false) if preview_texture.is_none() => "正在載入預覽…",
        (1, false, _) => "無法載入預覽項目",
        (_, _, _) => "選取單一項目以預覽",
    };
    let has_texture = preview_texture.is_some();
    let broker_message = state.broker_health().message();
    let selected_kind = selected
        .and_then(|entry| entry.metadata.type_display.as_deref())
        .unwrap_or("");
    let selected_size = selected
        .and_then(|entry| entry.metadata.size_bytes)
        .map(format_explorer_size)
        .unwrap_or_default();
    let retry_visible = preview_failed || broker_message.is_some();
    div()
        .id("preview-side-pane")
        .role(Role::Complementary)
        .aria_label("預覽窗格")
        .w(px(f32::from(state.view_settings().preview_pane_width)))
        .h_full()
        .flex_none()
        .flex()
        .flex_col()
        .gap(px(tokens.layout.content_spacing.value()))
        .items_center()
        .p(px(tokens.layout.control_padding_horizontal.value()))
        .border_l(px(1.0))
        .border_color(colors.divider.to_gpui())
        .bg(colors.surface.to_gpui())
        .when_some(preview_texture, |pane, texture| {
            pane.child(
                div()
                    .id("preview-image-host")
                    .role(Role::Image)
                    .aria_label("Preview image loaded")
                    .w_full()
                    .flex_1()
                    .min_h(px(crate::layout::feature::PREVIEW_IMAGE_MIN_HEIGHT.value()))
                    .overflow_hidden()
                    .child(img(texture).size_full().object_fit(ObjectFit::Contain)),
            )
        })
        .when_some(selected, |pane, entry| {
            pane.child(
                div()
                    .id("preview-file-name")
                    .role(Role::Label)
                    .aria_label(format!("Preview file: {}", entry.display_name))
                    .w_full()
                    .text_size(px(tokens.typography.address.size.value()))
                    .child(entry.display_name.clone()),
            )
            .child(
                div()
                    .id("preview-file-properties")
                    .role(Role::Label)
                    .aria_label(format!("檔案類型：{selected_kind}；大小：{selected_size}"))
                    .w_full()
                    .text_color(colors.text_secondary.to_gpui())
                    .child(format!("{selected_kind}  {selected_size}")),
            )
        })
        .when(!has_texture, |pane| {
            pane.child(
                div()
                    .id("preview-live-status")
                    .role(Role::Status)
                    .aria_label(message)
                    .flex_1()
                    .flex()
                    .relative()
                    .overflow_hidden()
                    .items_center()
                    .justify_center()
                    .child(message)
                    .child(preview_host_boundary_probe(on_action.clone())),
            )
        })
        .when_some(broker_message, |pane, status| {
            pane.child(
                div()
                    .id("preview-broker-status")
                    .role(Role::Status)
                    .aria_label(status)
                    .child(status),
            )
        })
        .when(retry_visible, |pane| {
            pane.child(semantic_button(
                "preview-broker-retry",
                "重試預覽",
                None,
                Some("重試"),
                Some(ExplorerAction::RetryExtensionBroker),
                state.broker_health() != crate::state::BrokerUiHealth::Retrying,
                tokens,
                on_action,
            ))
        })
}

#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "finite GPUI logical bounds are rounded and clamped before becoming bounded Win32 client coordinates"
)]
fn preview_host_boundary_probe(on_action: Option<ActionCallback>) -> impl IntoElement {
    canvas(
        move |bounds, window: &mut Window, cx: &mut App| {
            let Some(callback) = on_action.clone() else {
                return;
            };
            let Some(hwnd) = crate::pointer_capture::window_handle_value(window)
                .and_then(|value| u64::try_from(value).ok())
            else {
                return;
            };
            let scale = window.scale_factor();
            let window_origin = window.bounds().origin;
            let left = ((f32::from(bounds.origin.x) - f32::from(window_origin.x)) * scale).round();
            let top = ((f32::from(bounds.origin.y) - f32::from(window_origin.y)) * scale).round();
            let width = (f32::from(bounds.size.width) * scale).round();
            let height = (f32::from(bounds.size.height) * scale).round();
            if !left.is_finite()
                || !top.is_finite()
                || !width.is_finite()
                || !height.is_finite()
                || width < 1.0
                || height < 1.0
            {
                return;
            }
            let action = ExplorerAction::UpdatePreviewHostBoundary {
                parent_window: hwnd,
                left_physical: left as i32,
                top_physical: top as i32,
                width_physical: width.clamp(1.0, 16_384.0) as u32,
                height_physical: height.clamp(1.0, 16_384.0) as u32,
                dpi: u32::from(crate::dpi_from_scale(scale)),
            };
            window.defer(cx, move |window, cx| callback(&action, window, cx));
        },
        |_, (), _, _| {},
    )
    .absolute()
    .inset_0()
}

/// M1 command strip. Unsupported commands deliberately have no action.
#[derive(IntoElement)]
pub struct CommandBar {
    tokens: UiTokens,
    state: CommandBarViewModel,
    on_action: Option<ActionCallback>,
    menu_focus: Option<gpui::FocusHandle>,
    extension_view: Option<crate::size_map_view::SizeMapViewConfigV1>,
}

impl CommandBar {
    pub fn new(
        tokens: UiTokens,
        state: CommandBarViewModel,
        on_action: Option<ActionCallback>,
    ) -> Self {
        Self {
            tokens,
            state,
            on_action,
            menu_focus: None,
            extension_view: None,
        }
    }

    #[must_use]
    pub fn with_menu_focus(mut self, handle: Option<gpui::FocusHandle>) -> Self {
        self.menu_focus = handle;
        self
    }

    #[must_use]
    pub fn with_extension_view(
        mut self,
        view: Option<crate::size_map_view::SizeMapViewConfigV1>,
    ) -> Self {
        self.extension_view = view;
        self
    }
}

impl RenderOnce for CommandBar {
    fn render(self, window: &mut Window, _cx: &mut App) -> impl IntoElement {
        let layout = self.tokens.layout;
        let compact = f32::from(window.bounds().size.width) < layout.compact_window_width.value();
        let selection_count = self.state.tabs().active_tab().selection.len();
        let has_selection = selection_count > 0;
        let can_write = self.state.active_presentation().can_write;
        let can_paste = !matches!(
            self.state.clipboard(),
            explorer_model::ClipboardState::None { .. }
                | explorer_model::ClipboardState::Unsupported { .. }
        ) && can_write;
        let new_open = self.state.new_menu_open();
        let new_index = self.state.new_menu_index();
        let new_items = self.state.new_items().to_vec();
        let sort_index = self.state.sort_menu_index();
        let view_index = self.state.view_menu_index();
        let more_open = self.state.more_menu_open();
        let more_index = self.state.more_menu_index();
        let extensions_open = self.state.extensions_menu_open();
        let tortoise_git_available = self.state.tortoise_git_available();
        let loaded_extension_summary = self.state.loaded_extension_summary().map(str::to_owned);
        let extension_commands = self
            .state
            .extensions()
            .iter()
            .filter(|extension| extension.enabled)
            .filter_map(|extension| {
                extension
                    .command_contribution
                    .map(|id| (id, extension.display_name))
            })
            .collect::<Vec<_>>();
        let extension_view = self.extension_view;
        div()
            .id(COMMAND_BAR_ID)
            .debug_selector(|| COMMAND_BAR_ID.to_owned())
            .role(Role::Document)
            .when_some(self.menu_focus, |element, handle| {
                element.track_focus(&handle)
            })
            .relative()
            .aria_label("Explorer command bar")
            .h(px(layout.command_bar_height.value()))
            .flex_none()
            .flex()
            .items_center()
            .gap(px(layout.content_spacing.value()))
            .px(px(layout.control_padding_horizontal.value()))
            .border_b(px(layout.focus_stroke.value()))
            .border_color(self.tokens.theme.colors.divider.to_gpui())
            .bg(self.tokens.theme.colors.surface.to_gpui())
            .child(region_probe(
                COMMAND_BAR_ID,
                Some(EXPLORER_WINDOW_ID),
                "normal",
            ))
            .child(typography_probe(
                COMMAND_BAR_ID,
                typography_diagnostic(self.tokens, self.tokens.typography.command),
            ))
            .when(
                self.state.focused_surface() == FocusSurface::CommandBar,
                |element| {
                    element
                        .border(px(layout.focus_stroke.value()))
                        .border_color(self.tokens.theme.colors.focus.to_gpui())
                },
            )
            .child(semantic_button_with_popup(
                "command-new",
                "Create a new item",
                Some(ExplorerIcon::New),
                Some("新增"),
                Some(ExplorerAction::ToggleNewMenu),
                self.state.active_presentation().can_write,
                self.tokens,
                self.on_action.clone(),
                new_open.then(|| {
                    new_item_menu(self.tokens, new_items, new_index, self.on_action.clone())
                        .into_any_element()
                }),
            ))
            .when(!compact, |element| {
                element
                    .child(semantic_button(
                        "command-cut",
                        "Cut selected items",
                        Some(ExplorerIcon::Cut),
                        None,
                        Some(ExplorerAction::CutSelected),
                        has_selection,
                        self.tokens,
                        self.on_action.clone(),
                    ))
                    .child(semantic_button(
                        "command-copy",
                        "Copy selected items",
                        Some(ExplorerIcon::Copy),
                        None,
                        Some(ExplorerAction::CopySelected),
                        has_selection,
                        self.tokens,
                        self.on_action.clone(),
                    ))
                    .child(semantic_button(
                        "command-paste",
                        "Paste clipboard items",
                        Some(ExplorerIcon::Paste),
                        None,
                        Some(ExplorerAction::Paste),
                        can_paste,
                        self.tokens,
                        self.on_action.clone(),
                    ))
                    .child(semantic_button(
                        "command-rename",
                        "Rename selected item",
                        Some(ExplorerIcon::Rename),
                        None,
                        Some(ExplorerAction::BeginRenameFocused),
                        selection_count == 1 && can_write,
                        self.tokens,
                        self.on_action.clone(),
                    ))
                    .child(semantic_button(
                        "command-share",
                        "Share selected items",
                        Some(ExplorerIcon::Share),
                        None,
                        Some(ExplorerAction::ShareSelected),
                        has_selection,
                        self.tokens,
                        self.on_action.clone(),
                    ))
                    .child(semantic_button(
                        "command-delete",
                        "Move selected items to the Recycle Bin",
                        Some(ExplorerIcon::Delete),
                        None,
                        Some(ExplorerAction::RecycleDeleteSelected),
                        has_selection,
                        self.tokens,
                        self.on_action.clone(),
                    ))
            })
            .child(semantic_button_with_popup(
                "command-sort",
                "Sort",
                Some(ExplorerIcon::Sort),
                Some("排序"),
                Some(ExplorerAction::ToggleSortMenu),
                true,
                self.tokens,
                self.on_action.clone(),
                self.state.sort_menu_open().then(|| {
                    sort_menu(
                        self.tokens,
                        &self.state.view_settings(),
                        sort_index,
                        self.on_action.clone(),
                    )
                    .into_any_element()
                }),
            ))
            .child(semantic_button_with_popup(
                "command-view",
                "View",
                Some(ExplorerIcon::View),
                Some("檢視"),
                Some(ExplorerAction::ToggleViewMenu),
                true,
                self.tokens,
                self.on_action.clone(),
                self.state.view_menu_open().then(|| {
                    view_menu(
                        self.tokens,
                        self.state.view_settings(),
                        self.state.view_show_submenu_open(),
                        extension_view,
                        view_index,
                        self.on_action.clone(),
                    )
                    .into_any_element()
                }),
            ))
            .child(semantic_button_with_popup(
                "command-more-menu",
                "其它",
                Some(ExplorerIcon::More),
                None,
                Some(ExplorerAction::ToggleMoreMenu),
                true,
                self.tokens,
                self.on_action.clone(),
                more_open.then(|| {
                    command_more_menu_v2(
                        self.tokens,
                        has_selection,
                        more_index,
                        self.on_action.clone(),
                    )
                    .into_any_element()
                }),
            ))
            .child(semantic_button_with_popup(
                "command-extensions-menu",
                "擴充功能",
                Some(ExplorerIcon::Details),
                Some("擴充功能"),
                Some(ExplorerAction::ToggleExtensionsMenu),
                true,
                self.tokens,
                self.on_action.clone(),
                extensions_open.then(|| {
                    command_extensions_menu(
                        self.tokens,
                        tortoise_git_available,
                        loaded_extension_summary,
                        extension_commands,
                        self.state.extension_command_panel(),
                        has_selection,
                        self.on_action.clone(),
                    )
                    .into_any_element()
                }),
            ))
    }
}

#[allow(
    clippy::needless_pass_by_value,
    reason = "render builders clone and move callbacks into independent GPUI handlers"
)]
fn new_item_menu(
    tokens: UiTokens,
    items: Vec<explorer_model::ShellNewItemDescriptor>,
    focused_index: usize,
    on_action: Option<ActionCallback>,
) -> impl IntoElement {
    let outside = on_action.clone();
    let menu = div()
        .id("command-new-popup")
        .role(Role::Menu)
        .aria_label("新增")
        .occlude()
        .min_w(px(tokens.layout.address_min_width.value()))
        .max_h(px(crate::layout::feature::NEW_MENU_MAX_HEIGHT.value()))
        .overflow_y_scroll()
        .p(px(tokens.layout.content_spacing.value()))
        .rounded(px(tokens.layout.corner_radius.value()))
        .bg(tokens.theme.colors.menu_fill.to_gpui())
        .border(px(1.0))
        .border_color(tokens.theme.colors.divider.to_gpui())
        .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
        .when_some(outside, |menu, callback| {
            menu.on_mouse_up_out(MouseButton::Left, move |_, window, cx| {
                callback(&ExplorerAction::CloseNewMenu, window, cx);
            })
        })
        .children(items.into_iter().enumerate().map(|(index, descriptor)| {
            let id = format!("new-item-{}", descriptor.stable_id);
            let label = descriptor.display_name;
            let action = ExplorerAction::CreateNewItem { index };
            let callback = on_action.clone();
            div()
                .id(id)
                .role(Role::MenuItem)
                .aria_label(label.clone())
                .aria_selected(index == focused_index)
                .h(px(tokens.layout.minimum_hit_target.value()))
                .flex()
                .items_center()
                .gap(px(tokens.layout.content_spacing.value()))
                .px(px(tokens.layout.control_padding_horizontal.value()))
                .rounded(px(tokens.layout.corner_radius.value() / 2.0))
                .when(index == focused_index, |item| {
                    item.bg(tokens.theme.colors.selected_active.to_gpui())
                })
                .hover(move |style| style.bg(tokens.theme.colors.control_hover.to_gpui()))
                .active(move |style| style.bg(tokens.theme.colors.control_pressed.to_gpui()))
                .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                .when_some(callback, move |item, callback| {
                    let a11y_callback = callback.clone();
                    let a11y_action = action.clone();
                    item.on_click(move |_, window, cx| {
                        cx.stop_propagation();
                        callback(&action, window, cx);
                    })
                    .on_a11y_action(
                        AccessibleAction::Click,
                        move |_, window, cx| {
                            a11y_callback(&a11y_action, window, cx);
                        },
                    )
                })
                .child(chrome_icon("new-item-kind", ExplorerIcon::Details, tokens))
                .child(label)
        }));
    deferred(
        div()
            .absolute()
            .top(px(tokens.layout.minimum_hit_target.value()))
            .left_0()
            .child(menu),
    )
    .with_priority(150)
}

#[allow(dead_code)]
fn command_extensions_menu_legacy(
    tokens: UiTokens,
    tortoise_git_available: bool,
    loaded_extension_summary: Option<String>,
    extension_commands: Vec<(&'static str, &'static str)>,
    on_action: Option<ActionCallback>,
) -> impl IntoElement {
    let outside = on_action.clone();
    let menu = div()
        .id("command-extensions-popup")
        .role(Role::Menu)
        .occlude()
        .aria_label("擴充功能")
        .min_w(px(tokens.layout.navigation_pane_min_width.value()))
        .p(px(tokens.layout.content_spacing.value()))
        .rounded(px(tokens.layout.corner_radius.value()))
        .bg(tokens.theme.colors.menu_fill.to_gpui())
        .border(px(1.0))
        .border_color(tokens.theme.colors.divider.to_gpui())
        .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
        .when_some(outside, |menu, callback| {
            menu.on_mouse_up_out(MouseButton::Left, move |_, window, cx| {
                callback(&ExplorerAction::CloseExtensionsMenu, window, cx);
            })
        })
        .when_some(loaded_extension_summary, |menu, summary| {
            menu.child(
                div()
                    .id("extensions-loaded-development-plugin")
                    .role(Role::MenuItem)
                    .aria_label(summary.clone())
                    .h(px(tokens.layout.minimum_hit_target.value()))
                    .flex()
                    .items_center()
                    .px(px(tokens.layout.control_padding_horizontal.value()))
                    .text_color(tokens.theme.colors.text_primary.to_gpui())
                    .child(summary),
            )
        })
        .children(
            extension_commands
                .into_iter()
                .map(|(contribution_id, label)| {
                    let element_id = match contribution_id {
                        "lua-bulk-folder:button" => "extension-command-lua-bulk-folder-button",
                        "rust-exif-rename:button" => "extension-command-rust-exif-rename-button",
                        _ => "extension-command-unknown",
                    };
                    command_more_item(
                        element_id,
                        label,
                        ExplorerAction::InvokeExtensionCommand {
                            contribution_id: contribution_id.to_owned(),
                        },
                        true,
                        false,
                        None,
                        tokens,
                        on_action.clone(),
                    )
                }),
        )
        .child(command_more_item(
            "extensions-refresh-tortoisegit",
            if tortoise_git_available {
                "更新 TortoiseGit 狀態"
            } else {
                "沒有可用的擴充功能"
            },
            ExplorerAction::RefreshTortoiseGitStatus,
            tortoise_git_available,
            false,
            None,
            tokens,
            on_action,
        ));
    deferred(
        div()
            .absolute()
            .top(px(tokens.layout.minimum_hit_target.value()))
            .left_0()
            .right_0()
            .flex()
            .justify_center()
            .child(menu),
    )
    .with_priority(140)
}

fn command_more_menu_v2(
    tokens: UiTokens,
    has_selection: bool,
    focused_index: usize,
    on_action: Option<ActionCallback>,
) -> impl IntoElement {
    let item = |id, label, action, enabled, index, callback| {
        let hover_action = ExplorerAction::SetMoreMenuFocus { index };
        command_more_item(
            id,
            label,
            action,
            enabled,
            focused_index == index,
            Some(hover_action),
            tokens,
            callback,
        )
    };
    let separator = || {
        div()
            .h(px(tokens.layout.focus_stroke.value() / 2.0))
            .my(px(tokens.layout.content_spacing.value()))
            .bg(tokens.theme.colors.divider.to_gpui())
    };
    let outside = on_action.clone();
    let menu = div()
        .id("command-more-popup")
        .role(Role::Menu)
        .occlude()
        .aria_label("更多命令")
        .w(px(tokens.layout.address_min_width.value()))
        .p(px(tokens.layout.content_spacing.value()))
        .rounded(px(tokens.layout.corner_radius.value()))
        .bg(tokens.theme.colors.menu_fill.to_gpui())
        .border(px(1.0))
        .border_color(tokens.theme.colors.divider.to_gpui())
        .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
        .when_some(outside, |menu, callback| {
            menu.on_mouse_up_out(MouseButton::Left, move |_, window, cx| {
                callback(&ExplorerAction::CloseMoreMenu, window, cx);
            })
        })
        .child(item(
            "more-undo",
            "復原",
            ExplorerAction::UndoCurrentFolder,
            true,
            0,
            on_action.clone(),
        ))
        .child(item(
            "more-compress-zip",
            "壓縮成 ZIP 檔案",
            ExplorerAction::CompressSelectedToZip,
            has_selection,
            1,
            on_action.clone(),
        ))
        .child(item(
            "more-add-favorite",
            "加到我的最愛",
            ExplorerAction::AddSelectedToFavorites,
            has_selection,
            2,
            on_action.clone(),
        ))
        .child(item(
            "more-add-bookmark",
            "加入書籤",
            ExplorerAction::AddSelectedToBookmarks,
            has_selection,
            3,
            on_action.clone(),
        ))
        .child(item(
            "more-copy-path",
            "複製路徑",
            ExplorerAction::CopySelectedPaths,
            has_selection,
            4,
            on_action.clone(),
        ))
        .child(separator())
        .child(item(
            "more-select-all",
            "全選",
            ExplorerAction::SelectAllItems,
            true,
            5,
            on_action.clone(),
        ))
        .child(item(
            "more-select-none",
            "全部不選",
            ExplorerAction::ClearSelection,
            has_selection,
            6,
            on_action.clone(),
        ))
        .child(item(
            "more-invert-selection",
            "反向選擇",
            ExplorerAction::InvertSelection,
            true,
            7,
            on_action.clone(),
        ))
        .child(separator())
        .child(item(
            "more-options",
            "選項",
            ExplorerAction::OpenFolderOptions,
            true,
            8,
            on_action.clone(),
        ))
        .child(item(
            "more-about",
            "關於",
            ExplorerAction::OpenAboutDialog,
            true,
            9,
            on_action,
        ));
    deferred(
        div()
            .absolute()
            .top(px(tokens.layout.minimum_hit_target.value()))
            .left_0()
            .right_0()
            .flex()
            .justify_center()
            .child(menu),
    )
    .with_priority(140)
}

#[allow(dead_code, reason = "retained temporarily for migration coverage")]
fn command_more_menu(
    tokens: UiTokens,
    has_selection: bool,
    can_write: bool,
    can_paste: bool,
    focused_index: usize,
    on_action: Option<ActionCallback>,
) -> impl IntoElement {
    let outside = on_action.clone();
    let menu = div()
        .id("command-more-popup")
        .role(Role::Menu)
        .occlude()
        .aria_label("More commands")
        .min_w(px(tokens.layout.navigation_pane_min_width.value()))
        .p(px(tokens.layout.content_spacing.value()))
        .rounded(px(tokens.layout.corner_radius.value()))
        .bg(tokens.theme.colors.menu_fill.to_gpui())
        .border(px(1.0))
        .border_color(tokens.theme.colors.divider.to_gpui())
        .when_some(outside, |menu, callback| {
            menu.on_mouse_up_out(MouseButton::Left, move |_, window, cx| {
                callback(&ExplorerAction::CloseMoreMenu, window, cx);
            })
        })
        .child(command_more_item(
            "more-new-folder",
            "新增資料夾",
            ExplorerAction::CreateFolder,
            can_write,
            focused_index == 0,
            None,
            tokens,
            on_action.clone(),
        ))
        .child(command_more_item(
            "more-cut",
            "剪下",
            ExplorerAction::CutSelected,
            has_selection,
            focused_index == 1,
            None,
            tokens,
            on_action.clone(),
        ))
        .child(command_more_item(
            "more-copy",
            "複製",
            ExplorerAction::CopySelected,
            has_selection,
            focused_index == 2,
            None,
            tokens,
            on_action.clone(),
        ))
        .child(command_more_item(
            "more-paste",
            "貼上",
            ExplorerAction::Paste,
            can_paste,
            focused_index == 3,
            None,
            tokens,
            on_action.clone(),
        ))
        .child(command_more_item(
            "more-delete",
            "刪除",
            ExplorerAction::RecycleDeleteSelected,
            has_selection,
            focused_index == 4,
            None,
            tokens,
            on_action,
        ));
    deferred(
        div()
            .absolute()
            .top(px(tokens.layout.minimum_hit_target.value()))
            .left_0()
            .right_0()
            .flex()
            .justify_center()
            .child(menu),
    )
    .with_priority(120)
}

fn command_extensions_menu(
    tokens: UiTokens,
    tortoise_git_available: bool,
    loaded_extension_summary: Option<String>,
    extension_commands: Vec<(&'static str, &'static str)>,
    panel: Option<ExtensionCommandPanel>,
    has_selection: bool,
    on_action: Option<ActionCallback>,
) -> impl IntoElement {
    let menu = div()
        .id("command-extensions-popup-v2")
        .role(Role::Menu)
        .aria_label("擴充功能")
        .occlude()
        .w(px(400.0))
        .overflow_hidden()
        .p(px(tokens.layout.content_spacing.value()))
        .rounded(px(tokens.layout.corner_radius.value()))
        .bg(tokens.theme.colors.menu_fill.to_gpui())
        .border(px(1.0))
        .border_color(tokens.theme.colors.divider.to_gpui())
        .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
        .when_some(loaded_extension_summary, |menu, summary| {
            menu.child(
                div()
                    .id("extensions-loaded-development-plugin-v2")
                    .role(Role::MenuItem)
                    .aria_label(summary.clone())
                    .h(px(tokens.layout.minimum_hit_target.value()))
                    .flex()
                    .min_w_0()
                    .items_center()
                    .px(px(tokens.layout.control_padding_horizontal.value()))
                    .text_color(tokens.theme.colors.text_primary.to_gpui())
                    .child(
                        div()
                            .min_w_0()
                            .flex_1()
                            .overflow_hidden()
                            .whitespace_nowrap()
                            .text_ellipsis()
                            .child(summary),
                    ),
            )
        })
        .when(panel.is_none(), |menu| {
            menu.children(
                extension_commands
                    .into_iter()
                    .map(|(contribution_id, label)| {
                        let element_id = match contribution_id {
                            "lua-bulk-folder:button" => {
                                "extension-command-lua-bulk-folder-button-v2"
                            }
                            "rust-exif-rename:button" => {
                                "extension-command-rust-exif-rename-button-v2"
                            }
                            _ => "extension-command-unknown-v2",
                        };
                        command_more_item(
                            element_id,
                            label,
                            ExplorerAction::InvokeExtensionCommand {
                                contribution_id: contribution_id.to_owned(),
                            },
                            true,
                            false,
                            None,
                            tokens,
                            on_action.clone(),
                        )
                    }),
            )
            .child(command_more_item(
                "extensions-refresh-tortoisegit-v2",
                if tortoise_git_available {
                    "更新 TortoiseGit 狀態"
                } else {
                    "沒有可用的擴充功能"
                },
                ExplorerAction::RefreshTortoiseGitStatus,
                tortoise_git_available,
                false,
                None,
                tokens,
                on_action.clone(),
            ))
        })
        .when_some(panel, |menu, panel| {
            menu.child(extension_command_panel(
                panel,
                has_selection,
                tokens,
                on_action.clone(),
            ))
        });
    deferred(
        div()
            .absolute()
            .top(px(tokens.layout.minimum_hit_target.value()))
            .left_0()
            .right_0()
            .flex()
            .justify_center()
            .child(menu),
    )
    .with_priority(120)
}

fn extension_command_panel(
    selected_panel: ExtensionCommandPanel,
    has_selection: bool,
    tokens: UiTokens,
    on_action: Option<ActionCallback>,
) -> impl IntoElement {
    let panel = div()
        .id("extension-command-panel")
        .role(Role::Menu)
        .min_w_0()
        .overflow_hidden();
    match selected_panel {
        ExtensionCommandPanel::ExifRename => panel
            .child(command_panel_heading("Rename from EXIF", tokens))
            .child(command_more_item(
                "extension-exif-date-time",
                "依拍攝日期改名（20260805_123456）",
                ExplorerAction::RunExifRenamePreset {
                    preset: ExifRenamePreset::DateTime,
                },
                has_selection,
                false,
                None,
                tokens,
                on_action.clone(),
            ))
            .child(command_more_item(
                "extension-exif-date-original",
                "拍攝日期 + 原檔名",
                ExplorerAction::RunExifRenamePreset {
                    preset: ExifRenamePreset::DateTimeAndOriginal,
                },
                has_selection,
                false,
                None,
                tokens,
                on_action.clone(),
            ))
            .child(command_more_item(
                "extension-command-panel-cancel",
                "取消",
                ExplorerAction::CloseExtensionCommandPanel,
                true,
                false,
                None,
                tokens,
                on_action,
            )),
        ExtensionCommandPanel::BulkFolder => panel
            .child(command_panel_heading("Bulk folder generator", tokens))
            .child(command_more_item(
                "extension-bulk-create-10",
                "建立 10 個資料夾（Folder-001…010）",
                ExplorerAction::RunBulkFolderPreset { count: 10 },
                true,
                false,
                None,
                tokens,
                on_action.clone(),
            ))
            .child(command_more_item(
                "extension-bulk-create-100",
                "建立 100 個資料夾（Folder-001…100）",
                ExplorerAction::RunBulkFolderPreset { count: 100 },
                true,
                false,
                None,
                tokens,
                on_action.clone(),
            ))
            .child(command_more_item(
                "extension-command-panel-cancel",
                "取消",
                ExplorerAction::CloseExtensionCommandPanel,
                true,
                false,
                None,
                tokens,
                on_action,
            )),
    }
}

fn command_panel_heading(label: &'static str, tokens: UiTokens) -> impl IntoElement {
    div()
        .id("extension-command-panel-heading")
        .role(Role::MenuItem)
        .h(px(tokens.layout.minimum_hit_target.value()))
        .flex()
        .min_w_0()
        .items_center()
        .px(px(tokens.layout.control_padding_horizontal.value()))
        .text_color(tokens.theme.colors.text_primary.to_gpui())
        .child(
            div()
                .min_w_0()
                .flex_1()
                .overflow_hidden()
                .whitespace_nowrap()
                .text_ellipsis()
                .child(label),
        )
}

fn command_more_item(
    id: &'static str,
    label: &'static str,
    action: ExplorerAction,
    enabled: bool,
    selected: bool,
    hover_action: Option<ExplorerAction>,
    tokens: UiTokens,
    on_action: Option<ActionCallback>,
) -> impl IntoElement {
    div()
        .id(id)
        .role(Role::MenuItem)
        .aria_label(label)
        .aria_selected(selected)
        .h(px(tokens.layout.minimum_hit_target.value()))
        .flex()
        .min_w_0()
        .overflow_hidden()
        .items_center()
        .px(px(tokens.layout.control_padding_horizontal.value()))
        .text_color(if enabled {
            tokens.theme.colors.text_primary.to_gpui()
        } else {
            tokens.theme.colors.text_secondary.to_gpui()
        })
        .when(selected, |item| {
            item.bg(tokens.theme.colors.selected_inactive.to_gpui())
        })
        .when(enabled, |item| {
            item.hover(move |style| style.bg(tokens.theme.colors.selected_inactive.to_gpui()))
                .active(move |style| style.bg(tokens.theme.colors.control_pressed.to_gpui()))
                .when_some(on_action, move |item, callback| {
                    let hover_callback = callback.clone();
                    let a11y_callback = callback.clone();
                    let a11y_action = action.clone();
                    item.when_some(hover_action, move |item, hover_action| {
                        item.on_mouse_move(move |_, window, cx| {
                            hover_callback(&hover_action, window, cx);
                        })
                    })
                    .on_click(move |_, window, cx| {
                        cx.stop_propagation();
                        callback(&action, window, cx);
                    })
                    .on_a11y_action(
                        AccessibleAction::Click,
                        move |_, window, cx| {
                            a11y_callback(&a11y_action, window, cx);
                        },
                    )
                })
        })
        .child(
            div()
                .min_w_0()
                .flex_1()
                .overflow_hidden()
                .whitespace_nowrap()
                .text_ellipsis()
                .child(label),
        )
}

fn sort_menu(
    tokens: UiTokens,
    settings: &explorer_model::ViewSettings,
    focused_index: usize,
    on_action: Option<ActionCallback>,
) -> impl IntoElement {
    let layout = tokens.layout;
    let outside_action = on_action.clone();
    let menu = div()
        .id("sort-menu")
        .debug_selector(|| "sort-menu".to_owned())
        .role(Role::Menu)
        .occlude()
        .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
        .w(px(
            layout.navigation_pane_min_width.value() + layout.minimum_hit_target.value()
        ))
        .p(px(layout.content_spacing.value()))
        .rounded(px(layout.corner_radius.value()))
        .bg(tokens.theme.colors.menu_fill.to_gpui())
        .border(px(1.0))
        .border_color(tokens.theme.colors.divider.to_gpui())
        .when_some(outside_action, |menu, callback| {
            menu.on_mouse_up_out(MouseButton::Left, move |_, window, cx| {
                callback(&ExplorerAction::CloseSortMenu, window, cx);
            })
        })
        .child(view_menu_item(
            "sort-name".to_owned(),
            "名稱",
            settings.sort.column == explorer_model::ColumnId::Name,
            false,
            focused_index == 0,
            Some(ExplorerAction::SetSortMenuFocus { index: 0 }),
            ExplorerAction::SetColumnId(explorer_model::ColumnId::Name),
            tokens,
            on_action.clone(),
        ))
        .child(view_menu_item(
            "sort-date".to_owned(),
            "修改日期",
            settings.sort.column == explorer_model::ColumnId::DateModified,
            false,
            focused_index == 1,
            Some(ExplorerAction::SetSortMenuFocus { index: 1 }),
            ExplorerAction::SetColumnId(explorer_model::ColumnId::DateModified),
            tokens,
            on_action.clone(),
        ))
        .child(view_menu_item(
            "sort-type".to_owned(),
            "類型",
            settings.sort.column == explorer_model::ColumnId::Type,
            false,
            focused_index == 2,
            Some(ExplorerAction::SetSortMenuFocus { index: 2 }),
            ExplorerAction::SetColumnId(explorer_model::ColumnId::Type),
            tokens,
            on_action.clone(),
        ))
        .child(view_menu_item(
            "sort-size".to_owned(),
            "大小",
            settings.sort.column == explorer_model::ColumnId::Size,
            false,
            focused_index == 3,
            Some(ExplorerAction::SetSortMenuFocus { index: 3 }),
            ExplorerAction::SetColumnId(explorer_model::ColumnId::Size),
            tokens,
            on_action.clone(),
        ))
        .child(view_menu_separator(tokens))
        .child(view_menu_item(
            "sort-ascending".to_owned(),
            "遞增",
            settings.sort.direction == explorer_model::SortDirection::Ascending,
            false,
            focused_index == 4,
            Some(ExplorerAction::SetSortMenuFocus { index: 4 }),
            ExplorerAction::SetSortDirection(explorer_model::SortDirection::Ascending),
            tokens,
            on_action.clone(),
        ))
        .child(view_menu_item(
            "sort-descending".to_owned(),
            "遞減",
            settings.sort.direction == explorer_model::SortDirection::Descending,
            false,
            focused_index == 5,
            Some(ExplorerAction::SetSortMenuFocus { index: 5 }),
            ExplorerAction::SetSortDirection(explorer_model::SortDirection::Descending),
            tokens,
            on_action,
        ));
    deferred(
        div()
            .absolute()
            .top(px(layout.minimum_hit_target.value()))
            .left_0()
            .right_0()
            .flex()
            .justify_center()
            .child(menu),
    )
    .with_priority(90)
}

#[allow(
    clippy::needless_pass_by_value,
    reason = "the GPUI menu builder may move settings into independent deferred child elements"
)]
fn view_menu(
    tokens: UiTokens,
    settings: explorer_model::ViewSettings,
    show_submenu: bool,
    extension_view: Option<crate::size_map_view::SizeMapViewConfigV1>,
    focused_index: usize,
    on_action: Option<ActionCallback>,
) -> impl IntoElement {
    let layout = tokens.layout;
    let colors = tokens.theme.colors;
    let outside_action = on_action.clone();
    let modes = [
        (explorer_model::ViewMode::ExtraLargeIcons, "超大圖示"),
        (explorer_model::ViewMode::LargeIcons, "大圖示"),
        (explorer_model::ViewMode::MediumIcons, "中圖示"),
        (explorer_model::ViewMode::SmallIcons, "小圖示"),
        (explorer_model::ViewMode::List, "清單"),
        (explorer_model::ViewMode::Details, "詳細資料"),
        (explorer_model::ViewMode::Tiles, "並排"),
        (explorer_model::ViewMode::Content, "內容"),
    ];
    let menu = div()
        .id("view-menu")
        .debug_selector(|| "view-menu".to_owned())
        .role(Role::Menu)
        .occlude()
        .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
        .w(px(
            layout.navigation_pane_min_width.value() + layout.minimum_hit_target.value()
        ))
        .p(px(layout.content_spacing.value()))
        .rounded(px(layout.corner_radius.value()))
        .bg(colors.menu_fill.to_gpui())
        .border(px(1.0))
        .border_color(colors.divider.to_gpui())
        .when_some(outside_action, |menu, callback| {
            menu.on_mouse_up_out(MouseButton::Left, move |_, window, cx| {
                callback(&ExplorerAction::CloseViewMenu, window, cx);
            })
        })
        .children(modes.into_iter().enumerate().map(|(index, (mode, label))| {
            view_menu_item(
                format!("view-mode-{mode:?}"),
                label,
                settings.mode == mode,
                false,
                focused_index == index,
                Some(ExplorerAction::SetViewMenuFocus { index }),
                ExplorerAction::SetViewMode(mode),
                tokens,
                on_action.clone(),
            )
        }))
        .child(view_menu_separator(tokens))
        .child(view_menu_item(
            "view-details-pane".to_owned(),
            "詳細資料窗格",
            settings.details_pane,
            false,
            focused_index == 8,
            Some(ExplorerAction::SetViewMenuFocus { index: 8 }),
            ExplorerAction::ToggleDetailsPane,
            tokens,
            on_action.clone(),
        ))
        .child(view_menu_item(
            "view-preview-pane".to_owned(),
            "預覽窗格",
            settings.preview_pane,
            false,
            focused_index == 9,
            Some(ExplorerAction::SetViewMenuFocus { index: 9 }),
            ExplorerAction::TogglePreviewPane,
            tokens,
            on_action.clone(),
        ))
        .child(view_menu_separator(tokens))
        .child(view_menu_item(
            "view-show-submenu".to_owned(),
            "顯示",
            false,
            true,
            focused_index == 10,
            Some(ExplorerAction::SetViewMenuFocus { index: 10 }),
            ExplorerAction::ToggleViewShowSubmenu,
            tokens,
            on_action.clone(),
        ))
        .when_some(extension_view, |element, extension| {
            let checked = settings
                .extension_view_id
                .as_deref()
                .is_some_and(|id| id == extension.view_id);
            element
                .child(view_menu_separator(tokens))
                .child(view_menu_item(
                    "view-extension-size-map".to_owned(),
                    "Size Map",
                    checked,
                    false,
                    focused_index == 11,
                    Some(ExplorerAction::SetViewMenuFocus { index: 11 }),
                    ExplorerAction::SetExtensionView {
                        view_id: extension.view_id,
                    },
                    tokens,
                    on_action.clone(),
                ))
        })
        .when(show_submenu, |element| {
            element.child(view_show_submenu(tokens, &settings, on_action))
        });
    deferred(
        div()
            .absolute()
            .top(px(layout.minimum_hit_target.value()))
            .left_0()
            .right_0()
            .flex()
            .justify_center()
            // Deferred paint must also own hit testing; otherwise the menu is
            // visible above the file view while clicks reach rows underneath.
            .occlude()
            .child(menu),
    )
    .with_priority(90)
}

fn view_show_submenu(
    tokens: UiTokens,
    settings: &explorer_model::ViewSettings,
    on_action: Option<ActionCallback>,
) -> impl IntoElement {
    let layout = tokens.layout;
    let colors = tokens.theme.colors;
    div()
        .id("view-show-menu")
        .role(Role::Menu)
        .occlude()
        .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
        .absolute()
        .top(px(layout.menu_row_height.value() * 9.0))
        .left(px(
            layout.navigation_pane_min_width.value() + layout.minimum_hit_target.value()
        ))
        .w(px(
            layout.navigation_pane_min_width.value() + layout.minimum_hit_target.value()
        ))
        .p(px(layout.content_spacing.value()))
        .rounded(px(layout.corner_radius.value()))
        .bg(colors.menu_fill.to_gpui())
        .border(px(1.0))
        .border_color(colors.divider.to_gpui())
        .child(view_menu_item(
            "view-item-check-boxes".to_owned(),
            "項目核取方塊",
            settings.item_check_boxes,
            false,
            false,
            None,
            ExplorerAction::ToggleItemCheckBoxes,
            tokens,
            on_action.clone(),
        ))
        .child(view_menu_item(
            "view-file-name-extensions".to_owned(),
            "檔案副檔名",
            settings.file_name_extensions,
            false,
            false,
            None,
            ExplorerAction::ToggleFileNameExtensions,
            tokens,
            on_action.clone(),
        ))
        .child(view_menu_item(
            "view-hidden-items".to_owned(),
            "隱藏的項目",
            settings.hidden_items,
            false,
            false,
            None,
            ExplorerAction::ToggleHiddenItems,
            tokens,
            on_action.clone(),
        ))
        .child(view_menu_item(
            "view-compact".to_owned(),
            "精簡檢視",
            settings.compact_view,
            false,
            false,
            None,
            ExplorerAction::ToggleCompactView,
            tokens,
            on_action,
        ))
}

#[allow(clippy::too_many_arguments)]
#[allow(
    clippy::needless_pass_by_value,
    reason = "render builders clone and move callbacks into independent GPUI handlers"
)]
fn view_menu_item(
    id: String,
    label: &'static str,
    checked: bool,
    submenu: bool,
    focused: bool,
    hover_action: Option<ExplorerAction>,
    action: ExplorerAction,
    tokens: UiTokens,
    on_action: Option<ActionCallback>,
) -> gpui::AnyElement {
    let colors = tokens.theme.colors;
    // AccessKit's Windows provider currently exposes no InvokePattern for a
    // custom MenuItem. Expose every actionable view choice as a focusable
    // button so built-in and extension views share the same keyboard, UIA and
    // pointer activation path.
    div()
        .id(id.clone())
        .debug_selector(move || id.clone())
        .role(Role::Button)
        .aria_label(label)
        .aria_selected(focused)
        .focusable()
        .tab_stop(true)
        .h(px(tokens.layout.menu_row_height.value()))
        .flex()
        .items_center()
        .gap(px(tokens.layout.content_spacing.value()))
        .px(px(tokens.layout.content_spacing.value()))
        .rounded(px(tokens.layout.corner_radius.value() / 2.0))
        .when(focused, |item| item.bg(colors.selected_inactive.to_gpui()))
        .hover(move |style| style.bg(colors.selected_inactive.to_gpui()))
        .active(move |style| style.bg(colors.control_pressed.to_gpui()))
        .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
        .when_some(on_action.clone(), |element, callback| {
            let hover_callback = callback.clone();
            let accessibility_callback = callback.clone();
            let accessibility_action = action.clone();
            element
                .when_some(hover_action, move |element, hover_action| {
                    element.on_mouse_move(move |_, window, cx| {
                        hover_callback(&hover_action, window, cx);
                    })
                })
                .on_click(move |_, window, cx| {
                    cx.stop_propagation();
                    callback(&action, window, cx);
                })
                .on_a11y_action(AccessibleAction::Click, move |_, window, cx| {
                    accessibility_callback(&accessibility_action, window, cx);
                })
        })
        .child(
            div()
                .w(px(tokens.layout.content_spacing.value()))
                .h(px(tokens.layout.content_spacing.value()))
                .flex_none()
                .rounded(px(tokens.layout.content_spacing.value()))
                .bg(if checked {
                    colors.text_primary.to_gpui()
                } else {
                    colors.menu_fill.to_gpui()
                }),
        )
        .child(div().flex_1().child(label))
        .when(submenu, |element| {
            element.child(chrome_icon(
                "view-show-submenu",
                ExplorerIcon::Chevron,
                tokens,
            ))
        })
        .into_any_element()
}

fn view_menu_separator(tokens: UiTokens) -> impl IntoElement {
    div()
        .h(px(tokens.layout.content_spacing.value()))
        .border_b(px(1.0))
        .border_color(tokens.theme.colors.divider.to_gpui())
}

#[derive(IntoElement)]
pub struct NavigationBar {
    tokens: UiTokens,
    state: NavigationAddressViewModel,
    on_action: Option<ActionCallback>,
    address_input: Option<gpui::WeakEntity<EditableTextState>>,
    search_input: Option<gpui::WeakEntity<EditableTextState>>,
    breadcrumb_menu_focus: Option<gpui::FocusHandle>,
    shell_icons: HashMap<explorer_model::ShellIconKey, Arc<RenderImage>>,
    shell_icon_dpi: u16,
}

impl NavigationBar {
    pub const fn new(
        tokens: UiTokens,
        state: NavigationAddressViewModel,
        address_input: Option<gpui::WeakEntity<EditableTextState>>,
        search_input: Option<gpui::WeakEntity<EditableTextState>>,
        breadcrumb_menu_focus: Option<gpui::FocusHandle>,
        shell_icons: HashMap<explorer_model::ShellIconKey, Arc<RenderImage>>,
        shell_icon_dpi: u16,
        on_action: Option<ActionCallback>,
    ) -> Self {
        Self {
            tokens,
            state,
            on_action,
            address_input,
            search_input,
            breadcrumb_menu_focus,
            shell_icons,
            shell_icon_dpi,
        }
    }
}

impl RenderOnce for NavigationBar {
    fn render(self, _window: &mut Window, _cx: &mut App) -> impl IntoElement {
        let layout = self.tokens.layout;
        let has_address_error = self.state.tabs().active_tab().view.address.error.is_some();
        let navigation_height = layout.address_bar_height.value()
            + if has_address_error {
                layout.minimum_hit_target.value()
            } else {
                0.0
            };
        let navigation_top_padding =
            (layout.address_bar_height.value() - layout.minimum_hit_target.value()) / 2.0;
        let available = self.state.command_availability();
        let history_menu = self.state.navigation_history_menu_direction();
        let history_index = self.state.navigation_history_menu_index();
        let back_history = self
            .state
            .navigation_history_entries(NavigationHistoryDirection::Back);
        let forward_history = self
            .state
            .navigation_history_entries(NavigationHistoryDirection::Forward);
        div()
            .id(NAVIGATION_BAR_ID)
            .debug_selector(|| NAVIGATION_BAR_ID.to_owned())
            .role(Role::Document)
            .relative()
            .aria_label("Explorer navigation bar")
            .h(px(navigation_height))
            .flex_none()
            .flex()
            .items_start()
            .pt(px(navigation_top_padding))
            .gap(px(layout.content_spacing.value()))
            .px(px(layout.control_padding_horizontal.value()))
            .border_b(px(layout.focus_stroke.value()))
            .border_color(self.tokens.theme.colors.divider.to_gpui())
            .bg(self.tokens.theme.colors.surface.to_gpui())
            .child(region_probe(
                NAVIGATION_BAR_ID,
                Some(EXPLORER_WINDOW_ID),
                "normal",
            ))
            .child(navigation_history_button(
                "navigation-back",
                "Back",
                ExplorerIcon::Back,
                ExplorerAction::Back,
                NavigationHistoryDirection::Back,
                back_history,
                history_menu == Some(NavigationHistoryDirection::Back),
                history_index,
                available.is_enabled(CommandKind::Back),
                self.tokens,
                self.breadcrumb_menu_focus.clone(),
                self.on_action.as_ref(),
            ))
            .child(navigation_history_button(
                "navigation-forward",
                "Forward",
                ExplorerIcon::Forward,
                ExplorerAction::Forward,
                NavigationHistoryDirection::Forward,
                forward_history,
                history_menu == Some(NavigationHistoryDirection::Forward),
                history_index,
                available.is_enabled(CommandKind::Forward),
                self.tokens,
                self.breadcrumb_menu_focus.clone(),
                self.on_action.as_ref(),
            ))
            .child(semantic_button(
                "navigation-up",
                "Up",
                Some(ExplorerIcon::Up),
                None,
                Some(ExplorerAction::Up),
                available.is_enabled(CommandKind::Up),
                self.tokens,
                self.on_action.clone(),
            ))
            .child(semantic_button(
                "navigation-refresh",
                "Refresh",
                Some(ExplorerIcon::Refresh),
                None,
                Some(ExplorerAction::Refresh),
                available.is_enabled(CommandKind::Refresh),
                self.tokens,
                self.on_action.clone(),
            ))
            .child(BreadcrumbAddressEditor::new(
                self.tokens,
                self.state.clone(),
                self.address_input,
                self.breadcrumb_menu_focus,
                self.shell_icons,
                self.shell_icon_dpi,
                self.on_action.clone(),
            ))
            .child(SearchBox::new(
                self.tokens,
                self.state,
                self.search_input,
                self.on_action,
            ))
    }
}

#[derive(IntoElement)]
pub struct BreadcrumbAddressEditor {
    tokens: UiTokens,
    state: NavigationAddressViewModel,
    on_action: Option<ActionCallback>,
    input: Option<gpui::WeakEntity<EditableTextState>>,
    menu_focus: Option<gpui::FocusHandle>,
    shell_icons: HashMap<explorer_model::ShellIconKey, Arc<RenderImage>>,
    shell_icon_dpi: u16,
}

impl BreadcrumbAddressEditor {
    pub const fn new(
        tokens: UiTokens,
        state: NavigationAddressViewModel,
        input: Option<gpui::WeakEntity<EditableTextState>>,
        menu_focus: Option<gpui::FocusHandle>,
        shell_icons: HashMap<explorer_model::ShellIconKey, Arc<RenderImage>>,
        shell_icon_dpi: u16,
        on_action: Option<ActionCallback>,
    ) -> Self {
        Self {
            tokens,
            state,
            on_action,
            input,
            menu_focus,
            shell_icons,
            shell_icon_dpi,
        }
    }
}

impl RenderOnce for BreadcrumbAddressEditor {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let presentation = self.state.active_presentation();
        let address = self.state.tabs().active_tab().view.address.clone();
        if !matches!(
            address.mode,
            explorer_model::AddressBarMode::Editing
                | explorer_model::AddressBarMode::NavigationError
        ) {
            let window_width = f32::from(window.bounds().size.width);
            return breadcrumb_browse_field(
                self.tokens,
                address,
                window_width,
                self.menu_focus,
                self.shell_icons,
                self.shell_icon_dpi,
                self.on_action,
            )
            .into_any_element();
        }
        let input_focus = self
            .input
            .as_ref()
            .and_then(gpui::WeakEntity::upgrade)
            .map(|input| input.read(cx).focus_handle(cx));
        let error = address.error;
        div()
            .relative()
            .flex_1()
            .child(editable_focus_field(
                ADDRESS_EDITOR_ID,
                format!("Address: {}", presentation.address_title),
                presentation.address_title,
                ExplorerAction::FocusAddress,
                self.state.focused_surface() == FocusSurface::AddressBar,
                self.tokens,
                self.input,
                input_focus,
                self.on_action,
                f32::from(window.bounds().size.width)
                    < self.tokens.layout.compact_window_width.value(),
            ))
            .when_some(error, |element, error| {
                element.child(
                    div()
                        .id("breadcrumb-address-error")
                        .role(Role::Alert)
                        .absolute()
                        .top(px(self.tokens.layout.minimum_hit_target.value()))
                        .left_0()
                        .px(px(self.tokens.layout.content_spacing.value()))
                        .py(px(self.tokens.layout.content_spacing.value() / 2.0))
                        .rounded(px(self.tokens.layout.corner_radius.value() / 2.0))
                        .bg(self.tokens.theme.colors.menu_fill.to_gpui())
                        .border(px(1.0))
                        .border_color(self.tokens.theme.colors.danger.to_gpui())
                        .text_color(self.tokens.theme.colors.danger.to_gpui())
                        .child(error),
                )
            })
            .into_any_element()
    }
}

fn breadcrumb_browse_field(
    tokens: UiTokens,
    address: explorer_model::AddressBarState,
    window_width: f32,
    menu_focus: Option<gpui::FocusHandle>,
    shell_icons: HashMap<explorer_model::ShellIconKey, Arc<RenderImage>>,
    shell_icon_dpi: u16,
    on_action: Option<ActionCallback>,
) -> impl IntoElement {
    let layout = tokens.layout;
    let colors = tokens.theme.colors;
    let active_menu = match address.mode {
        explorer_model::AddressBarMode::EnumeratingMenu { segment_id, .. } => Some(segment_id),
        _ => None,
    };
    let menu_children = address.menu_children.clone();
    let menu_error = address.menu_error.clone();
    let menu_loading = address.menu_loading;
    let overflow_open = address.overflow_open;
    let keyboard_segment_id = address.focused_segment().map(|segment| segment.id);
    let keyboard_menu_index = address.keyboard_menu_index;
    let (hidden_ancestry, visible_ancestry) = breadcrumb_ancestry_partition(
        address
            .resolved_ancestry
            .into_iter()
            .filter(|segment| segment.id != explorer_model::BreadcrumbSegmentId(0))
            .collect(),
        window_width,
    );
    let shell_icon_theme = match tokens.theme.mode {
        crate::theme::ThemeMode::Light => explorer_model::ShellIconTheme::Light,
        crate::theme::ThemeMode::Dark => explorer_model::ShellIconTheme::Dark,
    };
    let generic_shell_icon = shell_icons.iter().find_map(|(key, texture)| {
        is_generic_breadcrumb_folder_icon_key(key).then(|| Arc::clone(texture))
    });
    let background_action = on_action.clone();
    div()
        .id(ADDRESS_EDITOR_ID)
        .debug_selector(|| ADDRESS_EDITOR_ID.to_owned())
        .role(Role::Document)
        .relative()
        .aria_label(format!("Address: {}", address.draft))
        .min_w(px(layout.address_min_width.value()))
        .h(px(layout.minimum_hit_target.value()))
        .flex_1()
        .flex()
        .items_center()
        .rounded(px(layout.corner_radius.value()))
        .bg(colors.address_fill.to_gpui())
        .when_some(background_action, |element, callback| {
            element.on_click(move |_, window, cx| {
                callback(&ExplorerAction::EnterAddressEdit, window, cx);
            })
        })
        .child(region_probe(
            ADDRESS_EDITOR_ID,
            Some(NAVIGATION_BAR_ID),
            if active_menu.is_some() {
                "enumerating-menu"
            } else {
                "browsing"
            },
        ))
        .child(typography_probe(
            ADDRESS_EDITOR_ID,
            typography_diagnostic(tokens, tokens.typography.address),
        ))
        .child(breadcrumb_root(
            tokens,
            keyboard_segment_id == Some(explorer_model::BreadcrumbSegmentId(0)),
            active_menu == Some(explorer_model::BreadcrumbSegmentId(0)),
            menu_children.clone(),
            menu_error.clone(),
            menu_loading,
            keyboard_menu_index,
            menu_focus.clone(),
            shell_icons.clone(),
            shell_icon_theme,
            shell_icon_dpi,
            on_action.clone(),
        ))
        .when(!hidden_ancestry.is_empty(), |element| {
            element.child(breadcrumb_overflow(
                tokens,
                hidden_ancestry,
                overflow_open,
                shell_icons.clone(),
                generic_shell_icon.clone(),
                shell_icon_theme,
                shell_icon_dpi,
                on_action.clone(),
            ))
        })
        .children(visible_ancestry.into_iter().map(move |segment| {
            let activate = on_action.clone();
            let open = on_action.clone();
            let segment_dom_id = format!("breadcrumb-segment-{:016x}", segment.id.0);
            let chevron_dom_id = format!("breadcrumb-chevron-{:016x}", segment.id.0);
            let location = segment.location.clone();
            let id = segment.id;
            let keyboard_focused = keyboard_segment_id == Some(id);
            let segment_name = segment.display_name.clone();
            let children = menu_children.clone();
            let error = menu_error.clone();
            let menu_action = on_action.clone();
            let segment_icon = breadcrumb_location_shell_texture(
                &shell_icons,
                &segment.location,
                shell_icon_theme,
                shell_icon_dpi,
            );
            let child_icons = shell_icons.clone();
            let child_focus = menu_focus.clone();
            div()
                .relative()
                .h_full()
                .flex()
                .items_center()
                .child(
                    div()
                        .id(segment_dom_id)
                        .role(Role::Button)
                        .aria_label(format!("Go to {}", segment.display_name))
                        .aria_selected(keyboard_focused)
                        .h_full()
                        .flex()
                        .items_center()
                        .px(px(layout.content_spacing.value()))
                        .hover(move |style| style.bg(colors.control_hover.to_gpui()))
                        .active(move |style| style.bg(colors.control_pressed.to_gpui()))
                        .when_some(activate, move |element, callback| {
                            let accessibility_callback = callback.clone();
                            let accessibility_location = location.clone();
                            element
                                .on_click(move |_, window, cx| {
                                    cx.stop_propagation();
                                    callback(
                                        &ExplorerAction::ActivateBreadcrumbSegment {
                                            location: location.clone(),
                                        },
                                        window,
                                        cx,
                                    );
                                })
                                .on_a11y_action(AccessibleAction::Click, move |_, window, cx| {
                                    accessibility_callback(
                                        &ExplorerAction::ActivateBreadcrumbSegment {
                                            location: accessibility_location.clone(),
                                        },
                                        window,
                                        cx,
                                    );
                                })
                        })
                        .child(breadcrumb_shell_icon(
                            segment_icon,
                            generic_shell_icon.clone(),
                            tokens,
                        ))
                        .child(segment.display_name),
                )
                .child(breadcrumb_chevron_button(
                    chevron_dom_id,
                    &segment_name,
                    id,
                    active_menu == Some(id),
                    menu_loading,
                    tokens,
                    open,
                ))
                .when(active_menu == Some(id), |element| {
                    element.child(breadcrumb_child_overlay(
                        tokens,
                        id,
                        children,
                        error,
                        menu_loading,
                        keyboard_menu_index,
                        child_focus,
                        child_icons,
                        generic_shell_icon.clone(),
                        shell_icon_theme,
                        shell_icon_dpi,
                        menu_action,
                    ))
                })
        }))
}

fn breadcrumb_chevron_button(
    dom_id: String,
    segment_name: &str,
    segment_id: explorer_model::BreadcrumbSegmentId,
    expanded: bool,
    busy: bool,
    tokens: UiTokens,
    on_action: Option<ActionCallback>,
) -> impl IntoElement {
    let label = if busy {
        format!("列出 {segment_name} 的子資料夾，載入中")
    } else {
        format!("列出 {segment_name} 的子資料夾")
    };
    div()
        .id(dom_id)
        .role(Role::Button)
        .aria_label(label)
        .aria_expanded(expanded)
        .h_full()
        .w(px(tokens.layout.minimum_hit_target.value() / 2.0))
        .flex()
        .items_center()
        .justify_center()
        .hover(move |style| style.bg(tokens.theme.colors.control_hover.to_gpui()))
        .active(move |style| style.bg(tokens.theme.colors.control_pressed.to_gpui()))
        .when_some(on_action, move |element, callback| {
            let accessibility_callback = callback.clone();
            element
                .on_click(move |_, window, cx| {
                    cx.stop_propagation();
                    callback(
                        &ExplorerAction::OpenBreadcrumbChildren { segment_id },
                        window,
                        cx,
                    );
                })
                .on_a11y_action(AccessibleAction::Click, move |_, window, cx| {
                    accessibility_callback(
                        &ExplorerAction::OpenBreadcrumbChildren { segment_id },
                        window,
                        cx,
                    );
                })
        })
        .child(chrome_icon(
            ADDRESS_EDITOR_ID,
            ExplorerIcon::Chevron,
            tokens,
        ))
}

fn breadcrumb_ancestry_partition(
    segments: Vec<explorer_model::BreadcrumbSegment>,
    window_width: f32,
) -> (
    Vec<explorer_model::BreadcrumbSegment>,
    Vec<explorer_model::BreadcrumbSegment>,
) {
    if segments.len() <= 1 {
        return (Vec::new(), segments);
    }
    let budget = (window_width - 620.0).max(140.0);
    let mut used = 36.0; // reserve the overflow affordance before selecting the visible tail
    let mut visible_start = segments.len() - 1;
    for index in (0..segments.len()).rev() {
        let character_count =
            u16::try_from(segments[index].display_name.chars().count()).unwrap_or(u16::MAX);
        let label_width = f32::from(character_count) * 8.0;
        let estimated = 54.0 + label_width;
        if index != segments.len() - 1 && used + estimated > budget {
            break;
        }
        used += estimated;
        visible_start = index;
    }
    if visible_start == 0 {
        (Vec::new(), segments)
    } else {
        let visible = segments[visible_start..].to_vec();
        let hidden = segments[..visible_start].to_vec();
        (hidden, visible)
    }
}

fn breadcrumb_overflow(
    tokens: UiTokens,
    hidden: Vec<explorer_model::BreadcrumbSegment>,
    open: bool,
    shell_icons: HashMap<explorer_model::ShellIconKey, Arc<RenderImage>>,
    generic_shell_icon: Option<Arc<RenderImage>>,
    shell_icon_theme: explorer_model::ShellIconTheme,
    shell_icon_dpi: u16,
    on_action: Option<ActionCallback>,
) -> impl IntoElement {
    let colors = tokens.theme.colors;
    let toggle = on_action.clone();
    let menu_action = on_action;
    let outside_action = menu_action.clone();
    div()
        .relative()
        .h_full()
        .flex()
        .items_center()
        .child(
            div()
                .id("breadcrumb-overflow")
                .role(Role::Button)
                .aria_label("顯示較舊的路徑層級")
                .aria_expanded(open)
                .h_full()
                .w(px(tokens.layout.minimum_hit_target.value()))
                .flex()
                .items_center()
                .justify_center()
                .hover(move |style| style.bg(colors.control_hover.to_gpui()))
                .active(move |style| style.bg(colors.control_pressed.to_gpui()))
                .when_some(toggle, |element, callback| {
                    let accessibility_callback = callback.clone();
                    element
                        .on_click(move |_, window, cx| {
                            cx.stop_propagation();
                            callback(&ExplorerAction::ToggleBreadcrumbOverflow, window, cx);
                        })
                        .on_a11y_action(AccessibleAction::Click, move |_, window, cx| {
                            accessibility_callback(
                                &ExplorerAction::ToggleBreadcrumbOverflow,
                                window,
                                cx,
                            );
                        })
                })
                .child(chrome_icon(
                    ADDRESS_EDITOR_ID,
                    ExplorerIcon::More,
                    tokens,
                )),
        )
        .when(open, |element| {
            element.child(
                deferred(
                    anchored()
                        .anchor(Anchor::TopLeft)
                        .position_mode(AnchoredPositionMode::Local)
                        .offset(point(
                            px(0.0),
                            px(tokens.layout.minimum_hit_target.value()),
                        ))
                        .snap_to_window()
                        .child(
                            div()
                                .id("breadcrumb-overflow-menu")
                                .role(Role::Menu)
                                .min_w(px(tokens.layout.navigation_pane_min_width.value()))
                                .py(px(tokens.layout.content_spacing.value() / 2.0))
                                .rounded(px(tokens.layout.corner_radius.value()))
                                .bg(colors.menu_fill.to_gpui())
                                .border(px(1.0))
                                .border_color(colors.divider.to_gpui())
                                .when_some(outside_action, |menu, callback| {
                                    menu.on_mouse_up_out(
                                        MouseButton::Left,
                                        move |_, window, cx| {
                                            callback(
                                                &ExplorerAction::CloseBreadcrumbMenu,
                                                window,
                                                cx,
                                            );
                                        },
                                    )
                                })
                                .children(hidden.into_iter().map(move |segment| {
                                    let callback = menu_action.clone();
                                    let location = segment.location.clone();
                                    let stable_id = breadcrumb_location_id(&location);
                                    let icon = breadcrumb_location_shell_texture(
                                        &shell_icons,
                                        &location,
                                        shell_icon_theme,
                                        shell_icon_dpi,
                                    );
                                    div()
                                        .id(format!("breadcrumb-overflow-{stable_id:016x}"))
                                        .role(Role::MenuItem)
                                        .aria_label(segment.display_name.clone())
                                        .h(px(tokens.layout.minimum_hit_target.value()))
                                        .flex()
                                        .items_center()
                                        .gap(px(tokens.layout.content_spacing.value()))
                                        .px(px(tokens.layout.control_padding_horizontal.value()))
                                        .hover(move |style| {
                                            style.bg(colors.control_hover.to_gpui())
                                        })
                                        .active(move |style| {
                                            style.bg(colors.control_pressed.to_gpui())
                                        })
                                        .when_some(callback, move |item, callback| {
                                            let accessibility_callback = callback.clone();
                                            let accessibility_location = location.clone();
                                            item.on_click(move |_, window, cx| {
                                                cx.stop_propagation();
                                                callback(
                                                    &ExplorerAction::ActivateBreadcrumbSegment {
                                                        location: location.clone(),
                                                    },
                                                    window,
                                                    cx,
                                                );
                                            })
                                            .on_a11y_action(
                                                AccessibleAction::Click,
                                                move |_, window, cx| {
                                                    accessibility_callback(
                                                        &ExplorerAction::ActivateBreadcrumbSegment {
                                                            location: accessibility_location.clone(),
                                                        },
                                                        window,
                                                        cx,
                                                    );
                                                },
                                            )
                                        })
                                        .child(breadcrumb_shell_icon(
                                            icon,
                                            generic_shell_icon.clone(),
                                            tokens,
                                        ))
                                        .child(segment.display_name)
                                })),
                        ),
                )
                .with_priority(100),
            )
        })
}

fn breadcrumb_child_menu(
    tokens: UiTokens,
    segment_id: explorer_model::BreadcrumbSegmentId,
    children: Vec<explorer_model::BreadcrumbMenuItem>,
    error: Option<String>,
    loading: bool,
    keyboard_menu_index: Option<usize>,
    menu_focus: Option<gpui::FocusHandle>,
    shell_icons: HashMap<explorer_model::ShellIconKey, Arc<RenderImage>>,
    generic_shell_icon: Option<Arc<RenderImage>>,
    shell_icon_theme: explorer_model::ShellIconTheme,
    shell_icon_dpi: u16,
    on_action: Option<ActionCallback>,
) -> impl IntoElement {
    let layout = tokens.layout;
    let colors = tokens.theme.colors;
    let has_error = error.is_some();
    let outside_action = on_action.clone();
    let keyboard_action = on_action.clone();
    let keyboard_location = keyboard_menu_index
        .and_then(|index| children.get(index))
        .map(|item| item.location.clone());
    let container_focus = keyboard_menu_index
        .is_none()
        .then(|| menu_focus.clone())
        .flatten();
    div()
        .id("breadcrumb-child-menu")
        .role(Role::Menu)
        .aria_label(if loading {
            "Breadcrumb child folders, loading"
        } else if has_error {
            "Breadcrumb child folders, recoverable error"
        } else {
            "Breadcrumb child folders"
        })
        .min_w(px(layout.navigation_pane_min_width.value()))
        .max_h(px(layout.menu_max_height.value()))
        .overflow_y_scroll()
        .py(px(layout.content_spacing.value() / 2.0))
        .rounded(px(layout.corner_radius.value()))
        .bg(colors.menu_fill.to_gpui())
        .border(px(1.0))
        .border_color(colors.divider.to_gpui())
        .when_some(container_focus, |menu, handle| menu.track_focus(&handle))
        .when_some(keyboard_action, |menu, callback| {
            menu.on_key_down(move |event, window, cx| {
                let movement = match event.keystroke.key.as_str() {
                    "up" => Some(explorer_model::MenuFocusMovement::Previous),
                    "down" => Some(explorer_model::MenuFocusMovement::Next),
                    "home" => Some(explorer_model::MenuFocusMovement::First),
                    "end" => Some(explorer_model::MenuFocusMovement::Last),
                    "pageup" => Some(explorer_model::MenuFocusMovement::PagePrevious),
                    "pagedown" => Some(explorer_model::MenuFocusMovement::PageNext),
                    _ => None,
                };
                let action = movement
                    .map(|movement| ExplorerAction::MoveBreadcrumbMenuFocus { movement })
                    .or_else(|| match event.keystroke.key.as_str() {
                        "escape" | "left" | "right" => Some(ExplorerAction::CloseBreadcrumbMenu),
                        "enter" | "space" => keyboard_location
                            .clone()
                            .map(|location| ExplorerAction::ActivateBreadcrumbChild { location }),
                        _ if !event.keystroke.modifiers.control
                            && !event.keystroke.modifiers.alt
                            && !event.keystroke.modifiers.platform =>
                        {
                            event.keystroke.key_char.as_ref().and_then(|text| {
                                (!text.chars().all(char::is_whitespace)).then(|| {
                                    ExplorerAction::TypeAheadBreadcrumbMenu { text: text.clone() }
                                })
                            })
                        }
                        _ => None,
                    });
                if let Some(action) = action {
                    cx.stop_propagation();
                    callback(&action, window, cx);
                }
            })
        })
        .when_some(outside_action, |menu, callback| {
            menu.on_mouse_up_out(MouseButton::Left, move |_, window, cx| {
                callback(&ExplorerAction::CloseBreadcrumbMenu, window, cx);
            })
        })
        .when(loading, |menu| {
            menu.child(
                div()
                    .px(px(layout.control_padding_horizontal.value()))
                    .py(px(layout.content_spacing.value()))
                    .text_color(colors.text_secondary.to_gpui())
                    .child("正在列舉子資料夾…"),
            )
        })
        .when_some(error, |menu, error| {
            let retry = on_action.clone();
            menu.child(
                div()
                    .id("breadcrumb-child-retry")
                    .role(Role::Button)
                    .aria_label("重試列出子資料夾")
                    .px(px(layout.control_padding_horizontal.value()))
                    .py(px(layout.content_spacing.value()))
                    .text_color(colors.danger.to_gpui())
                    .hover(move |style| style.bg(colors.control_hover.to_gpui()))
                    .active(move |style| style.bg(colors.control_pressed.to_gpui()))
                    .when_some(retry, move |item, callback| {
                        let accessibility_callback = callback.clone();
                        item.on_click(move |_, window, cx| {
                            cx.stop_propagation();
                            callback(
                                &ExplorerAction::RetryBreadcrumbChildren { segment_id },
                                window,
                                cx,
                            );
                        })
                        .on_a11y_action(
                            AccessibleAction::Click,
                            move |_, window, cx| {
                                accessibility_callback(
                                    &ExplorerAction::RetryBreadcrumbChildren { segment_id },
                                    window,
                                    cx,
                                );
                            },
                        )
                    })
                    .child(error),
            )
        })
        .when(!loading && children.is_empty() && !has_error, |menu| {
            menu.child(
                div()
                    .px(px(layout.control_padding_horizontal.value()))
                    .py(px(layout.content_spacing.value()))
                    .text_color(colors.text_secondary.to_gpui())
                    .child("沒有子資料夾"),
            )
        })
        .children(children.into_iter().enumerate().map(move |(index, child)| {
            let callback = on_action.clone();
            let location = child.location;
            let focus_handle = (keyboard_menu_index == Some(index))
                .then(|| menu_focus.clone())
                .flatten();
            let stable_id = breadcrumb_location_id(&location);
            let shell_icon = breadcrumb_location_shell_texture(
                &shell_icons,
                &location,
                shell_icon_theme,
                shell_icon_dpi,
            );
            div()
                .id(format!("breadcrumb-child-{stable_id:016x}"))
                .role(Role::MenuItem)
                .aria_label(child.display_name.clone())
                .aria_selected(keyboard_menu_index == Some(index))
                .h(px(layout.minimum_hit_target.value()))
                .flex()
                .items_center()
                .px(px(layout.control_padding_horizontal.value()))
                .hover(move |style| style.bg(colors.selected_inactive.to_gpui()))
                .active(move |style| style.bg(colors.control_pressed.to_gpui()))
                .when(keyboard_menu_index == Some(index), move |item| {
                    item.bg(colors.selected_inactive.to_gpui())
                })
                .when_some(focus_handle, |item, handle| item.track_focus(&handle))
                .when_some(callback, move |item, callback| {
                    let pointer_callback = callback.clone();
                    let accessibility_callback = callback.clone();
                    let accessibility_location = location.clone();
                    item.on_mouse_move(move |_, window, cx| {
                        pointer_callback(
                            &ExplorerAction::SetBreadcrumbMenuFocus { index },
                            window,
                            cx,
                        );
                    })
                    .on_click(move |_, window, cx| {
                        cx.stop_propagation();
                        callback(
                            &ExplorerAction::ActivateBreadcrumbChild {
                                location: location.clone(),
                            },
                            window,
                            cx,
                        );
                    })
                    .on_a11y_action(
                        AccessibleAction::Click,
                        move |_, window, cx| {
                            accessibility_callback(
                                &ExplorerAction::ActivateBreadcrumbChild {
                                    location: accessibility_location.clone(),
                                },
                                window,
                                cx,
                            );
                        },
                    )
                })
                .child(breadcrumb_shell_icon(
                    shell_icon,
                    generic_shell_icon.clone(),
                    tokens,
                ))
                .child(child.display_name)
        }))
}

fn breadcrumb_child_overlay(
    tokens: UiTokens,
    segment_id: explorer_model::BreadcrumbSegmentId,
    children: Vec<explorer_model::BreadcrumbMenuItem>,
    error: Option<String>,
    loading: bool,
    keyboard_menu_index: Option<usize>,
    menu_focus: Option<gpui::FocusHandle>,
    shell_icons: HashMap<explorer_model::ShellIconKey, Arc<RenderImage>>,
    generic_shell_icon: Option<Arc<RenderImage>>,
    shell_icon_theme: explorer_model::ShellIconTheme,
    shell_icon_dpi: u16,
    on_action: Option<ActionCallback>,
) -> impl IntoElement {
    deferred(
        anchored()
            .anchor(Anchor::TopLeft)
            .position_mode(AnchoredPositionMode::Local)
            .offset(point(px(0.0), px(tokens.layout.minimum_hit_target.value())))
            .snap_to_window()
            .child(breadcrumb_child_menu(
                tokens,
                segment_id,
                children,
                error,
                loading,
                keyboard_menu_index,
                menu_focus,
                shell_icons,
                generic_shell_icon,
                shell_icon_theme,
                shell_icon_dpi,
                on_action,
            )),
    )
    .with_priority(100)
}

fn breadcrumb_root(
    tokens: UiTokens,
    keyboard_focused: bool,
    menu_open: bool,
    menu_children: Vec<explorer_model::BreadcrumbMenuItem>,
    menu_error: Option<String>,
    menu_loading: bool,
    keyboard_menu_index: Option<usize>,
    menu_focus: Option<gpui::FocusHandle>,
    shell_icons: HashMap<explorer_model::ShellIconKey, Arc<RenderImage>>,
    shell_icon_theme: explorer_model::ShellIconTheme,
    shell_icon_dpi: u16,
    on_action: Option<ActionCallback>,
) -> impl IntoElement {
    let generic_shell_icon = shell_icons.iter().find_map(|(key, texture)| {
        is_generic_breadcrumb_folder_icon_key(key).then(|| Arc::clone(texture))
    });
    let activate = on_action.clone();
    let open = on_action.clone();
    let computer_location =
        explorer_model::LocationDescriptor::ParsingName("shell:MyComputerFolder".to_owned());
    let computer_icon = breadcrumb_location_shell_texture(
        &shell_icons,
        &computer_location,
        shell_icon_theme,
        shell_icon_dpi,
    );
    div()
        .relative()
        .h_full()
        .flex()
        .items_center()
        .child(
            div()
                .id("breadcrumb-root-computer")
                .role(Role::Button)
                .aria_selected(keyboard_focused)
                .aria_label("本機")
                .h_full()
                .flex()
                .items_center()
                .gap(px(tokens.layout.content_spacing.value()))
                .px(px(tokens.layout.content_spacing.value()))
                .hover(move |style| style.bg(tokens.theme.colors.control_hover.to_gpui()))
                .active(move |style| style.bg(tokens.theme.colors.control_pressed.to_gpui()))
                .when_some(activate, |element, callback| {
                    let accessibility_callback = callback.clone();
                    element
                        .on_click(move |_, window, cx| {
                            cx.stop_propagation();
                            callback(
                                &ExplorerAction::ActivateBreadcrumbSegment {
                                    location: explorer_model::LocationDescriptor::ParsingName(
                                        "shell:MyComputerFolder".to_owned(),
                                    ),
                                },
                                window,
                                cx,
                            );
                        })
                        .on_a11y_action(AccessibleAction::Click, move |_, window, cx| {
                            accessibility_callback(
                                &ExplorerAction::ActivateBreadcrumbSegment {
                                    location: explorer_model::LocationDescriptor::ParsingName(
                                        "shell:MyComputerFolder".to_owned(),
                                    ),
                                },
                                window,
                                cx,
                            );
                        })
                })
                .child(breadcrumb_shell_icon(
                    computer_icon,
                    generic_shell_icon.clone(),
                    tokens,
                ))
                .child("本機"),
        )
        .child(
            div()
                .id("breadcrumb-root-chevron")
                .role(Role::Button)
                .aria_label("列出磁碟機")
                .aria_expanded(menu_open)
                .h_full()
                .w(px(tokens.layout.minimum_hit_target.value() / 2.0))
                .flex()
                .items_center()
                .justify_center()
                .hover(move |style| style.bg(tokens.theme.colors.control_hover.to_gpui()))
                .active(move |style| style.bg(tokens.theme.colors.control_pressed.to_gpui()))
                .when_some(open, |element, callback| {
                    let accessibility_callback = callback.clone();
                    element
                        .on_click(move |_, window, cx| {
                            cx.stop_propagation();
                            callback(
                                &ExplorerAction::OpenBreadcrumbChildren {
                                    segment_id: explorer_model::BreadcrumbSegmentId(0),
                                },
                                window,
                                cx,
                            );
                        })
                        .on_a11y_action(AccessibleAction::Click, move |_, window, cx| {
                            accessibility_callback(
                                &ExplorerAction::OpenBreadcrumbChildren {
                                    segment_id: explorer_model::BreadcrumbSegmentId(0),
                                },
                                window,
                                cx,
                            );
                        })
                })
                .child(chrome_icon(
                    ADDRESS_EDITOR_ID,
                    ExplorerIcon::Chevron,
                    tokens,
                )),
        )
        .when(menu_open, |element| {
            element.child(breadcrumb_child_overlay(
                tokens,
                explorer_model::BreadcrumbSegmentId(0),
                menu_children,
                menu_error,
                menu_loading,
                keyboard_menu_index,
                menu_focus,
                shell_icons,
                generic_shell_icon,
                shell_icon_theme,
                shell_icon_dpi,
                on_action,
            ))
        })
}

fn breadcrumb_shell_icon(
    shell_icon: Option<Arc<RenderImage>>,
    generic_shell_icon: Option<Arc<RenderImage>>,
    tokens: UiTokens,
) -> gpui::AnyElement {
    let size = px(tokens.layout.navigation_icon_size.value());
    match select_breadcrumb_shell_icon(shell_icon, generic_shell_icon) {
        Some(texture) => img(texture).size(size).flex_none().into_any_element(),
        None => div().size(size).flex_none().into_any_element(),
    }
}

fn select_breadcrumb_shell_icon<T>(specific: Option<T>, generic: Option<T>) -> Option<T> {
    specific.or(generic)
}

fn breadcrumb_location_id(location: &explorer_model::LocationDescriptor) -> u64 {
    use std::hash::{Hash as _, Hasher as _};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    location.hash(&mut hasher);
    hasher.finish()
}

#[derive(IntoElement)]
pub struct SearchBox {
    tokens: UiTokens,
    state: NavigationAddressViewModel,
    on_action: Option<ActionCallback>,
    input: Option<gpui::WeakEntity<EditableTextState>>,
}

impl SearchBox {
    pub const fn new(
        tokens: UiTokens,
        state: NavigationAddressViewModel,
        input: Option<gpui::WeakEntity<EditableTextState>>,
        on_action: Option<ActionCallback>,
    ) -> Self {
        Self {
            tokens,
            state,
            on_action,
            input,
        }
    }
}

impl RenderOnce for SearchBox {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let presentation = self.state.active_presentation();
        let input_focus = self
            .input
            .as_ref()
            .and_then(gpui::WeakEntity::upgrade)
            .map(|input| input.read(cx).focus_handle(cx));
        let search_hint = localized_search_placeholder(
            self.state.tabs().active_tab().history.current(),
            &presentation.address_title,
            &self.state.tabs().active_tab().view.address,
        );
        let visible_text = match &presentation.search {
            TabSearchState::Idle => search_hint.clone(),
            TabSearchState::Editing(input)
            | TabSearchState::Loading { input, .. }
            | TabSearchState::Ready { input, .. }
            | TabSearchState::Partial { input, .. }
            | TabSearchState::Cancelled { input, .. }
            | TabSearchState::Error { input, .. } => input.clone(),
        };
        let compact =
            f32::from(window.bounds().size.width) < self.tokens.layout.compact_window_width.value();
        let search_box_width = self
            .tokens
            .layout
            .search_box_width_for_window(f32::from(window.bounds().size.width));
        let show_clear =
            !matches!(presentation.search, TabSearchState::Idle) && !visible_text.is_empty();
        let clear = self.on_action.clone();
        let field = editable_focus_field(
            SEARCH_BOX_ID,
            format!(
                "{search_hint}; 最近搜尋 {} 筆",
                self.state.tabs().active_tab().search_history.len()
            ),
            visible_text,
            ExplorerAction::FocusSearch,
            self.state.focused_surface() == FocusSurface::Search,
            self.tokens,
            self.input,
            input_focus,
            self.on_action,
            compact,
        );
        div()
            .id("search-box-container")
            .relative()
            .h(px(self.tokens.layout.minimum_hit_target.value()))
            .w(px(search_box_width))
            .flex_none()
            .child(field)
            .child(
                div()
                    .absolute()
                    .right(px(self.tokens.layout.content_spacing.value()))
                    .top(px(self.tokens.layout.content_spacing.value()))
                    .size(px(self.tokens.layout.navigation_icon_size.value()))
                    .child(chrome_icon(
                        SEARCH_BOX_ID,
                        ExplorerIcon::Search,
                        self.tokens,
                    )),
            )
            .when(show_clear, |element| {
                element.child(
                    div()
                        .id("search-clear")
                        .role(Role::Button)
                        .aria_label("清除搜尋")
                        .absolute()
                        .right(px(0.0))
                        .top(px(0.0))
                        .size(px(self.tokens.layout.minimum_hit_target.value()))
                        .flex()
                        .items_center()
                        .justify_center()
                        .rounded(px(self.tokens.layout.corner_radius.value()))
                        .hover(move |style| {
                            style.bg(self.tokens.theme.colors.control_hover.to_gpui())
                        })
                        .active(move |style| {
                            style.bg(self.tokens.theme.colors.control_pressed.to_gpui())
                        })
                        .when_some(clear, |button, callback| {
                            let accessibility_callback = callback.clone();
                            button
                                .on_click(move |_, window, cx| {
                                    cx.stop_propagation();
                                    callback(&ExplorerAction::ClearSearch, window, cx);
                                })
                                .on_a11y_action(AccessibleAction::Click, move |_, window, cx| {
                                    accessibility_callback(
                                        &ExplorerAction::ClearSearch,
                                        window,
                                        cx,
                                    );
                                })
                        })
                        .child(chrome_icon(
                            "search-clear",
                            ExplorerIcon::Close,
                            self.tokens,
                        )),
                )
            })
    }
}

fn localized_search_placeholder(
    current: Option<&explorer_model::HistoryEntry>,
    address_title: &str,
    address: &explorer_model::AddressBarState,
) -> String {
    let committed_path_leaf = current
        .and_then(|entry| entry.location.path())
        .and_then(std::path::Path::file_name)
        .and_then(std::ffi::OsStr::to_str)
        .map(str::trim)
        .filter(|name| !name.is_empty());
    let breadcrumb_leaf = address
        .resolved_ancestry
        .last()
        .map(|segment| segment.display_name.trim())
        .filter(|name| !name.is_empty());
    let committed_title = current
        .map(|entry| entry.display_title.trim())
        .filter(|name| !name.is_empty());
    let folder_name = committed_path_leaf
        .or(breadcrumb_leaf)
        .or(committed_title)
        .unwrap_or(address_title);
    format!("搜尋 {folder_name}")
}

#[derive(IntoElement)]
pub struct NavigationPane {
    tokens: UiTokens,
    state: NavigationPaneViewModel,
    on_action: Option<ActionCallback>,
    scroll_handle: Option<gpui::ScrollHandle>,
    shell_icons: HashMap<explorer_model::ShellIconKey, Arc<RenderImage>>,
    shell_icon_dpi: u16,
}

#[derive(IntoElement)]
pub struct NavigationDivider {
    tokens: UiTokens,
    state: NavigationPaneViewModel,
    on_action: Option<ActionCallback>,
}

impl NavigationDivider {
    pub const fn new(
        tokens: UiTokens,
        state: NavigationPaneViewModel,
        on_action: Option<ActionCallback>,
    ) -> Self {
        Self {
            tokens,
            state,
            on_action,
        }
    }
}

impl RenderOnce for NavigationDivider {
    fn render(self, _window: &mut Window, _cx: &mut App) -> impl IntoElement {
        let begin_callback = self.on_action.clone();
        let move_callback = self.on_action.clone();
        let end_callback = self.on_action.clone();
        let outside_end_callback = self.on_action.clone();
        let decrement_callback = self.on_action.clone();
        let increment_callback = self.on_action.clone();
        let reset_callback = self.on_action;
        div()
            .id(NAVIGATION_DIVIDER_ID)
            .debug_selector(|| NAVIGATION_DIVIDER_ID.to_owned())
            .role(Role::Splitter)
            .relative()
            .aria_label("Resize navigation pane")
            .aria_numeric_value(f64::from(self.state.navigation_pane_width().value()))
            .aria_min_numeric_value(f64::from(
                self.tokens.layout.navigation_pane_min_width.value(),
            ))
            .aria_max_numeric_value(f64::from(
                self.tokens.layout.navigation_pane_max_width.value(),
            ))
            .w(px(self.tokens.layout.divider_width.value()))
            .h_full()
            .flex_none()
            .cursor_col_resize()
            .bg(self.tokens.theme.colors.divider.to_gpui())
            .hover(move |style| style.bg(self.tokens.theme.colors.focus.to_gpui()))
            .active(move |style| style.bg(self.tokens.theme.colors.accent.to_gpui()))
            .child(region_probe(
                NAVIGATION_DIVIDER_ID,
                Some(EXPLORER_WINDOW_ID),
                "normal",
            ))
            .when_some(decrement_callback, |element, callback| {
                element.on_a11y_action(AccessibleAction::Decrement, move |_, window, cx| {
                    callback(
                        &ExplorerAction::AdjustNavigationPaneWidth { direction: -1 },
                        window,
                        cx,
                    );
                })
            })
            .when_some(increment_callback, |element, callback| {
                element.on_a11y_action(AccessibleAction::Increment, move |_, window, cx| {
                    callback(
                        &ExplorerAction::AdjustNavigationPaneWidth { direction: 1 },
                        window,
                        cx,
                    );
                })
            })
            .when_some(begin_callback, |element, callback| {
                element.on_mouse_down(MouseButton::Left, move |event, window, cx| {
                    callback(
                        &ExplorerAction::BeginNavigationPaneResize {
                            pointer_x: f32::from(event.position.x),
                        },
                        window,
                        cx,
                    );
                })
            })
            .when(self.state.divider_interaction().is_dragging(), |element| {
                element
                    .when_some(move_callback, |element, callback| {
                        element.on_mouse_move(move |event: &MouseMoveEvent, window, cx| {
                            callback(
                                &ExplorerAction::UpdateNavigationPaneResize {
                                    pointer_x: f32::from(event.position.x),
                                },
                                window,
                                cx,
                            );
                        })
                    })
                    .when_some(end_callback, |element, callback| {
                        element.on_mouse_up(MouseButton::Left, move |_, window, cx| {
                            callback(&ExplorerAction::EndNavigationPaneResize, window, cx);
                        })
                    })
                    .when_some(outside_end_callback, |element, callback| {
                        element.on_mouse_up_out(MouseButton::Left, move |_, window, cx| {
                            callback(&ExplorerAction::EndNavigationPaneResize, window, cx);
                        })
                    })
            })
            .when_some(reset_callback, |element, callback| {
                element.on_click(move |event, window, cx| {
                    if event.click_count() == 2 {
                        callback(&ExplorerAction::ResetNavigationPaneWidth, window, cx);
                    }
                })
            })
    }
}

impl NavigationPane {
    pub(crate) fn new(
        tokens: UiTokens,
        state: NavigationPaneViewModel,
        scroll_handle: Option<gpui::ScrollHandle>,
        shell_icons: HashMap<explorer_model::ShellIconKey, Arc<RenderImage>>,
        shell_icon_dpi: u16,
        on_action: Option<ActionCallback>,
    ) -> Self {
        Self {
            tokens,
            state,
            on_action,
            scroll_handle,
            shell_icons,
            shell_icon_dpi,
        }
    }
}

impl RenderOnce for NavigationPane {
    fn render(self, _window: &mut Window, _cx: &mut App) -> impl IntoElement {
        let layout = self.tokens.layout;
        let colors = self.tokens.theme.colors;
        let scroll_handle = self.scroll_handle;
        let on_action = self.on_action;
        let current_location = self
            .state
            .tabs()
            .active_tab()
            .history
            .current()
            .map(|entry| entry.location.clone());
        let can_write = self.state.active_presentation().can_write;
        let navigation_drop_cue = matches!(
            self.state.drag_session().state(),
            explorer_model::DragSessionState::Dragging {
                target: Some(explorer_model::DropTargetKind::NavigationItem),
                ..
            }
        );
        let shell_icon_theme = match self.tokens.theme.mode {
            crate::theme::ThemeMode::Light => explorer_model::ShellIconTheme::Light,
            crate::theme::ThemeMode::Dark => explorer_model::ShellIconTheme::Dark,
        };
        let generic_folder_texture = self.shell_icons.iter().find_map(|(key, texture)| {
            is_generic_breadcrumb_folder_icon_key(key).then(|| Arc::clone(texture))
        });
        div()
            .id(NAVIGATION_PANE_ID)
            .debug_selector(|| NAVIGATION_PANE_ID.to_owned())
            .role(Role::Document)
            .relative()
            .aria_label("Navigation pane; services unavailable")
            .w(px(self.state.navigation_pane_width().value()))
            .h_full()
            .flex_none()
            .flex()
            .flex_col()
            .overflow_y_scroll()
            .when_some(scroll_handle, |element, handle| {
                element.track_scroll(&handle)
            })
            .on_scroll_wheel(|_, _, cx| cx.refresh_windows())
            .py(px(layout.navigation_pane_vertical_padding.value()))
            .px(px(layout.control_padding_horizontal.value()))
            .border_r(px(layout.focus_stroke.value()))
            .border_color(colors.divider.to_gpui())
            .when(
                self.state.focused_surface() == FocusSurface::NavigationPane,
                |element| {
                    element
                        .border(px(layout.focus_stroke.value()))
                        .border_color(colors.focus.to_gpui())
                },
            )
            .when(navigation_drop_cue, |element| {
                element.border_color(colors.accent.to_gpui())
            })
            .bg(colors.surface.to_gpui())
            .child(region_probe(
                NAVIGATION_PANE_ID,
                Some(EXPLORER_WINDOW_ID),
                "normal",
            ))
            .child(typography_probe(
                NAVIGATION_PANE_ID,
                typography_diagnostic(self.tokens, self.tokens.typography.navigation),
            ))
            .when_some(on_action.clone(), |element, callback| {
                let move_callback = callback.clone();
                let can_drop_destination = current_location.clone();
                let drop_destination = current_location.clone();
                let move_destination = current_location.clone();
                element
                    .can_drop(move |value, _, _| {
                        value
                            .downcast_ref::<gpui::ExternalPaths>()
                            .is_some_and(|paths| {
                                negotiate_external_paths(
                                    paths,
                                    can_write,
                                    can_drop_destination.as_ref(),
                                ) != explorer_model::DragEffect::None
                            })
                    })
                    .on_drop(move |paths: &gpui::ExternalPaths, window, cx| {
                        let effect =
                            negotiate_external_paths(paths, can_write, drop_destination.as_ref());
                        callback(
                            &ExplorerAction::DropExternal {
                                paths: paths.paths().to_vec(),
                                destination_row: None,
                                effect,
                                right_button: paths.drop_metadata().right_button,
                                allowed: external_transfer_effects(paths),
                            },
                            window,
                            cx,
                        );
                    })
                    .on_drag_move::<gpui::ExternalPaths>(move |event, window, cx| {
                        let effect = negotiate_external_paths(
                            event.drag(cx),
                            can_write,
                            move_destination.as_ref(),
                        );
                        move_callback(
                            &ExplorerAction::UpdateExternalDrag {
                                destination_row: None,
                                target: explorer_model::DropTargetKind::NavigationItem,
                                pointer_y: f32::from(event.event.position.y),
                                top: f32::from(event.bounds.top()),
                                bottom: f32::from(event.bounds.bottom()),
                                effect,
                            },
                            window,
                            cx,
                        );
                    })
            })
            .children(bookmark_navigation_rows(
                &self.state,
                self.tokens,
                on_action.clone(),
            ))
            .children({
                let mut items =
                    windows_navigation_items_with_pins(self.state.quick_access_navigation_pins());
                let mut flattened = Vec::with_capacity(items.len());
                for mut item in items.drain(..) {
                    if let Some(location) = item.location.as_ref() {
                        item.expanded =
                            item.expanded || self.state.navigation_node_expanded(location);
                        if self.state.navigation_node_loading(location) {
                            item.label.push_str(" (Loading...)");
                        } else if self.state.navigation_node_error(location).is_some() {
                            item.label.push_str(" (Unavailable - expand to retry)");
                        }
                    }
                    let parent = item.location.clone();
                    let depth = item.depth;
                    let suppress_static_drive_roots = item.id == "this-pc";
                    flattened.push(item);
                    if let Some(parent) = parent
                        && self.state.navigation_node_expanded(&parent)
                    {
                        append_navigation_descendants(
                            &mut flattened,
                            &self.state,
                            &parent,
                            depth.saturating_add(1),
                            0,
                            suppress_static_drive_roots,
                        );
                    }
                }
                flattened.into_iter().map(|item| {
                    let texture = navigation_item_shell_texture(
                        item.icon,
                        item.icon_location.as_ref(),
                        &self.shell_icons,
                        generic_folder_texture.as_ref(),
                        shell_icon_theme,
                        self.shell_icon_dpi,
                    );
                    navigation_item_row(
                        item.clone(),
                        is_selected(&item, current_location.as_ref()),
                        self.tokens,
                        texture,
                        on_action.clone(),
                    )
                })
            })
    }
}

fn bookmark_navigation_rows(
    state: &AppViewState,
    tokens: UiTokens,
    callback: Option<ActionCallback>,
) -> Vec<gpui::AnyElement> {
    fn visit(
        output: &mut Vec<gpui::AnyElement>,
        state: &AppViewState,
        tokens: UiTokens,
        callback: &Option<ActionCallback>,
        parent: Option<explorer_model::BookmarkFolderId>,
        depth: u8,
    ) {
        for folder in state.bookmarks().child_folders(parent) {
            let id = folder.id;
            let expanded = state.bookmark_folder_expanded(id);
            let open = ExplorerAction::ToggleBookmarkFolderMenu { id };
            let left_cb = callback.clone();
            let right_cb = callback.clone();
            output.push(
                div()
                    .id(("favorite-folder-nav", id.as_u128() as u64))
                    .role(Role::Button)
                    .aria_label(format!("Favorite folder {}", folder.name))
                    .cursor_pointer()
                    .pl(px(8.0 + f32::from(depth) * 14.0))
                    .pr(px(8.0))
                    .py(px(5.0))
                    .rounded(px(4.0))
                    .hover(|style| style.bg(tokens.theme.colors.control_hover.to_gpui()))
                    .child(format!(
                        "{} 📁 {}",
                        if expanded { "▾" } else { "▸" },
                        folder.name
                    ))
                    .when_some(left_cb, {
                        let open = open.clone();
                        move |element, cb| {
                            element.on_click(move |_, window, cx| cb(&open, window, cx))
                        }
                    })
                    .when_some(right_cb, move |element, cb| {
                        element.on_mouse_down(MouseButton::Right, move |_, window, cx| {
                            cb(&open, window, cx);
                            cx.stop_propagation();
                        })
                    })
                    .into_any_element(),
            );
            if expanded {
                visit(
                    output,
                    state,
                    tokens,
                    callback,
                    Some(id),
                    depth.saturating_add(1),
                );
            }
        }
        for bookmark in state.bookmarks().child_entries(parent) {
            let action = ExplorerAction::ActivateBookmark { id: bookmark.id };
            let callback = callback.clone();
            let context_callback = callback.clone();
            let id = bookmark.id;
            output.push(
                div()
                    .id(("favorite-bookmark-nav", bookmark.id.as_u128() as u64))
                    .role(Role::Button)
                    .aria_label(format!("Favorite {}", bookmark.name))
                    .cursor_pointer()
                    .pl(px(22.0 + f32::from(depth) * 14.0))
                    .pr(px(8.0))
                    .py(px(5.0))
                    .rounded(px(4.0))
                    .hover(|style| style.bg(tokens.theme.colors.control_hover.to_gpui()))
                    .child(bookmark_label(&bookmark.target, bookmark.name.clone()))
                    .when_some(callback, move |element, cb| {
                        element.on_click(move |_, window, cx| cb(&action, window, cx))
                    })
                    .when_some(context_callback, move |element, cb| {
                        element.on_mouse_down(MouseButton::Right, move |event, window, cx| {
                            cx.stop_propagation();
                            cb(
                                &ExplorerAction::OpenBookmarkContextMenu {
                                    id,
                                    x: f32::from(event.position.x),
                                    y: f32::from(event.position.y),
                                },
                                window,
                                cx,
                            );
                        })
                    })
                    .on_mouse_up(MouseButton::Right, |_, _, cx| cx.stop_propagation())
                    .into_any_element(),
            );
        }
    }

    let add_root = ExplorerAction::AddBookmarkFolder { parent_id: None };
    let mut output = vec![
        div()
            .id("favorites-tree-heading")
            .role(Role::Heading)
            .aria_label("Favorites; right click to add a bookmark folder")
            .px(px(8.0))
            .py(px(5.0))
            .child("我的最愛")
            .when_some(callback.clone(), move |element, cb| {
                element.on_mouse_down(MouseButton::Right, move |_, window, cx| {
                    cb(&add_root, window, cx);
                    cx.stop_propagation();
                })
            })
            .into_any_element(),
    ];
    visit(&mut output, state, tokens, &callback, None, 0);
    output
}

fn navigation_item_shell_texture(
    icon: Option<crate::navigation_pane::NavigationIcon>,
    location: Option<&explorer_model::LocationDescriptor>,
    shell_icons: &HashMap<explorer_model::ShellIconKey, Arc<RenderImage>>,
    generic_folder_texture: Option<&Arc<RenderImage>>,
    theme: explorer_model::ShellIconTheme,
    dpi: u16,
) -> Option<Arc<RenderImage>> {
    if icon == Some(crate::navigation_pane::NavigationIcon::Folder) {
        return generic_folder_texture.cloned();
    }
    location.and_then(|location| navigation_shell_texture(shell_icons, location, theme, dpi))
}

fn navigation_shell_texture(
    shell_icons: &HashMap<explorer_model::ShellIconKey, Arc<RenderImage>>,
    location: &explorer_model::LocationDescriptor,
    theme: explorer_model::ShellIconTheme,
    dpi: u16,
) -> Option<Arc<RenderImage>> {
    let exact = shell_icon_key(location, theme, dpi);
    shell_icons.get(&exact).cloned().or_else(|| {
        shell_icons
            .iter()
            .filter(|(key, _)| key.location == *location && key.theme == theme && key.dpi == dpi)
            .max_by_key(|(key, _)| {
                (
                    key.association_generation,
                    key.overlay_generation,
                    key.item_id.is_some(),
                    key.size_bucket == exact.size_bucket,
                )
            })
            .map(|(_, texture)| Arc::clone(texture))
    })
}

fn breadcrumb_location_shell_texture(
    shell_icons: &HashMap<explorer_model::ShellIconKey, Arc<RenderImage>>,
    location: &explorer_model::LocationDescriptor,
    theme: explorer_model::ShellIconTheme,
    dpi: u16,
) -> Option<Arc<RenderImage>> {
    navigation_shell_texture(shell_icons, location, theme, dpi)
}

fn append_navigation_descendants(
    output: &mut Vec<NavigationItem>,
    state: &AppViewState,
    parent: &explorer_model::LocationDescriptor,
    depth: u8,
    recursion_depth: u8,
    suppress_static_drive_roots: bool,
) {
    if recursion_depth >= 32 || output.len() >= 4096 {
        return;
    }
    for child in state.navigation_node_children(parent) {
        if output.len() >= 4096 {
            break;
        }
        if suppress_static_drive_roots
            && !crate::navigation_pane::should_render_discovered_child(
                parent,
                &child.location,
                &child.display_name,
            )
        {
            continue;
        }
        let expanded = state.navigation_node_expanded(&child.location);
        let mut label = child.display_name.clone();
        if state.navigation_node_loading(&child.location) {
            label.push_str(" (Loading...)");
        } else if state.navigation_node_error(&child.location).is_some() {
            label.push_str(" (Unavailable - expand to retry)");
        }
        output.push(NavigationItem::child_container(
            label,
            child.location.clone(),
            depth,
            expanded,
        ));
        if expanded {
            append_navigation_descendants(
                output,
                state,
                &child.location,
                depth.saturating_add(1),
                recursion_depth.saturating_add(1),
                false,
            );
        }
    }
}

fn navigation_item_row(
    item: NavigationItem,
    selected: bool,
    tokens: UiTokens,
    shell_icon: Option<Arc<RenderImage>>,
    on_action: Option<ActionCallback>,
) -> gpui::AnyElement {
    let colors = tokens.theme.colors;
    if item.kind == NavigationItemKind::Separator {
        let probe_id = format!("navigation-item-{}", item.id);
        return div()
            .id(format!("nav-{}", item.id))
            .h(px(tokens.layout.navigation_separator_height.value()))
            .flex_none()
            .border_b(px(1.0))
            .border_color(colors.divider.to_gpui())
            .child(region_probe(
                probe_id,
                Some(NAVIGATION_PANE_ID),
                "separator",
            ))
            .into_any_element();
    }

    let probe_id = format!("navigation-item-{}", item.id);
    let chevron_probe_id = format!("navigation-item-{}-chevron", item.id);
    let chevron_element_id = format!("nav-chevron-{}", item.id);
    let location = item.location.clone();
    let available = item.availability == NavigationItemAvailability::Available;
    let has_chevron = available
        && item.location.is_some()
        && (item.kind == NavigationItemKind::Section
            || matches!(
                item.icon,
                Some(
                    crate::navigation_pane::NavigationIcon::Drive
                        | crate::navigation_pane::NavigationIcon::OneDrive
                        | crate::navigation_pane::NavigationIcon::Network
                        | crate::navigation_pane::NavigationIcon::Folder
                        | crate::navigation_pane::NavigationIcon::Phone
                        | crate::navigation_pane::NavigationIcon::Server
                )
            ));
    let toggle_location = item.location.clone();
    let toggle_callback = on_action.clone();
    let row = div()
        .id(format!("nav-{}", item.id))
        .debug_selector({
            let id = item.id.clone();
            move || format!("navigation-item-{id}")
        })
        .role(Role::Button)
        .aria_label(if available {
            item.label.clone()
        } else {
            format!("{} (Unavailable)", item.label)
        })
        .h(px(tokens.layout.navigation_row_height.value()))
        .w_full()
        .flex_none()
        .flex()
        .items_center()
        .rounded(px(3.0))
        .pl(px(f32::from(item.depth) * 17.0))
        .pr(px(5.0))
        .when(available, |element| {
            element.hover(move |style| style.bg(colors.row_hover.to_gpui()))
        })
        .when(selected, |element| {
            element.bg(colors.subtle_surface.to_gpui())
        })
        .when(!available, |element| {
            element
                .text_color(colors.text_disabled.to_gpui())
                .cursor_default()
        })
        .child(region_probe(
            probe_id,
            Some(NAVIGATION_PANE_ID),
            if selected { "selected" } else { "normal" },
        ))
        .child(
            div()
                .id(chevron_element_id)
                .w(px(tokens.layout.navigation_icon_size.value()))
                .h(px(tokens.layout.navigation_icon_size.value()))
                .flex_none()
                .flex()
                .items_center()
                .justify_center()
                .text_color(colors.text_secondary.to_gpui())
                .when(has_chevron, |element| {
                    element
                        .role(Role::Button)
                        .aria_label(if item.expanded { "Collapse" } else { "Expand" })
                        .aria_expanded(item.expanded)
                        .hover(move |style| style.bg(colors.control_hover.to_gpui()))
                        .when_some(
                            toggle_callback.zip(toggle_location),
                            |element, (callback, location)| {
                                element.on_click(move |_, window, cx| {
                                    cx.stop_propagation();
                                    callback(
                                        &ExplorerAction::ToggleNavigationNode {
                                            location: location.clone(),
                                        },
                                        window,
                                        cx,
                                    );
                                })
                            },
                        )
                        .child(chrome_icon(
                            chevron_probe_id,
                            if item.expanded {
                                ExplorerIcon::ChevronDown
                            } else {
                                ExplorerIcon::Chevron
                            },
                            tokens,
                        ))
                }),
        )
        .child(match (available, shell_icon, item.icon) {
            (false, _, _) => unavailable_navigation_icon(tokens).into_any_element(),
            (true, Some(texture), _) => div()
                .w(px(tokens.layout.navigation_icon_size.value()))
                .h(px(tokens.layout.navigation_icon_size.value()))
                .flex_none()
                .child(img(texture).size_full())
                .into_any_element(),
            (
                true,
                None,
                Some(
                    crate::navigation_pane::NavigationIcon::Drive
                    | crate::navigation_pane::NavigationIcon::Folder,
                ),
            ) => div()
                .w(px(tokens.layout.navigation_icon_size.value()))
                .h(px(tokens.layout.navigation_icon_size.value()))
                .flex_none()
                .into_any_element(),
            (true, None, Some(icon)) => navigation_icon(icon, tokens).into_any_element(),
            (true, None, None) => div().into_any_element(),
        })
        .child(
            div()
                .ml(px(8.0))
                .flex_1()
                .overflow_hidden()
                .whitespace_nowrap()
                .child(item.label),
        )
        .when(item.pinned, |element| {
            element.child(div().opacity(0.55).child(chrome_icon(
                "navigation-pin",
                ExplorerIcon::Pin,
                tokens,
            )))
        });
    row.when_some(
        available.then_some(()).zip(on_action).zip(location),
        |element, (((), callback), location)| {
            element.on_click(move |_, window, cx| {
                callback(
                    &ExplorerAction::ActivateNavigationItem {
                        location: location.clone(),
                    },
                    window,
                    cx,
                );
            })
        },
    )
    .into_any_element()
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FileViewState {
    Loading,
    Empty,
    Error,
    Ready,
    Disconnected,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FileViewStatus {
    state: FileViewState,
    error_message: Option<String>,
}

#[derive(Clone)]
pub struct FileViewModel {
    status: FileViewStatus,
    presentation: Option<crate::file_view::DirectoryPresentation>,
    performance: Option<Arc<crate::performance::FileViewPerformanceCounters>>,
    selection: explorer_model::SelectionModel,
}

impl FileViewStatus {
    pub fn from_directory(directory: &DirectoryState) -> Self {
        Self {
            state: FileViewState::from_directory(directory),
            error_message: match directory {
                DirectoryState::Error { error, .. } => Some(error.user_message.clone()),
                _ => None,
            },
        }
    }

    pub fn from_tab(tab: &explorer_model::TabState) -> Self {
        match &tab.search {
            TabSearchState::Loading { results, .. } => Self {
                state: if results.entries().is_empty() {
                    FileViewState::Loading
                } else {
                    FileViewState::Ready
                },
                error_message: None,
            },
            TabSearchState::Ready { results, .. } | TabSearchState::Cancelled { results, .. } => {
                Self {
                    state: if results.entries().is_empty() {
                        FileViewState::Empty
                    } else {
                        FileViewState::Ready
                    },
                    error_message: None,
                }
            }
            TabSearchState::Partial { error, .. } | TabSearchState::Error { error, .. } => Self {
                state: FileViewState::Error,
                error_message: Some(error.user_message.clone()),
            },
            TabSearchState::Idle | TabSearchState::Editing(_) => {
                Self::from_directory(&tab.directory)
            }
        }
    }

    fn message(&self) -> String {
        self.error_message
            .clone()
            .unwrap_or_else(|| self.state.message().to_owned())
    }
}

impl FileViewState {
    pub const fn message(self) -> &'static str {
        match self {
            Self::Loading => "Loading folder",
            Self::Empty => "This folder is empty",
            Self::Error => "Folder could not be displayed",
            Self::Ready => "Folder is ready",
            Self::Disconnected => "Directory service is not connected",
        }
    }

    pub fn from_directory(state: &DirectoryState) -> Self {
        match state {
            DirectoryState::Idle => Self::Disconnected,
            DirectoryState::Loading { snapshot, .. } if snapshot.entries().is_empty() => {
                Self::Loading
            }
            DirectoryState::Ready(snapshot) if snapshot.entries().is_empty() => Self::Empty,
            DirectoryState::Loading { .. } | DirectoryState::Ready(_) => Self::Ready,
            DirectoryState::Error { .. } => Self::Error,
        }
    }
}

#[derive(IntoElement)]
pub struct FileViewHost {
    tokens: UiTokens,
    model: FileViewModel,
    rename_editor: Option<explorer_model::RenameEditorState>,
    provisional_new_folder: Option<explorer_model::FileEntry>,
    rename_input: Option<gpui::WeakEntity<EditableTextState>>,
    clipboard: explorer_model::ClipboardState,
    drag_state: explorer_model::DragSessionState,
    drop_target_row: Option<usize>,
    target_can_write: bool,
    drop_destination: Option<explorer_model::LocationDescriptor>,
    context_menu_pending: bool,
    marquee: Option<crate::state::MarqueeSelectionSession>,
    file_origin_x: f32,
    file_origin_y: f32,
    view_settings: explorer_model::ViewSettings,
    column_registry: explorer_model::ColumnRegistry,
    viewport_width: f32,
    scroll_handle: Option<gpui::ScrollHandle>,
    shell_icons: HashMap<explorer_model::ShellIconKey, Arc<RenderImage>>,
    thumbnail_icon_keys: HashSet<explorer_model::ShellIconKey>,
    shell_icon_dpi: u16,
    details_column_menu: Option<explorer_model::ColumnId>,
    details_filter_menu: Option<explorer_model::ColumnId>,
    details_filters: crate::file_view::DetailsFilters,
    details_filter_options:
        HashMap<explorer_model::ColumnId, Vec<crate::file_view::DetailsFilterOption>>,
    folder_size_visuals: Option<crate::folder_size_column::FolderSizeColumnVisuals>,
    visual_column_runtime: Option<crate::folder_size_column::VisualColumnRuntimeHandleV1>,
    code_lines_visuals: Vec<crate::code_lines_column::CodeLinesColumnVisuals>,
    code_lines_runtimes: Vec<crate::code_lines_column::CodeLinesRuntimeHandleV1>,
    size_map_active: bool,
    size_map_visuals: Option<crate::size_map_view::SizeMapVisualsV1>,
    size_map_runtime: Option<crate::size_map_view::SizeMapRuntimeHandleV1>,
    size_map_context: Option<explorer_model::RequestContext>,
    active_request_context: explorer_model::RequestContext,
    on_action: Option<ActionCallback>,
}

impl FileViewHost {
    #[allow(
        clippy::too_many_arguments,
        reason = "render-once file view receives one immutable value for each independent presentation concern"
    )]
    pub(crate) fn new(
        tokens: UiTokens,
        status: FileViewStatus,
        presentation: Option<crate::file_view::DirectoryPresentation>,
        performance: Option<Arc<crate::performance::FileViewPerformanceCounters>>,
        selection: explorer_model::SelectionModel,
        rename_editor: Option<explorer_model::RenameEditorState>,
        provisional_new_folder: Option<explorer_model::FileEntry>,
        rename_input: Option<gpui::WeakEntity<EditableTextState>>,
        clipboard: explorer_model::ClipboardState,
        drag_state: explorer_model::DragSessionState,
        drop_target_row: Option<usize>,
        target_can_write: bool,
        drop_destination: Option<explorer_model::LocationDescriptor>,
        context_menu_pending: bool,
        marquee: Option<crate::state::MarqueeSelectionSession>,
        file_origin_x: f32,
        file_origin_y: f32,
        view_settings: explorer_model::ViewSettings,
        column_registry: explorer_model::ColumnRegistry,
        viewport_width: f32,
        scroll_handle: Option<gpui::ScrollHandle>,
        shell_icons: HashMap<explorer_model::ShellIconKey, Arc<RenderImage>>,
        thumbnail_icon_keys: HashSet<explorer_model::ShellIconKey>,
        shell_icon_dpi: u16,
        details_column_menu: Option<explorer_model::ColumnId>,
        details_filter_menu: Option<explorer_model::ColumnId>,
        details_filters: crate::file_view::DetailsFilters,
        details_filter_options: HashMap<
            explorer_model::ColumnId,
            Vec<crate::file_view::DetailsFilterOption>,
        >,
        folder_size_visuals: Option<crate::folder_size_column::FolderSizeColumnVisuals>,
        visual_column_runtime: Option<crate::folder_size_column::VisualColumnRuntimeHandleV1>,
        code_lines_visuals: Vec<crate::code_lines_column::CodeLinesColumnVisuals>,
        code_lines_runtimes: Vec<crate::code_lines_column::CodeLinesRuntimeHandleV1>,
        size_map_active: bool,
        size_map_visuals: Option<crate::size_map_view::SizeMapVisualsV1>,
        size_map_runtime: Option<crate::size_map_view::SizeMapRuntimeHandleV1>,
        size_map_context: Option<explorer_model::RequestContext>,
        active_request_context: explorer_model::RequestContext,
        on_action: Option<ActionCallback>,
    ) -> Self {
        Self {
            tokens,
            model: FileViewModel {
                status,
                presentation,
                performance,
                selection,
            },
            rename_editor,
            provisional_new_folder,
            rename_input,
            clipboard,
            drag_state,
            drop_target_row,
            target_can_write,
            drop_destination,
            context_menu_pending,
            marquee,
            file_origin_x,
            file_origin_y,
            view_settings,
            column_registry,
            viewport_width,
            scroll_handle,
            shell_icons,
            thumbnail_icon_keys,
            shell_icon_dpi,
            details_column_menu,
            details_filter_menu,
            details_filters,
            details_filter_options,
            folder_size_visuals,
            visual_column_runtime,
            code_lines_visuals,
            code_lines_runtimes,
            size_map_active,
            size_map_visuals,
            size_map_runtime,
            size_map_context,
            active_request_context,
            on_action,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct MarqueeContentRect {
    left: f32,
    top: f32,
    width: f32,
    height: f32,
}

fn file_view_local_pointer(
    pointer_x: f32,
    pointer_y: f32,
    actual_origin: Option<(f32, f32)>,
    fallback_origin: (f32, f32),
) -> (f32, f32) {
    let (origin_x, origin_y) = actual_origin.unwrap_or(fallback_origin);
    (pointer_x - origin_x, pointer_y - origin_y)
}

fn details_name_column_contains(
    viewport_x: f32,
    horizontal_scroll: f32,
    leading_padding: f32,
    name_width: f32,
) -> bool {
    viewport_x + horizontal_scroll <= leading_padding + name_width
}

fn marquee_content_rect(
    origin_x: f32,
    origin_y: f32,
    current_x: f32,
    current_y: f32,
    horizontal_scroll: f32,
    vertical_scroll: f32,
) -> MarqueeContentRect {
    MarqueeContentRect {
        left: origin_x.min(current_x) + horizontal_scroll,
        top: origin_y.min(current_y) + vertical_scroll,
        width: (current_x - origin_x).abs(),
        height: (current_y - origin_y).abs(),
    }
}

fn authorize_view_selection(
    bridge: &crate::size_map_view::ViewSelectionBridgeV1,
    node_id: explorer_extension_ui_api::StableIdV1,
) -> Option<usize> {
    bridge.authorize_selection(&explorer_extension_ui_api::ViewSelectionRequestV1 {
        snapshot: bridge.snapshot(),
        operation: explorer_extension_ui_api::ViewSelectionOperationV1::REPLACE,
        node_ids: vec![node_id].into(),
    })
}

fn size_map_surface(
    tokens: UiTokens,
    plan: crate::size_map_view::SizeMapRenderPlanV1,
    indexes: HashMap<explorer_model::ShellItemId, usize>,
    selected: HashSet<explorer_model::ShellItemId>,
    on_action: Option<ActionCallback>,
) -> impl IntoElement {
    let colors = tokens.theme.colors;
    let layout = tokens.layout;
    let map_background = crate::theme::Rgba8::opaque(24, 24, 27).to_gpui();
    let map_divider = crate::theme::Rgba8::opaque(12, 12, 14).to_gpui();
    let map_text = crate::theme::Rgba8::opaque(255, 255, 255).to_gpui();
    let status = plan.status.clone();
    let interaction_bridge = plan.snapshot.map(|snapshot| {
        Arc::new(crate::size_map_view::ViewSelectionBridgeV1::new(
            snapshot,
            plan.rectangles
                .iter()
                .filter_map(|rectangle| {
                    Some((
                        rectangle.node_id?,
                        *indexes.get(&rectangle.interaction_target.as_ref()?.selection_item_id)?,
                    ))
                })
                .collect(),
        ))
    });
    div()
        .id("size-map-view")
        .debug_selector(|| "size-map-view".to_owned())
        .role(Role::TabPanel)
        .absolute()
        .inset_0()
        .overflow_hidden()
        .bg(map_background)
        .child(div().absolute().inset_0().child(region_probe(
            "size-map-view",
            Some(FILE_VIEW_HOST_ID),
            "normal",
        )))
        .children(plan.rectangles.into_iter().filter_map(move |rectangle| {
            let row_index = rectangle
                .item_id
                .as_ref()
                .and_then(|item_id| indexes.get(item_id).copied());
            let public_node_id = rectangle.node_id;
            let item_id = rectangle.item_id.clone();
            let interaction_target = rectangle.interaction_target.clone();
            let is_selected = item_id
                .as_ref()
                .is_some_and(|item_id| selected.contains(item_id));
            let label = rectangle.label.clone();
            let detail = rectangle.detail.clone();
            let visible_label = detail
                .split_once(" bytes")
                .and_then(|(bytes, _)| bytes.trim().parse::<u64>().ok())
                .map_or_else(
                    || label.clone(),
                    |bytes| format!("{label} ({})", crate::format_file_size(bytes)),
                );
            let status = rectangle.status.clone();
            let aggregate_items = rectangle.aggregate_items;
            let selector = item_id.as_ref().map_or_else(
                || "size-map-node-other".to_owned(),
                |item_id| format!("size-map-node-{:02x?}", item_id.provider_bytes()),
            );
            let node = div()
                .id(selector)
                .role(if item_id.is_some() {
                    Role::Button
                } else {
                    Role::Group
                })
                .absolute()
                .left(px(rectangle.x.max(0.0)))
                .top(px(rectangle.y.max(0.0)))
                .w(px(rectangle.width.max(1.0)))
                .h(px(rectangle.height.max(1.0)))
                .overflow_hidden()
                .p(px(layout.content_spacing.value() / 2.0))
                .border(px(if is_selected { 3.0 } else { 1.0 }))
                .border_color(if is_selected {
                    map_text
                } else {
                    map_divider
                })
                .bg(rectangle.color.to_gpui())
                .text_color(map_text)
                .hover(move |style| style.border_color(map_text))
                .aria_label(format!("{label}: {detail}. {status}"))
                // Every projected rectangle, including the synthetic `Other`
                // group, participates in normal keyboard/UIA traversal. Only
                // rectangles with a real ShellItemId receive select/open
                // authority below.
                .focusable()
                .tab_stop(true)
                .child(
                    div()
                        .whitespace_nowrap()
                        .text_ellipsis()
                        .child(visible_label),
                )
                .children(aggregate_items.into_iter().filter_map(|aggregate| {
                    let row_index = indexes.get(&aggregate.item_id).copied()?;
                    let callback = on_action.clone()?;
                    let focus_callback = callback.clone();
                    let selector = format!(
                        "size-map-other-item-{:02x?}",
                        aggregate.item_id.provider_bytes()
                    );
                    Some(
                        div()
                            .id(selector)
                            .role(Role::Button)
                            .aria_label(format!("{}: {}", aggregate.label, aggregate.detail))
                            .focusable()
                            .tab_stop(true)
                            .absolute()
                            .left(px(0.0))
                            .top(px(0.0))
                            .size(px(1.0))
                            .opacity(0.0)
                            .overflow_hidden()
                            .child(aggregate.label)
                            .on_a11y_action(AccessibleAction::Focus, move |_, window, cx| {
                                focus_callback(
                                    &ExplorerAction::SelectItem { row_index },
                                    window,
                                    cx,
                                );
                            })
                            .on_a11y_action(AccessibleAction::Click, move |_, window, cx| {
                                callback(&ExplorerAction::SelectItem { row_index }, window, cx);
                            }),
                    )
                }));
            let node = node.when_some(
                row_index
                    .zip(item_id)
                    .zip(public_node_id)
                    .zip(interaction_bridge.clone())
                    .zip(on_action.clone()),
                |node, ((((_row_index, _), node_id), bridge), callback)| {
                    let select_callback = callback.clone();
                    let open_callback = callback.clone();
                    let accessibility_select = callback.clone();
                    let accessibility_focus = callback;
                    let mouse_bridge = Arc::clone(&bridge);
                    let click_bridge = Arc::clone(&bridge);
                    let focus_bridge = bridge;
                    node.on_mouse_down(MouseButton::Left, move |event, window, cx| {
                        cx.stop_propagation();
                        if event.click_count == 2 {
                            let request = explorer_extension_ui_api::NavigationRequestV1 {
                                snapshot: mouse_bridge.snapshot(),
                                operation: if event.modifiers.control {
                                    explorer_extension_ui_api::ViewNavigationOperationV1::OPEN_NEW_TAB
                                } else {
                                    explorer_extension_ui_api::ViewNavigationOperationV1::ENTER
                                },
                                node_id,
                            };
                            if let Some(authorized) = mouse_bridge.authorize_navigation(&request) {
                                open_callback(
                                    &ExplorerAction::OpenItem {
                                        row_index: authorized.row_index,
                                        new_tab: authorized.new_tab,
                                    },
                                    window,
                                    cx,
                                );
                            }
                        } else if let Some(row_index) =
                            authorize_view_selection(&mouse_bridge, node_id)
                        {
                            select_callback(&ExplorerAction::SelectItem { row_index }, window, cx);
                        }
                    })
                    .on_a11y_action(AccessibleAction::Click, move |_, window, cx| {
                        if let Some(row_index) = authorize_view_selection(&click_bridge, node_id) {
                            accessibility_select(
                                &ExplorerAction::SelectItem { row_index },
                                window,
                                cx,
                            );
                        }
                    })
                    .on_a11y_action(
                        AccessibleAction::Focus,
                        move |_, window, cx| {
                            if let Some(row_index) = authorize_view_selection(&focus_bridge, node_id) {
                                accessibility_focus(
                                    &ExplorerAction::SelectItem { row_index },
                                    window,
                                    cx,
                                );
                            }
                        },
                    )
                },
            );
            Some(node.when_some(
                row_index
                    .is_none()
                    .then_some(interaction_target)
                    .flatten()
                    .zip(public_node_id)
                    .zip(interaction_bridge.clone())
                    .zip(on_action.clone()),
                |node, (((target, node_id), bridge), callback)| {
                    let select_callback = callback.clone();
                    let open_callback = callback.clone();
                    let mouse_bridge = bridge;
                    node.on_mouse_down(MouseButton::Left, move |event, window, cx| {
                        cx.stop_propagation();
                        if event.click_count == 2 {
                            open_callback(
                                &ExplorerAction::OpenExtensionViewItem {
                                    item_id: target.item_id.clone(),
                                    location: target.location.clone(),
                                    is_container: target.is_container,
                                    new_tab: event.modifiers.control,
                                },
                                window,
                                cx,
                            );
                        } else if let Some(row_index) =
                            authorize_view_selection(&mouse_bridge, node_id)
                        {
                            select_callback(&ExplorerAction::SelectItem { row_index }, window, cx);
                        }
                    })
                },
            ))
        }))
        .when_some(status, |element, status| {
            element.child(
                div()
                    .absolute()
                    .left(px(layout.content_spacing.value()))
                    .bottom(px(layout.content_spacing.value()))
                    .rounded(px(layout.corner_radius.value()))
                    .bg(colors.menu_fill.to_gpui())
                    .px(px(layout.content_spacing.value()))
                    .py(px(layout.content_spacing.value() / 2.0))
                    .child(status),
            )
        })
}

impl RenderOnce for FileViewHost {
    #[allow(
        clippy::too_many_lines,
        clippy::cast_precision_loss,
        reason = "file rows share one render pass so selection and cut-state styling cannot diverge"
    )]
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let render_started = Instant::now();
        let performance = self.model.performance;
        let presentation = self.model.presentation;
        let message = self.model.status.message();
        let colors = self.tokens.theme.colors;
        let window_active = window.is_window_active();
        let selection_active = file_row_selection_active(window_active, self.context_menu_pending);
        let layout = self.tokens.layout;
        let on_action = self.on_action;
        let details_filter_menu = self.details_filter_menu;
        let details_filters = self.details_filters;
        let details_filter_options = self.details_filter_options;
        let folder_size_visuals = self.folder_size_visuals;
        let visual_column_runtime = self.visual_column_runtime;
        let code_lines_visuals = self.code_lines_visuals;
        let code_lines_runtimes = self.code_lines_runtimes;
        let mut code_lines_columns = code_lines_visuals
            .iter()
            .cloned()
            .map(|visuals| {
                match code_lines_runtimes
                    .iter()
                    .find(|runtime| runtime.config().descriptor.id == visuals.config.descriptor.id)
                    .cloned()
                {
                    Some(runtime) => CodeLinesDetailColumn::Ready(visuals, runtime),
                    None => CodeLinesDetailColumn::Unavailable(visuals.config.descriptor.clone()),
                }
            })
            .collect::<Vec<_>>();
        code_lines_columns.sort_by(|left, right| left.id().cmp(right.id()));
        let size_map_active = self.size_map_active;
        let size_map_visuals = self.size_map_visuals;
        let size_map_runtime = self.size_map_runtime;
        let size_map_context = self.size_map_context;
        let cell_request_generation = self.active_request_context.generation.value();
        let row_folder_size_visuals = folder_size_visuals.clone();
        let row_visual_column_runtime = visual_column_runtime.clone();
        let row_code_lines_columns = code_lines_columns;
        let background_drop = on_action.clone();
        let background_menu_action = on_action.clone();
        let zoom_action = on_action.clone();
        let rename_editor = self.rename_editor;
        let provisional_new_folder = self.provisional_new_folder;
        let rename_input = self.rename_input;
        let rename_input_focus = rename_input
            .as_ref()
            .and_then(gpui::WeakEntity::upgrade)
            .map(|input| input.read(cx).focus_handle(cx));
        let selection = self.model.selection;
        let clipboard = self.clipboard;
        let drag_state = self.drag_state;
        let drop_target_row = self.drop_target_row;
        let target_can_write = self.target_can_write;
        let drop_destination = self.drop_destination;
        let viewport_width = self.viewport_width;
        let marquee = self.marquee;
        let file_origin_x = self.file_origin_x;
        let file_origin_y = self.file_origin_y;
        let view_settings = self.view_settings;
        let column_registry = self.column_registry;
        let details_mode = view_settings.mode == explorer_model::ViewMode::Details;
        let details_name_width =
            view_settings.details_column_width(&explorer_model::ColumnId::Name);
        let drive_view = presentation.as_ref().is_some_and(|presentation| {
            (0..presentation.len()).any(|index| {
                presentation
                    .entry(index)
                    .is_some_and(|(_, entry)| entry.metadata.drive.is_some())
            })
        });
        let spatial_metrics =
            spatial_grid_metrics_with_registry(&view_settings, &column_registry, layout);
        let spatial_metrics = if drive_view {
            this_pc_spatial_grid_metrics(view_settings.mode, layout)
        } else {
            spatial_metrics
        };
        let presentation_len = presentation
            .as_ref()
            .map_or(0, crate::file_view::DirectoryPresentation::len);
        let item_count = presentation_len + usize::from(provisional_new_folder.is_some());
        let spatial_layout = spatial_grid_layout(spatial_metrics, viewport_width, item_count);
        let spatial_metrics = spatial_layout.metrics;
        let wrapped_view = spatial_metrics.wrapped;
        let render_item_width =
            if drive_view && view_settings.mode == explorer_model::ViewMode::Details {
                this_pc_details_width()
            } else {
                view_item_width_with_registry(&view_settings, &column_registry)
            };
        let scroll_handle = self.scroll_handle;
        let marquee_scroll = scroll_handle.clone();
        let background_scroll = scroll_handle.clone();
        let row_pointer_scroll = scroll_handle.clone();
        if view_settings.mode == explorer_model::ViewMode::Details
            && let Some(handle) = scroll_handle.as_ref()
        {
            let maximum = details_horizontal_maximum_with_registry(
                &view_settings,
                &column_registry,
                self.viewport_width,
            );
            let offset = handle.offset();
            let clamped_x = (-f32::from(offset.x)).clamp(0.0, maximum);
            if (f32::from(offset.x) + clamped_x).abs() > f32::EPSILON {
                handle.set_offset(point(px(-clamped_x), offset.y));
            }
        }
        let scroll_offset = scroll_handle
            .as_ref()
            .map_or(0.0, |handle| -f32::from(handle.offset().y));
        let horizontal_scroll_offset = scroll_handle
            .as_ref()
            .map_or(0.0, |handle| -f32::from(handle.offset().x));
        let viewport_height = scroll_handle
            .as_ref()
            .map(|handle| f32::from(handle.bounds().size.height))
            .filter(|height| *height > 0.0)
            .unwrap_or_else(|| {
                explorer_file_viewport_height(window, self.tokens).max(spatial_metrics.cell_height)
            });
        let background_menu_viewport_height = explorer_file_viewport_height(window, self.tokens);
        let background_menu_header_height = if details_mode {
            layout.details_header_height.value()
        } else {
            0.0
        };
        let size_map_plan = if size_map_active {
            match (
                size_map_runtime.as_ref(),
                size_map_visuals.as_ref(),
                size_map_context.as_ref(),
            ) {
                (Some(runtime), Some(visuals), Some(context)) => {
                    let nodes = presentation.as_ref().map_or_else(Vec::new, |presentation| {
                        let entries = (0..presentation.len())
                            .filter_map(|index| {
                                presentation.entry(index).map(|(_, entry)| entry.clone())
                            })
                            .collect::<Vec<_>>();
                        visuals.recursive_nodes_for(&entries)
                    });
                    let indexes = presentation
                        .as_ref()
                        .map_or_else(HashMap::new, |presentation| {
                            (0..presentation.len())
                                .filter_map(|index| {
                                    presentation
                                        .entry(index)
                                        .map(|(_, entry)| (entry.id.clone(), index))
                                })
                                .collect()
                        });
                    let context = crate::size_map_view::SizeMapRenderContextV1 {
                        request_context: context.clone(),
                        nodes,
                        selected: selection.iter().cloned().collect(),
                        viewport_width_milli: (viewport_width.max(0.0) * 1_000.0) as u32,
                        viewport_height_milli: (viewport_height.max(0.0) * 1_000.0) as u32,
                        dark_theme: matches!(self.tokens.theme.mode, crate::theme::ThemeMode::Dark),
                    };
                    let plan = runtime.render_size_map(context);
                    plan.available.then(|| {
                        (
                            plan,
                            indexes,
                            selection.iter().cloned().collect::<HashSet<_>>(),
                        )
                    })
                }
                _ => None,
            }
        } else {
            None
        };
        let has_size_map_plan = size_map_plan.is_some();
        let (realized_range, leading_space, trailing_space) = if item_count == 0 {
            (0..0, 0, 0)
        } else {
            if wrapped_view {
                let grid = crate::file_view::fixed_grid_virtual_range(
                    item_count,
                    spatial_metrics.cell_width,
                    spatial_metrics.cell_height,
                    viewport_width.max(spatial_metrics.cell_width),
                    viewport_height,
                    scroll_offset,
                    2,
                );
                (
                    grid.items,
                    grid.leading_logical_pixels,
                    grid.trailing_logical_pixels,
                )
            } else {
                let header_height = if view_settings.mode == explorer_model::ViewMode::Details {
                    layout.details_header_height.value()
                } else {
                    0.0
                };
                let range = crate::file_view::fixed_virtual_range(
                    item_count,
                    spatial_metrics.cell_height,
                    (viewport_height - header_height).max(spatial_metrics.cell_height),
                    (scroll_offset - header_height).max(0.0),
                    2,
                );
                (
                    range.items,
                    range.leading_logical_pixels,
                    range.trailing_logical_pixels,
                )
            }
        };
        let presentation_empty = item_count == 0;
        let accessibility_set_size = item_count;
        let entries = realized_range
            .filter_map(|visible_index| {
                if visible_index == presentation_len {
                    provisional_new_folder
                        .as_ref()
                        .cloned()
                        .map(|entry| (visible_index, usize::MAX, entry))
                } else {
                    presentation.as_ref().and_then(|presentation| {
                        presentation
                            .entry(visible_index)
                            .map(|(snapshot_index, entry)| {
                                (visible_index, snapshot_index, entry.clone())
                            })
                    })
                }
            })
            .collect::<Vec<_>>();
        if let Some(performance) = performance.as_ref() {
            performance.record_realized_items(entries.len());
        }
        let (fixed_header_left, fixed_header_top) =
            details_header_overlay_position(scroll_handle.as_ref().map_or((0.0, 0.0), |handle| {
                let offset = handle.offset();
                (f32::from(offset.x), f32::from(offset.y))
            }));
        let header_action = on_action.clone();
        let column_menu = self.details_column_menu;
        let column_menu_action = on_action.clone();
        let filter_menu_dismiss = on_action.clone();
        let column_menu_dismiss = on_action.clone();
        let shell_icons = self.shell_icons;
        let generic_folder_icon = shell_icons.iter().find_map(|(key, texture)| {
            is_generic_breadcrumb_folder_icon_key(key).then(|| Arc::clone(texture))
        });
        let thumbnail_icon_keys = self.thumbnail_icon_keys;
        let shell_icon_dpi = self.shell_icon_dpi;
        let shell_icon_theme = match self.tokens.theme.mode {
            crate::theme::ThemeMode::Light => explorer_model::ShellIconTheme::Light,
            crate::theme::ThemeMode::Dark => explorer_model::ShellIconTheme::Dark,
        };
        let visual_column_theme = shared_visual_column_theme(colors);
        let (rename_text, rename_selection, rename_selection_text, rename_caret) =
            editable_input_colors(self.tokens);
        let background_cue = matches!(
            drag_state,
            explorer_model::DragSessionState::Dragging {
                target: Some(explorer_model::DropTargetKind::FileView),
                ..
            }
        );
        let scroll_performance = performance.clone();
        let row_settings = Arc::new(view_settings.clone());
        let row_column_registry = column_registry.clone();
        let size_map_action = on_action.clone();
        let scroll_content = div()
            .id(FILE_VIEW_HOST_ID)
            .debug_selector(|| FILE_VIEW_HOST_ID.to_owned())
            .role(Role::TabPanel)
            .relative()
            .aria_label(message.clone())
            .flex_1()
            .h_full()
            .flex()
            .flex_col()
            .when(wrapped_view, |element| {
                element.flex_row().flex_wrap().items_start().content_start()
            })
            .when(
                view_settings.mode == explorer_model::ViewMode::Details && !has_size_map_plan,
                StatefulInteractiveElement::overflow_scroll,
            )
            .when(
                view_settings.mode != explorer_model::ViewMode::Details,
                StatefulInteractiveElement::overflow_y_scroll,
            )
            .when_some(scroll_handle, |element, handle| {
                element.track_scroll(&handle)
            })
            .on_scroll_wheel(move |event, window, cx| {
                let scroll_started = Instant::now();
                if event.modifiers.control {
                    let delta_y = match event.delta {
                        gpui::ScrollDelta::Pixels(delta) => f32::from(delta.y),
                        gpui::ScrollDelta::Lines(delta) => delta.y,
                    };
                    if delta_y != 0.0
                        && let Some(callback) = zoom_action.as_ref()
                    {
                        callback(
                            &ExplorerAction::ZoomView {
                                direction: if delta_y > 0.0 { 1 } else { -1 },
                            },
                            window,
                            cx,
                        );
                    }
                } else {
                    window.refresh();
                }
                if let Some(performance) = scroll_performance.as_ref() {
                    performance.record_scroll(scroll_started.elapsed());
                }
            })
            .bg(colors.surface.to_gpui())
            .text_color(colors.text_secondary.to_gpui())
            .child(region_probe(
                FILE_VIEW_HOST_ID,
                Some(EXPLORER_WINDOW_ID),
                "normal",
            ))
            .when(background_cue, |element| {
                element
                    .border(px(layout.focus_stroke.value()))
                    .border_color(colors.accent.to_gpui())
            })
            .when_some(background_drop, |element, callback| {
                let move_callback = callback.clone();
                let marquee_begin = callback.clone();
                let marquee_move = callback.clone();
                let marquee_end = callback.clone();
                let can_drop_destination = drop_destination.clone();
                let background_destination = drop_destination.clone();
                let move_destination = drop_destination.clone();
                element
                    .can_drop(move |value, _, _| {
                        value
                            .downcast_ref::<gpui::ExternalPaths>()
                            .is_some_and(|paths| {
                                negotiate_external_paths(
                                    paths,
                                    target_can_write,
                                    can_drop_destination.as_ref(),
                                ) != explorer_model::DragEffect::None
                            })
                    })
                    .on_drop(move |paths: &gpui::ExternalPaths, window, cx| {
                        let effect = negotiate_external_paths(
                            paths,
                            target_can_write,
                            background_destination.as_ref(),
                        );
                        callback(
                            &ExplorerAction::DropExternal {
                                paths: paths.paths().to_vec(),
                                destination_row: None,
                                effect,
                                right_button: paths.drop_metadata().right_button,
                                allowed: external_transfer_effects(paths),
                            },
                            window,
                            cx,
                        );
                    })
                    .on_drag_move::<gpui::ExternalPaths>(move |event, window, cx| {
                        let effect = negotiate_external_paths(
                            event.drag(cx),
                            target_can_write,
                            move_destination.as_ref(),
                        );
                        move_callback(
                            &ExplorerAction::UpdateExternalDrag {
                                destination_row: None,
                                target: explorer_model::DropTargetKind::FileView,
                                pointer_y: f32::from(event.event.position.y),
                                top: f32::from(event.bounds.top()),
                                bottom: f32::from(event.bounds.bottom()),
                                effect,
                            },
                            window,
                            cx,
                        );
                    })
                    .on_mouse_down(MouseButton::Left, move |event, window, cx| {
                        if view_settings.mode == explorer_model::ViewMode::Details
                            && background_scroll.as_ref().is_some_and(|handle| {
                                event.position.y
                                    < handle.bounds().top()
                                        + px(layout.details_header_height.value())
                            })
                        {
                            // The fixed Details header is painted inside the scroll host. Its
                            // buttons and resize splitters own this pointer press; the background
                            // must never start a marquee underneath them.
                            cx.stop_propagation();
                            return;
                        }
                        let actual_origin = background_scroll.as_ref().map(|handle| {
                            let bounds = handle.bounds();
                            (f32::from(bounds.left()), f32::from(bounds.top()))
                        });
                        let (x, y) = file_view_local_pointer(
                            f32::from(event.position.x),
                            f32::from(event.position.y),
                            actual_origin,
                            (file_origin_x, file_origin_y),
                        );
                        marquee_begin(
                            &ExplorerAction::BeginMarquee {
                                x,
                                y,
                                additive: event.modifiers.control,
                            },
                            window,
                            cx,
                        );
                    })
                    .on_mouse_move(move |event, window, cx| {
                        if event.dragging() {
                            let actual_origin = marquee_scroll.as_ref().map(|handle| {
                                let bounds = handle.bounds();
                                (f32::from(bounds.left()), f32::from(bounds.top()))
                            });
                            let (x, y) = file_view_local_pointer(
                                f32::from(event.position.x),
                                f32::from(event.position.y),
                                actual_origin,
                                (file_origin_x, file_origin_y),
                            );
                            marquee_move(
                                &ExplorerAction::UpdateMarquee {
                                    x,
                                    y,
                                    scroll_y: marquee_scroll
                                        .as_ref()
                                        .map_or(0.0, |handle| -f32::from(handle.offset().y)),
                                    viewport_width,
                                },
                                window,
                                cx,
                            );
                        }
                    })
                    .on_mouse_up(MouseButton::Left, move |_, window, cx| {
                        marquee_end(&ExplorerAction::EndMarquee, window, cx);
                    })
            })
            .when(
                view_settings.mode == explorer_model::ViewMode::Details && !has_size_map_plan,
                |element| {
                    element.child(
                        div()
                            .w_full()
                            .min_w(px(render_item_width))
                            .h(px(layout.details_header_height.value()))
                            .flex_none(),
                    )
                },
            )
            .when(presentation_empty, |element| {
                element.child(
                    div()
                        .flex_1()
                        .flex()
                        .items_center()
                        .justify_center()
                        .child(message),
                )
            })
            .when(drive_view && !presentation_empty, |element| {
                element.child(
                    div()
                        .id("this-pc-devices-and-drives")
                        .w_full()
                        .h(px(layout.minimum_hit_target.value()))
                        .flex_none()
                        .flex()
                        .items_center()
                        .px(px(layout.control_padding_horizontal.value()))
                        .gap(px(layout.content_spacing.value() / 2.0))
                        .text_color(colors.focus.to_gpui())
                        .child(chrome_icon(
                            "this-pc-devices-chevron",
                            ExplorerIcon::ChevronDown,
                            self.tokens,
                        ))
                        .child("裝置和磁碟機"),
                )
            })
            .when(leading_space > 0, |element| {
                element.child(div().w_full().h(px(leading_space as f32)).flex_none())
            })
            .children(entries.into_iter().map(move |item| {
                let view_settings = Arc::clone(&row_settings);
                let (visible_index, _snapshot_index, entry) = item;
                let code_lines_columns = row_code_lines_columns.clone();
                let row_column_registry = row_column_registry.clone();
                // Built-in File Count and Folder Count consume the same
                // host-owned directory facts even when the optional Folder
                // Size extension column is not registered. Keep those facts
                // available to the row and gate only the extension renderer.
                let folder_size_visuals = row_folder_size_visuals.clone();
                let visual_column_runtime = row_visual_column_runtime.clone().filter(|runtime| {
                    folder_size_visuals.as_ref().is_some_and(|visuals| {
                        row_column_registry.contains(&visuals.config.descriptor.id)
                            && visuals.config.descriptor.id == runtime.config().descriptor.id
                    })
                });
                let editor = rename_editor
                    .as_ref()
                    .filter(|editor| editor.item.id == entry.id)
                    .cloned();
                let cut_pending = matches!(
                    &clipboard,
                    explorer_model::ClipboardState::Owned {
                        mode: explorer_model::ClipboardMode::Cut,
                        items,
                        ..
                    } if items.iter().any(|item| item.id == entry.id)
                );
                let row_id = format!("shell-row-{:02x?}", entry.id.provider_bytes());
                let display_name = file_display_name(&entry, &view_settings);
                let selected = selection.contains(&entry.id) || editor.is_some();
                let context_item_id = entry.id.clone();
                let kind = if entry.is_container { "Folder" } else { "File" };
                let modified = entry.metadata.modified_display.clone().unwrap_or_default();
                let type_display = entry.metadata.type_display.clone().unwrap_or_else(|| {
                    if entry.is_container {
                        "檔案資料夾".to_owned()
                    } else {
                        "檔案".to_owned()
                    }
                });
                let size_bytes = crate::folder_size_column::builtin_size_bytes(
                    entry.is_container,
                    entry.metadata.size_bytes,
                    folder_size_visuals
                        .as_ref()
                        .and_then(|visuals| visuals.value_for(&entry.id)),
                );
                let size_display = size_bytes
                    .map(format_explorer_size)
                    .unwrap_or_default();
                let created = entry.metadata.created_display.clone().unwrap_or_default();
                let authors = entry.metadata.authors_display.clone().unwrap_or_default();
                let tags = entry.metadata.tags_display.clone().unwrap_or_default();
                let title = entry.metadata.title_display.clone().unwrap_or_default();
                let permissions = explorer_model::format_unix_mode(entry.metadata.unix_mode);
                // File Count and Folder Count are directory facts. Some shell
                // providers report a synthetic size for directories, which is
                // relevant to Folder Size but must not suppress count requests.
                let count_eligible = entry.is_container;
                let file_count = builtin_count_display(
                    count_eligible,
                    folder_size_visuals
                        .as_ref()
                        .and_then(|visuals| visuals.file_count_for(&entry.id)),
                );
                let folder_count = builtin_count_display(
                    count_eligible,
                    folder_size_visuals
                        .as_ref()
                        .and_then(|visuals| visuals.folder_count_for(&entry.id)),
                );
                let mut ordered_detail_cells = Vec::new();
                if view_settings.mode == explorer_model::ViewMode::Details && !drive_view {
                    for column_id in
                        visible_details_column_ids(&view_settings, &row_column_registry)
                    {
                        if column_id == explorer_model::ColumnId::Name {
                            continue;
                        }
                        let builtin_text = match &column_id {
                            explorer_model::ColumnId::DateModified => Some(modified.clone()),
                            explorer_model::ColumnId::Type => Some(type_display.clone()),
                            explorer_model::ColumnId::Size => Some(size_display.clone()),
                            explorer_model::ColumnId::DateCreated => Some(created.clone()),
                            explorer_model::ColumnId::Authors => Some(authors.clone()),
                            explorer_model::ColumnId::Tags => Some(tags.clone()),
                            explorer_model::ColumnId::Title => Some(title.clone()),
                            explorer_model::ColumnId::FileCount => Some(file_count.clone()),
                            explorer_model::ColumnId::FolderCount => Some(folder_count.clone()),
                            explorer_model::ColumnId::Permissions => Some(permissions.clone()),
                            _ => None,
                        };
                        if let Some(text) = builtin_text {
                            let cell_label = row_column_registry
                                .get(&column_id)
                                .map_or_else(|| column_id.stable_id(), |descriptor| {
                                    descriptor.display_name.clone()
                                });
                            let accessible_value = format!("{cell_label}: {text}");
                            ordered_detail_cells.push(
                                div()
                                    .id(details_column_selector(
                                        &format!("{row_id}-details-cell"),
                                        &column_id,
                                    ))
                                    .role(Role::Cell)
                                    .aria_label(accessible_value)
                                    .w(px(f32::from(
                                        view_settings.details_column_width(&column_id),
                                    )))
                                    .flex_none()
                                    .child(text)
                                    .into_any_element(),
                            );
                            continue;
                        }
                        if folder_size_visuals
                            .as_ref()
                            .is_some_and(|visuals| visuals.config.descriptor.id == column_id)
                        {
                            let is_file_system_directory =
                                crate::folder_size_column::applies_to_shell_entry(
                                    entry.is_container,
                                    entry.metadata.size_bytes,
                                );
                            if !is_file_system_directory {
                                ordered_detail_cells.push(
                                    div()
                                        .w(px(f32::from(
                                            view_settings.details_column_width(&column_id),
                                        )))
                                        .h_full()
                                        .flex_none()
                                        .into_any_element(),
                                );
                                continue;
                            }
                            ordered_detail_cells.push(match (
                                folder_size_visuals.clone(),
                                visual_column_runtime.clone(),
                            ) {
                                (Some(visuals), Some(runtime)) => folder_size_detail_cell(
                                    visuals,
                                    runtime,
                                    &entry.id,
                                    selected,
                                    shell_icon_dpi,
                                    visual_column_theme,
                                    cell_request_generation,
                                    &view_settings,
                                    visible_index,
                                    layout,
                                    colors,
                                ),
                                (Some(visuals), None) => unavailable_detail_cell(
                                    &visuals.config.descriptor,
                                    &view_settings,
                                    visible_index,
                                    layout,
                                ),
                                _ => unreachable!(),
                            });
                            continue;
                        }
                        if let Some(column) = code_lines_columns
                            .iter()
                            .find(|column| column.id() == &column_id)
                            .cloned()
                        {
                            ordered_detail_cells.push(code_lines_detail_column_cell(
                                column,
                                &entry.id,
                                selected,
                                shell_icon_dpi,
                                visual_column_theme,
                                cell_request_generation,
                                &view_settings,
                                &row_column_registry,
                                visible_index,
                                layout,
                                colors,
                            ));
                        } else if let Some(descriptor) = row_column_registry.get(&column_id) {
                            ordered_detail_cells.push(unavailable_detail_cell(
                                descriptor,
                                &view_settings,
                                visible_index,
                                layout,
                            ));
                        }
                    }
                }
                let drive = entry.metadata.drive.clone();
                let this_pc_type_display = if drive_view && drive.is_none() && entry.is_container {
                    "系統資料夾".to_owned()
                } else {
                    type_display.clone()
                };
                let drive_capacity_text = drive.as_ref().map(this_pc_drive_capacity_text);
                let drive_total_display = drive
                    .as_ref()
                    .and_then(|drive| drive.total_bytes)
                    .map(format_explorer_size)
                    .unwrap_or_default();
                let drive_free_display = drive
                    .as_ref()
                    .and_then(|drive| drive.available_bytes)
                    .map(format_explorer_size)
                    .unwrap_or_default();
                let drive_filesystem_display = drive
                    .as_ref()
                    .and_then(|drive| drive.filesystem_name.clone())
                    .unwrap_or_default();
                let item_width = spatial_metrics.cell_width;
                let item_height = spatial_metrics.cell_height;
                let icon_size = spatial_metrics.icon_size;
                let explicit_row_width = file_row_explicit_width(
                    view_settings.mode,
                    wrapped_view,
                    item_width,
                    render_item_width,
                    layout.control_padding_horizontal.value()
                        - layout.divider_width.value() / 2.0,
                );
                let can_accept_drop = entry.is_container;
                let row_drop_destination = entry.location.clone();
                let row_drop_cue = drop_target_row == Some(visible_index);
                let activate = on_action.clone();
                let row_pointer_scroll = row_pointer_scroll.clone();
                let accessibility_activate = on_action.clone();
                let row_region_id = format!("file-row-{visible_index}");
                let row_visual = file_row_visual(colors, selected, selection_active);
                let file_icon_key = crate::navigation_pane::file_icon_key_for_size(
                    &entry,
                    shell_icon_theme,
                    shell_icon_dpi,
                    crate::navigation_pane::view_icon_logical_size_for_settings(&view_settings),
                );
                let file_icon_is_thumbnail = thumbnail_icon_keys.contains(&file_icon_key);
                let file_icon = select_file_row_shell_icon(
                    shell_icons.get(&file_icon_key).cloned(),
                    generic_folder_icon.clone(),
                    entry.is_container,
                );
                div()
                    .id(row_id.clone())
                    .debug_selector(move || row_id.clone())
                    .role(Role::ListItem)
                    .aria_position_in_set(visible_index.saturating_add(1))
                    .aria_size_of_set(accessibility_set_size)
                    .aria_label(drive_capacity_text.as_ref().map_or_else(
                        || format!("{display_name} {kind}"),
                        |capacity| format!("{display_name}; {capacity}"),
                    ))
                    .aria_selected(selected)
                    .relative()
                    .when_some(explicit_row_width, |element, width| {
                        element.w(px(width)).min_w(px(width))
                    })
                    .when(explicit_row_width.is_none(), Styled::w_full)
                    .h(px(item_height))
                    .flex_none()
                    .flex()
                    .items_center()
                    .text_color(row_visual.text_color.to_gpui())
                    .when(drive_view, |element| {
                        element.gap(px(layout.content_spacing.value()))
                    })
                    .when(spatial_metrics.stacked, |element| {
                        element.flex_col().justify_center()
                    })
                    .px(px(layout.control_padding_horizontal.value()))
                    .when(
                        view_settings.mode == explorer_model::ViewMode::Content && !drive_view,
                        |element| {
                            element
                                .border_b(px(
                                    crate::layout::feature::CONTENT_ROW_DIVIDER_HEIGHT.value(),
                                ))
                                .border_color(colors.divider.to_gpui())
                        },
                    )
                    .when_some(row_visual.hover_fill, |element, hover_fill| {
                        element.hover(move |style| style.bg(hover_fill.to_gpui()))
                    })
                    .when_some(row_visual.selection_fill, |element, selection_fill| {
                        element.bg(selection_fill.to_gpui())
                    })
                    .when_some(row_visual.selection_border, |element, border| {
                        element
                            .border(px(f32::from(row_visual.selection_border_width)))
                            .border_color(border.to_gpui())
                    })
                    .when(row_drop_cue, |element| {
                        element
                            .border(px(layout.focus_stroke.value()))
                            .border_color(colors.accent.to_gpui())
                    })
                    .when(cut_pending, |element| element.opacity(0.55))
                    .when_some(activate, |element, callback| {
                        let move_callback = callback.clone();
                        let up_callback = callback.clone();
                        let out_callback = callback.clone();
                        let right_up_callback = callback.clone();
                        let right_out_callback = callback.clone();
                        let right_callback = callback.clone();
                        let right_up_item_id = context_item_id.clone();
                        let right_out_item_id = context_item_id.clone();
                        let drop_callback = callback.clone();
                        let drag_move_callback = callback.clone();
                        let can_drop_destination = row_drop_destination.clone();
                        let drop_destination = row_drop_destination.clone();
                        let move_destination = row_drop_destination.clone();
                        element
                            // File rows must remain transparent to external drops so the file-view
                            // background can accept them into the current directory. Registering a
                            // rejecting child drop target here makes a full Details viewport
                            // impossible to drop onto, which is especially visible in populated
                            // remote directories such as adb://.../Download.
                            .when(can_accept_drop, |element| {
                                element
                                    .can_drop(move |value, _, _| {
                                        value
                                            .downcast_ref::<gpui::ExternalPaths>()
                                            .is_some_and(|paths| {
                                                negotiate_external_paths(
                                                    paths,
                                                    true,
                                                    Some(&can_drop_destination),
                                                ) != explorer_model::DragEffect::None
                                            })
                                    })
                                    .on_drop(move |paths: &gpui::ExternalPaths, window, cx| {
                                        // A folder row owns the drop. Letting this bubble to the
                                        // background can change a valid child-folder effect.
                                        cx.stop_propagation();
                                        let effect = negotiate_external_paths(
                                            paths,
                                            true,
                                            Some(&drop_destination),
                                        );
                                        drop_callback(
                                            &ExplorerAction::DropExternal {
                                                paths: paths.paths().to_vec(),
                                                destination_row: Some(visible_index),
                                                effect,
                                                right_button: paths.drop_metadata().right_button,
                                                allowed: external_transfer_effects(paths),
                                            },
                                            window,
                                            cx,
                                        );
                                    })
                                    .on_drag_move::<gpui::ExternalPaths>(
                                        move |event, window, cx| {
                                            cx.stop_propagation();
                                            let effect = negotiate_external_paths(
                                                event.drag(cx),
                                                true,
                                                Some(&move_destination),
                                            );
                                            drag_move_callback(
                                                &ExplorerAction::UpdateExternalDrag {
                                                    destination_row: Some(visible_index),
                                                    target: explorer_model::DropTargetKind::FolderItem,
                                                    pointer_y: f32::from(event.event.position.y),
                                                    top: f32::from(event.bounds.top()),
                                                    bottom: f32::from(event.bounds.bottom()),
                                                    effect,
                                                },
                                                window,
                                                cx,
                                            );
                                        },
                                    )
                            })
                            .on_mouse_down(MouseButton::Left, move |event, window, cx| {
                                cx.stop_propagation();
                                let actual_origin = row_pointer_scroll.as_ref().map(|handle| {
                                    let bounds = handle.bounds();
                                    (f32::from(bounds.left()), f32::from(bounds.top()))
                                });
                                let (local_x, local_y) = file_view_local_pointer(
                                    f32::from(event.position.x),
                                    f32::from(event.position.y),
                                    actual_origin,
                                    (file_origin_x, file_origin_y),
                                );
                                let horizontal_scroll = row_pointer_scroll
                                    .as_ref()
                                    .map_or(0.0, |handle| -f32::from(handle.offset().x));
                                if details_mode
                                    && !details_name_column_contains(
                                        local_x,
                                        horizontal_scroll,
                                        layout.control_padding_horizontal.value(),
                                        f32::from(details_name_width),
                                    )
                                {
                                    callback(
                                        &ExplorerAction::BeginMarquee {
                                            x: local_x,
                                            y: local_y,
                                            additive: event.modifiers.control,
                                        },
                                        window,
                                        cx,
                                    );
                                    return;
                                }
                                if event.click_count == 2 {
                                    callback(
                                        &ExplorerAction::OpenItem {
                                            row_index: visible_index,
                                            new_tab: event.modifiers.control,
                                        },
                                        window,
                                        cx,
                                    );
                                } else {
                                    callback(
                                        &if event.modifiers.shift {
                                            ExplorerAction::SelectRange {
                                                row_index: visible_index,
                                                additive: event.modifiers.control,
                                            }
                                        } else if event.modifiers.control {
                                            ExplorerAction::SelectAdditionalItem {
                                                row_index: visible_index,
                                            }
                                        } else {
                                            ExplorerAction::SelectItem {
                                                row_index: visible_index,
                                            }
                                        },
                                        window,
                                        cx,
                                    );
                                    callback(
                                        &ExplorerAction::BeginFileDrag {
                                            x: f32::from(event.position.x),
                                            y: f32::from(event.position.y),
                                            button: explorer_model::DragButton::Left,
                                        },
                                        window,
                                        cx,
                                    );
                                }
                            })
                            .on_mouse_down(MouseButton::Right, move |event, window, cx| {
                                // The row owns the complete right-button gesture. Letting this
                                // bubble reaches the file-view background handler, which replaces
                                // the item target with a background target and therefore asks the
                                // Shell for the much shorter folder-background menu.
                                cx.stop_propagation();
                                // Explorer selects an unselected item on right-button down before
                                // it establishes the right-drag candidate. The candidate is also
                                // the pointer-session proof required before mouse-up may open an
                                // item Shell menu. Keep an existing multi-selection intact when
                                // the pressed row already belongs to it.
                                right_callback(
                                    &ExplorerAction::BeginContextItemGesture {
                                        item_id: context_item_id.clone(),
                                        x: f32::from(event.position.x),
                                        y: f32::from(event.position.y),
                                        extended_verbs: event.modifiers.shift,
                                    },
                                    window,
                                    cx,
                                );
                            })
                            .on_mouse_move(move |event, window, cx| {
                                move_callback(
                                    &ExplorerAction::UpdateFileDrag {
                                        x: f32::from(event.position.x),
                                        y: f32::from(event.position.y),
                                    },
                                    window,
                                    cx,
                                );
                            })
                            .on_mouse_up(MouseButton::Left, move |_, window, cx| {
                                up_callback(&ExplorerAction::CancelFileDrag, window, cx);
                            })
                            .on_mouse_up_out(MouseButton::Left, move |_, window, cx| {
                                out_callback(&ExplorerAction::CancelFileDrag, window, cx);
                            })
                            .on_mouse_up(MouseButton::Right, move |event, window, cx| {
                                // Stop before dispatch so the parent cannot race this item request
                                // with a second background-target ShowContextMenu request.
                                cx.stop_propagation();
                                let (owner_window, x, y) =
                                    context_menu_coordinates(event.position, window);
                                right_up_callback(
                                    &ExplorerAction::ShowContextMenu {
                                        item_id: Some(right_up_item_id.clone()),
                                        owner_window,
                                        x,
                                        y,
                                        client_x: f32::from(event.position.x),
                                        client_y: f32::from(event.position.y),
                                        keyboard_invoked: false,
                                        extended_verbs: event.modifiers.shift,
                                    },
                                    window,
                                    cx,
                                );
                                right_up_callback(&ExplorerAction::CancelFileDrag, window, cx);
                            })
                            .on_mouse_up_out(MouseButton::Right, move |event, window, cx| {
                                // Selecting an unselected row can replace the rendered element
                                // between right-button down and up. GPUI then reports the release
                                // as mouse-up-out even when the pointer never left the row. Complete
                                // the same item-menu gesture here; begin_context_menu_request rejects
                                // a real right-drag once its session has advanced past Candidate.
                                cx.stop_propagation();
                                let (owner_window, x, y) =
                                    context_menu_coordinates(event.position, window);
                                right_out_callback(
                                    &ExplorerAction::ShowContextMenu {
                                        item_id: Some(right_out_item_id.clone()),
                                        owner_window,
                                        x,
                                        y,
                                        client_x: f32::from(event.position.x),
                                        client_y: f32::from(event.position.y),
                                        keyboard_invoked: false,
                                        extended_verbs: event.modifiers.shift,
                                    },
                                    window,
                                    cx,
                                );
                                right_out_callback(&ExplorerAction::CancelFileDrag, window, cx);
                            })
                    })
                    .when_some(accessibility_activate, |element, callback| {
                        let focus_callback = callback.clone();
                        element
                            .on_a11y_action(AccessibleAction::Focus, move |_, window, cx| {
                                // An overscanned row can exist in the accessibility tree while
                                // remaining outside the viewport. Selection routes through the
                                // root scroll handle, revealing it before focus is transferred.
                                focus_callback(
                                    &ExplorerAction::SelectItem {
                                        row_index: visible_index,
                                    },
                                    window,
                                    cx,
                                );
                            })
                            .on_a11y_action(AccessibleAction::Click, move |_, window, cx| {
                                // A list-item Invoke maps to Explorer's single-click selection;
                                // activation remains the explicit double-click/Enter contract.
                                // Selection also reveals overscanned offscreen targets.
                                callback(
                                    &ExplorerAction::SelectItem {
                                        row_index: visible_index,
                                    },
                                    window,
                                    cx,
                                );
                            })
                    })
                    .child(region_probe(
                        row_region_id,
                        Some(FILE_VIEW_HOST_ID),
                        if selected { "selected" } else { "normal" },
                    ))
                    .child(typography_probe(
                        format!("file-row-{visible_index}"),
                        typography_diagnostic(self.tokens, self.tokens.typography.file_row),
                    ))
                    .child(
                        div()
                            .w(px(
                                if view_settings.mode == explorer_model::ViewMode::Details {
                                    f32::from(
                                        view_settings.details_column_width(
                                            &explorer_model::ColumnId::Name,
                                        ),
                                    )
                                } else {
                                    item_width - layout.control_padding_horizontal.value() * 2.0
                                },
                            ))
                            .flex_none()
                            .flex()
                            .items_center()
                            .when(spatial_metrics.stacked, |element| {
                                element
                                    .h_full()
                                    .flex_col()
                                    .justify_center()
                                    .text_center()
                            })
                            .gap(px(if spatial_metrics.stacked {
                                crate::layout::feature::STACKED_ICON_LABEL_GAP.value()
                            } else {
                                layout.content_spacing.value()
                            }))
                            .when(view_settings.item_check_boxes, |element| {
                                element.child(
                                    div()
                                        .id(format!("file-row-checkbox-{visible_index}"))
                                        .role(Role::CheckBox)
                                        .aria_label(format!("Select {}", entry.display_name))
                                        .w(px(layout.navigation_icon_size.value()))
                                        .h(px(layout.navigation_icon_size.value()))
                                        .flex_none()
                                        .flex()
                                        .items_center()
                                        .justify_center()
                                        .rounded(px(layout.focus_stroke.value()))
                                        .border(px(layout.focus_stroke.value()))
                                        .border_color(if selected {
                                            colors.focus.to_gpui()
                                        } else {
                                            colors.text_secondary.to_gpui()
                                        })
                                        .when(selected, |checkbox| {
                                            checkbox
                                                .bg(colors.focus.to_gpui())
                                                .text_color(colors.surface.to_gpui())
                                                .child("✓")
                                        }),
                                )
                            })
                            .child(match file_icon {
                                Some(texture) => {
                                    let source_size = texture.size(0);
                                    let (host_width, host_height) = file_visual_host_size(
                                        file_icon_is_thumbnail,
                                        spatial_metrics.stacked,
                                        item_width,
                                        icon_size,
                                    );
                                    let (render_width, render_height) = aspect_fit_size(
                                        u32::from(source_size.width),
                                        u32::from(source_size.height),
                                        host_width,
                                        host_height,
                                    );
                                    div()
                                        .id(format!("file-row-icon-{visible_index}"))
                                        .role(Role::Image)
                                        .aria_label(format!("{} icon", entry.display_name))
                                        .w(px(host_width))
                                        .h(px(host_height))
                                        .flex_none()
                                        .flex()
                                        .items_center()
                                        .justify_center()
                                        .overflow_hidden()
                                        .child(
                                            img(texture)
                                                .w(px(render_width))
                                                .h(px(render_height))
                                                .object_fit(ObjectFit::Contain),
                                        )
                                        .into_any_element()
                                }
                                None => {
                                    let remote_kind =
                                        crate::remote_file_fallback_icon_kind(&entry);
                                    div()
                                        .id(format!("file-row-icon-{visible_index}"))
                                        .role(Role::Image)
                                        .aria_label(format!("{} icon", entry.display_name))
                                        .w(px(icon_size))
                                        .h(px(icon_size))
                                        .flex_none()
                                        .flex()
                                        .items_center()
                                        .justify_center()
                                        .child(if entry.is_container {
                                            navigation_icon(
                                                crate::navigation_pane::NavigationIcon::Folder,
                                                self.tokens,
                                            )
                                            .into_any_element()
                                        } else if let Some(kind) = remote_kind {
                                            crate::icons::remote_file_icon(
                                                kind,
                                                icon_size,
                                                self.tokens,
                                            )
                                            .into_any_element()
                                        } else {
                                            navigation_icon(
                                                crate::navigation_pane::NavigationIcon::Documents,
                                                self.tokens,
                                            )
                                            .into_any_element()
                                        })
                                        .into_any_element()
                                }
                            })
                            .child(if let Some(editor) = editor {
                                let input = rename_input.clone();
                                let rename_height = layout.inline_rename_height.value();
                                let rename_metrics = editable_selection_metrics(
                                    rename_height,
                                    1.0,
                                    self.tokens.typography.file_row.line_height.value(),
                                    layout.focus_stroke.value() / 2.0,
                                );
                                div()
                                    .id("inline-rename-editor-container")
                                    .on_mouse_down(MouseButton::Left, |_, _, cx| {
                                        cx.stop_propagation();
                                    })
                                    .on_mouse_up(MouseButton::Left, |_, _, cx| {
                                        cx.stop_propagation();
                                    })
                                    .on_mouse_up_out(MouseButton::Left, |_, _, cx| {
                                        cx.stop_propagation();
                                    })
                                    .relative()
                                    .role(Role::TextInput)
                                    .aria_label(format!("Rename {}", editor.buffer))
                                    .when_some(
                                        rename_input_focus.clone(),
                                        |element, focus_handle| element.track_focus(&focus_handle),
                                    )
                                    .h(px(rename_height))
                                    .flex()
                                    .items_center()
                                    .child(if let Some(input) = input {
                                        text_input("inline-rename-editor")
                                            .state(input)
                                            .multiline(false)
                                            .caret_blink_interval_500ms()
                                            .w_full()
                                            .h(px(rename_metrics.line_height))
                                            .px(px(layout.focus_stroke.value() * 2.0))
                                            .py(px(rename_metrics.vertical_padding))
                                            .text_size(px(self
                                                .tokens
                                                .typography
                                                .file_row
                                                .size
                                                .value()))
                                            .line_height(px(rename_metrics.line_height))
                                            .bg(colors.control_fill.to_gpui())
                                            .text_color(rename_text)
                                            .selection_color(rename_selection.into())
                                            .selection_text_color(rename_selection_text.into())
                                            .caret_color(rename_caret.into())
                                            .border(px(1.0))
                                            .border_color(colors.focus.to_gpui())
                                            .into_any_element()
                                    } else {
                                        div().child(editor.buffer.clone()).into_any_element()
                                    })
                                    .when_some(editor.error, |element, error| {
                                        element.child(
                                            div()
                                                .absolute()
                                                .top(px(rename_height))
                                                .text_color(colors.danger.to_gpui())
                                                .child(error.user_message),
                                        )
                                    })
                                    .into_any_element()
                            } else if drive_view {
                                match view_settings.mode {
                                    explorer_model::ViewMode::Details => div()
                                        .id("this-pc-details-name")
                                        .w(px(
                                            crate::layout::feature::THIS_PC_DETAILS_NAME_WIDTH
                                                .value()
                                                - icon_size
                                                - layout.content_spacing.value(),
                                        ))
                                        .flex_none()
                                        .overflow_hidden()
                                        .whitespace_nowrap()
                                        .text_ellipsis()
                                        .text_color(colors.text_primary.to_gpui())
                                        .child(display_name)
                                        .into_any_element(),
                                    explorer_model::ViewMode::Content => {
                                        let left = div()
                                            .min_w(px(0.0))
                                            .flex_1()
                                            .flex()
                                            .flex_col()
                                            .gap(px(layout.focus_stroke.value() * 2.0))
                                            .child(
                                                div()
                                                    .overflow_hidden()
                                                    .whitespace_nowrap()
                                                    .text_ellipsis()
                                                    .text_color(colors.text_primary.to_gpui())
                                                    .child(display_name),
                                            );
                                        let left = if let Some(drive) = drive.as_ref() {
                                            left.child(this_pc_capacity_bar(
                                                self.tokens,
                                                drive,
                                                Some(
                                                    crate::layout::feature::THIS_PC_CONTENT_BAR_WIDTH
                                                        .value(),
                                                ),
                                            ))
                                        } else {
                                            left
                                        };
                                        div()
                                            .id("this-pc-content-status")
                                            .min_w(px(0.0))
                                            .flex_1()
                                            .flex()
                                            .items_center()
                                            .gap(px(layout.content_spacing.value() * 2.0))
                                            .child(left)
                                            .child(
                                                div()
                                                    .w(px(
                                                        crate::layout::feature::THIS_PC_CONTENT_TRAILING_WIDTH
                                                            .value(),
                                                    ))
                                                    .flex_none()
                                                    .flex()
                                                    .flex_col()
                                                    .child(drive_filesystem_display.clone())
                                                    .child(
                                                        drive_capacity_text
                                                            .clone()
                                                            .unwrap_or_default(),
                                                    ),
                                            )
                                            .into_any_element()
                                    }
                                    _ => {
                                        let status = div()
                                            .id("this-pc-drive-status")
                                            .w(px(
                                                crate::layout::feature::THIS_PC_DRIVE_STATUS_WIDTH
                                                    .value(),
                                            ))
                                            .flex_none()
                                            .flex()
                                            .flex_col()
                                            .gap(px(layout.focus_stroke.value() * 2.0))
                                            .child(
                                                div()
                                                    .overflow_hidden()
                                                    .whitespace_nowrap()
                                                    .text_ellipsis()
                                                    .text_color(colors.text_primary.to_gpui())
                                                    .child(display_name),
                                            );
                                        let status = if let Some(drive) = drive.as_ref() {
                                            status.child(this_pc_capacity_bar(
                                                self.tokens,
                                                drive,
                                                Some(
                                                    crate::layout::feature::THIS_PC_CAPACITY_BAR_WIDTH
                                                        .value(),
                                                ),
                                            ))
                                        } else {
                                            status
                                        };
                                        status
                                            .child(
                                                div()
                                                    .overflow_hidden()
                                                    .whitespace_nowrap()
                                                    .text_ellipsis()
                                                    .child(
                                                        drive_capacity_text
                                                            .clone()
                                                            .unwrap_or_default(),
                                                    ),
                                            )
                                            .into_any_element()
                                    }
                                }
                            } else if view_settings.mode == explorer_model::ViewMode::Content {
                                div()
                                    .id("file-row-name")
                                    .min_w(px(0.0))
                                    .flex_1()
                                    .flex()
                                    .flex_col()
                                    .justify_center()
                                    .overflow_hidden()
                                    .child(
                                        div()
                                            .whitespace_nowrap()
                                            .text_ellipsis()
                                            .text_color(colors.text_primary.to_gpui())
                                            .child(display_name),
                                    )
                                    .when(!entry.is_container, |element| {
                                        element.child(
                                            div()
                                                .whitespace_nowrap()
                                                .text_ellipsis()
                                                .text_color(colors.text_secondary.to_gpui())
                                                .child(format!("類型: {type_display}")),
                                        )
                                    })
                                    .into_any_element()
                            } else {
                                div()
                                    .id("file-row-name")
                                    .w_full()
                                    .max_w_full()
                                    .min_w(px(0.0))
                                    .overflow_hidden()
                                    .when(spatial_metrics.stacked, |name| {
                                        name
                                            .h(px(crate::layout::feature::STACKED_ICON_LABEL_HEIGHT.value()))
                                            .flex_none()
                                            .whitespace_normal()
                                            .text_ellipsis()
                                            .line_clamp(stacked_icon_label_lines(selected))
                                    })
                                    .when(!spatial_metrics.stacked, |name| {
                                        name.whitespace_nowrap().text_ellipsis()
                                    })
                                    .child(display_name)
                                    .into_any_element()
                            }),
                    )
                    .when(
                        view_settings.mode == explorer_model::ViewMode::Details && !drive_view,
                        |element| element.children(ordered_detail_cells),
                    )
                    .when(
                        view_settings.mode == explorer_model::ViewMode::Details && drive_view,
                        |element| {
                            element
                                .child(
                                    div()
                                        .id(format!("this-pc-drive-type-{visible_index}"))
                                        .role(Role::Status)
                                        .aria_label(format!("磁碟類型：{this_pc_type_display}"))
                                        .w(px(
                                            crate::layout::feature::THIS_PC_DETAILS_TYPE_WIDTH
                                                .value(),
                                        ))
                                        .flex_none()
                                        .child(this_pc_type_display),
                                )
                                .child(
                                    div()
                                        .id(format!("this-pc-drive-total-{visible_index}"))
                                        .role(Role::Status)
                                        .aria_label(format!("大小總計：{drive_total_display}"))
                                        .w(px(
                                            crate::layout::feature::THIS_PC_DETAILS_TOTAL_WIDTH
                                                .value(),
                                        ))
                                        .flex_none()
                                        .child(drive_total_display),
                                )
                                .child(
                                    div()
                                        .id(format!("this-pc-drive-free-{visible_index}"))
                                        .role(Role::Status)
                                        .aria_label(
                                            drive_capacity_text.clone().unwrap_or_default(),
                                        )
                                        .w(px(
                                            crate::layout::feature::THIS_PC_DETAILS_FREE_WIDTH
                                                .value(),
                                        ))
                                        .flex_none()
                                        .child(drive_free_display),
                                )
                        },
                    )
                    .when(
                        view_settings.mode == explorer_model::ViewMode::Content && !drive_view,
                        |element| {
                            element.child(
                                div()
                                    .flex_1()
                                    .flex()
                                    .flex_col()
                                    .justify_center()
                                    .text_color(colors.text_secondary.to_gpui())
                                    .child(format!("修改日期: {modified}"))
                                    .when(!entry.is_container && !size_display.is_empty(), |row| {
                                        row.child(format!("大小: {size_display}"))
                                    }),
                            )
                        },
                    )
            }))
            .when(trailing_space > 0, |element| {
                element.child(div().w_full().h(px(trailing_space as f32)).flex_none())
            })
            .when_some(marquee, |element, marquee| {
                let rect = marquee_content_rect(
                    marquee.origin_x,
                    marquee.origin_y,
                    marquee.current_x,
                    marquee.current_y,
                    horizontal_scroll_offset,
                    scroll_offset,
                );
                element.child(
                    div()
                        .id("file-selection-marquee")
                        .absolute()
                        .left(px(rect.left))
                        .top(px(rect.top))
                        .w(px(rect.width))
                        .h(px(rect.height))
                        .border(px(1.0))
                        .border_color(colors.focus.to_gpui())
                        .bg(crate::theme::Rgba8 {
                            alpha: 72,
                            ..colors.selected_active
                        }
                        .to_gpui()),
                )
            })
            .when(
                view_settings.mode == explorer_model::ViewMode::Details && !has_size_map_plan,
                |element| {
                    element.child(
                        div()
                            .w(px(render_item_width))
                            .min_w(px(render_item_width))
                            .h(px(layout.content_spacing.value() * 1.5))
                            .flex_none(),
                    )
                },
            );
        let rendered = div()
            .relative()
            .flex_1()
            .h_full()
            .min_h_0()
            .flex()
            .flex_col()
            .overflow_hidden()
            .when_some(background_menu_action, |element, menu_callback| {
                element.on_mouse_down(MouseButton::Right, move |event, window, cx| {
                    if !file_view_background_context_hit(
                        f32::from(event.position.x),
                        f32::from(event.position.y),
                        file_origin_x,
                        file_origin_y,
                        viewport_width,
                        background_menu_viewport_height,
                        background_menu_header_height,
                    ) {
                        return;
                    }
                    // Row right-down handlers stop propagation first. Reaching this viewport owner
                    // therefore means the press belongs to the directory background. Dispatch on
                    // down because the scroll gesture can capture and consume the matching release.
                    cx.stop_propagation();
                    let (owner_window, x, y) = context_menu_coordinates(event.position, window);
                    menu_callback(
                        &ExplorerAction::ShowContextMenu {
                            item_id: None,
                            owner_window,
                            x,
                            y,
                            client_x: f32::from(event.position.x),
                            client_y: f32::from(event.position.y),
                            keyboard_invoked: false,
                            extended_verbs: event.modifiers.shift,
                        },
                        window,
                        cx,
                    );
                })
            })
            .child(scroll_content)
            .when_some(size_map_plan, |element, (plan, indexes, selected)| {
                element.child(size_map_surface(
                    self.tokens,
                    plan,
                    indexes,
                    selected,
                    size_map_action.clone(),
                ))
            })
            .when(
                view_settings.mode == explorer_model::ViewMode::Details && !has_size_map_plan,
                |element| {
                    element.child(
                        div()
                            .absolute()
                            .top(px(fixed_header_top))
                            .left(px(fixed_header_left))
                            .w_full()
                            .min_w(px(render_item_width))
                            .bg(colors.surface.to_gpui())
                            // This fixed overlay is painted after the scroll surface and must
                            // also win hit testing; otherwise rows beneath a visually pinned
                            // splitter receive the press and start file selection/dragging.
                            .occlude()
                            .child(details_header(
                                self.tokens,
                                view_settings.clone(),
                                &column_registry,
                                drive_view,
                                details_filter_menu.clone(),
                                &details_filters,
                                &details_filter_options,
                                header_action,
                            )),
                    )
                },
            );
        let rendered = rendered.when_some(
            details_filter_menu.clone().zip(filter_menu_dismiss),
            |element, (_, callback)| {
                let right_callback = callback.clone();
                element.child(
                    div()
                        .id("details-filter-menu-dismiss-layer")
                        .absolute()
                        .inset_0()
                        .occlude()
                        .on_mouse_down(MouseButton::Left, move |_, window, cx| {
                            callback(&ExplorerAction::CloseDetailsFilterMenu, window, cx);
                            cx.stop_propagation();
                        })
                        .on_mouse_down(MouseButton::Right, move |_, window, cx| {
                            right_callback(&ExplorerAction::CloseDetailsFilterMenu, window, cx);
                            cx.stop_propagation();
                        }),
                )
            },
        );
        let rendered = rendered.when_some(column_menu, |element, target| {
            element
                .when_some(column_menu_dismiss, |element, callback| {
                    let right_callback = callback.clone();
                    element.child(
                        div()
                            .id("details-column-menu-dismiss-layer")
                            .absolute()
                            .inset_0()
                            .occlude()
                            .on_mouse_down(MouseButton::Left, move |_, window, cx| {
                                callback(&ExplorerAction::CloseDetailsColumnMenu, window, cx);
                                cx.stop_propagation();
                            })
                            .on_mouse_down(MouseButton::Right, move |_, window, cx| {
                                right_callback(&ExplorerAction::CloseDetailsColumnMenu, window, cx);
                                cx.stop_propagation();
                            }),
                    )
                })
                .child(details_column_menu(
                    self.tokens,
                    target,
                    view_settings,
                    &column_registry,
                    folder_size_visuals,
                    code_lines_visuals,
                    column_menu_action,
                ))
        });
        if let Some(performance) = performance.as_ref() {
            performance.record_render(render_started.elapsed());
        }
        rendered
    }
}

fn file_view_background_context_hit(
    x: f32,
    y: f32,
    origin_x: f32,
    origin_y: f32,
    viewport_width: f32,
    viewport_height: f32,
    header_height: f32,
) -> bool {
    let content_top = origin_y + header_height.max(0.0);
    x >= origin_x
        && x <= origin_x + viewport_width.max(0.0)
        && y >= content_top
        && y <= origin_y + viewport_height.max(0.0)
}

fn format_explorer_size(bytes: u64) -> String {
    crate::format_file_size(bytes)
}

fn builtin_count_display(eligible_container: bool, value: Option<u64>) -> String {
    if eligible_container {
        value.map_or_else(|| "—".to_owned(), |value| value.to_string())
    } else {
        String::new()
    }
}

fn file_display_name(
    entry: &explorer_model::FileEntry,
    settings: &explorer_model::ViewSettings,
) -> String {
    if settings.file_name_extensions || entry.is_container {
        return entry.display_name.clone();
    }
    entry
        .location
        .path()
        .and_then(std::path::Path::file_stem)
        .and_then(std::ffi::OsStr::to_str)
        .filter(|name| !name.is_empty())
        .map_or_else(|| entry.display_name.clone(), str::to_owned)
}

const fn is_stacked_icon_view(mode: explorer_model::ViewMode) -> bool {
    matches!(
        mode,
        explorer_model::ViewMode::ExtraLargeIcons
            | explorer_model::ViewMode::LargeIcons
            | explorer_model::ViewMode::MediumIcons
    )
}

/// Returns the largest aspect-preserving image size that fits inside a bounded icon host.
/// Invalid source dimensions deliberately collapse to zero instead of escaping the host.
#[allow(
    clippy::cast_precision_loss,
    reason = "thumbnail pixel dimensions are reduced to bounded logical layout geometry"
)]
fn aspect_fit_size(
    source_width: u32,
    source_height: u32,
    host_width: f32,
    host_height: f32,
) -> (f32, f32) {
    if source_width == 0
        || source_height == 0
        || !host_width.is_finite()
        || !host_height.is_finite()
        || host_width <= 0.0
        || host_height <= 0.0
    {
        return (0.0, 0.0);
    }
    let scale = (host_width / source_width as f32).min(host_height / source_height as f32);
    (source_width as f32 * scale, source_height as f32 * scale)
}

fn file_visual_host_size(
    is_thumbnail: bool,
    stacked: bool,
    cell_width: f32,
    icon_size: f32,
) -> (f32, f32) {
    let icon_size = if icon_size.is_finite() {
        icon_size.max(0.0)
    } else {
        0.0
    };
    if !is_thumbnail || !stacked {
        return (icon_size, icon_size);
    }
    let cell_width = if cell_width.is_finite() {
        cell_width.max(0.0)
    } else {
        0.0
    };
    (cell_width, icon_size)
}

fn select_file_row_shell_icon<T>(
    specific: Option<T>,
    generic_folder: Option<T>,
    is_container: bool,
) -> Option<T> {
    specific.or_else(|| is_container.then_some(generic_folder).flatten())
}

const fn stacked_icon_label_lines(selected: bool) -> usize {
    if selected { 3 } else { 2 }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct SpatialGridMetrics {
    pub cell_width: f32,
    pub cell_height: f32,
    pub icon_size: f32,
    pub wrapped: bool,
    pub stacked: bool,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct SpatialGridLayout {
    pub metrics: SpatialGridMetrics,
    pub columns: usize,
}

const SPATIAL_CELL_WIDTH_TOLERANCE: f32 = 0.10;

pub(crate) fn spatial_grid_metrics(
    settings: &explorer_model::ViewSettings,
    layout: crate::layout::LayoutTokens,
) -> SpatialGridMetrics {
    spatial_grid_metrics_with_registry(
        settings,
        &explorer_model::ColumnRegistry::built_ins(),
        layout,
    )
}

pub(crate) fn spatial_grid_metrics_with_registry(
    settings: &explorer_model::ViewSettings,
    registry: &explorer_model::ColumnRegistry,
    layout: crate::layout::LayoutTokens,
) -> SpatialGridMetrics {
    let base_height = view_item_height(settings, layout);
    let cell_height = if settings.compact_view {
        base_height - layout.content_spacing.value()
    } else {
        base_height
    };
    SpatialGridMetrics {
        cell_width: view_item_width_with_registry(settings, registry),
        cell_height,
        icon_size: view_icon_size(settings, layout),
        wrapped: matches!(
            settings.mode,
            explorer_model::ViewMode::ExtraLargeIcons
                | explorer_model::ViewMode::LargeIcons
                | explorer_model::ViewMode::MediumIcons
                | explorer_model::ViewMode::SmallIcons
                | explorer_model::ViewMode::Tiles
        ),
        stacked: is_stacked_icon_view(settings.mode),
    }
}

pub(crate) fn this_pc_spatial_grid_metrics(
    mode: explorer_model::ViewMode,
    layout: crate::layout::LayoutTokens,
) -> SpatialGridMetrics {
    match mode {
        explorer_model::ViewMode::Details => SpatialGridMetrics {
            cell_width: this_pc_details_width(),
            cell_height: layout.file_row_height.value(),
            icon_size: layout.navigation_icon_size.value(),
            wrapped: false,
            stacked: false,
        },
        explorer_model::ViewMode::Content => SpatialGridMetrics {
            cell_width: this_pc_details_width(),
            cell_height: crate::layout::feature::THIS_PC_CONTENT_HEIGHT.value(),
            icon_size: crate::layout::feature::THIS_PC_TILE_ICON_SIZE.value(),
            wrapped: false,
            stacked: false,
        },
        _ => SpatialGridMetrics {
            cell_width: crate::layout::feature::THIS_PC_TILE_WIDTH.value(),
            cell_height: crate::layout::feature::THIS_PC_TILE_HEIGHT.value(),
            icon_size: crate::layout::feature::THIS_PC_TILE_ICON_SIZE.value(),
            wrapped: true,
            stacked: false,
        },
    }
}

pub(crate) const fn this_pc_details_width() -> f32 {
    crate::layout::feature::THIS_PC_DETAILS_NAME_WIDTH.value()
        + crate::layout::feature::THIS_PC_DETAILS_TYPE_WIDTH.value()
        + crate::layout::feature::THIS_PC_DETAILS_TOTAL_WIDTH.value()
        + crate::layout::feature::THIS_PC_DETAILS_FREE_WIDTH.value()
}

fn this_pc_drive_capacity_text(drive: &explorer_model::DriveMetadata) -> String {
    match (drive.available_bytes, drive.total_bytes, drive.availability) {
        (Some(available), Some(total), explorer_model::DriveAvailability::Available) => format!(
            "剩餘 {}，共 {}",
            format_explorer_size(available),
            format_explorer_size(total)
        ),
        (_, _, explorer_model::DriveAvailability::NoMedia) => "沒有媒體".to_owned(),
        (_, _, explorer_model::DriveAvailability::Disconnected) => "已中斷連線".to_owned(),
        (_, _, explorer_model::DriveAvailability::AccessDenied) => "拒絕存取".to_owned(),
        _ => "無法取得容量".to_owned(),
    }
}

fn this_pc_capacity_bar(
    tokens: UiTokens,
    drive: &explorer_model::DriveMetadata,
    width: Option<f32>,
) -> gpui::AnyElement {
    let colors = tokens.theme.colors;
    let width = width.unwrap_or(crate::layout::feature::THIS_PC_CAPACITY_BAR_WIDTH.value());
    let used_width = drive.used_fraction().unwrap_or(0.0).clamp(0.0, 1.0) * width;
    let bar_color = if drive.is_low_space() {
        colors.danger.to_gpui()
    } else {
        colors.accent.to_gpui()
    };
    div()
        .id("this-pc-capacity-bar")
        .role(Role::Status)
        .aria_label(this_pc_drive_capacity_text(drive))
        .w(px(width))
        .h(px(
            crate::layout::feature::THIS_PC_CAPACITY_BAR_HEIGHT.value()
        ))
        .border(px(1.0))
        .border_color(colors.divider.to_gpui())
        .bg(colors.control_fill.to_gpui())
        .child(div().h_full().w(px(used_width)).bg(bar_color))
        .into_any_element()
}

#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "finite positive cell geometry is floored and clamped to the item count"
)]
pub(crate) fn spatial_grid_columns(
    metrics: SpatialGridMetrics,
    viewport_width: f32,
    item_count: usize,
) -> usize {
    spatial_grid_layout(metrics, viewport_width, item_count).columns
}

/// Fits a complete Explorer-style icon row into the usable file viewport.
///
/// The base cell width remains the visual profile, but a full row may contract or expand by ten
/// percent so its final edge lands before the overlay scrollbar. When the viewport falls between
/// two feasible tolerance bands, the nearest column count wins and still consumes the exact usable
/// width. Incomplete rows retain their base width instead of stretching a handful of items across
/// the whole folder.
#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    clippy::cast_sign_loss,
    reason = "finite positive column candidates are bounded by the visible UI collection"
)]
pub(crate) fn spatial_grid_layout(
    mut metrics: SpatialGridMetrics,
    viewport_width: f32,
    item_count: usize,
) -> SpatialGridLayout {
    if !metrics.wrapped {
        return SpatialGridLayout {
            metrics,
            columns: 1,
        };
    }

    let available = viewport_width.max(0.0);
    let base_width = metrics.cell_width.max(1.0);
    let maximum_columns = item_count.max(1);
    if available <= 0.0 {
        metrics.cell_width = 1.0;
        return SpatialGridLayout {
            metrics,
            columns: 1,
        };
    }

    let minimum_width = base_width * (1.0 - SPATIAL_CELL_WIDTH_TOLERANCE);
    let maximum_width = base_width * (1.0 + SPATIAL_CELL_WIDTH_TOLERANCE);
    let minimum_columns = (available / maximum_width).ceil().max(1.0) as usize;
    let maximum_tolerated_columns = (available / minimum_width).floor().max(0.0) as usize;

    // Fewer items than a full fitted row keep the profile width and trailing whitespace, matching
    // Explorer instead of turning two files into two enormous tiles.
    if item_count > 0 && item_count < minimum_columns {
        metrics.cell_width = base_width.min(available);
        return SpatialGridLayout {
            metrics,
            columns: item_count,
        };
    }

    let nearest = (available / base_width)
        .round()
        .max(1.0)
        .min(maximum_columns as f32) as usize;
    let columns = if minimum_columns <= maximum_tolerated_columns {
        nearest.clamp(
            minimum_columns.min(maximum_columns),
            maximum_tolerated_columns.min(maximum_columns).max(1),
        )
    } else {
        nearest
    };
    metrics.cell_width = available / columns as f32;
    SpatialGridLayout { metrics, columns }
}

fn view_icon_size(
    settings: &explorer_model::ViewSettings,
    layout: crate::layout::LayoutTokens,
) -> f32 {
    match settings.mode {
        explorer_model::ViewMode::ExtraLargeIcons
        | explorer_model::ViewMode::LargeIcons
        | explorer_model::ViewMode::MediumIcons
        | explorer_model::ViewMode::SmallIcons => {
            f32::from(explorer_model::effective_icon_size(settings))
        }
        explorer_model::ViewMode::List => 20.0,
        explorer_model::ViewMode::Details => layout.navigation_icon_size.value(),
        explorer_model::ViewMode::Tiles => 40.0,
        explorer_model::ViewMode::Content => crate::layout::feature::CONTENT_ICON_SIZE.value(),
    }
}

pub(crate) fn view_item_width_with_registry(
    settings: &explorer_model::ViewSettings,
    registry: &explorer_model::ColumnRegistry,
) -> f32 {
    match settings.mode {
        explorer_model::ViewMode::ExtraLargeIcons
        | explorer_model::ViewMode::LargeIcons
        | explorer_model::ViewMode::MediumIcons => {
            f32::from(explorer_model::effective_icon_size(settings)) + 56.0
        }
        explorer_model::ViewMode::SmallIcons => {
            f32::from(explorer_model::effective_icon_size(settings)) + 192.0
        }
        explorer_model::ViewMode::List => 240.0,
        explorer_model::ViewMode::Details | explorer_model::ViewMode::Content => registry
            .iter()
            .filter(|descriptor| settings.details_column_visible(&descriptor.id))
            .map(|descriptor| f32::from(settings.details_column_width(&descriptor.id)))
            .sum(),
        explorer_model::ViewMode::Tiles => 280.0,
    }
}

const fn file_row_explicit_width(
    mode: explorer_model::ViewMode,
    wrapped: bool,
    item_width: f32,
    details_width: f32,
    details_leading_inset: f32,
) -> Option<f32> {
    if wrapped {
        Some(item_width)
    } else if matches!(mode, explorer_model::ViewMode::Details) {
        Some(details_width + details_leading_inset)
    } else {
        None
    }
}

pub(crate) fn details_horizontal_maximum_with_registry(
    settings: &explorer_model::ViewSettings,
    registry: &explorer_model::ColumnRegistry,
    viewport_width: f32,
) -> f32 {
    (view_item_width_with_registry(settings, registry) - viewport_width.max(0.0)).max(0.0)
}

pub(crate) const fn details_header_overlay_position(scroll_offset: (f32, f32)) -> (f32, f32) {
    // The header is a sibling of the scroll host, so Y is permanently viewport
    // pinned. Follow X only to keep its columns aligned with horizontally scrolled rows.
    (scroll_offset.0, 0.0)
}

pub(crate) fn view_item_height(
    settings: &explorer_model::ViewSettings,
    layout: crate::layout::LayoutTokens,
) -> f32 {
    match settings.mode {
        explorer_model::ViewMode::ExtraLargeIcons
        | explorer_model::ViewMode::LargeIcons
        | explorer_model::ViewMode::MediumIcons => {
            f32::from(explorer_model::effective_icon_size(settings))
                + crate::layout::feature::STACKED_ICON_LABEL_GAP.value()
                + crate::layout::feature::STACKED_ICON_LABEL_HEIGHT.value()
        }
        explorer_model::ViewMode::SmallIcons => {
            let requested = f32::from(explorer_model::effective_icon_size(settings)) + 12.0;
            if requested > layout.file_row_height.value() {
                requested
            } else {
                layout.file_row_height.value()
            }
        }
        explorer_model::ViewMode::List | explorer_model::ViewMode::Details => {
            layout.file_row_height.value()
        }
        explorer_model::ViewMode::Content => crate::layout::feature::CONTENT_ROW_HEIGHT.value(),
        explorer_model::ViewMode::Tiles => 64.0,
    }
}

fn explorer_vertical_scrollbar(
    id: &'static str,
    kind: crate::interaction::ScrollbarKind,
    handle: &gpui::ScrollHandle,
    tokens: UiTokens,
    on_action: Option<ActionCallback>,
) -> impl IntoElement {
    let colors = tokens.theme.colors;
    let bounds = handle.bounds();
    let viewport = f32::from(bounds.size.height).max(0.0);
    let maximum = f32::from(handle.max_offset().y).max(0.0);
    let current = (-f32::from(handle.offset().y)).clamp(0.0, maximum);
    let minimum_thumb = tokens.layout.minimum_hit_target.value();
    let track_width = tokens.layout.content_spacing.value() * 1.5;
    let thumb_width = (track_width - tokens.layout.focus_stroke.value() * 2.0).max(8.0);
    let thumb_height = crate::interaction::scrollbar_thumb_height(viewport, maximum, minimum_thumb)
        .unwrap_or(viewport);
    let thumb_top = if maximum > 0.0 {
        current / maximum * (viewport - thumb_height)
    } else {
        0.0
    };
    let click_handle = handle.clone();
    let begin_callback = on_action;
    div()
        .id(id)
        .debug_selector(move || id.to_owned())
        .role(Role::ScrollBar)
        .aria_label(if id == "navigation-scrollbar" {
            "Navigation pane vertical scroll bar"
        } else {
            "File view vertical scroll bar"
        })
        .aria_numeric_value(f64::from(current))
        .aria_min_numeric_value(0.0)
        .aria_max_numeric_value(f64::from(maximum))
        .absolute()
        .top_0()
        .right_0()
        .bottom_0()
        .w(px(track_width))
        .bg(colors.surface.to_gpui())
        .when(maximum <= 0.0 || viewport <= 0.0, |element| {
            // Keep the stable RangeValue provider so an overflow transition can update
            // in place, but collapse its hitbox and UIA bounds exactly like Explorer.
            element
                .invisible()
                .w(gpui::Pixels::ZERO)
                .h(gpui::Pixels::ZERO)
        })
        .on_mouse_down(MouseButton::Left, move |event, window, cx| {
            let bounds = click_handle.bounds();
            let viewport = f32::from(bounds.size.height).max(0.0);
            let maximum = f32::from(click_handle.max_offset().y).max(0.0);
            if viewport <= 0.0 || maximum <= 0.0 {
                return;
            }
            let current = (-f32::from(click_handle.offset().y)).clamp(0.0, maximum);
            let Some(thumb_height) =
                crate::interaction::scrollbar_thumb_height(viewport, maximum, minimum_thumb)
            else {
                return;
            };
            let thumb_top = current / maximum * (viewport - thumb_height);
            let pointer = f32::from(event.position.y - bounds.top());
            if pointer >= thumb_top && pointer <= thumb_top + thumb_height {
                if let Some(callback) = &begin_callback {
                    callback(
                        &ExplorerAction::BeginScrollbarDrag {
                            kind,
                            grab_offset_y: pointer - thumb_top,
                        },
                        window,
                        cx,
                    );
                }
                cx.stop_propagation();
                return;
            }
            let target = if pointer < thumb_top {
                current - viewport
            } else {
                current + viewport
            }
            .clamp(0.0, maximum);
            let offset = click_handle.offset();
            click_handle.set_offset(point(offset.x, px(-target)));
            cx.stop_propagation();
            cx.refresh_windows();
        })
        .child(
            div()
                .absolute()
                .top(px(thumb_top))
                .right(px((track_width - thumb_width) / 2.0))
                .w(px(thumb_width))
                .h(px(thumb_height))
                .rounded(px(tokens.layout.corner_radius.value()))
                .bg(colors.text_disabled.to_gpui())
                .hover(|style| style.bg(colors.text_secondary.to_gpui())),
        )
}

fn explorer_horizontal_scrollbar(
    handle: &gpui::ScrollHandle,
    settings: explorer_model::ViewSettings,
    registry: explorer_model::ColumnRegistry,
    viewport_width: f32,
    tokens: UiTokens,
    on_action: Option<ActionCallback>,
) -> impl IntoElement {
    let colors = tokens.theme.colors;
    let viewport = viewport_width.max(0.0);
    let maximum = details_horizontal_maximum_with_registry(&settings, &registry, viewport_width);
    let current = (-f32::from(handle.offset().x)).clamp(0.0, maximum);
    let minimum_thumb = tokens.layout.minimum_hit_target.value();
    let track_height = tokens.layout.content_spacing.value() * 1.5;
    let thumb_height = (track_height - tokens.layout.focus_stroke.value() * 2.0).max(8.0);
    let thumb_width = crate::interaction::scrollbar_thumb_height(viewport, maximum, minimum_thumb)
        .unwrap_or(viewport);
    let thumb_left = if maximum > 0.0 {
        current / maximum * (viewport - thumb_width)
    } else {
        0.0
    };
    let click_handle = handle.clone();
    let begin_callback = on_action;
    div()
        .id("file-view-horizontal-scrollbar")
        .debug_selector(|| "file-view-horizontal-scrollbar".to_owned())
        .role(Role::ScrollBar)
        .aria_label("File view horizontal scroll bar")
        .aria_numeric_value(f64::from(current))
        .aria_min_numeric_value(0.0)
        .aria_max_numeric_value(f64::from(maximum))
        .absolute()
        .left_0()
        .right(px(track_height))
        .bottom_0()
        .h(px(track_height))
        .bg(colors.surface.to_gpui())
        .when(maximum <= 0.0 || viewport <= 0.0, |element| {
            element
                .invisible()
                .w(gpui::Pixels::ZERO)
                .h(gpui::Pixels::ZERO)
        })
        .when_some(begin_callback, move |element, callback| {
            element.on_mouse_down(MouseButton::Left, move |event, window, cx| {
                let maximum =
                    details_horizontal_maximum_with_registry(&settings, &registry, viewport_width);
                let viewport = viewport_width.max(0.0);
                if maximum <= 0.0 || viewport <= 0.0 {
                    return;
                }
                let current = (-f32::from(click_handle.offset().x)).clamp(0.0, maximum);
                let Some(thumb_width) =
                    crate::interaction::scrollbar_thumb_height(viewport, maximum, minimum_thumb)
                else {
                    return;
                };
                let thumb_left = current / maximum * (viewport - thumb_width);
                let pointer = f32::from(event.position.x - click_handle.bounds().left());
                if pointer >= thumb_left && pointer <= thumb_left + thumb_width {
                    callback(
                        &ExplorerAction::BeginScrollbarDrag {
                            kind: crate::interaction::ScrollbarKind::FileViewHorizontal,
                            grab_offset_y: pointer - thumb_left,
                        },
                        window,
                        cx,
                    );
                    cx.stop_propagation();
                    return;
                }
                let target = if pointer < thumb_left {
                    current - viewport
                } else {
                    current + viewport
                }
                .clamp(0.0, maximum);
                let offset = click_handle.offset();
                click_handle.set_offset(point(px(-target), offset.y));
                cx.stop_propagation();
                cx.refresh_windows();
            })
        })
        .child(
            div()
                .absolute()
                .left(px(thumb_left))
                .bottom(px((track_height - thumb_height) / 2.0))
                .w(px(thumb_width))
                .h(px(thumb_height))
                .rounded(px(tokens.layout.corner_radius.value()))
                .bg(colors.text_disabled.to_gpui())
                .hover(|style| style.bg(colors.text_secondary.to_gpui())),
        )
}

#[allow(
    clippy::needless_pass_by_value,
    reason = "render builders clone and move callbacks into independent GPUI handlers"
)]
pub(crate) fn visible_details_column_ids(
    settings: &explorer_model::ViewSettings,
    registry: &explorer_model::ColumnRegistry,
) -> Vec<explorer_model::ColumnId> {
    settings
        .details_layout
        .visible_registered(registry)
        .map(|entry| entry.id.clone())
        .collect()
}

fn details_header(
    tokens: UiTokens,
    settings: explorer_model::ViewSettings,
    registry: &explorer_model::ColumnRegistry,
    this_pc: bool,
    filter_menu: Option<explorer_model::ColumnId>,
    filters: &crate::file_view::DetailsFilters,
    filter_options: &HashMap<explorer_model::ColumnId, Vec<crate::file_view::DetailsFilterOption>>,
    on_action: Option<ActionCallback>,
) -> gpui::AnyElement {
    if this_pc {
        return this_pc_details_header(tokens);
    }
    let layout = tokens.layout;
    let colors = tokens.theme.colors;
    let visible_descriptors = visible_details_column_ids(&settings, registry)
        .into_iter()
        .filter_map(|column| registry.get(&column))
        .cloned()
        .collect::<Vec<_>>();
    let accessible_columns = visible_descriptors
        .iter()
        .map(|descriptor| descriptor.display_name.as_str())
        .collect::<Vec<_>>()
        .join(", ");
    div()
        .id(DETAILS_HEADER_ID)
        .relative()
        .role(Role::Row)
        .aria_label(format!("Details columns: {accessible_columns}"))
        .h(px(layout.details_header_height.value()))
        .flex_none()
        .flex()
        .items_center()
        .px(px(layout.control_padding_horizontal.value()))
        .text_size(px(tokens.typography.details_header.size.value()))
        .text_color(colors.text_secondary.to_gpui())
        .border_b(px(1.0))
        .border_color(colors.divider.to_gpui())
        .child(region_probe(
            DETAILS_HEADER_ID,
            Some(FILE_VIEW_HOST_ID),
            "normal",
        ))
        .child(typography_probe(
            DETAILS_HEADER_ID,
            typography_diagnostic(tokens, tokens.typography.details_header),
        ))
        .children(visible_descriptors.iter().map(|descriptor| {
            let descriptor = descriptor.clone();
            let column = descriptor.id;
            details_header_column(
                details_column_selector("details-column", &column),
                descriptor.display_name,
                column.clone(),
                settings.clone(),
                filter_menu.clone(),
                filters,
                filter_options.get(&column).cloned().unwrap_or_default(),
                on_action.clone(),
                tokens,
            )
        }))
        .into_any_element()
}

#[allow(dead_code, reason = "retained as the registry-order regression seam")]
fn ordered_detail_extension_column_ids<'a>(
    registry: &explorer_model::ColumnRegistry,
    folder_size_id: Option<&'a explorer_model::ColumnId>,
    code_lines_ids: impl IntoIterator<Item = &'a explorer_model::ColumnId>,
) -> Vec<explorer_model::ColumnId> {
    let code_lines_ids = code_lines_ids.into_iter().collect::<HashSet<_>>();
    registry
        .iter()
        .filter(|descriptor| {
            folder_size_id == Some(&descriptor.id) || code_lines_ids.contains(&descriptor.id)
        })
        .map(|descriptor| descriptor.id.clone())
        .collect()
}

#[derive(Clone)]
enum CodeLinesDetailColumn {
    Ready(
        crate::code_lines_column::CodeLinesColumnVisuals,
        crate::code_lines_column::CodeLinesRuntimeHandleV1,
    ),
    Unavailable(explorer_model::ColumnDescriptor),
}

impl CodeLinesDetailColumn {
    fn id(&self) -> &explorer_model::ColumnId {
        match self {
            Self::Ready(visuals, _) => &visuals.config.descriptor.id,
            Self::Unavailable(descriptor) => &descriptor.id,
        }
    }
}

fn unavailable_detail_cell(
    descriptor: &explorer_model::ColumnDescriptor,
    view_settings: &explorer_model::ViewSettings,
    visible_index: usize,
    layout: crate::layout::LayoutTokens,
) -> gpui::AnyElement {
    let width = f32::from(view_settings.details_column_width(&descriptor.id));
    div()
        .id(format!(
            "{}-{visible_index}",
            details_column_selector("extension-column-unavailable", &descriptor.id)
        ))
        .role(Role::Status)
        .aria_label(format!("{}: unavailable", descriptor.display_name))
        .w(px(width))
        .h_full()
        .flex_none()
        .flex()
        .items_center()
        .justify_end()
        .px(px(layout.content_spacing.value() / 2.0))
        .child("—")
        .into_any_element()
}

#[allow(
    clippy::too_many_arguments,
    reason = "details-cell rendering receives one immutable row snapshot"
)]
fn folder_size_detail_cell(
    visuals: crate::folder_size_column::FolderSizeColumnVisuals,
    runtime: crate::folder_size_column::VisualColumnRuntimeHandleV1,
    entry_id: &explorer_model::ShellItemId,
    selected: bool,
    shell_icon_dpi: u16,
    visual_column_theme: explorer_extension_ui_api::CellThemeV1,
    cell_request_generation: u64,
    view_settings: &explorer_model::ViewSettings,
    visible_index: usize,
    layout: crate::layout::LayoutTokens,
    colors: crate::theme::SemanticColors,
) -> gpui::AnyElement {
    let descriptor = &visuals.config.descriptor;
    let exact_bytes = visuals.value_for(entry_id);
    let partial_bytes = visuals.partial_value_for(entry_id);
    let measurement_error = visuals.error_for(entry_id);
    let maximum = visuals.maximum_value();
    let item_id = extension_render_item_id(entry_id);
    if visuals.partial_pending_for(entry_id)
        || partial_bytes.is_some()
        || measurement_error.is_some()
    {
        let label = "Unavailable";
        let width = f32::from(view_settings.details_column_width(&descriptor.id));
        return div()
            .id(format!("folder-size-column-{visible_index}"))
            .role(Role::Status)
            .aria_label(format!("{}: {label}", descriptor.display_name))
            .w(px(width))
            .h_full()
            .flex_none()
            .flex()
            .items_center()
            .justify_end()
            .child(label)
            .into_any_element();
    }
    let render_generation = extension_render_generation(
        entry_id,
        format!(
            "{exact_bytes:?}:{measurement_error:?}:{maximum}:{selected}:{:?}",
            visuals.config.folder_size_display,
        ),
    );
    let plan = runtime.render_cell(crate::folder_size_column::CellRenderContextV1 {
        value: ROption::RNone,
        exact_bytes: exact_bytes.map_or(ROption::RNone, ROption::RSome),
        aggregate: ROption::RSome(explorer_extension_ui_api::CellAggregateV1 {
            largest_sibling_value: ROption::RNone,
            largest_sibling_bytes: (maximum > 0)
                .then_some(maximum)
                .map_or(ROption::RNone, ROption::RSome),
        }),
        loading: exact_bytes.is_none() && measurement_error.is_none(),
        error: measurement_error
            .map(|error| ROption::RSome(error.into()))
            .unwrap_or(ROption::RNone),
        selected,
        hovered: false,
        dpi_milli: u32::from(shell_icon_dpi).saturating_mul(1_000) / 96,
        theme: visual_column_theme,
        settings: RString::from(match visuals.config.folder_size_display {
            crate::folder_size_column::FolderSizeDisplayMode::BarAndText => "bar-and-text",
            crate::folder_size_column::FolderSizeDisplayMode::TextOnly => "text-only",
        }),
        item_id,
        render_generation,
        request_generation: cell_request_generation,
    });
    let width = f32::from(view_settings.details_column_width(&descriptor.id));
    div()
        .id(format!("folder-size-column-{visible_index}"))
        .role(Role::Status)
        .aria_label(format!("{}: {}", descriptor.display_name, plan.label))
        .w(px(width))
        .h_full()
        .flex_none()
        .flex()
        .items_center()
        .justify_end()
        .gap(px(layout.content_spacing.value() / 2.0))
        .px(px(layout.content_spacing.value() / 2.0))
        .when(plan.proportional_bar_millionths > 0, |cell| {
            let fill_color = crate::theme::Rgba8 {
                red: plan.bar_color.red,
                green: plan.bar_color.green,
                blue: plan.bar_color.blue,
                alpha: plan.bar_color.alpha,
            };
            cell.child(
                div()
                    .id(format!("folder-size-bar-track-{visible_index}"))
                    .w(px((width * 0.42).max(16.0)))
                    .h(px(6.0))
                    .rounded(px(3.0))
                    .border(px(1.0))
                    .border_color(colors.divider.to_gpui())
                    .bg(colors.control_fill.to_gpui())
                    .child(
                        div()
                            .h_full()
                            .w(px((width * 0.42 * plan.proportional_bar_millionths as f32
                                / 1_000_000.0)
                                .max(1.0)))
                            .rounded(px(2.0))
                            .bg(fill_color.to_gpui()),
                    ),
            )
        })
        .child(
            div()
                .min_w_0()
                .flex_1()
                .text_right()
                .overflow_hidden()
                .whitespace_nowrap()
                .text_ellipsis()
                .text_color(
                    crate::theme::Rgba8 {
                        red: plan.text_color.red,
                        green: plan.text_color.green,
                        blue: plan.text_color.blue,
                        alpha: plan.text_color.alpha,
                    }
                    .to_gpui(),
                )
                .child(plan.label.to_string()),
        )
        .into_any_element()
}

#[allow(
    clippy::too_many_arguments,
    reason = "details-cell rendering receives one immutable row snapshot"
)]
fn code_lines_detail_column_cell(
    column: CodeLinesDetailColumn,
    entry_id: &explorer_model::ShellItemId,
    selected: bool,
    shell_icon_dpi: u16,
    visual_column_theme: explorer_extension_ui_api::CellThemeV1,
    cell_request_generation: u64,
    view_settings: &explorer_model::ViewSettings,
    row_column_registry: &explorer_model::ColumnRegistry,
    visible_index: usize,
    layout: crate::layout::LayoutTokens,
    colors: crate::theme::SemanticColors,
) -> gpui::AnyElement {
    match column {
        CodeLinesDetailColumn::Ready(visuals, runtime) => {
            if let Some(admission) = visuals.admissions.get(entry_id) {
                host_admission_detail_cell(
                    &visuals.config.descriptor,
                    *admission,
                    view_settings,
                    visible_index,
                    layout,
                    colors,
                )
            } else {
                code_lines_detail_cell(
                    visuals,
                    runtime,
                    entry_id,
                    selected,
                    shell_icon_dpi,
                    visual_column_theme,
                    cell_request_generation,
                    view_settings,
                    row_column_registry,
                    visible_index,
                    layout,
                    colors,
                )
            }
        }
        CodeLinesDetailColumn::Unavailable(descriptor) => {
            unavailable_detail_cell(&descriptor, view_settings, visible_index, layout)
        }
    }
}

fn host_admission_detail_cell(
    descriptor: &explorer_model::ColumnDescriptor,
    admission: crate::code_lines_column::FolderAdmissionStateV1,
    view_settings: &explorer_model::ViewSettings,
    visible_index: usize,
    layout: crate::layout::LayoutTokens,
    colors: crate::theme::SemanticColors,
) -> gpui::AnyElement {
    let width = f32::from(view_settings.details_column_width(&descriptor.id));
    let (label, reason, is_limit) = admission_cell_presentation(admission);
    div()
        .id(format!(
            "{}-{visible_index}",
            details_column_selector("extension-column-admission", &descriptor.id)
        ))
        .role(Role::Status)
        .aria_label(format!("{}: {reason}", descriptor.display_name))
        .w(px(width))
        .h_full()
        .flex_none()
        .flex()
        .items_center()
        .justify_end()
        .px(px(layout.content_spacing.value() / 2.0))
        .when(is_limit, |element| {
            let reason = SharedString::from(reason);
            element
                .text_color(colors.danger.to_gpui())
                .tooltip(move |_, cx| {
                    cx.new(|_| AdmissionLimitTooltip {
                        reason: reason.clone(),
                        colors,
                        layout,
                    })
                    .into()
                })
        })
        .child(label.to_owned())
        .into_any_element()
}

fn admission_cell_presentation(
    admission: crate::code_lines_column::FolderAdmissionStateV1,
) -> (&'static str, &'static str, bool) {
    (admission.label(), admission.reason(), admission.is_limit())
}

struct AdmissionLimitTooltip {
    reason: SharedString,
    colors: crate::theme::SemanticColors,
    layout: crate::layout::LayoutTokens,
}

impl Render for AdmissionLimitTooltip {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .px(px(self.layout.control_padding_horizontal.value()))
            .py(px(self.layout.content_spacing.value() / 2.0))
            .rounded(px(self.layout.corner_radius.value()))
            .border_1()
            .border_color(self.colors.divider.to_gpui())
            .bg(self.colors.menu_fill.to_gpui())
            .text_color(self.colors.text_primary.to_gpui())
            .shadow_md()
            .child(self.reason.clone())
    }
}

#[allow(
    clippy::too_many_arguments,
    reason = "details-cell rendering receives one immutable row snapshot"
)]
fn code_lines_detail_cell(
    visuals: crate::code_lines_column::CodeLinesColumnVisuals,
    runtime: crate::code_lines_column::CodeLinesRuntimeHandleV1,
    entry_id: &explorer_model::ShellItemId,
    selected: bool,
    shell_icon_dpi: u16,
    visual_column_theme: explorer_extension_ui_api::CellThemeV1,
    cell_request_generation: u64,
    view_settings: &explorer_model::ViewSettings,
    row_column_registry: &explorer_model::ColumnRegistry,
    visible_index: usize,
    layout: crate::layout::LayoutTokens,
    colors: crate::theme::SemanticColors,
) -> gpui::AnyElement {
    let descriptor = &visuals.config.descriptor;
    let value = visuals.values.get(entry_id);
    let error = visuals.presentation_error_for(entry_id);
    let maximum = visuals.maximum_value();
    let item_id = extension_render_item_id(entry_id);
    let render_generation = extension_render_generation(
        entry_id,
        format!(
            "{value:?}:{error:?}:{maximum}:{selected}:{:?}",
            visuals.config.display
        ),
    );
    let plan = runtime.render_cell(crate::code_lines_column::CellRenderContextV1 {
        value: value
            .and_then(|value| {
                serde_json::to_vec(&serde_json::json!({
                    "blanks": value.blanks,
                    "code": value.code,
                    "comments": value.comments,
                    "language": value.language,
                    "total": value.total,
                }))
                .ok()
                .and_then(|bytes| {
                    explorer_extension_ui_api::PluginValueV1::structured_canonical_json(bytes).ok()
                })
            })
            .map_or(ROption::RNone, ROption::RSome),
        exact_bytes: ROption::RNone,
        aggregate: ROption::RSome(explorer_extension_ui_api::CellAggregateV1 {
            largest_sibling_value: (maximum > 0)
                .then(|| {
                    serde_json::to_vec(&serde_json::json!({
                        "blanks": 0,
                        "code": maximum,
                        "comments": 0,
                        "language": "aggregate",
                        "total": maximum,
                    }))
                    .ok()
                })
                .flatten()
                .and_then(|bytes| {
                    explorer_extension_ui_api::PluginValueV1::structured_canonical_json(bytes).ok()
                })
                .map_or(ROption::RNone, ROption::RSome),
            largest_sibling_bytes: ROption::RNone,
        }),
        loading: value.is_none() && error.is_none(),
        error: error
            .map(|error| ROption::RSome(error.into()))
            .unwrap_or(ROption::RNone),
        selected,
        hovered: false,
        dpi_milli: u32::from(shell_icon_dpi).saturating_mul(1_000) / 96,
        theme: visual_column_theme,
        settings: if visuals.config.display.shows_detail() {
            value.map_or_else(
                || RString::from("with-detail"),
                |value| {
                    RString::from(format!(
                        "with-detail;language={};comments={};blanks={};total={}",
                        value.language, value.comments, value.blanks, value.total
                    ))
                },
            )
        } else {
            RString::from("code-only")
        },
        item_id,
        render_generation,
        request_generation: cell_request_generation,
    });
    div()
        .when(
            row_column_registry.contains(&descriptor.id)
                && view_settings.details_column_visible(&descriptor.id),
            |element| {
                let width = f32::from(view_settings.details_column_width(&descriptor.id));
                element.child(
                    div()
                        .id(format!(
                            "{}-{visible_index}",
                            details_column_selector("code-lines-column", &descriptor.id)
                        ))
                        .role(Role::Status)
                        .aria_label(format!(
                            "{}: {} {}",
                            descriptor.display_name, plan.label, plan.detail
                        ))
                        .w(px(width))
                        .h_full()
                        .flex_none()
                        .flex()
                        .items_center()
                        .justify_end()
                        .gap(px(layout.content_spacing.value() / 2.0))
                        .px(px(layout.content_spacing.value() / 2.0))
                        .when(plan.proportional_bar_millionths > 0, |cell| {
                            let fill_color = crate::theme::Rgba8 {
                                red: plan.bar_color.red,
                                green: plan.bar_color.green,
                                blue: plan.bar_color.blue,
                                alpha: plan.bar_color.alpha,
                            };
                            cell.child(
                                div()
                                    .id(format!(
                                        "{}-{visible_index}",
                                        details_column_selector(
                                            "code-lines-bar-track",
                                            &descriptor.id,
                                        )
                                    ))
                                    .w(px((width * 0.30).max(12.0)))
                                    .h(px(6.0))
                                    .rounded(px(3.0))
                                    .border(px(1.0))
                                    .border_color(colors.divider.to_gpui())
                                    .bg(colors.control_fill.to_gpui())
                                    .child(
                                        div()
                                            .h_full()
                                            .w(px((width
                                                * 0.30
                                                * plan.proportional_bar_millionths as f32
                                                / 1_000_000.0)
                                                .max(1.0)))
                                            .rounded(px(2.0))
                                            .bg(fill_color.to_gpui()),
                                    ),
                            )
                        })
                        .child(
                            div()
                                .min_w_0()
                                .flex_1()
                                .text_right()
                                .overflow_hidden()
                                .whitespace_nowrap()
                                .text_ellipsis()
                                .child(if plan.detail.is_empty() {
                                    plan.label.to_string()
                                } else {
                                    format!("{}  {}", plan.label, plan.detail)
                                }),
                        ),
                )
            },
        )
        .into_any_element()
}

fn this_pc_details_header(tokens: UiTokens) -> gpui::AnyElement {
    let layout = tokens.layout;
    let colors = tokens.theme.colors;
    let column = |id: &'static str, label: &'static str, width: f32| {
        div()
            .id(id)
            .role(Role::Label)
            .aria_label(label)
            .h_full()
            .w(px(width))
            .flex_none()
            .flex()
            .items_center()
            .px(px(layout.content_spacing.value() / 2.0))
            .border_r(px(1.0))
            .border_color(colors.divider.to_gpui())
            .child(label)
    };
    div()
        .id(DETAILS_HEADER_ID)
        .role(Role::Row)
        .aria_label("本機詳細資料欄位：名稱、類型、大小總計、可用空間")
        .h(px(layout.details_header_height.value()))
        .flex_none()
        .flex()
        .items_center()
        .px(px(layout.control_padding_horizontal.value()))
        .text_size(px(tokens.typography.details_header.size.value()))
        .text_color(colors.text_secondary.to_gpui())
        .border_b(px(1.0))
        .border_color(colors.divider.to_gpui())
        .child(column(
            "this-pc-column-name",
            "名稱",
            crate::layout::feature::THIS_PC_DETAILS_NAME_WIDTH.value(),
        ))
        .child(column(
            "this-pc-column-type",
            "類型",
            crate::layout::feature::THIS_PC_DETAILS_TYPE_WIDTH.value(),
        ))
        .child(column(
            "this-pc-column-total",
            "大小總計",
            crate::layout::feature::THIS_PC_DETAILS_TOTAL_WIDTH.value(),
        ))
        .child(column(
            "this-pc-column-free",
            "可用空間",
            crate::layout::feature::THIS_PC_DETAILS_FREE_WIDTH.value(),
        ))
        .into_any_element()
}

#[allow(
    clippy::needless_pass_by_value,
    reason = "render builders clone and move callbacks into independent GPUI handlers"
)]
fn details_column_menu(
    tokens: UiTokens,
    target: explorer_model::ColumnId,
    settings: explorer_model::ViewSettings,
    registry: &explorer_model::ColumnRegistry,
    folder_size_visuals: Option<crate::folder_size_column::FolderSizeColumnVisuals>,
    code_lines_visuals: Vec<crate::code_lines_column::CodeLinesColumnVisuals>,
    on_action: Option<ActionCallback>,
) -> impl IntoElement {
    let colors = tokens.theme.colors;
    let current = on_action.clone();
    let all = on_action.clone();
    let target_for_auto_size = target.clone();
    div()
        .id("details-column-menu")
        .role(Role::Menu)
        .aria_label("Choose details columns")
        .absolute()
        .top(px(tokens.layout.details_header_height.value() + 4.0))
        .bottom(px(tokens.layout.content_spacing.value()))
        .left(px(tokens.layout.control_padding_horizontal.value()))
        .w(px(crate::layout::feature::DETAILS_COLUMN_MENU_WIDTH.value()))
        .max_h(px(tokens.layout.menu_max_height.value()))
        .overflow_x_hidden()
        .overflow_y_scroll()
        .p(px(
            crate::layout::feature::DETAILS_COLUMN_MENU_PADDING.value()
        ))
        .rounded(px(tokens.layout.corner_radius.value()))
        .border(px(1.0))
        .border_color(colors.divider.to_gpui())
        .bg(colors.surface.to_gpui())
        .shadow_md()
        .occlude()
        .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
        .on_mouse_down(MouseButton::Right, |_, _, cx| cx.stop_propagation())
        .when_some(current, move |element, callback| {
            element.child(column_menu_row(
                tokens,
                "details-column-menu-auto-size-current".to_owned(),
                "Auto size this column".to_owned(),
                false,
                true,
                ExplorerAction::AutoSizeDetailsColumn {
                    column: target_for_auto_size,
                },
                callback,
            ))
        })
        .when_some(all, move |element, callback| {
            element.child(column_menu_row(
                tokens,
                "details-column-menu-auto-size-all".to_owned(),
                "Auto size all columns".to_owned(),
                false,
                true,
                ExplorerAction::AutoSizeAllDetailsColumns,
                callback,
            ))
        })
        .when_some(folder_size_visuals, |element, visuals| {
            let descriptor = &visuals.config.descriptor;
            element.when(target == descriptor.id, |element| {
                element.when_some(on_action.clone(), |element, callback| {
                    element.child(column_menu_row(
                        tokens,
                        "details-column-menu-folder-size-bar".to_owned(),
                        "Show proportional bar".to_owned(),
                        visuals.config.folder_size_display.shows_bar(),
                        true,
                        ExplorerAction::ToggleFolderSizeProportionalBar,
                        callback,
                    ))
                })
            })
        })
        .children(code_lines_visuals.into_iter().filter_map(|visuals| {
            let descriptor = &visuals.config.descriptor;
            (target == descriptor.id).then(|| {
                div().when_some(on_action.clone(), |element, callback| {
                    element.child(column_menu_row(
                        tokens,
                        "details-column-menu-code-lines-detail".to_owned(),
                        "Show comment and blank detail".to_owned(),
                        visuals.config.display.shows_detail(),
                        true,
                        ExplorerAction::ToggleCodeLinesDetail,
                        callback,
                    ))
                })
            })
        }))
        .child(
            div()
                .h(px(
                    crate::layout::feature::DETAILS_COLUMN_SEPARATOR_HEIGHT.value()
                ))
                .my(px(
                    crate::layout::feature::DETAILS_COLUMN_SEPARATOR_MARGIN.value()
                ))
                .bg(colors.divider.to_gpui()),
        )
        .children(
            settings
                .details_layout
                .entries()
                .iter()
                .filter_map(|entry| {
                    let descriptor = registry.get(&entry.id)?;
                    let column = descriptor.id.clone();
                    on_action.clone().map(|callback| {
                        column_menu_row(
                            tokens,
                            details_column_selector("details-column-menu", &column),
                            descriptor.display_name.clone(),
                            settings.details_column_visible(&column),
                            column != explorer_model::ColumnId::Name,
                            ExplorerAction::ToggleDetailsColumn(column.clone()),
                            callback,
                        )
                    })
                }),
        )
}

fn column_menu_row(
    tokens: UiTokens,
    id: String,
    label: String,
    checked: bool,
    enabled: bool,
    action: ExplorerAction,
    callback: ActionCallback,
) -> impl IntoElement {
    let colors = tokens.theme.colors;
    div()
        .id(id)
        .role(Role::MenuItem)
        .aria_label(format!(
            "{label}, {}",
            if checked { "checked" } else { "unchecked" }
        ))
        .aria_selected(checked)
        .h(px(tokens.layout.minimum_hit_target.value()))
        .px(px(
            crate::layout::feature::DETAILS_COLUMN_ROW_PADDING.value()
        ))
        .flex()
        .items_center()
        .gap(px(crate::layout::feature::DETAILS_COLUMN_ROW_GAP.value()))
        .text_color(if enabled {
            colors.text_primary.to_gpui()
        } else {
            colors.text_disabled.to_gpui()
        })
        .when(enabled, |row| {
            row.cursor_pointer()
                .hover(move |style| style.bg(colors.control_hover.to_gpui()))
                .on_click(move |_, window, cx| {
                    callback(&action, window, cx);
                    cx.stop_propagation();
                })
        })
        .child(if checked { "✓" } else { "" })
        .child(label)
}

fn details_column_selector(prefix: &str, column: &explorer_model::ColumnId) -> String {
    format!("{prefix}-{}", column.stable_id().replace(':', "-"))
}

#[allow(
    clippy::needless_pass_by_value,
    reason = "the deferred filter menu captures its column and callbacks into independently-lived GPUI handlers"
)]
fn details_filter_menu(
    tokens: UiTokens,
    column: explorer_model::ColumnId,
    column_label: String,
    options: Vec<crate::file_view::DetailsFilterOption>,
    filters: &crate::file_view::DetailsFilters,
    on_action: Option<ActionCallback>,
) -> impl IntoElement {
    let colors = tokens.theme.colors;
    let action_column = column.clone();
    let rows = options.into_iter().map(|option| {
        let action_column = action_column.clone();
        let selected = filters.is_selected(&column, &option.key);
        let callback = on_action.clone();
        div()
            .id(format!("details-filter-{}", option.key))
            .role(Role::MenuItem)
            .aria_label(format!(
                "{}, {}",
                option.label,
                if selected { "checked" } else { "unchecked" }
            ))
            .aria_selected(selected)
            .h(px(tokens.layout.minimum_hit_target.value()))
            .px(px(tokens.layout.content_spacing.value()))
            .flex()
            .items_center()
            .gap(px(tokens.layout.content_spacing.value()))
            .hover(move |style| style.bg(colors.control_hover.to_gpui()))
            .when_some(callback, move |element, callback| {
                let action_column = action_column.clone();
                let key = option.key.clone();
                element.on_click(move |_, window, cx| {
                    callback(
                        &ExplorerAction::ToggleDetailsFilter {
                            column: action_column.clone(),
                            key: key.clone(),
                        },
                        window,
                        cx,
                    );
                    cx.stop_propagation();
                })
            })
            .child(if selected { "☑" } else { "☐" })
            .child(option.label)
    });
    let clear_callback = on_action.clone();
    deferred(
        div()
            .id(format!("details-filter-menu-{column:?}"))
            .role(Role::Menu)
            .aria_label(format!("Filter {column_label}"))
            .absolute()
            .top(px(tokens.layout.details_header_height.value()))
            .left_0()
            .min_w(px(220.0))
            .p(px(4.0))
            .rounded(px(tokens.layout.corner_radius.value()))
            .border(px(1.0))
            .border_color(colors.divider.to_gpui())
            .bg(colors.surface.to_gpui())
            .shadow_md()
            .occlude()
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .children(rows)
            .when(filters.is_active(&column), |element| {
                element.child(
                    div()
                        .id("details-filter-clear")
                        .role(Role::MenuItem)
                        .aria_label("Clear filter")
                        .h(px(tokens.layout.minimum_hit_target.value()))
                        .px(px(tokens.layout.content_spacing.value()))
                        .flex()
                        .items_center()
                        .hover(move |style| style.bg(colors.control_hover.to_gpui()))
                        .when_some(clear_callback, move |element, callback| {
                            element.on_click(move |_, window, cx| {
                                callback(
                                    &ExplorerAction::ClearDetailsFilter {
                                        column: column.clone(),
                                    },
                                    window,
                                    cx,
                                );
                                cx.stop_propagation();
                            })
                        })
                        .child("清除篩選"),
                )
            }),
    )
}

#[allow(
    clippy::needless_pass_by_value,
    reason = "the header builder clones command-boundary values into independent resize and menu handlers"
)]
fn details_header_column(
    id: String,
    label: String,
    column: explorer_model::ColumnId,
    settings: explorer_model::ViewSettings,
    filter_menu: Option<explorer_model::ColumnId>,
    filters: &crate::file_view::DetailsFilters,
    filter_options: Vec<crate::file_view::DetailsFilterOption>,
    on_action: Option<ActionCallback>,
    tokens: UiTokens,
) -> impl IntoElement {
    let active = settings.sort.column == column;
    let filter_open = filter_menu == Some(column.clone());
    let filter_active = filters.is_active(&column);
    let separator_id = format!(
        "{}-separator",
        details_column_selector("details-column", &column)
    );
    let separator_debug_id = separator_id.clone();
    let indicator = if active {
        match settings.sort.direction {
            explorer_model::SortDirection::Ascending => "  ↑",
            explorer_model::SortDirection::Descending => "  ↓",
        }
    } else {
        ""
    };
    let begin_callback = on_action.clone();
    let move_callback = on_action.clone();
    let end_callback = on_action.clone();
    let outside_end_callback = on_action.clone();
    let decrement_callback = on_action.clone();
    let increment_callback = on_action.clone();
    let context_callback = on_action.clone();
    let sort_context_callback = on_action.clone();
    let sort_callback = on_action.clone();
    let drag_move_callback = on_action.clone();
    let drag_outside_cancel_callback = on_action.clone();
    let sort_drag_outside_cancel_callback = on_action.clone();
    let sort_accessible_callback = on_action.clone();
    let filter_callback = on_action.clone();
    let context_column = column.clone();
    let sort_context_column = column.clone();
    let sort_column = column.clone();
    let drag_target_column = column.clone();
    let accessible_sort_column = column.clone();
    let filter_column = column.clone();
    let decrement_column = column.clone();
    let increment_column = column.clone();
    let begin_column = column.clone();
    let draggable_column = column.clone();
    let sort_draggable_column = column.clone();
    let drop_callback = on_action.clone();
    let sort_drop_callback = on_action.clone();
    let drag_label = label.clone();
    let sort_drag_label = label.clone();
    let decrement_settings = settings.clone();
    let increment_settings = settings.clone();
    div()
        .id(id.clone())
        .debug_selector({
            let debug_id = id.clone();
            move || debug_id.clone()
        })
        .role(Role::Group)
        .aria_label(if active {
            format!("{label}, sorted {indicator}")
        } else {
            format!("Sort by {label}")
        })
        // The resize grip is positioned against this column. Without an explicit
        // positioning context GPUI can paint/report the grip at the column edge
        // while hit-testing it against an ancestor, allowing the file host below
        // to receive the press and start a marquee drag.
        .relative()
        .h_full()
        .w(px(f32::from(settings.details_column_width(&column))))
        .flex_none()
        .flex()
        .items_center()
        .when(column != explorer_model::ColumnId::Name, |element| {
            element.cursor_move().on_drag(
                DetailsColumnDrag {
                    column: draggable_column,
                    label: drag_label,
                },
                |drag, _, _, cx| {
                    cx.new(|_| DetailsColumnDragPreview {
                        label: drag.label.clone(),
                    })
                },
            )
        })
        .when_some(drag_outside_cancel_callback, move |element, callback| {
            element.on_mouse_up_out(MouseButton::Left, move |_, window, cx| {
                if cx.has_active_drag() {
                    let callback = callback.clone();
                    window.defer(cx, move |window, cx| {
                        callback(&ExplorerAction::CancelDetailsColumnDrag, window, cx);
                    });
                }
            })
        })
        .when_some(drag_move_callback, move |element, callback| {
            element.on_drag_move::<DetailsColumnDrag>(move |event, window, cx| {
                if !event.bounds.contains(&event.event.position) {
                    return;
                }
                let drag = event.drag(cx);
                callback(
                    &ExplorerAction::UpdateDetailsColumnDragPreview {
                        column: drag.column.clone(),
                        target: drag_target_column.clone(),
                        pointer_x: f32::from(event.event.position.x),
                        target_left: f32::from(event.bounds.left()),
                        target_right: f32::from(event.bounds.right()),
                    },
                    window,
                    cx,
                );
                cx.stop_propagation();
            })
        })
        .when_some(drop_callback, move |element, callback| {
            element.on_drop(move |drag: &DetailsColumnDrag, window, cx| {
                let _ = drag;
                callback(&ExplorerAction::CommitDetailsColumnDrag, window, cx);
                cx.stop_propagation();
            })
        })
        .hover(move |style| style.bg(tokens.theme.colors.control_hover.to_gpui()))
        .when_some(context_callback, move |element, callback| {
            element.on_mouse_down(MouseButton::Right, move |_, window, cx| {
                cx.stop_propagation();
                callback(
                    &ExplorerAction::OpenDetailsColumnMenu {
                        column: context_column.clone(),
                    },
                    window,
                    cx,
                );
            })
        })
        .child(
            div()
                .id(format!("{id}-sort"))
                .role(Role::Button)
                .aria_label(if active {
                    format!("{label}, sorted {indicator}")
                } else {
                    format!("Sort by {label}")
                })
                .h_full()
                .flex_1()
                .min_w_0()
                .flex()
                .items_center()
                .px(px(tokens.layout.content_spacing.value() / 2.0))
                .when(column != explorer_model::ColumnId::Name, |element| {
                    element.cursor_move().on_drag(
                        DetailsColumnDrag {
                            column: sort_draggable_column,
                            label: sort_drag_label,
                        },
                        |drag, _, _, cx| {
                            cx.new(|_| DetailsColumnDragPreview {
                                label: drag.label.clone(),
                            })
                        },
                    )
                })
                .when_some(
                    sort_drag_outside_cancel_callback,
                    move |element, callback| {
                        element.on_mouse_up_out(MouseButton::Left, move |_, window, cx| {
                            if cx.has_active_drag() {
                                let callback = callback.clone();
                                window.defer(cx, move |window, cx| {
                                    callback(&ExplorerAction::CancelDetailsColumnDrag, window, cx);
                                });
                            }
                        })
                    },
                )
                .when_some(sort_drop_callback, move |element, callback| {
                    element.on_drop(move |drag: &DetailsColumnDrag, window, cx| {
                        let _ = drag;
                        callback(&ExplorerAction::CommitDetailsColumnDrag, window, cx);
                        cx.stop_propagation();
                    })
                })
                .when_some(sort_callback, move |element, callback| {
                    element.on_click(move |_, window, cx| {
                        callback(
                            &ExplorerAction::SetColumnId(accessible_sort_column.clone()),
                            window,
                            cx,
                        );
                        cx.stop_propagation();
                    })
                })
                .when_some(sort_accessible_callback, move |element, callback| {
                    element.on_a11y_action(AccessibleAction::Click, move |_, window, cx| {
                        callback(
                            &ExplorerAction::SetColumnId(sort_column.clone()),
                            window,
                            cx,
                        );
                        cx.stop_propagation();
                    })
                })
                .when_some(sort_context_callback, move |element, callback| {
                    element.on_mouse_down(MouseButton::Right, move |_, window, cx| {
                        callback(
                            &ExplorerAction::OpenDetailsColumnMenu {
                                column: sort_context_column.clone(),
                            },
                            window,
                            cx,
                        );
                        cx.stop_propagation();
                    })
                })
                .child(format!("{label}{indicator}")),
        )
        .child(
            div()
                .id(format!("{id}-filter"))
                .role(Role::Button)
                .aria_label(format!("Filter {label}"))
                .aria_expanded(filter_open)
                .h_full()
                .w(px(tokens.layout.minimum_hit_target.value()))
                .flex_none()
                .flex()
                .items_center()
                .justify_center()
                .when(filter_active, |element| {
                    element.text_color(tokens.theme.colors.accent.to_gpui())
                })
                .hover(move |style| style.bg(tokens.theme.colors.control_hover.to_gpui()))
                .when_some(filter_callback, move |element, callback| {
                    element.on_click(move |_, window, cx| {
                        callback(
                            &ExplorerAction::OpenDetailsFilterMenu {
                                column: filter_column.clone(),
                            },
                            window,
                            cx,
                        );
                        cx.stop_propagation();
                    })
                })
                .child(chrome_icon(
                    format!("{id}-filter-chevron"),
                    ExplorerIcon::ChevronDown,
                    tokens,
                )),
        )
        .when(filter_open, |element| {
            element.child(details_filter_menu(
                tokens,
                column.clone(),
                label.clone(),
                filter_options,
                filters,
                on_action.clone(),
            ))
        })
        .child(
            div()
                .id(separator_id)
                .debug_selector(move || separator_debug_id.clone())
                .role(Role::Splitter)
                .aria_label(format!("Resize {label} column"))
                .aria_numeric_value(f64::from(settings.details_column_width(&column)))
                .aria_min_numeric_value(f64::from(
                    explorer_model::OrderedColumnLayout::MINIMUM_WIDTH,
                ))
                .aria_max_numeric_value(f64::from(
                    explorer_model::OrderedColumnLayout::MAXIMUM_WIDTH,
                ))
                .absolute()
                // Keep the complete interactive area inside the owning header so
                // the visible splitter and its pointer target cannot disagree.
                .right_0()
                .top_0()
                .w(px(tokens.layout.divider_width.value()))
                .h_full()
                .cursor_col_resize()
                .child(
                    div()
                        .absolute()
                        .left(px(3.5))
                        .top_0()
                        .w(px(tokens.layout.focus_stroke.value() / 2.0))
                        .h_full()
                        .bg(tokens.theme.colors.divider.to_gpui()),
                )
                .hover(move |style| style.bg(tokens.theme.colors.focus.to_gpui()))
                .active(move |style| style.bg(tokens.theme.colors.accent.to_gpui()))
                .when_some(decrement_callback, move |element, callback| {
                    element.on_a11y_action(AccessibleAction::Decrement, move |_, window, cx| {
                        callback(
                            &ExplorerAction::SetDetailsColumnWidth {
                                column: decrement_column.clone(),
                                width: decrement_settings
                                    .details_column_width(&decrement_column)
                                    .saturating_sub(8),
                            },
                            window,
                            cx,
                        );
                    })
                })
                .when_some(increment_callback, move |element, callback| {
                    element.on_a11y_action(AccessibleAction::Increment, move |_, window, cx| {
                        callback(
                            &ExplorerAction::SetDetailsColumnWidth {
                                column: increment_column.clone(),
                                width: increment_settings
                                    .details_column_width(&increment_column)
                                    .saturating_add(8),
                            },
                            window,
                            cx,
                        );
                    })
                })
                .when_some(begin_callback, move |element, callback| {
                    element.on_mouse_down(MouseButton::Left, move |event, window, cx| {
                        cx.stop_propagation();
                        if event.click_count == 2 {
                            callback(
                                &ExplorerAction::AutoSizeDetailsColumn {
                                    column: begin_column.clone(),
                                },
                                window,
                                cx,
                            );
                        } else {
                            callback(
                                &ExplorerAction::BeginDetailsColumnResize {
                                    column: begin_column.clone(),
                                    pointer_x: f32::from(event.position.x),
                                },
                                window,
                                cx,
                            );
                        }
                    })
                })
                .when_some(move_callback, move |element, callback| {
                    element.on_mouse_move(move |event: &MouseMoveEvent, window, cx| {
                        callback(
                            &ExplorerAction::UpdateDetailsColumnResize {
                                pointer_x: f32::from(event.position.x),
                            },
                            window,
                            cx,
                        );
                    })
                })
                .when_some(end_callback, move |element, callback| {
                    element.on_mouse_up(MouseButton::Left, move |_, window, cx| {
                        cx.stop_propagation();
                        callback(&ExplorerAction::EndDetailsColumnResize, window, cx);
                    })
                })
                .when_some(outside_end_callback, move |element, callback| {
                    element.on_mouse_up_out(MouseButton::Left, move |_, window, cx| {
                        callback(&ExplorerAction::EndDetailsColumnResize, window, cx);
                    })
                }),
        )
}

#[allow(
    clippy::cast_possible_truncation,
    reason = "a Windows screen coordinate is an i32 and GPUI positions are bounded by the desktop"
)]
pub(crate) fn context_menu_coordinates(
    position: gpui::Point<gpui::Pixels>,
    window: &Window,
) -> (u64, i32, i32) {
    let owner = HasWindowHandle::window_handle(window)
        .ok()
        .and_then(|handle| match handle.as_raw() {
            RawWindowHandle::Win32(handle) => u64::try_from(handle.hwnd.get()).ok(),
            _ => None,
        })
        .unwrap_or(0);
    let (x, y) = client_to_screen_point(window.bounds().origin, position);
    (owner, x, y)
}

#[allow(
    clippy::cast_possible_truncation,
    reason = "Win32 POINT coordinates are signed i32 values"
)]
fn client_to_screen_point(
    window_origin: gpui::Point<gpui::Pixels>,
    client_position: gpui::Point<gpui::Pixels>,
) -> (i32, i32) {
    // GPUI supplies both values in the same physical desktop coordinate space. In particular,
    // negative monitor origins remain signed and no DPI multiplier is applied a second time.
    (
        (f32::from(window_origin.x) + f32::from(client_position.x)).round() as i32,
        (f32::from(window_origin.y) + f32::from(client_position.y)).round() as i32,
    )
}

#[derive(IntoElement)]
pub struct OperationCenter {
    tokens: UiTokens,
    state: OperationCenterViewModel,
    on_action: Option<ActionCallback>,
}

fn operation_location_text(location: &explorer_model::LocationDescriptor) -> String {
    let editable = location.editable_text();
    if !editable.is_empty() {
        return editable;
    }
    match location {
        explorer_model::LocationDescriptor::ShellNamespace(_) => "Shell namespace".to_owned(),
        explorer_model::LocationDescriptor::KnownFolder(_) => "Windows known folder".to_owned(),
        explorer_model::LocationDescriptor::FileSystem(_)
        | explorer_model::LocationDescriptor::ParsingName(_)
        | explorer_model::LocationDescriptor::Virtual(_) => "Unknown location".to_owned(),
    }
}

fn operation_child_location_text(
    parent: &explorer_model::LocationDescriptor,
    name: &str,
) -> String {
    match parent {
        explorer_model::LocationDescriptor::FileSystem(path) => {
            path.join(name).to_string_lossy().into_owned()
        }
        explorer_model::LocationDescriptor::Virtual(location) => {
            let mut child = location.clone();
            child.entry_id = None;
            child.components.push(name.to_owned());
            explorer_model::LocationDescriptor::Virtual(child).editable_text()
        }
        _ => format!("{} → {name}", operation_location_text(parent)),
    }
}

fn renamed_location_text(item: &explorer_model::ItemDescriptor, new_name: &str) -> String {
    match &item.location {
        explorer_model::LocationDescriptor::FileSystem(path) => path
            .parent()
            .map(|parent| parent.join(new_name).to_string_lossy().into_owned())
            .unwrap_or_else(|| new_name.to_owned()),
        explorer_model::LocationDescriptor::Virtual(location) => {
            let mut renamed = location.clone();
            if let Some(last) = renamed.components.last_mut() {
                new_name.clone_into(last);
                renamed.entry_id = None;
                explorer_model::LocationDescriptor::Virtual(renamed).editable_text()
            } else {
                format!("{} → {new_name}", operation_location_text(&item.location))
            }
        }
        _ => format!("{} → {new_name}", operation_location_text(&item.location)),
    }
}

fn operation_sources_text(items: &[explorer_model::ItemDescriptor]) -> String {
    let Some(first) = items.first() else {
        return "來源由系統剪貼簿提供".to_owned();
    };
    let first = operation_location_text(&first.location);
    if items.len() == 1 {
        first
    } else {
        format!("{first}（另有 {} 個項目）", items.len() - 1)
    }
}

fn operation_request_summary(record: &explorer_model::OperationRecord) -> String {
    use explorer_model::FileOperationKind as Kind;
    match &record.request.kind {
        Kind::CreateFolder { parent, name } => {
            format!(
                "新增資料夾｜{}",
                operation_child_location_text(parent, name)
            )
        }
        Kind::CreateItem {
            parent,
            name,
            recipe,
        } => {
            let label = if matches!(recipe, explorer_model::ShellNewItemRecipe::Folder) {
                "新增資料夾"
            } else {
                "新增檔案"
            };
            format!("{label}｜{}", operation_child_location_text(parent, name))
        }
        Kind::Rename { item, new_name } => format!(
            "重新命名｜{} → {}",
            operation_location_text(&item.location),
            renamed_location_text(item, new_name)
        ),
        Kind::SetUnixMode { item, mode } => format!(
            "變更權限｜{} → {mode:04o}",
            operation_location_text(&item.location)
        ),
        Kind::Copy { items, destination } => format!(
            "複製 {} 個項目｜{} → {}",
            record.progress.total_items,
            operation_sources_text(items),
            operation_location_text(destination)
        ),
        Kind::Move { items, destination } => format!(
            "移動 {} 個項目｜{} → {}",
            record.progress.total_items,
            operation_sources_text(items),
            operation_location_text(destination)
        ),
        Kind::RecycleDelete { items } => format!(
            "移至資源回收筒 {} 個項目｜{}",
            record.progress.total_items,
            operation_sources_text(items)
        ),
        Kind::PermanentDelete { items, .. } => format!(
            "永久刪除 {} 個項目｜{}",
            record.progress.total_items,
            operation_sources_text(items)
        ),
        Kind::CreateShortcut { items } => format!(
            "建立捷徑 {} 個項目｜{}",
            record.progress.total_items,
            operation_sources_text(items)
        ),
    }
}

fn operation_message(record: &explorer_model::OperationRecord) -> String {
    let summary = operation_request_summary(record);
    match &record.terminal {
        None => {
            let phase = match record.progress.phase {
                explorer_model::TransferProgressPhase::Preparing => "準備中",
                explorer_model::TransferProgressPhase::Transferring => "傳輸中",
                explorer_model::TransferProgressPhase::Finalizing => "完成處理中",
            };
            let bytes = format_transfer_bytes(record.progress.completed_bytes);
            match record.progress.total_bytes {
                Some(0) => format!(
                    "{summary}｜{phase}｜進度 {}/{} 項目",
                    record.progress.completed_items, record.progress.total_items
                ),
                Some(total) => {
                    let percent = record
                        .progress
                        .completed_bytes
                        .saturating_mul(100)
                        .checked_div(total)
                        .unwrap_or(0)
                        .min(99);
                    format!(
                        "{summary}｜{phase} {percent}%（{bytes} / {}）｜進度 {}/{} 項目",
                        format_transfer_bytes(total),
                        record.progress.completed_items,
                        record.progress.total_items
                    )
                }
                None => format!(
                    "{summary}｜{phase} {bytes}｜進度 {}/{} 項目",
                    record.progress.completed_items, record.progress.total_items
                ),
            }
        }
        Some(explorer_model::OperationTerminal::Finished) => format!("{summary}｜完成"),
        Some(explorer_model::OperationTerminal::Cancelled) => format!("{summary}｜已取消"),
        Some(explorer_model::OperationTerminal::Failed(error)) => {
            format!("{summary}｜失敗：{}", error.user_message)
        }
        Some(explorer_model::OperationTerminal::Partial { outcomes }) => {
            let succeeded = outcomes
                .iter()
                .filter(|outcome| {
                    matches!(
                        outcome.result,
                        explorer_model::OperationItemResult::Succeeded
                    )
                })
                .count();
            format!("{summary}｜部分完成：{succeeded}/{} 成功", outcomes.len())
        }
    }
}

fn format_transfer_bytes(bytes: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KB", "MB", "GB", "TB"];
    let mut value = bytes as f64;
    let mut unit = 0;
    while value >= 1024.0 && unit + 1 < UNITS.len() {
        value /= 1024.0;
        unit += 1;
    }
    if unit == 0 {
        format!("{bytes} {}", UNITS[unit])
    } else {
        format!("{value:.1} {}", UNITS[unit])
    }
}

fn operation_item_name(item: &explorer_model::ItemDescriptor) -> Option<String> {
    match &item.location {
        explorer_model::LocationDescriptor::FileSystem(path) => path
            .file_name()
            .map(|name| name.to_string_lossy().into_owned()),
        explorer_model::LocationDescriptor::Virtual(location) => {
            location.components.last().cloned()
        }
        _ => None,
    }
}

fn operation_outcome_destination_text(
    outcome: &explorer_model::OperationItemOutcome,
    append_item_name: bool,
) -> String {
    let Some(destination) = &outcome.destination else {
        return "未提供目的地".to_owned();
    };
    if !append_item_name {
        return operation_location_text(destination);
    }
    let Some(item) = &outcome.item else {
        return operation_location_text(destination);
    };
    let Some(name) = operation_item_name(item) else {
        return operation_location_text(destination);
    };
    operation_child_location_text(destination, &name)
}

fn operation_outcome_route_text(
    outcome: &explorer_model::OperationItemOutcome,
    append_item_name: bool,
) -> String {
    let source = outcome.item.as_ref().map_or_else(
        || "未提供來源".to_owned(),
        |item| operation_location_text(&item.location),
    );
    format!(
        "{source} → {}",
        operation_outcome_destination_text(outcome, append_item_name)
    )
}

fn operation_outcome_message(
    outcome: &explorer_model::OperationItemOutcome,
    append_item_name: bool,
) -> String {
    let route = operation_outcome_route_text(outcome, append_item_name);
    match &outcome.result {
        explorer_model::OperationItemResult::Succeeded => format!("成功｜{route}"),
        explorer_model::OperationItemResult::Skipped => format!("略過｜{route}"),
        explorer_model::OperationItemResult::Cancelled => format!("已取消｜{route}"),
        explorer_model::OperationItemResult::Partial(error)
        | explorer_model::OperationItemResult::Failed(error) => {
            let status = if matches!(
                &outcome.result,
                explorer_model::OperationItemResult::Partial(_)
            ) {
                "部分完成"
            } else {
                "失敗"
            };
            let native = error
                .native_code
                .map_or_else(String::new, |code| format!("｜錯誤碼 {code}"));
            format!(
                "{status}｜{route}｜{}｜{}{native}",
                error.operation, error.user_message
            )
        }
    }
}

fn operation_message_opacity(terminal: bool, terminal_elapsed: Option<Duration>) -> Option<f32> {
    if !terminal {
        return Some(1.0);
    }
    let elapsed = terminal_elapsed?;
    if elapsed < Duration::from_secs(7) {
        Some(1.0)
    } else if elapsed < Duration::from_secs(8) {
        Some(1.0 - (elapsed - Duration::from_secs(7)).as_secs_f32())
    } else {
        None
    }
}

impl OperationCenter {
    pub const fn new(
        tokens: UiTokens,
        state: OperationCenterViewModel,
        on_action: Option<ActionCallback>,
    ) -> Self {
        Self {
            tokens,
            state,
            on_action,
        }
    }
}

impl RenderOnce for OperationCenter {
    fn render(self, _window: &mut Window, _cx: &mut App) -> impl IntoElement {
        let colors = self.tokens.theme.colors;
        let terminal_elapsed = self.state.latest_operation_terminal_elapsed(Instant::now());
        let latest = self
            .state
            .operation_center()
            .latest()
            .cloned()
            .and_then(|record| {
                operation_message_opacity(record.phase.is_terminal(), terminal_elapsed)
                    .map(|opacity| (record, opacity))
            });
        div()
            .id(OPERATION_CENTER_ID)
            .debug_selector(|| OPERATION_CENTER_ID.to_owned())
            .role(Role::Status)
            .relative()
            .child(region_probe(
                OPERATION_CENTER_ID,
                Some(EXPLORER_WINDOW_ID),
                "normal",
            ))
            .when_some(latest, |element, (record, opacity)| {
                let summary = operation_message(&record);
                let progress_ratio = if matches!(
                    record.terminal,
                    Some(explorer_model::OperationTerminal::Finished)
                ) {
                    Some(1.0)
                } else if let Some(total) = record.progress.total_bytes.filter(|total| *total > 0) {
                    Some((record.progress.completed_bytes as f32 / total as f32).clamp(0.0, 0.99))
                } else if record.progress.total_bytes == Some(0) && record.progress.total_items > 0
                {
                    Some(
                        (record.progress.completed_items as f32
                            / record.progress.total_items as f32)
                            .clamp(0.0, 0.99),
                    )
                } else {
                    None
                };
                let indeterminate =
                    !record.phase.is_terminal() && record.progress.total_bytes.is_none();
                let append_item_name = matches!(
                    record.request.kind,
                    explorer_model::FileOperationKind::Copy { .. }
                        | explorer_model::FileOperationKind::Move { .. }
                );
                let cancel = (!record.phase.is_terminal()).then_some(semantic_button(
                    "operation-cancel",
                    "Cancel file operation",
                    Some(ExplorerIcon::Close),
                    Some("Cancel"),
                    Some(ExplorerAction::CancelOperation {
                        request_id: record.id,
                    }),
                    true,
                    self.tokens,
                    self.on_action.clone(),
                ));
                let outcome_rows = record
                    .terminal
                    .as_ref()
                    .and_then(|terminal| match terminal {
                        explorer_model::OperationTerminal::Partial { outcomes } => Some(outcomes),
                        _ => None,
                    })
                    .into_iter()
                    .flatten()
                    .take(5)
                    .map(|outcome| {
                        let result = operation_outcome_message(outcome, append_item_name);
                        div()
                            .text_color(colors.text_secondary.to_gpui())
                            .child(result)
                    });
                element
                    .aria_label(summary.clone())
                    .opacity(opacity)
                    .p(px(self.tokens.layout.control_padding_horizontal.value()))
                    .border_b(px(self.tokens.layout.focus_stroke.value()))
                    .border_color(colors.divider.to_gpui())
                    .bg(colors.subtle_surface.to_gpui())
                    .child(summary)
                    .when_some(progress_ratio, |element, ratio| {
                        element.child(
                            div()
                                .id("operation-byte-progress")
                                .absolute()
                                .left(px(0.0))
                                .bottom(px(0.0))
                                .h(px(3.0))
                                .w(relative(ratio))
                                .bg(colors.accent.to_gpui()),
                        )
                    })
                    .when(indeterminate, |element| {
                        element.child(
                            div()
                                .id("operation-indeterminate-progress")
                                .absolute()
                                .left(px(0.0))
                                .bottom(px(0.0))
                                .h(px(3.0))
                                .w_full()
                                .opacity(0.35)
                                .bg(colors.accent.to_gpui()),
                        )
                    })
                    .when_some(cancel, ParentElement::child)
                    .children(outcome_rows)
            })
    }
}

#[derive(IntoElement)]
pub struct StatusBar {
    tokens: UiTokens,
    state: StatusBarViewModel,
    folder_size_backend_status: Option<String>,
    on_action: Option<ActionCallback>,
}

impl StatusBar {
    pub fn new(
        tokens: UiTokens,
        state: StatusBarViewModel,
        folder_size_backend_status: Option<String>,
        on_action: Option<ActionCallback>,
    ) -> Self {
        Self {
            tokens,
            state,
            folder_size_backend_status,
            on_action,
        }
    }
}

impl RenderOnce for StatusBar {
    fn render(self, _window: &mut Window, _cx: &mut App) -> impl IntoElement {
        let layout = self.tokens.layout;
        let colors = self.tokens.theme.colors;
        let presentation = self.state.active_presentation();
        let tab = self.state.tabs().active_tab();
        let index_unavailable = tab.search_sources.iter().any(|source| {
            source.backend == explorer_model::SearchBackend::WindowsIndex
                && source.phase == explorer_model::SearchSourcePhase::Unavailable
        });
        let phase = match &tab.search {
            TabSearchState::Loading { .. } if index_unavailable => {
                "Searching (filesystem fallback; index unavailable) · "
            }
            TabSearchState::Loading { .. } => "Searching (indexed + fallback) · ",
            TabSearchState::Partial { .. } => "Partial search results · ",
            TabSearchState::Error { .. } => "Search error · ",
            TabSearchState::Cancelled { .. } => "Search cancelled · ",
            TabSearchState::Ready { .. } => "Search results · ",
            TabSearchState::Idle | TabSearchState::Editing(_) => match &tab.directory {
                DirectoryState::Loading { .. } => "Loading · ",
                DirectoryState::Error { .. } => "Error · ",
                DirectoryState::Idle | DirectoryState::Ready(_) => "",
            },
        };
        let status = if presentation.selected_count > 0 {
            format!(
                "{} items — {} selected",
                presentation.item_count, presentation.selected_count
            )
        } else {
            format!("{} items", presentation.item_count)
        };
        let operation_status = self
            .state
            .operation_center()
            .records()
            .find(|record| !record.phase.is_terminal())
            .map(|record| {
                let current_name = match &record.request.kind {
                    explorer_model::FileOperationKind::CreateFolder { name, .. }
                    | explorer_model::FileOperationKind::CreateItem { name, .. } => name.clone(),
                    explorer_model::FileOperationKind::Rename { new_name, .. } => new_name.clone(),
                    explorer_model::FileOperationKind::SetUnixMode { item, .. } => {
                        match &item.location {
                            explorer_model::LocationDescriptor::Virtual(location) => location
                                .components
                                .last()
                                .cloned()
                                .unwrap_or_else(|| "item".to_owned()),
                            _ => "item".to_owned(),
                        }
                    }
                    explorer_model::FileOperationKind::Copy { items, .. }
                    | explorer_model::FileOperationKind::Move { items, .. }
                    | explorer_model::FileOperationKind::RecycleDelete { items }
                    | explorer_model::FileOperationKind::PermanentDelete { items, .. }
                    | explorer_model::FileOperationKind::CreateShortcut { items } => items
                        .get(record.progress.completed_items)
                        .and_then(|item| item.location.path())
                        .and_then(std::path::Path::file_name)
                        .map_or_else(
                            || "item".to_owned(),
                            |name| name.to_string_lossy().into_owned(),
                        ),
                };
                format!(
                    "Operation {}/{} · {current_name}",
                    record.progress.completed_items, record.progress.total_items
                )
            });
        let mut full_status = operation_status.map_or_else(
            || format!("{phase}{status}"),
            |operation| format!("{phase}{status} · {operation}"),
        );
        if let Some(error) = self.state.context_menu_error() {
            full_status.push_str(" · ");
            full_status.push_str(&error.user_message);
        }
        if let Some(notice) = self.state.thumbnail_cache_notice() {
            full_status.push_str(" · ");
            full_status.push_str(notice);
        }
        if let Some(notice) = self.state.quick_access_notice() {
            full_status.push_str(" | ");
            full_status.push_str(notice);
        }
        if let Some(notice) = self.state.bookmark_notice() {
            full_status.push_str(" · ");
            full_status.push_str(notice);
        }
        if let Some(notice) = self.state.session_reset_notice() {
            full_status.push_str(" · ");
            full_status.push_str(notice);
        }
        if let Some(backend) = &self.folder_size_backend_status {
            full_status.push_str(" | ");
            full_status.push_str(backend);
        }
        div()
            .id(STATUS_BAR_ID)
            .debug_selector(|| STATUS_BAR_ID.to_owned())
            .role(Role::Status)
            .relative()
            .aria_label(full_status.clone())
            .h(px(layout.status_bar_height.value()))
            .flex_none()
            .flex()
            .items_center()
            .justify_between()
            .px(px(layout.control_padding_horizontal.value()))
            .border_t(px(layout.focus_stroke.value()))
            .border_color(colors.divider.to_gpui())
            .when(
                self.state.focused_surface() == FocusSurface::StatusBar,
                |element| {
                    element
                        .border(px(layout.focus_stroke.value()))
                        .border_color(colors.focus.to_gpui())
                },
            )
            .text_color(colors.text_secondary.to_gpui())
            .child(region_probe(
                STATUS_BAR_ID,
                Some(EXPLORER_WINDOW_ID),
                "normal",
            ))
            .child(typography_probe(
                STATUS_BAR_ID,
                typography_diagnostic(self.tokens, self.tokens.typography.status),
            ))
            .child(full_status)
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(layout.content_spacing.value()))
                    .when_some(self.folder_size_backend_status, |element, status| {
                        element.child(div().id("status-folder-size-backend").child(status))
                    })
                    .child(status_view_button(
                        "status-details-view",
                        "Details view",
                        ExplorerIcon::Details,
                        self.tokens,
                        Some(ExplorerAction::SetViewMode(
                            explorer_model::ViewMode::Details,
                        )),
                        self.on_action.clone(),
                    ))
                    .child(status_view_button(
                        "status-icon-view",
                        "Icon view",
                        ExplorerIcon::View,
                        self.tokens,
                        Some(ExplorerAction::SetViewMode(
                            explorer_model::ViewMode::LargeIcons,
                        )),
                        self.on_action,
                    )),
            )
    }
}

fn status_view_button(
    id: &'static str,
    label: &'static str,
    icon: ExplorerIcon,
    tokens: UiTokens,
    action: Option<ExplorerAction>,
    on_action: Option<ActionCallback>,
) -> impl IntoElement {
    div()
        .id(id)
        .role(Role::Button)
        .relative()
        .aria_label(label)
        .w(px(tokens.layout.status_bar_height.value()))
        .h(px(tokens.layout.status_bar_height.value()))
        .flex()
        .items_center()
        .justify_center()
        .hover(move |style| style.bg(tokens.theme.colors.control_hover.to_gpui()))
        .when_some(action.zip(on_action), |element, (action, callback)| {
            element.on_click(move |_, window, cx| {
                callback(&action, window, cx);
            })
        })
        .child(region_probe(id, Some(STATUS_BAR_ID), "enabled"))
        .child(chrome_icon(id, icon, tokens))
}

fn semantic_button(
    id: &'static str,
    semantic_label: &'static str,
    icon: Option<ExplorerIcon>,
    visible_label: Option<&'static str>,
    action: Option<ExplorerAction>,
    enabled: bool,
    tokens: UiTokens,
    on_action: Option<ActionCallback>,
) -> impl IntoElement {
    semantic_button_with_popup(
        id,
        semantic_label,
        icon,
        visible_label,
        action,
        enabled,
        tokens,
        on_action,
        None,
    )
}

#[allow(clippy::too_many_arguments)]
fn navigation_history_button(
    id: &'static str,
    semantic_label: &'static str,
    icon: ExplorerIcon,
    primary_action: ExplorerAction,
    direction: NavigationHistoryDirection,
    entries: Vec<explorer_model::HistoryEntry>,
    menu_open: bool,
    focused_index: usize,
    enabled: bool,
    tokens: UiTokens,
    menu_focus: Option<gpui::FocusHandle>,
    on_action: Option<&ActionCallback>,
) -> impl IntoElement {
    let colors = tokens.theme.colors;
    let left_callback = on_action.cloned();
    let right_callback = on_action.cloned();
    div()
        .id(id)
        .debug_selector(move || id.to_owned())
        .role(Role::Button)
        .relative()
        .aria_label(semantic_label)
        .h(px(tokens.layout.minimum_hit_target.value()))
        .min_w(px(tokens.layout.minimum_hit_target.value()))
        .flex()
        .items_center()
        .justify_center()
        .rounded(px(tokens.layout.corner_radius.value()))
        .bg(colors.address_fill.to_gpui())
        .text_color(if enabled {
            colors.text_primary.to_gpui()
        } else {
            colors.text_disabled.to_gpui()
        })
        .when(enabled, Styled::cursor_pointer)
        .when(enabled, move |element| {
            element
                .hover(move |style| style.bg(colors.control_hover.to_gpui()))
                .active(move |style| style.bg(colors.control_pressed.to_gpui()))
                .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                // The history popup opens on secondary-button down. Own the matching release as
                // well, otherwise the file-view background sees it and immediately opens a native
                // folder menu over the history entries.
                .on_mouse_up(MouseButton::Right, |_, _, cx| cx.stop_propagation())
                .on_mouse_up_out(MouseButton::Right, |_, _, cx| cx.stop_propagation())
                .when_some(left_callback, move |element, callback| {
                    element.on_click(move |_, window, cx| {
                        callback(&primary_action, window, cx);
                    })
                })
                .when_some(right_callback, move |element, callback| {
                    element.on_mouse_down(MouseButton::Right, move |_, window, cx| {
                        cx.stop_propagation();
                        callback(
                            &ExplorerAction::OpenNavigationHistory { direction },
                            window,
                            cx,
                        );
                    })
                })
        })
        .child(region_probe(
            id,
            None,
            if enabled { "enabled" } else { "disabled" },
        ))
        .child(navigation_history_icon(id, icon, enabled, tokens))
        .child(semantic_tooltip(semantic_label))
        .when(menu_open, |element| {
            element.child(navigation_history_menu(
                tokens,
                direction,
                entries,
                focused_index,
                menu_focus,
                on_action,
            ))
        })
}

fn navigation_history_menu(
    tokens: UiTokens,
    direction: NavigationHistoryDirection,
    entries: Vec<explorer_model::HistoryEntry>,
    focused_index: usize,
    menu_focus: Option<gpui::FocusHandle>,
    on_action: Option<&ActionCallback>,
) -> impl IntoElement {
    let outside = on_action.cloned();
    let menu = div()
        .id(match direction {
            NavigationHistoryDirection::Back => "navigation-back-history-popup",
            NavigationHistoryDirection::Forward => "navigation-forward-history-popup",
        })
        .role(Role::Menu)
        .aria_label(match direction {
            NavigationHistoryDirection::Back => "Back history",
            NavigationHistoryDirection::Forward => "Forward history",
        })
        .occlude()
        .when_some(menu_focus, |menu, focus| menu.track_focus(&focus))
        .min_w(px(tokens.layout.navigation_pane_min_width.value()))
        .max_h(px(crate::layout::feature::NEW_MENU_MAX_HEIGHT.value()))
        .overflow_y_scroll()
        .p(px(tokens.layout.content_spacing.value()))
        .rounded(px(tokens.layout.corner_radius.value()))
        .bg(tokens.theme.colors.menu_fill.to_gpui())
        .border(px(1.0))
        .border_color(tokens.theme.colors.divider.to_gpui())
        .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
        .when_some(outside, |menu, callback| {
            menu.on_mouse_up_out(MouseButton::Left, move |_, window, cx| {
                callback(&ExplorerAction::CloseNavigationHistory, window, cx);
            })
        })
        .children(entries.into_iter().enumerate().map(|(index, entry)| {
            let label = if entry.display_title.trim().is_empty() {
                entry
                    .location
                    .path()
                    .map_or_else(|| "Location".to_owned(), |path| path.display().to_string())
            } else {
                entry.display_title
            };
            let accessible_label = entry.location.path().map_or_else(
                || label.clone(),
                |path| format!("{label}, {}", path.display()),
            );
            let action = ExplorerAction::ActivateNavigationHistory {
                direction,
                steps: index + 1,
            };
            let callback = on_action.cloned();
            div()
                .id(format!("navigation-history-{}", index + 1))
                .role(Role::MenuItem)
                .aria_label(accessible_label)
                .aria_selected(index == focused_index)
                .w_full()
                .h(px(tokens.layout.minimum_hit_target.value()))
                .flex()
                .items_center()
                .gap(px(tokens.layout.content_spacing.value()))
                .px(px(tokens.layout.control_padding_horizontal.value()))
                .rounded(px(tokens.layout.corner_radius.value() / 2.0))
                .when(index == focused_index, |item| {
                    item.bg(tokens.theme.colors.selected_inactive.to_gpui())
                })
                .hover(move |style| style.bg(tokens.theme.colors.selected_inactive.to_gpui()))
                .active(move |style| style.bg(tokens.theme.colors.control_pressed.to_gpui()))
                .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                .when_some(callback, move |item, callback| {
                    let pointer_callback = callback.clone();
                    let accessible_callback = callback.clone();
                    let accessible_action = action.clone();
                    item.on_mouse_move(move |_, window, cx| {
                        pointer_callback(
                            &ExplorerAction::SetNavigationHistoryFocus { index },
                            window,
                            cx,
                        );
                    })
                    .on_click(move |_, window, cx| {
                        cx.stop_propagation();
                        callback(&action, window, cx);
                    })
                    .on_a11y_action(
                        AccessibleAction::Click,
                        move |_, window, cx| {
                            accessible_callback(&accessible_action, window, cx);
                        },
                    )
                })
                .child(navigation_icon(
                    crate::navigation_pane::NavigationIcon::Folder,
                    tokens,
                ))
                .child(label)
        }));
    deferred(
        div()
            .absolute()
            .top(px(tokens.layout.minimum_hit_target.value()))
            .left_0()
            .child(menu),
    )
    .with_priority(160)
}

#[allow(clippy::too_many_arguments)]
fn semantic_button_with_popup(
    id: &'static str,
    semantic_label: &'static str,
    icon: Option<ExplorerIcon>,
    visible_label: Option<&'static str>,
    action: Option<ExplorerAction>,
    enabled: bool,
    tokens: UiTokens,
    on_action: Option<ActionCallback>,
    popup: Option<gpui::AnyElement>,
) -> impl IntoElement {
    let layout = tokens.layout;
    let colors = tokens.theme.colors;
    div()
        .id(id)
        .debug_selector(move || id.to_owned())
        .role(Role::Button)
        .relative()
        .aria_label(semantic_label)
        .h(px(layout.minimum_hit_target.value()))
        .min_w(px(layout.minimum_hit_target.value()))
        .flex()
        .items_center()
        .justify_center()
        .px(px(layout.control_padding_horizontal.value()))
        .rounded(px(layout.corner_radius.value()))
        .bg(if id == SEARCH_BOX_ID {
            colors.search_fill.to_gpui()
        } else {
            colors.address_fill.to_gpui()
        })
        .text_color(if enabled {
            colors.text_primary.to_gpui()
        } else {
            colors.text_disabled.to_gpui()
        })
        .when(enabled, Styled::cursor_pointer)
        .when(enabled, |element| {
            element
                .hover(move |style| style.bg(colors.control_hover.to_gpui()))
                .active(move |style| style.bg(colors.control_pressed.to_gpui()))
                .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
        })
        .when_some(
            enabled.then_some(action).flatten().zip(on_action),
            |element, (action, callback)| {
                element.on_click(move |_, window, cx| callback(&action, window, cx))
            },
        )
        .child(region_probe(
            id,
            None,
            if enabled { "enabled" } else { "disabled" },
        ))
        .child(
            div()
                .flex()
                .items_center()
                .gap(px(layout.content_spacing.value()))
                .when_some(icon, |element, icon| {
                    element.child(chrome_icon(id, icon, tokens))
                })
                .when_some(visible_label, ParentElement::child),
        )
        .child(semantic_tooltip(semantic_label))
        .when_some(popup, ParentElement::child)
}

fn semantic_tooltip(label: &'static str) -> impl IntoElement {
    div().absolute().invisible().child(label)
}

fn right_drag_terminal_menu(
    tokens: UiTokens,
    allowed: explorer_model::TransferEffects,
    on_action: Option<ActionCallback>,
) -> impl IntoElement {
    div()
        .id("right-drag-terminal-menu")
        .role(Role::Menu)
        .aria_label("Right drag action")
        .flex()
        .gap(px(tokens.layout.content_spacing.value()))
        .p(px(tokens.layout.control_padding_horizontal.value()))
        .bg(tokens.theme.colors.subtle_surface.to_gpui())
        .child(semantic_button(
            "right-drag-copy",
            "Copy here",
            None,
            Some("Copy here"),
            Some(ExplorerAction::ResolveRightDrop {
                effect: explorer_model::DragEffect::Copy,
            }),
            allowed.copy,
            tokens,
            on_action.clone(),
        ))
        .child(semantic_button(
            "right-drag-move",
            "Move here",
            None,
            Some("Move here"),
            Some(ExplorerAction::ResolveRightDrop {
                effect: explorer_model::DragEffect::Move,
            }),
            allowed.move_item,
            tokens,
            on_action.clone(),
        ))
        .child(semantic_button(
            "right-drag-cancel",
            "Cancel right drag",
            None,
            Some("Cancel"),
            Some(ExplorerAction::ResolveRightDrop {
                effect: explorer_model::DragEffect::None,
            }),
            true,
            tokens,
            on_action,
        ))
}

fn negotiate_external_paths(
    paths: &gpui::ExternalPaths,
    target_can_write: bool,
    destination: Option<&explorer_model::LocationDescriptor>,
) -> explorer_model::DragEffect {
    let metadata = paths.drop_metadata();
    let allowed = external_transfer_effects(paths);
    let preferred = match metadata.preferred {
        gpui::ExternalDropEffect::None => explorer_model::DragEffect::None,
        gpui::ExternalDropEffect::Copy => explorer_model::DragEffect::Copy,
        gpui::ExternalDropEffect::Move => explorer_model::DragEffect::Move,
        gpui::ExternalDropEffect::Link => explorer_model::DragEffect::Link,
    };
    let modifiers = explorer_model::DragModifiers {
        control: metadata.modifiers.control,
        shift: metadata.modifiers.shift,
        alt: metadata.modifiers.alt,
    };
    let effect = destination.map_or_else(
        || explorer_model::DragEffect::None,
        |destination| match destination {
            explorer_model::LocationDescriptor::Virtual(_) => {
                negotiate_remote_external_effect(allowed, modifiers, target_can_write)
            }
            _ if destination.path().is_none() => {
                explorer_model::negotiate_effect(allowed, preferred, modifiers, target_can_write)
            }
            _ => {
                let Some(destination) = destination.path() else {
                    return explorer_model::DragEffect::None;
                };
                let effect = explorer_model::negotiate_filesystem_drop_effect(
                    allowed,
                    preferred,
                    modifiers,
                    target_can_write,
                    paths.paths(),
                    destination,
                );
                if explorer_model::filesystem_drop_destination_is_valid(
                    paths.paths(),
                    destination,
                    effect,
                ) {
                    effect
                } else {
                    explorer_model::DragEffect::None
                }
            }
        },
    );
    paths.set_negotiated_effect(match effect {
        explorer_model::DragEffect::None => gpui::ExternalDropEffect::None,
        explorer_model::DragEffect::Copy => gpui::ExternalDropEffect::Copy,
        explorer_model::DragEffect::Move => gpui::ExternalDropEffect::Move,
        explorer_model::DragEffect::Link => gpui::ExternalDropEffect::Link,
    });
    effect
}

const fn negotiate_remote_external_effect(
    allowed: explorer_model::TransferEffects,
    modifiers: explorer_model::DragModifiers,
    target_can_write: bool,
) -> explorer_model::DragEffect {
    if !target_can_write {
        explorer_model::DragEffect::None
    } else if modifiers.shift && allowed.move_item {
        explorer_model::DragEffect::Move
    } else if allowed.copy {
        explorer_model::DragEffect::Copy
    } else {
        explorer_model::DragEffect::None
    }
}

fn external_transfer_effects(paths: &gpui::ExternalPaths) -> explorer_model::TransferEffects {
    let allowed = paths.drop_metadata().allowed;
    explorer_model::TransferEffects {
        copy: allowed.copy,
        move_item: allowed.move_item,
        // Link creation is not implemented by the file-operation backend, so the target must not
        // advertise it even when a source offers DROPEFFECT_LINK.
        link: false,
    }
}

#[allow(
    clippy::too_many_arguments,
    reason = "the editable field keeps its semantic label, action, state, and visual tokens explicit"
)]
fn editable_focus_field(
    id: &'static str,
    semantic_label: String,
    visible_text: String,
    action: ExplorerAction,
    focused: bool,
    tokens: UiTokens,
    input: Option<gpui::WeakEntity<EditableTextState>>,
    input_focus: Option<gpui::FocusHandle>,
    on_action: Option<ActionCallback>,
    compact: bool,
) -> gpui::AnyElement {
    let Some(input) = input else {
        return focus_placeholder(
            id,
            semantic_label,
            visible_text,
            action,
            focused,
            tokens,
            on_action,
            compact,
        )
        .into_any_element();
    };
    let layout = tokens.layout;
    let colors = tokens.theme.colors;
    let typography = if id == SEARCH_BOX_ID {
        tokens.typography.search
    } else {
        tokens.typography.address
    };
    let focus_border = if focused {
        layout.focus_stroke.value()
    } else {
        0.0
    };
    let selection_metrics = editable_selection_metrics(
        layout.minimum_hit_target.value(),
        focus_border,
        typography.line_height.value(),
        layout.focus_stroke.value() / 2.0,
    );
    let (input_text, input_selection, input_selection_text, input_caret) =
        editable_input_colors(tokens);
    let field = text_input(id)
        .state(input)
        .multiline(false)
        .placeholder(visible_text)
        .caret_blink_interval_500ms()
        .caret_height(px(typography.line_height.value()))
        .caret_top_offset(px(((typography.line_height.value()
            - typography.size.value())
            / 2.0)
            .max(0.0)))
        .w_full()
        .h(px(selection_metrics.line_height))
        .flex_none()
        .overflow_hidden()
        .text_size(px(typography.size.value()))
        .line_height(px(selection_metrics.line_height))
        .px(px(layout.control_padding_horizontal.value()))
        .py(px(selection_metrics.vertical_padding))
        .when(id == SEARCH_BOX_ID, |element| {
            element
                .pl(px(layout.minimum_hit_target.value()))
                .pr(px(layout.minimum_hit_target.value()))
        })
        .rounded(px(layout.corner_radius.value()))
        .bg(colors.control_fill.to_gpui())
        .text_color(input_text)
        .placeholder_color(colors.text_secondary.to_gpui().into())
        .caret_color(input_caret.into())
        .selection_color(input_selection.into())
        .selection_text_color(input_selection_text.into())
        .marked_color(colors.focus.to_gpui().into())
        .border(px(if focused {
            layout.focus_stroke.value()
        } else {
            0.0
        }))
        .border_color(colors.focus.to_gpui())
        .whitespace_nowrap();
    div()
        .id(format!("{id}-accessibility"))
        .role(Role::TextInput)
        .relative()
        .aria_label(semantic_label)
        .h(px(layout.minimum_hit_target.value()))
        .flex()
        .overflow_hidden()
        .when(id == SEARCH_BOX_ID, |element| element.w_full().flex_none())
        .when(id != SEARCH_BOX_ID, |element| {
            element
                .min_w(px(if compact {
                    layout.compact_address_min_width.value()
                } else {
                    layout.address_min_width.value()
                }))
                .flex_1()
        })
        .when_some(input_focus, |element, focus_handle| {
            element.track_focus(&focus_handle)
        })
        .when_some(on_action, |element, callback| {
            // EditableText consumes left mouse-down while bubbling so it can place its caret.
            // Change the application focus surface during capture, before that consumption,
            // otherwise global file-view shortcuts steal Ctrl+A/paste/Enter from this field.
            element.capture_any_mouse_down(move |event, window, cx| {
                if event.button == MouseButton::Left {
                    callback(&action, window, cx);
                }
            })
        })
        .child(region_probe(id, Some(NAVIGATION_BAR_ID), "editable"))
        .child(typography_probe(
            id,
            typography_diagnostic(
                tokens,
                if id == SEARCH_BOX_ID {
                    tokens.typography.search
                } else {
                    tokens.typography.address
                },
            ),
        ))
        .child(field)
        .into_any_element()
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct EditableSelectionMetrics {
    line_height: f32,
    vertical_padding: f32,
}

fn editable_selection_metrics(
    container_height: f32,
    border_width: f32,
    minimum_line_height: f32,
    desired_inset: f32,
) -> EditableSelectionMetrics {
    let container_height = container_height.max(0.0);
    let border_width = border_width.max(0.0);
    let minimum_line_height = minimum_line_height.max(0.0);
    let desired_inset = desired_inset.max(0.0);
    let available_height = (container_height - border_width * 2.0).max(0.0);
    let (line_height, vertical_padding) =
        if available_height >= minimum_line_height + desired_inset * 2.0 {
            (available_height - desired_inset * 2.0, desired_inset)
        } else if available_height >= minimum_line_height {
            (
                minimum_line_height,
                (available_height - minimum_line_height) / 2.0,
            )
        } else {
            (available_height, 0.0)
        };
    EditableSelectionMetrics {
        line_height,
        vertical_padding,
    }
}

fn editable_input_colors(tokens: UiTokens) -> (gpui::Rgba, gpui::Rgba, gpui::Rgba, gpui::Rgba) {
    let colors = tokens.theme.colors;
    (
        colors.text_primary.to_gpui(),
        colors.selected_active.to_gpui(),
        colors.selected_text.to_gpui(),
        colors.focus.to_gpui(),
    )
}

fn focus_placeholder(
    id: &'static str,
    semantic_label: String,
    visible_text: String,
    action: ExplorerAction,
    focused: bool,
    tokens: UiTokens,
    on_action: Option<ActionCallback>,
    compact: bool,
) -> impl IntoElement {
    let layout = tokens.layout;
    let colors = tokens.theme.colors;
    div()
        .id(id)
        .debug_selector(move || id.to_owned())
        .role(Role::Button)
        .relative()
        .aria_label(semantic_label.clone())
        .h(px(layout.minimum_hit_target.value()))
        .min_w(px(if compact {
            layout.compact_address_min_width.value()
        } else {
            layout.address_min_width.value()
        }))
        .flex_1()
        .flex()
        .items_center()
        .px(px(layout.control_padding_horizontal.value()))
        .rounded(px(layout.corner_radius.value()))
        .bg(colors.control_fill.to_gpui())
        .border(px(if focused {
            layout.focus_stroke.value()
        } else {
            0.0
        }))
        .border_color(colors.focus.to_gpui())
        .cursor_pointer()
        .hover(move |style| style.bg(colors.control_hover.to_gpui()))
        .active(move |style| style.bg(colors.control_pressed.to_gpui()))
        .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
        .when_some(on_action, |element, callback| {
            element.on_click(move |_, window, cx| callback(&action, window, cx))
        })
        .child(region_probe(id, Some(NAVIGATION_BAR_ID), "placeholder"))
        .child(visible_text)
        .child(div().invisible().child(semantic_label))
        .child(
            div()
                .text_color(colors.text_disabled.to_gpui())
                .child("Submit disabled"),
        )
}

/// Owns only title/tab layout and Windows caption hit-test regions.
#[derive(IntoElement)]
pub struct WindowChrome {
    tokens: UiTokens,
    state: WindowChromeViewModel,
    window_active: bool,
    shell_icons: HashMap<explorer_model::ShellIconKey, Arc<RenderImage>>,
    shell_icon_dpi: u16,
    on_action: Option<ActionCallback>,
}

impl WindowChrome {
    pub fn new(
        tokens: UiTokens,
        state: WindowChromeViewModel,
        window_active: bool,
        on_action: Option<ActionCallback>,
    ) -> Self {
        Self {
            tokens,
            state,
            window_active,
            shell_icons: HashMap::new(),
            shell_icon_dpi: 96,
            on_action,
        }
    }

    #[must_use]
    pub fn with_shell_icons(
        mut self,
        shell_icons: HashMap<explorer_model::ShellIconKey, Arc<RenderImage>>,
        shell_icon_dpi: u16,
    ) -> Self {
        self.shell_icons = shell_icons;
        self.shell_icon_dpi = shell_icon_dpi;
        self
    }
}

impl RenderOnce for WindowChrome {
    fn render(self, window: &mut Window, _cx: &mut App) -> impl IntoElement {
        let layout = self.tokens.layout;
        let colors = self.tokens.theme.colors;
        let maximize_icon = if window.is_maximized() {
            ExplorerIcon::Restore
        } else {
            ExplorerIcon::Maximize
        };
        let active_tab_id = self.state.tabs().active_tab_id();
        let shell_icon_theme = match self.tokens.theme.mode {
            crate::theme::ThemeMode::Light => explorer_model::ShellIconTheme::Light,
            crate::theme::ThemeMode::Dark => explorer_model::ShellIconTheme::Dark,
        };
        let generic_shell_icon = self.shell_icons.iter().find_map(|(key, texture)| {
            is_generic_breadcrumb_folder_icon_key(key).then(|| Arc::clone(texture))
        });
        let tabs: Vec<_> = self
            .state
            .tabs()
            .tabs()
            .iter()
            .map(|tab| {
                let current = tab.history.current();
                let title = current.map_or_else(
                    || "Untitled".to_owned(),
                    |entry| entry.display_title.clone(),
                );
                let shell_icon = current.and_then(|entry| {
                    breadcrumb_location_shell_texture(
                        &self.shell_icons,
                        &entry.location,
                        shell_icon_theme,
                        self.shell_icon_dpi,
                    )
                });
                explorer_tab(
                    self.tokens,
                    tab.id,
                    title,
                    shell_icon,
                    generic_shell_icon.clone(),
                    tab.id == active_tab_id,
                    self.on_action.clone(),
                )
            })
            .collect();

        div()
            .id(WINDOW_CHROME_ID)
            .relative()
            .h(px(layout.title_tab_height.value()))
            .flex_none()
            .flex()
            .items_center()
            .when(
                self.state.focused_surface() == FocusSurface::WindowChrome,
                |element| {
                    element
                        .border(px(layout.focus_stroke.value()))
                        .border_color(colors.focus.to_gpui())
                },
            )
            .bg(colors.subtle_surface.to_gpui())
            .child(region_probe(
                WINDOW_CHROME_ID,
                Some(EXPLORER_WINDOW_ID),
                if self.window_active {
                    "active"
                } else {
                    "inactive"
                },
            ))
            .child(typography_probe(
                WINDOW_CHROME_ID,
                typography_diagnostic(self.tokens, self.tokens.typography.tab),
            ))
            .child(
                div()
                    .absolute()
                    .left_0()
                    .right_0()
                    .bottom_0()
                    .h(px(layout.focus_stroke.value()))
                    .bg(colors.divider.to_gpui()),
            )
            .child(
                div()
                    .id(WINDOW_DRAG_REGION_ID)
                    .relative()
                    .window_control_area(WindowControlArea::Drag)
                    .h_full()
                    .flex_1()
                    .flex()
                    .items_end()
                    .on_mouse_down(MouseButton::Left, |event, window, _| {
                        if event.click_count == 2 {
                            window.zoom_window();
                        } else {
                            window.start_window_move();
                        }
                    })
                    .child(region_probe(
                        WINDOW_DRAG_REGION_ID,
                        Some(WINDOW_CHROME_ID),
                        "normal",
                    ))
                    .child(
                        div()
                            .id(TAB_STRIP_ID)
                            .relative()
                            .h_full()
                            .max_w_full()
                            .flex()
                            .items_end()
                            .overflow_x_scroll()
                            .gap(px(layout.content_spacing.value()))
                            .px(px(layout.control_padding_horizontal.value()))
                            .child(region_probe(TAB_STRIP_ID, Some(WINDOW_CHROME_ID), "normal"))
                            .children(tabs)
                            .child(new_tab_button(self.tokens, self.on_action)),
                    ),
            )
            .child(caption_button(
                CAPTION_MINIMIZE_ID,
                "Minimize",
                ExplorerIcon::Minimize,
                WindowControlArea::Min,
                self.tokens,
                false,
            ))
            .child(caption_button(
                CAPTION_MAXIMIZE_ID,
                "Maximize or restore; Windows Snap Layout available",
                maximize_icon,
                WindowControlArea::Max,
                self.tokens,
                false,
            ))
            .child(caption_button(
                CAPTION_CLOSE_ID,
                "Close",
                ExplorerIcon::Close,
                WindowControlArea::Close,
                self.tokens,
                true,
            ))
    }
}

fn explorer_tab(
    tokens: UiTokens,
    tab_id: TabId,
    title: String,
    shell_icon: Option<Arc<RenderImage>>,
    generic_shell_icon: Option<Arc<RenderImage>>,
    active: bool,
    on_action: Option<ActionCallback>,
) -> impl IntoElement {
    let layout = tokens.layout;
    let colors = tokens.theme.colors;
    let id = if active {
        ACTIVE_TAB_ID.to_owned()
    } else {
        format!("background-tab-{tab_id:?}")
    };
    let debug_id = id.clone();
    let activate = on_action.clone();
    let middle_close = on_action.clone();
    let close = on_action;
    let close_id = if active {
        "active-tab-close".to_owned()
    } else {
        format!("close-tab-{tab_id:?}")
    };
    let icon_id = if active {
        "active-tab-location-icon".to_owned()
    } else {
        format!("background-tab-location-icon-{tab_id:?}")
    };
    let icon_label = format!("{title} folder icon");
    div()
        .id(id.clone())
        .debug_selector(move || debug_id.clone())
        .role(Role::Tab)
        .relative()
        .aria_label(title.clone())
        .aria_selected(active)
        .h(px(layout.minimum_hit_target.value()))
        .min_w(px(layout.navigation_pane_min_width.value()))
        .flex()
        .items_center()
        .justify_between()
        .px(px(layout.control_padding_horizontal.value()))
        .rounded_t(px(layout.corner_radius.value()))
        .bg(tab_background(colors, active).to_gpui())
        .when(!active, move |element| {
            element
                .hover(move |style| style.bg(colors.control_hover.to_gpui()))
                .active(move |style| style.bg(colors.control_pressed.to_gpui()))
        })
        .when(active, move |element| {
            element.child(
                div()
                    .id("active-tab-bottom-occluder")
                    .absolute()
                    .left_0()
                    .right_0()
                    .bottom(px(-layout.focus_stroke.value()))
                    .h(px(layout.focus_stroke.value() * 2.0))
                    .bg(colors.surface.to_gpui()),
            )
        })
        .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
        .on_mouse_down(MouseButton::Middle, |_, _, cx| cx.stop_propagation())
        .when_some(middle_close, |element, callback| {
            element.on_mouse_up(MouseButton::Middle, move |_, window, cx| {
                cx.stop_propagation();
                callback(&ExplorerAction::CloseTab { tab_id }, window, cx);
            })
        })
        .when_some(activate, |element, callback| {
            let accessibility_callback = callback.clone();
            element
                .on_click(move |_, window, cx| {
                    callback(&ExplorerAction::ActivateTab { tab_id }, window, cx);
                })
                .on_a11y_action(AccessibleAction::Click, move |_, window, cx| {
                    accessibility_callback(&ExplorerAction::ActivateTab { tab_id }, window, cx);
                })
        })
        .child(region_probe(
            id.clone(),
            Some(TAB_STRIP_ID),
            if active { "active" } else { "background" },
        ))
        .child(
            div()
                .flex()
                .items_center()
                .gap(px(layout.content_spacing.value()))
                .overflow_hidden()
                .child(
                    div()
                        .id(icon_id.clone())
                        .debug_selector(move || icon_id.clone())
                        .role(Role::Image)
                        .aria_label(icon_label)
                        .child(breadcrumb_shell_icon(
                            shell_icon,
                            generic_shell_icon,
                            tokens,
                        )),
                )
                .child(div().overflow_hidden().whitespace_nowrap().child(title)),
        )
        .child(
            div()
                .id(close_id.clone())
                .role(Role::Button)
                .aria_label("Close tab")
                .px(px(layout.content_spacing.value()))
                .rounded(px(layout.corner_radius.value()))
                .hover(move |style| style.bg(colors.control_hover.to_gpui()))
                .active(move |style| style.bg(colors.control_pressed.to_gpui()))
                .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                .when_some(close, |element, callback| {
                    let accessibility_callback = callback.clone();
                    element
                        .on_click(move |_, window, cx| {
                            cx.stop_propagation();
                            callback(&ExplorerAction::CloseTab { tab_id }, window, cx);
                        })
                        .on_a11y_action(AccessibleAction::Click, move |_, window, cx| {
                            accessibility_callback(
                                &ExplorerAction::CloseTab { tab_id },
                                window,
                                cx,
                            );
                        })
                })
                .child(region_probe(
                    close_id,
                    Some(ACTIVE_TAB_ID),
                    "tab-close-button",
                ))
                .child(chrome_icon(id, ExplorerIcon::Close, tokens)),
        )
}

const fn tab_background(
    colors: crate::theme::SemanticColors,
    selected: bool,
) -> crate::theme::Rgba8 {
    if selected {
        colors.surface
    } else {
        colors.subtle_surface
    }
}

const fn new_tab_button_background(colors: crate::theme::SemanticColors) -> crate::theme::Rgba8 {
    colors.subtle_surface
}

const fn file_row_selection_active(window_active: bool, context_menu_pending: bool) -> bool {
    window_active || context_menu_pending
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct FileRowVisual {
    selection_fill: Option<crate::theme::Rgba8>,
    hover_fill: Option<crate::theme::Rgba8>,
    selection_border: Option<crate::theme::Rgba8>,
    selection_border_width: u8,
    text_color: crate::theme::Rgba8,
}

const fn file_row_visual(
    colors: crate::theme::SemanticColors,
    selected: bool,
    selection_active: bool,
) -> FileRowVisual {
    if selected {
        FileRowVisual {
            selection_fill: Some(if selection_active {
                colors.file_row_selected_active
            } else {
                colors.file_row_selected_inactive
            }),
            hover_fill: None,
            selection_border: Some(if selection_active {
                colors.file_row_focus_outline
            } else {
                colors.divider
            }),
            selection_border_width: 1,
            text_color: colors.file_row_selected_text,
        }
    } else {
        FileRowVisual {
            selection_fill: None,
            hover_fill: Some(colors.file_row_hover),
            selection_border: None,
            selection_border_width: 0,
            text_color: colors.text_primary,
        }
    }
}

fn new_tab_button(tokens: UiTokens, on_action: Option<ActionCallback>) -> impl IntoElement {
    let layout = tokens.layout;
    let colors = tokens.theme.colors;
    div()
        .id(NEW_TAB_BUTTON_ID)
        .debug_selector(|| NEW_TAB_BUTTON_ID.to_owned())
        .role(Role::Button)
        .relative()
        .aria_label("New tab")
        .h(px(layout.minimum_hit_target.value()))
        .w(px(layout.minimum_hit_target.value()))
        .flex()
        .items_center()
        .justify_center()
        .rounded(px(layout.corner_radius.value()))
        .bg(new_tab_button_background(colors).to_gpui())
        .text_color(colors.text_primary.to_gpui())
        .cursor_pointer()
        .hover(move |style| style.bg(colors.control_hover.to_gpui()))
        .active(move |style| style.bg(colors.control_pressed.to_gpui()))
        // The tab strip lives inside the native drag region. Consume mouse-down here so pressing
        // `+` remains a client click instead of beginning a window move before click dispatch.
        .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
        .when_some(on_action, |element, callback| {
            let accessibility_callback = callback.clone();
            element
                .on_click(move |_, window, cx| {
                    callback(&ExplorerAction::NewTab, window, cx);
                })
                .on_a11y_action(AccessibleAction::Click, move |_, window, cx| {
                    accessibility_callback(&ExplorerAction::NewTab, window, cx);
                })
        })
        .child(region_probe(
            NEW_TAB_BUTTON_ID,
            Some(TAB_STRIP_ID),
            "enabled",
        ))
        .child(chrome_icon(NEW_TAB_BUTTON_ID, ExplorerIcon::Add, tokens))
}

fn caption_button(
    id: &'static str,
    semantic_label: &'static str,
    icon: ExplorerIcon,
    area: WindowControlArea,
    tokens: UiTokens,
    is_close: bool,
) -> impl IntoElement {
    let layout = tokens.layout;
    let colors = tokens.theme.colors;
    div()
        .id(id)
        .role(Role::Button)
        .relative()
        .aria_label(semantic_label)
        .window_control_area(area)
        .h_full()
        .w(px(layout.caption_button_width.value()))
        .flex_none()
        .flex()
        .items_center()
        .justify_center()
        .text_color(if is_close {
            colors.danger.to_gpui()
        } else {
            colors.text_primary.to_gpui()
        })
        .hover(move |style| style.bg(colors.caption_hover.to_gpui()))
        .active(move |style| style.bg(colors.control_pressed.to_gpui()))
        .on_a11y_action(AccessibleAction::Click, move |_, window, _| match area {
            WindowControlArea::Min => window.minimize_window(),
            WindowControlArea::Max => window.zoom_window(),
            WindowControlArea::Close => window.remove_window(),
            WindowControlArea::Drag => {}
        })
        .child(region_probe(id, Some(WINDOW_CHROME_ID), "enabled"))
        .child(
            div()
                .relative()
                .top(px(-3.5))
                .child(chrome_icon(id, icon, tokens)),
        )
}

#[cfg(test)]
mod tests {
    use super::{
        ACTIVE_TAB_ID, CAPTION_CLOSE_ID, CAPTION_CONTROL_AREAS, CAPTION_MAXIMIZE_ID,
        CAPTION_MINIMIZE_ID, COMMAND_BAR_ID, EXPLORER_WINDOW_ID, FILE_VIEW_HOST_ID, FileViewState,
        NAVIGATION_BAR_ID, NAVIGATION_PANE_ID, NEW_TAB_BUTTON_ID, RemoteMenuPlacement,
        SEARCH_BOX_ID, STATUS_BAR_ID, TAB_STRIP_ID, WINDOW_CHROME_ID, WINDOW_DRAG_REGION_ID,
        admission_cell_presentation, bookmark_icon, breadcrumb_ancestry_partition,
        breadcrumb_location_shell_texture, builtin_count_display, client_to_screen_point,
        context_menu_visual_tokens, details_name_column_contains, editable_input_colors,
        file_view_background_context_hit, file_view_local_pointer, format_explorer_size,
        is_generic_breadcrumb_folder_icon_key, localized_search_placeholder, marquee_content_rect,
        navigation_item_shell_texture, navigation_shell_texture, new_tab_button_background,
        operation_location_text, operation_message, operation_message_opacity,
        operation_outcome_message, operation_request_summary, remote_context_menu_position,
        remote_menu_commands, select_file_row_shell_icon, tab_background,
    };
    use crate::{UiTokens, actions::ExplorerAction, theme::ThemeTokens};
    use gpui::WindowControlArea;
    use std::{cmp::Ordering, time::Duration};

    #[test]
    fn bookmark_icons_distinguish_local_adb_sftp_and_lua() {
        let local = explorer_model::BookmarkTarget::FolderPath {
            path: "C:\\Work".into(),
        };
        let adb = explorer_model::BookmarkTarget::FolderPath {
            path: "ADB://device/missing".into(),
        };
        let sftp = explorer_model::BookmarkTarget::FilePath {
            path: "sftp://host/missing".into(),
        };
        let lua = explorer_model::BookmarkTarget::LuaScript {
            source: "return 1".into(),
        };
        assert_eq!(bookmark_icon(&local), "🔖");
        assert_eq!(bookmark_icon(&adb), "📱");
        assert_eq!(bookmark_icon(&sftp), "🖥");
        assert_eq!(bookmark_icon(&lua), "⚡");
    }

    fn virtual_location(
        provider: &str,
        authority: &str,
        components: &[&str],
    ) -> explorer_model::LocationDescriptor {
        explorer_model::LocationDescriptor::Virtual(explorer_model::VirtualLocationDescriptor {
            provider_id: provider.to_owned(),
            public_authority: Some(authority.to_owned()),
            container_identity: [1; 16],
            container_generation: 1,
            entry_id: Some(1),
            components: components.iter().map(|value| (*value).to_owned()).collect(),
        })
    }

    fn operation_item(
        id: u8,
        location: explorer_model::LocationDescriptor,
    ) -> explorer_model::ItemDescriptor {
        explorer_model::ItemDescriptor {
            id: explorer_model::ShellItemId::from_provider_bytes([id]).expect("item id"),
            location,
        }
    }

    #[test]
    fn operation_message_formats_local_adb_sftp_and_multiple_sources() {
        let adb = virtual_location("adb", "emulator-5554", &["sdcard", "Download", "a.txt"]);
        let sftp = virtual_location("sftp", "45.32.49.125", &["home", "linuxuser"]);
        assert_eq!(
            operation_location_text(&adb),
            "adb://emulator-5554/sdcard/Download/a.txt"
        );
        assert_eq!(
            operation_location_text(&sftp),
            "sftp://45.32.49.125/home/linuxuser"
        );
        let request = explorer_model::FileOperationRequest {
            kind: explorer_model::FileOperationKind::Copy {
                items: vec![
                    operation_item(
                        1,
                        explorer_model::LocationDescriptor::file_system(r"C:\Downloads\one.txt"),
                    ),
                    operation_item(2, adb),
                ],
                destination: sftp,
            },
            flags: explorer_model::FileOperationFlags::default(),
        };
        let mut record =
            explorer_model::OperationRecord::queued(explorer_common::RequestId::new(), request, 2);
        record.start().expect("operation starts");
        let summary = operation_request_summary(&record);
        assert!(summary.contains("複製 2 個項目"));
        assert!(summary.contains(r"C:\Downloads\one.txt"));
        assert!(summary.contains("另有 1 個項目"));
        assert!(summary.contains("sftp://45.32.49.125/home/linuxuser"));
        assert!(!summary.contains("password"));
    }

    #[test]
    fn operation_message_distinguishes_create_rename_delete_and_terminal_results() {
        let create = explorer_model::FileOperationRequest {
            kind: explorer_model::FileOperationKind::CreateFolder {
                parent: virtual_location("adb", "emulator-5554", &["sdcard", "Download"]),
                name: "New folder".to_owned(),
            },
            flags: explorer_model::FileOperationFlags::default(),
        };
        let mut record =
            explorer_model::OperationRecord::queued(explorer_common::RequestId::new(), create, 1);
        record.start().expect("operation starts");
        assert!(operation_message(&record).contains("進度 0/1"));
        record
            .finish(explorer_model::OperationTerminal::Finished)
            .expect("operation finishes");
        assert_eq!(
            operation_message(&record),
            "新增資料夾｜adb://emulator-5554/sdcard/Download/New folder｜完成"
        );

        for kind in [
            explorer_model::FileOperationKind::Rename {
                item: operation_item(
                    3,
                    explorer_model::LocationDescriptor::file_system(r"C:\old.txt"),
                ),
                new_name: "new.txt".to_owned(),
            },
            explorer_model::FileOperationKind::PermanentDelete {
                items: vec![operation_item(
                    4,
                    virtual_location("sftp", "host", &["tmp", "gone"]),
                )],
                confirmed: true,
            },
            explorer_model::FileOperationKind::CreateShortcut {
                items: vec![operation_item(
                    5,
                    explorer_model::LocationDescriptor::file_system(r"C:\target.txt"),
                )],
            },
        ] {
            let request = explorer_model::FileOperationRequest {
                kind,
                flags: explorer_model::FileOperationFlags::default(),
            };
            let record = explorer_model::OperationRecord::queued(
                explorer_common::RequestId::new(),
                request,
                1,
            );
            assert!(!operation_request_summary(&record).is_empty());
        }
    }

    #[test]
    fn operation_message_opacity_follows_seven_to_eight_second_contract() {
        assert_eq!(operation_message_opacity(false, None), Some(1.0));
        assert_eq!(
            operation_message_opacity(true, Some(Duration::from_millis(6_999))),
            Some(1.0)
        );
        assert_eq!(
            operation_message_opacity(true, Some(Duration::from_millis(7_500))),
            Some(0.5)
        );
        assert_eq!(
            operation_message_opacity(true, Some(Duration::from_secs(8))),
            None
        );
        assert_eq!(operation_message_opacity(true, None), None);
    }

    #[test]
    fn operation_failure_row_includes_source_target_stage_code_and_reason() {
        let error = explorer_common::ExplorerError::new(
            explorer_common::ExplorerErrorKind::Availability,
            "目的地上傳",
            true,
            "adb push failed: device offline",
            "remote transfer failed",
        )
        .with_native_code(17);
        let outcome = explorer_model::OperationItemOutcome {
            item: Some(operation_item(
                91,
                explorer_model::LocationDescriptor::file_system(r"C:\Downloads\report.zip"),
            )),
            destination: Some(virtual_location(
                "adb",
                "emulator-5554",
                &["sdcard", "Download"],
            )),
            result: explorer_model::OperationItemResult::Failed(error),
        };
        assert_eq!(
            operation_outcome_message(&outcome, true),
            r"失敗｜C:\Downloads\report.zip → adb://emulator-5554/sdcard/Download/report.zip｜目的地上傳｜adb push failed: device offline｜錯誤碼 17"
        );
    }

    #[test]
    fn operation_failure_rows_keep_distinct_sftp_reasons_and_fallback() {
        let make_outcome = |id, name: &str, reason: &str| explorer_model::OperationItemOutcome {
            item: Some(operation_item(
                id,
                virtual_location("sftp", "45.32.49.125", &["home", "linuxuser", name]),
            )),
            destination: Some(explorer_model::LocationDescriptor::file_system(
                r"C:\Downloads",
            )),
            result: explorer_model::OperationItemResult::Failed(
                explorer_common::ExplorerError::new(
                    explorer_common::ExplorerErrorKind::Availability,
                    "來源下載",
                    true,
                    reason,
                    "remote transfer failed",
                ),
            ),
        };
        let denied =
            operation_outcome_message(&make_outcome(92, "a.txt", "permission denied"), true);
        let missing = operation_outcome_message(&make_outcome(93, "b.txt", "未提供底層錯誤"), true);
        assert!(denied.contains("sftp://45.32.49.125/home/linuxuser/a.txt"));
        assert!(denied.contains(r"C:\Downloads\a.txt"));
        assert!(denied.contains("permission denied"));
        assert!(missing.contains("b.txt"));
        assert!(missing.contains("未提供底層錯誤"));
        assert_ne!(denied, missing);
        assert!(include_str!("chrome.rs").contains(".take(5)"));
    }

    #[test]
    fn remote_context_menu_position_uses_client_anchor_and_clamps_to_window() {
        assert_eq!(
            remote_context_menu_position(120.0, 80.0, 1_200.0, 800.0),
            (120.0, 80.0)
        );
        assert_eq!(
            remote_context_menu_position(-10.0, -20.0, 1_200.0, 800.0),
            (0.0, 0.0)
        );
        assert_eq!(
            remote_context_menu_position(1_190.0, 790.0, 1_200.0, 800.0),
            (904.0, 380.0)
        );
    }

    #[test]
    fn remote_item_menu_keeps_commands_in_classic_vertical_order() {
        let commands = remote_menu_commands(false, false, Some(3), false);
        let labels = commands
            .iter()
            .map(|command| command.label)
            .collect::<Vec<_>>();
        assert_eq!(
            labels,
            [
                "開啟",
                "下載到下載資料夾",
                "剪下",
                "複製",
                "重新命名",
                "永久刪除…",
                "複製遠端路徑",
                "加入書籤",
                "內容"
            ]
        );
        assert!(
            commands
                .iter()
                .find(|command| command.label == "永久刪除…")
                .expect("delete command")
                .danger
        );
    }

    #[test]
    fn remote_menu_paste_and_background_membership_are_contextual() {
        let item = remote_menu_commands(false, true, Some(3), false);
        assert_eq!(
            item.iter()
                .filter(|command| command.label == "貼上")
                .count(),
            1
        );
        assert_eq!(
            item.iter()
                .find(|command| command.label == "貼上")
                .expect("item paste")
                .placement,
            RemoteMenuPlacement::Text
        );

        let background = remote_menu_commands(true, true, None, false);
        assert_eq!(
            background
                .iter()
                .map(|command| command.label)
                .collect::<Vec<_>>(),
            ["新增資料夾", "新增捷徑", "內容", "貼上"]
        );
        assert!(
            background
                .iter()
                .all(|command| command.placement == RemoteMenuPlacement::Text)
        );
        for item_only in [
            "開啟",
            "剪下",
            "複製",
            "重新命名",
            "永久刪除…",
            "下載到下載資料夾",
            "複製遠端路徑",
            "加入書籤",
        ] {
            assert!(background.iter().all(|command| command.label != item_only));
        }
    }

    #[test]
    fn remote_folder_menu_adds_real_new_tab_path_and_common_commands() {
        let commands = remote_menu_commands(false, false, Some(7), true);
        assert!(commands.iter().any(|command| {
            command.label == "在新分頁開啟"
                && command.action
                    == ExplorerAction::OpenItem {
                        row_index: 7,
                        new_tab: true,
                    }
        }));
        assert!(commands.iter().any(|command| {
            command.label == "新增捷徑"
                && command.action == ExplorerAction::CreateRemoteSymlinkToFolder { row_index: 7 }
        }));
        assert!(commands.iter().any(|command| {
            command.label == "複製遠端路徑" && command.action == ExplorerAction::CopySelectedPaths
        }));
        assert!(commands.iter().any(|command| {
            command.label == "加入書籤" && command.action == ExplorerAction::AddSelectedToBookmarks
        }));
        assert!(commands.iter().any(|command| {
            command.label == "下載到下載資料夾"
                && command.action == ExplorerAction::DownloadSelectedToDownloads
        }));
        assert!(commands.iter().any(|command| {
            command.label == "內容" && command.action == ExplorerAction::ShowPropertiesSelected
        }));
    }

    #[test]
    fn remote_properties_dialog_exposes_metadata_permissions_and_apply_controls() {
        let source = include_str!("chrome.rs");
        let dialog = source
            .split("fn remote_properties_dialog(")
            .nth(1)
            .and_then(|source| source.split("fn remote_permission_toggle(").next())
            .expect("remote properties renderer");
        for required in [
            "remote-properties-dialog",
            "檔案類型：",
            "位置：",
            "大小：",
            "權限：{:04o}",
            "remote-properties-ok",
            "remote-properties-cancel",
            "remote-properties-apply",
        ] {
            assert!(dialog.contains(required), "missing {required}");
        }
        let toggle = source
            .split("fn remote_permission_toggle(")
            .nth(1)
            .expect("remote permission toggle renderer");
        assert!(toggle.contains("ToggleRemotePermission"));
        assert!(toggle.contains("mode & mask != 0"));
    }

    #[test]
    fn remote_windows_menu_keeps_lifecycle_accessibility_and_local_shell_contracts() {
        let production = include_str!("chrome.rs");
        let visual = production
            .split("fn context_menu_visual_tokens(")
            .nth(1)
            .and_then(|source| source.split("fn remote_menu_text_command(").next())
            .expect("remote context menu visual tokens");
        let row = production
            .split("fn remote_menu_text_command(")
            .nth(1)
            .and_then(|source| source.split("fn remote_context_menu(").next())
            .expect("remote context menu row renderer");
        let remote = production
            .split("fn remote_context_menu(")
            .nth(1)
            .and_then(|source| source.split("fn remote_context_menu_position(").next())
            .expect("remote context menu renderer");
        assert!(!remote.contains("remote-context-command-strip"));
        assert!(remote.contains("remote-context-primary-separator"));
        assert!(row.contains(".text_size(px(visual.font_size))"));
        assert!(row.contains(".line_height(px(visual.line_height))"));
        assert!(row.contains(".font_weight(visual.font_weight)"));
        assert!(row.contains(".h(px(visual.row_height))"));
        assert!(row.contains(".w(px(visual.icon_gutter))"));
        assert!(row.contains(".ml(px(visual.icon_left))"));
        assert!(!row.contains(".rounded("));
        assert!(remote.contains(".font_family(tokens.typography.family.primary)"));
        assert!(!remote.contains(".rounded("));
        assert!(visual.contains("WINDOWS_CONTEXT_MENU_LIGHT_PALETTE"));
        assert!(visual.contains("WINDOWS_CONTEXT_MENU_DARK_PALETTE"));
        assert!(visual.contains("WINDOWS_CONTEXT_MENU_VISUAL_METRICS"));
        assert!(visual.contains("offset: point(px(3.0), px(9.0))"));
        assert!(visual.contains("blur_radius: px(12.0)"));
        assert!(visual.contains("spread_radius: px(-4.0)"));
        assert!(remote.contains("Role::Menu"));
        assert!(row.contains(".role(Role::MenuItem)"));
        assert!(row.contains(".on_key_down("));
        assert!(row.contains("\"enter\" | \"space\""));
        assert!(production.contains(".on_a11y_action(AccessibleAction::Click"));
        assert!(remote.contains(".on_mouse_down(MouseButton::Left"));
        assert!(remote.contains(".on_mouse_down(MouseButton::Right"));
        assert!(production.contains("ContextMenuRequest"));
        assert!(production.contains("custom_virtual_parent"));
    }

    #[test]
    fn remote_context_menu_tokens_project_light_dark_and_high_contrast_independently() {
        let light_theme = ThemeTokens::light();
        let dark_theme = ThemeTokens::dark();
        let light = context_menu_visual_tokens(UiTokens {
            theme: light_theme,
            ..UiTokens::default()
        });
        let dark = context_menu_visual_tokens(UiTokens {
            theme: dark_theme,
            ..UiTokens::default()
        });
        assert_ne!(light.surface, dark.surface);
        assert_ne!(light.text, dark.text);
        assert_eq!(light.row_height, dark.row_height);
        assert_eq!(light.icon_size, dark.icon_size);
        assert_eq!(light.width, 282.0);
        assert_eq!(light.row_height, 23.0);
        assert_eq!(light.font_size, 12.0);
        assert_eq!(light.line_height, 16.0);
        assert_eq!(light.font_weight, gpui::FontWeight::NORMAL);
        assert!(light.line_height <= light.row_height);
        assert_eq!(light.row_height - light.line_height, 7.0);
        assert_eq!(light.icon_gutter, 42.0);
        assert_eq!(light.icon_left, 13.0);
        assert_eq!(light.divider_left_inset, 42.0);
        assert_eq!(light.divider_right_inset, 8.0);
        assert_ne!(
            light.surface, dark.surface,
            "light/dark surfaces must differ"
        );

        let high_contrast_theme = ThemeTokens::windows_high_contrast(|role| match role {
            crate::theme::SystemColorRole::Window => crate::theme::Rgba8::opaque(0, 0, 0),
            crate::theme::SystemColorRole::WindowText => crate::theme::Rgba8::opaque(255, 255, 255),
            crate::theme::SystemColorRole::Highlight => crate::theme::Rgba8::opaque(0, 255, 0),
            _ => crate::theme::Rgba8::opaque(128, 128, 128),
        });
        let high_contrast = context_menu_visual_tokens(UiTokens {
            theme: high_contrast_theme,
            ..UiTokens::default()
        });
        assert_eq!(
            high_contrast.text,
            high_contrast_theme.colors.text_primary.to_gpui()
        );
        assert_eq!(
            high_contrast.surface,
            high_contrast_theme.colors.menu_fill.to_gpui()
        );
        assert_eq!(high_contrast.shadow.color.a, 0.0);
        assert_eq!(high_contrast.font_size, light.font_size);
        assert_eq!(high_contrast.line_height, light.line_height);
        assert_eq!(high_contrast.font_weight, light.font_weight);
    }

    #[test]
    fn remote_context_menu_uses_windows_zh_tw_menu_typography_and_fallbacks() {
        let typography = crate::typography::TypographyTokens::WINDOWS_11_ZH_TW;
        assert_eq!(typography.family.primary, "Microsoft JhengHei UI");
        assert_eq!(
            typography.family.fallbacks,
            &["Segoe UI Variable Text", "Segoe UI", "sans-serif"]
        );
        assert_eq!(typography.menu.size.value(), 12.0);
        assert_eq!(typography.menu.line_height.value(), 16.0);
        assert_eq!(typography.menu.weight, 400);

        let visual = context_menu_visual_tokens(UiTokens::default());
        assert_eq!(visual.font_size, typography.menu.size.value());
        assert_eq!(visual.line_height, typography.menu.line_height.value());
        assert_eq!(visual.font_weight, gpui::FontWeight::NORMAL);
        assert_eq!(visual.row_height, 23.0);
        assert_eq!(visual.icon_gutter, 42.0);
        assert_eq!(visual.icon_size, 16.0);
        assert_eq!(visual.icon_left, 13.0);
    }

    #[test]
    fn file_view_background_context_hit_includes_short_listing_empty_space() {
        let origin_x = 240.0;
        let origin_y = 120.0;
        let width = 1_000.0;
        let height = 700.0;
        let header = 32.0;
        assert!(file_view_background_context_hit(
            600.0, 700.0, origin_x, origin_y, width, height, header
        ));
        assert!(!file_view_background_context_hit(
            600.0, 140.0, origin_x, origin_y, width, height, header
        ));
        assert!(!file_view_background_context_hit(
            200.0, 700.0, origin_x, origin_y, width, height, header
        ));
        assert!(!file_view_background_context_hit(
            1_300.0, 700.0, origin_x, origin_y, width, height, header
        ));
        assert!(!file_view_background_context_hit(
            600.0, 900.0, origin_x, origin_y, width, height, header
        ));
    }

    #[test]
    fn background_context_handler_is_owned_by_full_height_viewport() {
        let source = include_str!("chrome.rs");
        let viewport = source
            .find("let rendered = div()")
            .expect("full-height viewport render root");
        let handler = source
            .find(".when_some(background_menu_action")
            .expect("background menu handler");
        let content = source[handler..]
            .find(".child(scroll_content)")
            .map(|offset| handler + offset)
            .expect("viewport owns scroll content");
        assert!(viewport < handler && handler < content);
        assert!(source[handler..content].contains("on_mouse_down(MouseButton::Right"));
        assert!(!source[handler..content].contains("on_mouse_up(MouseButton::Right"));
    }

    #[test]
    fn file_row_shell_icon_selection_is_specific_first_and_container_safe() {
        assert_eq!(
            select_file_row_shell_icon(Some("specific"), Some("generic"), true),
            Some("specific")
        );
        assert_eq!(
            select_file_row_shell_icon(None, Some("generic"), true),
            Some("generic")
        );
        assert_eq!(
            select_file_row_shell_icon(None, Some("generic"), false),
            None
        );
        assert_eq!(select_file_row_shell_icon::<&str>(None, None, true), None);

        let generic_key = crate::navigation_pane::generic_breadcrumb_folder_icon_key(
            explorer_model::ShellIconTheme::Light,
            96,
            1,
        );
        let restored_snapshot =
            std::collections::HashMap::from([(generic_key, "restored-generic")]);
        let restored_generic = restored_snapshot.iter().find_map(|(key, texture)| {
            is_generic_breadcrumb_folder_icon_key(key).then_some(*texture)
        });
        assert_eq!(
            select_file_row_shell_icon(None, restored_generic, true),
            Some("restored-generic")
        );
    }

    #[test]
    fn code_lines_blocked_cells_share_limit_label_but_keep_distinct_reasons() {
        use crate::code_lines_column::FolderAdmissionStateV1;

        assert_eq!(
            admission_cell_presentation(FolderAdmissionStateV1::Unavailable),
            ("Limit", "依賴 File Count，因此未啟動", true)
        );
        assert_eq!(
            admission_cell_presentation(FolderAdmissionStateV1::OverLimit),
            ("Limit", "File Count 超過限制，因此未啟動", true)
        );
        assert_eq!(
            admission_cell_presentation(FolderAdmissionStateV1::Pending),
            ("等待 File Count…", "等待 File Count…", false)
        );
    }

    #[test]
    fn builtin_count_cells_distinguish_exact_unavailable_and_ineligible_rows() {
        assert_eq!(builtin_count_display(true, Some(0)), "0");
        assert_eq!(
            builtin_count_display(true, Some(u64::MAX)),
            u64::MAX.to_string()
        );
        assert_eq!(builtin_count_display(true, None), "—");
        assert_eq!(builtin_count_display(false, Some(42)), "");
        assert_eq!(builtin_count_display(false, None), "");
    }

    #[test]
    fn folder_options_exposes_independent_cache_controls_and_all_usage_sections() {
        let production = include_str!("chrome.rs")
            .split("#[cfg(test)]")
            .next()
            .expect("production source");
        for marker in [
            "Cache usage and limits (updates every second)",
            "updates every second",
            "folder-options-cache-budget-controls",
            "cache-budget-input-",
            "cache-budget-slider-",
            ".w(px(400.0))",
            "\"left\" | \"down\"",
            "\"home\"",
            "\"end\"",
            "Persisted MFT index",
            "Folder aggregates memory",
            "folder-options-mft-service-resources",
            "MFT Service resources",
            "MFT Service 資源",
            "由所有 SuperExplorer 程序共用",
            "磁碟索引會保留",
            "記憶體快取會在 MFT Service 重新啟動後重建",
        ] {
            assert!(
                production.contains(marker),
                "missing cache UI marker: {marker}"
            );
        }

        let mft_group_start = production
            .find("folder-options-mft-service-resources")
            .expect("MFT Service resource group");
        let ttl_row = production
            .find("CacheBudgetIdV1::FolderSizeCacheTtlSeconds")
            .expect("Folder size cache TTL row");
        assert!(
            mft_group_start > ttl_row,
            "the MFT resource group must be assembled separately from the TTL row"
        );
    }

    fn empty_render_image() -> std::sync::Arc<gpui::RenderImage> {
        std::sync::Arc::new(gpui::RenderImage::new(smallvec::SmallVec::<
            [image::Frame; 1],
        >::new()))
    }

    #[test]
    fn dynamic_column_selectors_are_package_namespaced_and_stable() {
        let first = explorer_model::ColumnId::extension("org.example.one", "value").unwrap();
        let second = explorer_model::ColumnId::extension("org.example.two", "value").unwrap();
        assert_eq!(
            super::details_column_selector("details-column", &first),
            "details-column-org.example.one-value"
        );
        assert_ne!(
            super::details_column_selector("details-column", &first),
            super::details_column_selector("details-column", &second)
        );
        assert_ne!(
            super::details_column_selector("details-column-menu", &first),
            super::details_column_selector("details-column", &first)
        );
    }

    #[test]
    fn details_left_press_uses_only_the_name_column_for_item_activation() {
        let leading_padding = 12.0;
        let name_width = 320.0;
        assert!(details_name_column_contains(
            leading_padding + name_width,
            0.0,
            leading_padding,
            name_width,
        ));
        assert!(!details_name_column_contains(
            leading_padding + name_width + 1.0,
            0.0,
            leading_padding,
            name_width,
        ));
        assert!(!details_name_column_contains(
            250.0,
            100.0,
            leading_padding,
            name_width,
        ));
    }

    #[test]
    fn marquee_pointer_uses_actual_laid_out_bounds_at_scaled_dpi() {
        let pointer = (825.0, 477.0);
        let actual_origin = (450.0, 240.0);
        let stale_token_origin = (420.0, 224.0);
        assert_eq!(
            file_view_local_pointer(
                pointer.0,
                pointer.1,
                Some(actual_origin),
                stale_token_origin,
            ),
            (375.0, 237.0),
        );
    }

    #[test]
    fn marquee_overlay_compensates_scroll_content_translation() {
        let rect = marquee_content_rect(40.0, 80.0, 340.0, 280.0, 35.0, 160.0);
        assert_eq!(rect.left, 75.0);
        assert_eq!(rect.top, 240.0);
        assert_eq!(rect.width, 300.0);
        assert_eq!(rect.height, 200.0);
        assert_eq!(rect.left - 35.0, 40.0);
        assert_eq!(rect.top - 160.0, 80.0);
    }

    fn external_paths_with_modifiers(
        source: &str,
        control: bool,
        shift: bool,
    ) -> gpui::ExternalPaths {
        gpui::ExternalPaths::with_metadata(
            [std::path::PathBuf::from(source)].into_iter().collect(),
            gpui::ExternalDropMetadata {
                allowed: gpui::ExternalDropEffects {
                    copy: true,
                    move_item: true,
                    link: false,
                },
                modifiers: gpui::Modifiers {
                    control,
                    shift,
                    ..gpui::Modifiers::default()
                },
                ..gpui::ExternalDropMetadata::default()
            },
        )
    }

    #[test]
    fn left_drag_effect_uses_live_modifiers_and_real_destination_volume() {
        let same_volume = explorer_model::LocationDescriptor::file_system(r"C:\destination");
        let cross_volume = explorer_model::LocationDescriptor::file_system(r"D:\destination");
        let plain = external_paths_with_modifiers(r"C:\source\one.txt", false, false);
        assert_eq!(
            super::negotiate_external_paths(&plain, true, Some(&same_volume)),
            explorer_model::DragEffect::Move
        );
        assert_eq!(
            super::negotiate_external_paths(&plain, true, Some(&cross_volume)),
            explorer_model::DragEffect::Copy
        );
        let control = external_paths_with_modifiers(r"C:\source\one.txt", true, false);
        assert_eq!(
            super::negotiate_external_paths(&control, true, Some(&same_volume)),
            explorer_model::DragEffect::Copy
        );
        let shift = external_paths_with_modifiers(r"C:\source\one.txt", false, true);
        assert_eq!(
            super::negotiate_external_paths(&shift, true, Some(&cross_volume)),
            explorer_model::DragEffect::Move
        );
    }

    #[test]
    fn remote_drop_defaults_to_copy_and_requires_shift_for_move() {
        let destination = virtual_location("sftp", "example", &["home"]);
        let plain = external_paths_with_modifiers(r"C:\source\one.txt", false, false);
        assert_eq!(
            super::negotiate_external_paths(&plain, true, Some(&destination)),
            explorer_model::DragEffect::Copy
        );
        let control = external_paths_with_modifiers(r"C:\source\one.txt", true, false);
        assert_eq!(
            super::negotiate_external_paths(&control, true, Some(&destination)),
            explorer_model::DragEffect::Copy
        );
        let shift = external_paths_with_modifiers(r"C:\source\one.txt", false, true);
        assert_eq!(
            super::negotiate_external_paths(&shift, true, Some(&destination)),
            explorer_model::DragEffect::Move
        );
        assert_eq!(
            super::negotiate_external_paths(&plain, false, Some(&destination)),
            explorer_model::DragEffect::None
        );
    }

    #[test]
    fn shift_row_selection_does_not_suppress_left_drag_candidate() {
        let source = include_str!("chrome.rs");
        let production = source
            .split("#[cfg(test)]")
            .next()
            .expect("production source precedes tests");
        assert!(!production.contains("if !event.modifiers.shift"));
        assert!(production.contains("ExplorerAction::BeginFileDrag"));
    }

    #[test]
    fn navigation_drive_resolves_newest_compatible_epoch_after_exact_key_eviction() {
        let location = explorer_model::LocationDescriptor::file_system(r"D:\");
        let theme = explorer_model::ShellIconTheme::Light;
        let dpi = 144;
        let mut newer = crate::navigation_pane::shell_icon_key(&location, theme, dpi);
        newer.association_generation = 4;
        newer.overlay_generation = 9;
        let newer_texture = empty_render_image();
        let mut icons = std::collections::HashMap::new();
        icons.insert(newer, std::sync::Arc::clone(&newer_texture));

        let resolved = navigation_shell_texture(&icons, &location, theme, dpi)
            .expect("newer same-location drive texture remains usable");
        assert!(std::sync::Arc::ptr_eq(&resolved, &newer_texture));

        let wrong_theme = crate::navigation_pane::shell_icon_key(
            &location,
            explorer_model::ShellIconTheme::Dark,
            dpi,
        );
        icons.clear();
        icons.insert(wrong_theme, empty_render_image());
        assert!(navigation_shell_texture(&icons, &location, theme, dpi).is_none());
    }

    #[test]
    fn breadcrumb_git_folder_reuses_newest_overlay_after_navigation_round_trip() {
        let location =
            explorer_model::LocationDescriptor::file_system(r"D:\fixture\git-working-tree");
        let theme = explorer_model::ShellIconTheme::Light;
        let dpi = 168;
        let exact = crate::navigation_pane::shell_icon_key(&location, theme, dpi);
        let mut tortoise_overlay = exact.clone();
        tortoise_overlay.item_id = Some(
            explorer_model::ShellItemId::from_provider_bytes([91]).expect("stable Git folder id"),
        );
        tortoise_overlay.association_generation = 4;
        tortoise_overlay.overlay_generation = 12;
        let overlay_texture = empty_render_image();
        let icons = std::collections::HashMap::from([(
            tortoise_overlay,
            std::sync::Arc::clone(&overlay_texture),
        )]);

        assert!(
            !icons.contains_key(&exact),
            "the old exact breadcrumb key was evicted"
        );
        let resolved = breadcrumb_location_shell_texture(&icons, &location, theme, dpi)
            .expect("the compatible TortoiseGit overlay remains visible after returning");
        assert!(std::sync::Arc::ptr_eq(&resolved, &overlay_texture));
    }

    #[test]
    fn navigation_folder_uses_shell_generic_and_never_location_specific_artwork() {
        let location = explorer_model::LocationDescriptor::file_system(r"D:\fixture");
        let specific = empty_render_image();
        let generic = empty_render_image();
        let key = crate::navigation_pane::shell_icon_key(
            &location,
            explorer_model::ShellIconTheme::Light,
            96,
        );
        let icons = std::collections::HashMap::from([(key, specific)]);

        let selected = navigation_item_shell_texture(
            Some(crate::navigation_pane::NavigationIcon::Folder),
            Some(&location),
            &icons,
            Some(&generic),
            explorer_model::ShellIconTheme::Light,
            96,
        )
        .expect("generic Shell folder texture");
        assert!(std::sync::Arc::ptr_eq(&selected, &generic));
        assert!(
            navigation_item_shell_texture(
                Some(crate::navigation_pane::NavigationIcon::Folder),
                Some(&location),
                &icons,
                None,
                explorer_model::ShellIconTheme::Light,
                96,
            )
            .is_none()
        );
    }

    #[test]
    fn navigation_drive_and_folder_missing_shell_pixels_reserve_an_empty_slot() {
        let source = include_str!("chrome.rs");
        let production = source
            .split("#[cfg(test)]")
            .next()
            .expect("production source precedes tests");
        assert!(production.contains("NavigationIcon::Drive\n                    | crate::navigation_pane::NavigationIcon::Folder"));
        assert!(production.contains("generic_folder_texture.cloned()"));
        assert!(!production.contains("(true, None, Some(icon)) if matches!(icon"));
    }

    fn compare_file_entries(
        left: &explorer_model::FileEntry,
        right: &explorer_model::FileEntry,
        sort: &explorer_model::SortDescriptor,
    ) -> Ordering {
        let mut snapshot = explorer_model::DirectorySnapshot::default();
        snapshot.upsert(left.clone());
        snapshot.upsert(right.clone());
        let ordered = sorted_file_entries(&snapshot, true, sort);
        let left_position = ordered
            .iter()
            .position(|(_, entry)| entry.id == left.id)
            .expect("left entry is presented");
        let right_position = ordered
            .iter()
            .position(|(_, entry)| entry.id == right.id)
            .expect("right entry is presented");
        left_position.cmp(&right_position)
    }

    fn sorted_file_entries(
        snapshot: &explorer_model::DirectorySnapshot,
        hidden_items: bool,
        sort: &explorer_model::SortDescriptor,
    ) -> Vec<(usize, explorer_model::FileEntry)> {
        let presentation =
            crate::file_view::DirectoryPresentation::build(snapshot, hidden_items, sort.clone());
        presentation
            .ordered_indices()
            .iter()
            .filter_map(|index| {
                presentation
                    .entries()
                    .get(*index)
                    .cloned()
                    .map(|entry| (*index, entry))
            })
            .collect()
    }

    #[test]
    fn chrome_contract_has_stable_unique_test_identifiers() {
        let ids = [
            EXPLORER_WINDOW_ID,
            WINDOW_CHROME_ID,
            WINDOW_DRAG_REGION_ID,
            TAB_STRIP_ID,
            ACTIVE_TAB_ID,
            NEW_TAB_BUTTON_ID,
            CAPTION_MINIMIZE_ID,
            CAPTION_MAXIMIZE_ID,
            CAPTION_CLOSE_ID,
            COMMAND_BAR_ID,
            NAVIGATION_BAR_ID,
            super::ADDRESS_EDITOR_ID,
            SEARCH_BOX_ID,
            NAVIGATION_PANE_ID,
            FILE_VIEW_HOST_ID,
            STATUS_BAR_ID,
        ];
        for (index, id) in ids.iter().enumerate() {
            assert!(!id.is_empty());
            assert!(!ids[..index].contains(id), "duplicate UI id: {id}");
        }
    }

    #[test]
    fn editable_inputs_keep_selected_text_visible_in_every_theme() {
        for theme in [ThemeTokens::light(), ThemeTokens::dark()] {
            let tokens = UiTokens {
                theme,
                ..UiTokens::default()
            };
            let (foreground, selection, selection_text, caret) = editable_input_colors(tokens);
            assert_eq!(foreground, theme.colors.text_primary.to_gpui());
            assert_eq!(caret, theme.colors.focus.to_gpui());
            assert_eq!(selection, theme.colors.selected_active.to_gpui());
            assert_eq!(selection_text, theme.colors.selected_text.to_gpui());
            assert!((selection.a - 1.0).abs() < f32::EPSILON);
        }

        let high_contrast = ThemeTokens::windows_high_contrast(|role| match role {
            crate::theme::SystemColorRole::Window => crate::theme::Rgba8::opaque(0, 0, 0),
            crate::theme::SystemColorRole::WindowText => crate::theme::Rgba8::opaque(255, 255, 255),
            crate::theme::SystemColorRole::Highlight => crate::theme::Rgba8::opaque(0, 255, 0),
            _ => crate::theme::Rgba8::opaque(128, 128, 128),
        });
        let (_, selection, selection_text, _) = editable_input_colors(UiTokens {
            theme: high_contrast,
            ..UiTokens::default()
        });
        assert_eq!(selection, crate::theme::Rgba8::opaque(0, 255, 0).to_gpui());
        assert_eq!(selection_text, high_contrast.colors.selected_text.to_gpui());
    }

    #[test]
    fn editable_selection_metrics_fill_the_address_inner_height_symmetrically() {
        let layout = crate::layout::LayoutTokens::WINDOWS_11;
        let typography = crate::typography::TypographyTokens::WINDOWS_11_ZH_TW.address;
        let height = layout.minimum_hit_target.value();
        let stroke = layout.focus_stroke.value();
        let focused = super::editable_selection_metrics(
            height,
            stroke,
            typography.line_height.value(),
            stroke / 2.0,
        );
        let idle = super::editable_selection_metrics(
            height,
            0.0,
            typography.line_height.value(),
            stroke / 2.0,
        );

        assert!((focused.vertical_padding - 1.0).abs() < f32::EPSILON);
        assert!((focused.line_height - 26.0).abs() < f32::EPSILON);
        assert!(focused.line_height > typography.line_height.value());
        assert!(
            (stroke * 2.0 + focused.vertical_padding * 2.0 + focused.line_height - height).abs()
                < f32::EPSILON
        );
        assert!((idle.vertical_padding - focused.vertical_padding).abs() < f32::EPSILON);
        assert!((idle.line_height - 30.0).abs() < f32::EPSILON);
    }

    #[test]
    fn editable_selection_metrics_clamp_constrained_controls_without_overflow() {
        let constrained = super::editable_selection_metrics(15.0, 2.0, 16.0, 1.0);
        assert!(constrained.vertical_padding.abs() < f32::EPSILON);
        assert!((constrained.line_height - 11.0).abs() < f32::EPSILON);

        let reduced_inset = super::editable_selection_metrics(24.0, 2.0, 19.0, 2.0);
        assert!((reduced_inset.line_height - 19.0).abs() < f32::EPSILON);
        assert!((reduced_inset.vertical_padding - 0.5).abs() < f32::EPSILON);
        assert!(
            (4.0 + reduced_inset.vertical_padding * 2.0 + reduced_inset.line_height - 24.0).abs()
                < f32::EPSILON
        );
    }

    #[test]
    fn address_search_and_rename_render_paths_share_selection_metrics() {
        let source = include_str!("chrome.rs");
        let production = source
            .split("#[cfg(test)]")
            .next()
            .expect("production source precedes tests");
        assert_eq!(production.matches("editable_selection_metrics(").count(), 3);
        assert!(production.contains(".h(px(selection_metrics.line_height))"));
        assert!(production.contains(".line_height(px(selection_metrics.line_height))"));
        assert!(production.contains(".h(px(rename_metrics.line_height))"));
        assert!(production.contains(".line_height(px(rename_metrics.line_height))"));
    }

    #[test]
    fn history_and_delete_dialog_share_neutral_pointer_keyboard_focus_rendering() {
        let source = include_str!("chrome.rs");
        let production = source
            .split("#[cfg(test)]")
            .next()
            .expect("production source precedes tests");

        assert!(production.contains("ExplorerAction::SetNavigationHistoryFocus { index }"));
        assert!(production.contains("ExplorerAction::SetPermanentDeleteDialogFocus { target }"));
        assert!(production.contains("item.bg(tokens.theme.colors.selected_inactive.to_gpui())"));
        assert!(production.contains("button.bg(tokens.theme.colors.selected_inactive.to_gpui())"));
    }

    #[test]
    fn win32_menu_points_preserve_negative_monitor_coordinates_without_dpi_rescaling() {
        let origin = gpui::point(gpui::px(-1_920.0), gpui::px(-180.0));
        let client = gpui::point(gpui::px(240.4), gpui::px(96.6));
        assert_eq!(client_to_screen_point(origin, client), (-1_680, -83));

        for percent in [100_u16, 125, 150, 175, 200] {
            let scale_percent = f32::from(percent);
            let physical_client = gpui::point(
                gpui::px(80.0 * scale_percent / 100.0),
                gpui::px(32.0 * scale_percent / 100.0),
            );
            let expected = (
                -1_920 + i32::from(80 * percent / 100),
                -180 + i32::from(32 * percent / 100),
            );
            assert_eq!(client_to_screen_point(origin, physical_client), expected);
        }
    }

    #[test]
    fn long_unicode_unc_and_namespace_ancestry_keep_current_item_in_narrow_windows() {
        let segments = [
            "server",
            "共享資料夾",
            "很長的 Unicode 層級",
            "archive.zip",
            "目前資料夾",
        ]
        .into_iter()
        .enumerate()
        .map(|(index, name)| explorer_model::BreadcrumbSegment {
            id: explorer_model::BreadcrumbSegmentId(index as u64 + 1),
            display_name: name.to_owned(),
            location: explorer_model::LocationDescriptor::ParsingName(format!("fixture:{index}")),
            icon_hint: explorer_model::BreadcrumbIconHint::Namespace,
            is_container: true,
        })
        .collect::<Vec<_>>();
        let (hidden, visible) = breadcrumb_ancestry_partition(segments.clone(), 720.0);
        assert!(!hidden.is_empty());
        assert_eq!(
            visible.last().map(|segment| segment.display_name.as_str()),
            Some("目前資料夾")
        );
        assert_eq!(hidden.len() + visible.len(), segments.len());
        assert_eq!(
            hidden
                .iter()
                .chain(&visible)
                .map(|segment| segment.id)
                .collect::<Vec<_>>(),
            segments
                .iter()
                .map(|segment| segment.id)
                .collect::<Vec<_>>()
        );

        let (hidden, visible) = breadcrumb_ancestry_partition(segments.clone(), 4_000.0);
        assert!(hidden.is_empty());
        assert_eq!(visible, segments);
    }

    #[test]
    fn breadcrumb_shell_icon_prefers_concrete_then_retains_generic_on_failure() {
        assert_eq!(
            super::select_breadcrumb_shell_icon(Some("concrete"), Some("generic")),
            Some("concrete")
        );
        assert_eq!(
            super::select_breadcrumb_shell_icon::<&str>(None, Some("generic")),
            Some("generic")
        );
        assert_eq!(
            super::select_breadcrumb_shell_icon::<&str>(None, None),
            None
        );
    }

    #[test]
    fn localized_search_hint_uses_resolved_leaf_even_while_address_is_edited() {
        let entry = explorer_model::HistoryEntry::new(
            explorer_model::LocationDescriptor::file_system(r"D:\test\file_explorer_reference"),
            "file_explorer_reference",
        );
        let mut address = explorer_model::AddressBarState::for_entry(&entry);
        assert_eq!(
            localized_search_placeholder(Some(&entry), "file_explorer_reference", &address),
            "搜尋 file_explorer_reference"
        );

        address.enter_editing();
        assert!(address.update_draft(r"C:\temporary-draft".to_owned()));
        assert_eq!(
            localized_search_placeholder(Some(&entry), r"C:\temporary-draft", &address),
            "搜尋 file_explorer_reference",
            "an address draft must not replace the resolved current-folder hint"
        );

        address.resolved_ancestry.clear();
        assert_eq!(
            localized_search_placeholder(Some(&entry), "尚未解析", &address),
            "搜尋 file_explorer_reference"
        );
    }

    #[test]
    fn search_hint_uses_committed_path_leaf_when_title_and_ancestry_are_stale() {
        let entry = explorer_model::HistoryEntry::new(
            explorer_model::LocationDescriptor::file_system(r"D:\ProgramData\QuarkCloudDrive"),
            "D:",
        );
        let stale_root = explorer_model::HistoryEntry::new(
            explorer_model::LocationDescriptor::file_system(r"D:\"),
            "D:",
        );
        let address = explorer_model::AddressBarState::for_entry(&stale_root);

        assert_eq!(
            localized_search_placeholder(Some(&entry), "D:", &address),
            "搜尋 QuarkCloudDrive"
        );
    }

    #[test]
    fn search_hint_keeps_drive_root_and_namespace_fallbacks() {
        let drive = explorer_model::HistoryEntry::new(
            explorer_model::LocationDescriptor::file_system(r"D:\"),
            "D:",
        );
        let drive_address = explorer_model::AddressBarState::for_entry(&drive);
        assert_eq!(
            localized_search_placeholder(Some(&drive), "D:", &drive_address),
            "搜尋 D:"
        );

        let namespace = explorer_model::HistoryEntry::new(
            explorer_model::LocationDescriptor::ParsingName("shell:Downloads".to_owned()),
            "下載",
        );
        let namespace_address = explorer_model::AddressBarState::for_entry(&namespace);
        assert_eq!(
            localized_search_placeholder(Some(&namespace), "下載", &namespace_address),
            "搜尋 下載"
        );
    }

    fn sortable_entry(
        id: u8,
        name: &str,
        is_container: bool,
        modified: Option<u64>,
        size: Option<u64>,
        item_type: Option<&str>,
    ) -> explorer_model::FileEntry {
        explorer_model::FileEntry {
            id: explorer_model::ShellItemId::from_provider_bytes([id]).expect("stable id"),
            display_name: name.to_owned(),
            location: explorer_model::LocationDescriptor::file_system(format!(
                r"C:\fixture\{name}"
            )),
            is_container,
            metadata: explorer_model::FileEntryMetadata {
                modified_display: modified.map(|value| value.to_string()),
                modified_sort_key: modified,
                size_bytes: size,
                type_display: item_type.map(str::to_owned),
                ..explorer_model::FileEntryMetadata::default()
            },
        }
    }

    #[test]
    fn details_sort_is_typed_stable_and_keeps_missing_values_last() {
        let folder = sortable_entry(1, "folder", true, None, None, Some("File folder"));
        let small = sortable_entry(2, "small.txt", false, Some(10), Some(9), Some("Text"));
        let large = sortable_entry(3, "large.bin", false, Some(20), Some(100), Some("Binary"));
        let missing = sortable_entry(4, "missing", false, None, None, None);
        let size_desc = explorer_model::SortDescriptor {
            column: explorer_model::ColumnId::Size,
            direction: explorer_model::SortDirection::Descending,
        };
        assert_eq!(
            compare_file_entries(&folder, &large, &size_desc),
            Ordering::Less
        );
        assert_eq!(
            compare_file_entries(&large, &small, &size_desc),
            Ordering::Less
        );
        assert_eq!(
            compare_file_entries(&missing, &small, &size_desc),
            Ordering::Greater
        );

        let date_asc = explorer_model::SortDescriptor {
            column: explorer_model::ColumnId::DateModified,
            direction: explorer_model::SortDirection::Ascending,
        };
        assert_eq!(
            compare_file_entries(&small, &large, &date_asc),
            Ordering::Less
        );
        assert_eq!(
            compare_file_entries(&missing, &large, &date_asc),
            Ordering::Greater
        );
    }

    #[test]
    fn details_horizontal_overflow_uses_the_sum_of_owned_column_widths() {
        let registry = explorer_model::ColumnRegistry::built_ins();
        let mut settings = explorer_model::ViewSettings {
            mode: explorer_model::ViewMode::Details,
            ..explorer_model::ViewSettings::default()
        };
        let _ = settings
            .details_layout
            .set_width(&explorer_model::ColumnId::Name, 700);
        let _ = settings
            .details_layout
            .set_width(&explorer_model::ColumnId::DateModified, 500);
        let _ = settings
            .details_layout
            .set_width(&explorer_model::ColumnId::Type, 400);
        let _ = settings
            .details_layout
            .set_width(&explorer_model::ColumnId::Size, 300);
        assert!(
            (super::view_item_width_with_registry(&settings, &registry) - 1_900.0).abs()
                < f32::EPSILON
        );
        assert!(
            (super::details_horizontal_maximum_with_registry(&settings, &registry, 1_200.0)
                - 700.0)
                .abs()
                < f32::EPSILON
        );
        assert!(
            super::details_horizontal_maximum_with_registry(&settings, &registry, 2_000.0).abs()
                < f32::EPSILON
        );
        let details_width = super::view_item_width_with_registry(&settings, &registry);
        assert_eq!(
            super::file_row_explicit_width(
                explorer_model::ViewMode::Details,
                false,
                details_width,
                details_width,
                8.0,
            ),
            Some(1_908.0),
            "row paint and hit boundary include the header's visible-divider inset"
        );
        assert_eq!(
            super::file_row_explicit_width(
                explorer_model::ViewMode::List,
                false,
                240.0,
                details_width,
                8.0,
            ),
            None,
            "non-Details linear views keep their full-width row behavior"
        );
        assert_eq!(
            super::file_row_explicit_width(
                explorer_model::ViewMode::Tiles,
                true,
                280.0,
                details_width,
                8.0,
            ),
            Some(280.0),
            "wrapped views retain their existing cell width"
        );
    }

    #[test]
    fn details_horizontal_extent_includes_visible_extension_descriptors() {
        let mut settings = explorer_model::ViewSettings {
            mode: explorer_model::ViewMode::Details,
            ..explorer_model::ViewSettings::default()
        };
        let id = explorer_model::ColumnId::extension("org.example.extent", "metric").unwrap();
        let descriptor = explorer_model::ColumnDescriptor {
            id: id.clone(),
            display_name: "Metric".into(),
            value_type: explorer_model::ColumnValueType::Integer,
            default_width: 240,
            minimum_width: 48,
            maximum_width: 600,
            alignment: explorer_model::ColumnAlignment::End,
            applicability: explorer_model::ColumnApplicability::AllEntries,
            file_systems: explorer_model::ColumnFileSystems::LOCAL,
            sort_semantics: explorer_model::ColumnSortSemantics::Integer,
            cost: explorer_model::ColumnCost::BackgroundBatch,
        };
        let mut registry = explorer_model::ColumnRegistry::built_ins();
        registry
            .replace_package("org.example.extent", [descriptor.clone()])
            .unwrap();
        assert!(settings.details_layout.ensure_descriptor(&descriptor, true));
        let built_ins = explorer_model::ColumnRegistry::built_ins();
        let built_in_width = super::view_item_width_with_registry(&settings, &built_ins);
        assert_eq!(
            super::view_item_width_with_registry(&settings, &registry),
            built_in_width + 240.0
        );
        assert_eq!(
            super::details_horizontal_maximum_with_registry(&settings, &registry, built_in_width),
            240.0
        );
        let extended_width = super::view_item_width_with_registry(&settings, &registry);
        assert_eq!(
            super::file_row_explicit_width(
                explorer_model::ViewMode::Details,
                false,
                extended_width,
                extended_width,
                8.0,
            ),
            Some(built_in_width + 248.0),
            "runtime columns and the visible-divider inset participate in the same row visual boundary"
        );
        assert!(settings.details_layout.set_visible(&id, false));
        assert_eq!(
            super::view_item_width_with_registry(&settings, &registry),
            built_in_width,
            "hiding the runtime column immediately shrinks the shared extent"
        );
    }

    #[test]
    fn live_details_preview_projects_stable_header_and_cell_id_order() {
        use explorer_model::ColumnId::{DateModified, Name, Size, Type};

        let mut state = crate::state::AppViewState::default();
        state.set_details_column_width(DateModified, 111);
        state.set_details_column_width(Type, 222);
        assert!(state.update_details_column_drag_preview(DateModified, Type, 75.0, 0.0, 100.0,));
        let settings = state.view_settings();
        let projected = super::visible_details_column_ids(&settings, state.column_registry());
        assert_eq!(projected[..4], [Name, Type, DateModified, Size]);
        assert_eq!(settings.details_column_width(&Type), 222);
        assert_eq!(settings.details_column_width(&DateModified), 111);

        assert!(state.cancel_details_column_drag());
        let restored = state.view_settings();
        let projected = super::visible_details_column_ids(&restored, state.column_registry());
        assert_eq!(projected[..4], [Name, DateModified, Type, Size]);
    }

    #[test]
    fn root_does_not_cancel_column_drag_for_unrelated_pointer_release() {
        let production = include_str!("chrome.rs")
            .split("#[cfg(test)]")
            .next()
            .expect("production source");
        assert!(!production.contains("let details_drag_cancel = self.on_action.clone();"));
        assert!(!production.contains(".when_some(details_drag_cancel, |element, callback|"));
    }

    #[test]
    fn idle_pointer_capture_leaves_external_ole_mouse_up_for_drop_target() {
        assert!(
            super::captured_left_mouse_up_action(false, None, false, false).is_none(),
            "an idle capture listener must not consume the synthetic MouseUp used by OLE Drop"
        );
        assert!(matches!(
            super::captured_left_mouse_up_action(false, None, true, false),
            Some(ExplorerAction::EndDetailsColumnResize)
        ));
        assert!(matches!(
            super::captured_left_mouse_up_action(false, None, false, true),
            Some(ExplorerAction::EndSidePaneResize)
        ));
    }

    #[test]
    fn details_column_menu_is_bounded_scrollable_and_owns_row_clicks() {
        let production = include_str!("chrome.rs")
            .split("#[cfg(test)]")
            .next()
            .expect("production source");
        let menu = production
            .split("fn details_column_menu(")
            .nth(1)
            .and_then(|source| source.split("fn column_menu_row(").next())
            .expect("details column menu source");
        assert!(menu.contains(".max_h(px(tokens.layout.menu_max_height.value()))"));
        assert!(menu.contains(".bottom(px(tokens.layout.content_spacing.value()))"));
        assert!(menu.contains(".overflow_x_hidden()"));
        assert!(menu.contains(".overflow_y_scroll()"));

        let row = production
            .split("fn column_menu_row(")
            .nth(1)
            .and_then(|source| source.split("fn details_column_selector(").next())
            .expect("details column row source");
        assert!(row.contains("cx.stop_propagation();"));
    }

    #[test]
    fn details_header_defers_cancel_but_commits_drop_synchronously() {
        let production = include_str!("chrome.rs")
            .split("#[cfg(test)]")
            .next()
            .expect("production source");
        let header = production
            .split("fn details_header_column(")
            .nth(1)
            .expect("details header source");
        assert_eq!(
            header.matches("window.defer(cx, move |window, cx|").count(),
            2,
            "outer and nested header hit areas must both defer fallback cancellation"
        );
        assert_eq!(
            header
                .matches("callback(&ExplorerAction::CommitDetailsColumnDrag, window, cx);")
                .count(),
            2,
            "outer and nested header hit areas must both commit valid drops synchronously"
        );
    }

    #[test]
    fn details_header_is_pinned_vertically_for_every_scroll_offset() {
        for vertical_offset in [0.0_f32, -1.0, -24.0, -240.0, -10_000.0] {
            let (_, header_top) = super::details_header_overlay_position((-32.0, vertical_offset));
            assert!(
                header_top.abs() < f32::EPSILON,
                "vertical scroll {vertical_offset} must not alter the fixed header top"
            );
        }
        for horizontal_offset in [0.0_f32, -32.0, -700.0] {
            let (fixed_header_left, fixed_header_top) =
                super::details_header_overlay_position((horizontal_offset, -240.0));
            assert!((fixed_header_left - horizontal_offset).abs() < f32::EPSILON);
            assert!(fixed_header_top.abs() < f32::EPSILON);
        }
    }

    #[test]
    fn explorer_icon_modes_expose_all_twelve_ctrl_wheel_sizes() {
        let layout = crate::layout::LayoutTokens::WINDOWS_11;
        let metrics = |mode, icon_size| {
            let settings = explorer_model::ViewSettings {
                mode,
                icon_size,
                ..explorer_model::ViewSettings::default()
            };
            super::spatial_grid_metrics(&settings, layout)
        };

        for (mode, sizes, stacked) in [
            (explorer_model::ViewMode::SmallIcons, [24, 32, 48], false),
            (explorer_model::ViewMode::MediumIcons, [64, 72, 84], true),
            (explorer_model::ViewMode::LargeIcons, [96, 108, 128], true),
            (
                explorer_model::ViewMode::ExtraLargeIcons,
                [256, 384, 512],
                true,
            ),
        ] {
            for size in sizes {
                let actual = metrics(mode, size);
                assert!((actual.icon_size - f32::from(size)).abs() < f32::EPSILON);
                assert!(actual.cell_width > actual.icon_size);
                assert!(actual.cell_height > actual.icon_size);
                assert!(actual.wrapped);
                assert_eq!(actual.stacked, stacked);
            }
        }

        let details = metrics(explorer_model::ViewMode::Details, 20);
        assert!(!details.wrapped);
        assert!(!details.stacked);

        let content = metrics(explorer_model::ViewMode::Content, 32);
        assert_eq!(
            (content.cell_height, content.icon_size),
            (
                crate::layout::feature::CONTENT_ROW_HEIGHT.value(),
                crate::layout::feature::CONTENT_ICON_SIZE.value(),
            )
        );
        assert!(!content.wrapped && !content.stacked);
        assert!(
            (crate::layout::feature::CONTENT_ROW_DIVIDER_HEIGHT.value() - 1.0).abs() < f32::EPSILON
        );
        let source = include_str!("chrome.rs");
        let production = source.split("#[cfg(test)]").next().unwrap();
        for contract in [
            "修改日期: {modified}",
            "類型: {type_display}",
            "大小: {size_display}",
        ] {
            assert!(
                production.contains(contract),
                "missing Content metadata contract: {contract}"
            );
        }
    }

    #[test]
    fn stacked_icon_tiles_reserve_a_non_overlapping_filename_region() {
        let layout = crate::layout::LayoutTokens::WINDOWS_11;
        let label_height = crate::layout::feature::STACKED_ICON_LABEL_HEIGHT.value();
        let label_gap = crate::layout::feature::STACKED_ICON_LABEL_GAP.value();

        for (mode, sizes) in [
            (explorer_model::ViewMode::MediumIcons, [64, 72, 84]),
            (explorer_model::ViewMode::LargeIcons, [96, 108, 128]),
            (explorer_model::ViewMode::ExtraLargeIcons, [256, 384, 512]),
        ] {
            for icon_size in sizes {
                let settings = explorer_model::ViewSettings {
                    mode,
                    icon_size,
                    ..explorer_model::ViewSettings::default()
                };
                let metrics = super::spatial_grid_metrics(&settings, layout);
                assert!(metrics.stacked);
                let expected_height = f32::from(icon_size) + label_gap + label_height;
                assert!(
                    (metrics.cell_height - expected_height).abs() < f32::EPSILON,
                    "{mode:?} at {icon_size}px must reserve an independent label region"
                );
            }
        }

        let source = include_str!("chrome.rs");
        let production = source.split("#[cfg(test)]").next().unwrap();
        assert!(production.contains("object_fit(ObjectFit::Contain)"));
        assert!(production.contains("STACKED_ICON_LABEL_HEIGHT"));
        assert!(production.contains("overflow_hidden()"));
        assert!(production.contains("whitespace_normal()"));
        assert!(production.contains("text_ellipsis()"));
        assert_eq!(super::stacked_icon_label_lines(false), 2);
        assert_eq!(super::stacked_icon_label_lines(true), 3);
    }

    #[test]
    fn thumbnail_aspect_fit_contains_portrait_landscape_and_square_sources() {
        for (source, expected, source_ratio) in [
            ((120, 480), (30.0, 120.0), 0.25_f32),
            ((480, 120), (120.0, 30.0), 4.0_f32),
            ((240, 240), (120.0, 120.0), 1.0_f32),
        ] {
            let fitted = super::aspect_fit_size(source.0, source.1, 120.0, 120.0);
            assert_eq!(fitted, expected);
            assert!(fitted.0 <= 120.0 && fitted.1 <= 120.0);
            let fitted_ratio = fitted.0 / fitted.1;
            assert!((source_ratio - fitted_ratio).abs() < f32::EPSILON);
        }
        assert_eq!(super::aspect_fit_size(0, 480, 120.0, 120.0), (0.0, 0.0));
    }

    #[test]
    fn thumbnail_edge_fit_uses_full_cell_while_shell_icons_remain_square() {
        let thumbnail_host = super::file_visual_host_size(true, true, 568.0, 512.0);
        assert_eq!(thumbnail_host, (568.0, 512.0));

        let landscape = super::aspect_fit_size(1600, 900, thumbnail_host.0, thumbnail_host.1);
        assert!((landscape.0 - thumbnail_host.0).abs() < f32::EPSILON);
        assert!((landscape.1 - 319.5).abs() < f32::EPSILON);

        let portrait = super::aspect_fit_size(900, 1600, thumbnail_host.0, thumbnail_host.1);
        assert!((portrait.0 - 288.0).abs() < f32::EPSILON);
        assert!((portrait.1 - thumbnail_host.1).abs() < f32::EPSILON);

        let square = super::aspect_fit_size(512, 512, thumbnail_host.0, thumbnail_host.1);
        assert_eq!(square, (512.0, 512.0));

        assert_eq!(
            super::file_visual_host_size(false, true, 568.0, 512.0),
            (512.0, 512.0)
        );
        assert_eq!(
            super::file_visual_host_size(true, false, 568.0, 512.0),
            (512.0, 512.0)
        );
        assert_eq!(
            super::file_visual_host_size(true, true, 20.0, 512.0),
            (20.0, 512.0)
        );
        assert_eq!(
            super::file_visual_host_size(true, true, f32::NAN, 512.0),
            (0.0, 512.0)
        );
    }

    #[test]
    fn this_pc_uses_explorer_specific_geometry_for_details_icons_and_content() {
        let layout = crate::layout::LayoutTokens::WINDOWS_11;
        let details =
            super::this_pc_spatial_grid_metrics(explorer_model::ViewMode::Details, layout);
        assert!((details.cell_width - super::this_pc_details_width()).abs() < f32::EPSILON);
        assert!((details.cell_height - layout.file_row_height.value()).abs() < f32::EPSILON);
        assert!(!details.wrapped && !details.stacked);

        let content =
            super::this_pc_spatial_grid_metrics(explorer_model::ViewMode::Content, layout);
        assert!(
            (content.cell_height - crate::layout::feature::THIS_PC_CONTENT_HEIGHT.value()).abs()
                < f32::EPSILON
        );
        assert!(!content.wrapped && !content.stacked);

        for mode in [
            explorer_model::ViewMode::SmallIcons,
            explorer_model::ViewMode::MediumIcons,
            explorer_model::ViewMode::LargeIcons,
        ] {
            let metrics = super::this_pc_spatial_grid_metrics(mode, layout);
            assert_eq!(
                (metrics.cell_width, metrics.cell_height, metrics.icon_size),
                (
                    crate::layout::feature::THIS_PC_TILE_WIDTH.value(),
                    crate::layout::feature::THIS_PC_TILE_HEIGHT.value(),
                    crate::layout::feature::THIS_PC_TILE_ICON_SIZE.value(),
                )
            );
            assert!(metrics.wrapped && !metrics.stacked);
        }
    }

    #[test]
    fn this_pc_capacity_and_column_contracts_match_explorer() {
        let drive = explorer_model::DriveMetadata {
            kind: explorer_model::DriveKind::Fixed,
            availability: explorer_model::DriveAvailability::Available,
            volume_label: Some("Data".to_owned()),
            filesystem_name: Some("NTFS".to_owned()),
            total_bytes: Some(2 * 1024 * 1024 * 1024 * 1024),
            available_bytes: Some(631 * 1024 * 1024 * 1024),
        };
        let capacity = super::this_pc_drive_capacity_text(&drive);
        assert!(capacity.starts_with("剩餘 631 GB，共 "));

        let source = include_str!("chrome.rs");
        let production = source.split("#[cfg(test)]").next().unwrap();
        for contract in [
            "裝置和磁碟機",
            "本機詳細資料欄位：名稱、類型、大小總計、可用空間",
            "this-pc-content-status",
            "this-pc-capacity-bar",
            "filesystem_name",
        ] {
            assert!(
                production.contains(contract),
                "missing This PC contract: {contract}"
            );
        }
    }

    #[test]
    fn spatial_columns_wrap_row_major_and_compact_height_is_shared() {
        let layout = crate::layout::LayoutTokens::WINDOWS_11;
        let mut settings = explorer_model::ViewSettings {
            mode: explorer_model::ViewMode::SmallIcons,
            ..explorer_model::ViewSettings::default()
        };
        let normal = super::spatial_grid_metrics(&settings, layout);
        assert_eq!(super::spatial_grid_columns(normal, 239.0, 20), 1);
        assert_eq!(super::spatial_grid_columns(normal, 480.0, 20), 2);
        assert_eq!(super::spatial_grid_columns(normal, 1_200.0, 3), 3);

        settings.compact_view = true;
        let compact = super::spatial_grid_metrics(&settings, layout);
        assert!(
            (compact.cell_height - (normal.cell_height - layout.content_spacing.value())).abs()
                < f32::EPSILON
        );
        assert_eq!(super::spatial_grid_columns(compact, 480.0, 20), 2);
    }

    #[test]
    fn pre_layout_icon_viewport_uses_the_same_chrome_height_as_file_rows() {
        let tokens = UiTokens::default();
        let window_height = 811.0;
        let expected = window_height
            - tokens.layout.title_tab_height.value()
            - tokens.layout.address_bar_height.value()
            - tokens.layout.command_bar_height.value();
        assert_eq!(
            super::explorer_file_viewport_height_for_window(window_height, tokens),
            expected
        );
        assert!(expected > tokens.layout.file_row_height.value() * 8.0);
    }

    #[test]
    fn spatial_grid_distributes_complete_rows_without_entering_scrollbar_space() {
        let metrics = super::SpatialGridMetrics {
            cell_width: 200.0,
            cell_height: 160.0,
            icon_size: 128.0,
            wrapped: true,
            stacked: true,
        };

        for (usable_width, expected_width) in [(950.0, 190.0), (1_030.0, 206.0)] {
            let grid = super::spatial_grid_layout(metrics, usable_width, 30);
            assert_eq!(grid.columns, 5);
            assert!((grid.metrics.cell_width - expected_width).abs() < f32::EPSILON);
            assert!(
                ((grid.metrics.cell_width / metrics.cell_width) - 1.0).abs()
                    <= super::SPATIAL_CELL_WIDTH_TOLERANCE
            );
            assert!(
                (grid.metrics.cell_width * grid.columns as f32 - usable_width).abs() < f32::EPSILON
            );
        }

        let narrow = super::spatial_grid_layout(metrics, 175.0, 30);
        assert_eq!(narrow.columns, 1);
        assert_eq!(narrow.metrics.cell_width, 175.0);
        assert!(narrow.metrics.cell_width <= 175.0);

        let incomplete = super::spatial_grid_layout(metrics, 1_030.0, 2);
        assert_eq!(incomplete.columns, 2);
        assert_eq!(incomplete.metrics.cell_width, metrics.cell_width);
    }

    #[test]
    fn inline_rename_field_uses_near_full_height_symmetric_selection_metrics() {
        let layout = crate::layout::LayoutTokens::WINDOWS_11;
        let typography = crate::typography::TypographyTokens::WINDOWS_11_ZH_TW.file_row;
        let height = layout.inline_rename_height.value();
        let metrics = super::editable_selection_metrics(
            height,
            1.0,
            typography.line_height.value(),
            layout.focus_stroke.value() / 2.0,
        );

        assert!(height < layout.file_row_height.value());
        assert!((height - 24.0).abs() < f32::EPSILON);
        assert!((metrics.vertical_padding - 1.0).abs() < f32::EPSILON);
        assert!((metrics.line_height - 20.0).abs() < f32::EPSILON);
        assert!(metrics.line_height > typography.line_height.value());
        assert!(
            (2.0 + metrics.vertical_padding * 2.0 + metrics.line_height - height).abs()
                < f32::EPSILON
        );
        for dpi_scale in [1.0_f32, 1.25, 1.5, 1.75, 2.0] {
            assert!(
                ((1.0 * dpi_scale) * 2.0
                    + (metrics.vertical_padding * dpi_scale) * 2.0
                    + metrics.line_height * dpi_scale
                    - height * dpi_scale)
                    .abs()
                    < f32::EPSILON
            );
        }
    }

    #[test]
    fn hidden_items_toggle_never_reveals_protected_system_items() {
        let mut normal = sortable_entry(1, "normal", false, None, None, None);
        let mut hidden = sortable_entry(2, "hidden", false, None, None, None);
        let mut system = sortable_entry(3, "system", false, None, None, None);
        normal.metadata.filesystem_attributes = 0;
        hidden.metadata.filesystem_attributes = 0x2;
        system.metadata.filesystem_attributes = 0x4;
        let mut snapshot = explorer_model::DirectorySnapshot::default();
        for entry in [normal, hidden, system] {
            snapshot.upsert(entry);
        }
        let sort = explorer_model::SortDescriptor::default();
        let concealed = sorted_file_entries(&snapshot, false, &sort)
            .into_iter()
            .map(|(_, entry)| entry.display_name)
            .collect::<Vec<_>>();
        assert_eq!(concealed, ["normal"]);
        let revealed = sorted_file_entries(&snapshot, true, &sort)
            .into_iter()
            .map(|(_, entry)| entry.display_name)
            .collect::<Vec<_>>();
        assert_eq!(revealed, ["hidden", "normal"]);
    }

    fn sorted_names(
        snapshot: &explorer_model::DirectorySnapshot,
        column: &explorer_model::ColumnId,
        direction: explorer_model::SortDirection,
    ) -> Vec<String> {
        sorted_file_entries(
            snapshot,
            true,
            &explorer_model::SortDescriptor {
                column: column.clone(),
                direction,
            },
        )
        .into_iter()
        .map(|(_, entry)| entry.display_name)
        .collect()
    }

    #[test]
    fn four_column_sort_matrix_covers_metadata_unicode_search_and_watcher_changes() {
        use explorer_model::{ColumnId, DirectorySnapshot, PresentationChange, SortDirection};

        let folder = sortable_entry(1, "資料夾", true, Some(30), None, Some("檔案資料夾"));
        let zero = sortable_entry(2, "Alpha.txt", false, Some(10), Some(0), Some("文字文件"));
        let case_tie = sortable_entry(3, "alpha.TXT", false, Some(10), Some(0), Some("文字文件"));
        let unicode = sortable_entry(
            4,
            "Éclair.bin",
            false,
            Some(20),
            Some(9),
            Some("二進位檔案"),
        );
        let missing = sortable_entry(5, "missing", false, None, None, None);
        let mut snapshot = DirectorySnapshot::default();
        for entry in [
            missing.clone(),
            unicode.clone(),
            case_tie.clone(),
            folder.clone(),
            zero.clone(),
        ] {
            let _ = snapshot.upsert(entry);
        }

        for column in [
            ColumnId::Name,
            ColumnId::DateModified,
            ColumnId::Type,
            ColumnId::Size,
        ] {
            let ascending = sorted_names(&snapshot, &column, SortDirection::Ascending);
            let descending = sorted_names(&snapshot, &column, SortDirection::Descending);
            assert_eq!(ascending.first().map(String::as_str), Some("資料夾"));
            assert_eq!(descending.first().map(String::as_str), Some("資料夾"));
            if column != ColumnId::Name {
                assert_eq!(ascending.last().map(String::as_str), Some("missing"));
                assert_eq!(descending.last().map(String::as_str), Some("missing"));
            }
            assert_eq!(
                sorted_names(&snapshot, &column, SortDirection::Ascending),
                ascending,
                "equal values must resolve to a repeatable name/id tie order"
            );
        }
        assert_eq!(
            sorted_names(&snapshot, &ColumnId::Size, SortDirection::Ascending),
            ["資料夾", "Alpha.txt", "alpha.TXT", "Éclair.bin", "missing"]
        );
        assert_eq!(
            sorted_names(&snapshot, &ColumnId::Size, SortDirection::Descending),
            ["資料夾", "Éclair.bin", "Alpha.txt", "alpha.TXT", "missing"]
        );
        assert_eq!(
            compare_file_entries(
                &zero,
                &case_tie,
                &explorer_model::SortDescriptor {
                    column: ColumnId::Name,
                    direction: SortDirection::Ascending,
                },
            ),
            Ordering::Less,
            "case-insensitive ties use stable Shell identity"
        );

        // Search owns a separate snapshot but traverses the exact same render sorter.
        let mut search_results = DirectorySnapshot::default();
        let _ = search_results.upsert(unicode.clone());
        let _ = search_results.upsert(zero.clone());
        assert_eq!(
            sorted_names(
                &search_results,
                &ColumnId::DateModified,
                SortDirection::Descending
            ),
            ["Éclair.bin", "Alpha.txt"]
        );

        // Watcher refreshes can insert/remove identities without retaining stale presentation order.
        let inserted = sortable_entry(6, "watcher-zero.dat", false, Some(5), Some(0), Some("DAT"));
        assert!(matches!(
            snapshot.upsert(inserted.clone()),
            PresentationChange::Inserted(_)
        ));
        assert_eq!(
            sorted_names(&snapshot, &ColumnId::DateModified, SortDirection::Ascending)[1],
            "watcher-zero.dat"
        );
        assert!(matches!(
            snapshot.remove(&inserted.id),
            PresentationChange::Removed(_)
        ));
        assert!(
            !sorted_names(&snapshot, &ColumnId::Name, SortDirection::Ascending)
                .iter()
                .any(|name| name == "watcher-zero.dat")
        );
    }

    #[test]
    fn file_view_states_have_truthful_terminal_messages() {
        assert_eq!(
            FileViewState::Disconnected.message(),
            "Directory service is not connected"
        );
        for state in [
            FileViewState::Loading,
            FileViewState::Empty,
            FileViewState::Error,
            FileViewState::Ready,
            FileViewState::Disconnected,
        ] {
            assert!(!state.message().is_empty());
        }
    }

    #[test]
    fn address_and_search_are_real_ime_capable_editors() {
        assert!(super::M1_ADDRESS_INPUT_MODE.accepts_ime());
        assert!(super::M1_SEARCH_INPUT_MODE.accepts_ime());
        assert!(!super::PlaceholderInputMode::FocusOnly.accepts_ime());
        let production = include_str!("chrome.rs")
            .split("#[cfg(test)]")
            .next()
            .expect("production source precedes tests");
        let compact_production = production
            .chars()
            .filter(|character| !character.is_whitespace())
            .collect::<String>();
        assert!(compact_production.contains(".caret_height(px(typography.line_height.value()))"));
        assert!(compact_production.contains(
            ".caret_top_offset(px(((typography.line_height.value()-typography.size.value())/2.0).max(0.0)))"
        ));
    }

    #[test]
    fn tab_surfaces_match_content_and_strip_semantics() {
        let colors = ThemeTokens::light().colors;
        assert_eq!(tab_background(colors, true), colors.surface);
        assert_eq!(tab_background(colors, false), colors.subtle_surface);
        assert_eq!(new_tab_button_background(colors), colors.subtle_surface);

        let source = include_str!("chrome.rs");
        let production = source
            .split("#[cfg(test)]")
            .next()
            .expect("production source precedes tests");
        assert!(production.contains("active-tab-bottom-occluder"));
        assert!(production.contains(".on_mouse_up(MouseButton::Middle"));
        assert!(production.contains("ExplorerAction::CloseTab { tab_id }"));
        assert!(production.contains("ExplorerIcon::Add"));
        assert!(!production.contains("let tab_focused ="));
        let tab_renderer = production
            .split("fn explorer_tab(")
            .nth(1)
            .expect("tab renderer exists")
            .split("const fn tab_background")
            .next()
            .expect("tab renderer ends before tab helpers");
        assert!(!tab_renderer.contains("colors.focus"));
        assert!(!tab_renderer.contains(".top_0()"));
        assert!(production.contains("ExplorerAction::SetBreadcrumbMenuFocus { index }"));
        assert!(!production.contains("item.bg(colors.selected_active.to_gpui())"));
    }

    #[test]
    fn production_chrome_does_not_use_unicode_icon_placeholders() {
        let source = include_str!("chrome.rs");
        let production = source
            .split("#[cfg(test)]")
            .next()
            .expect("production source precedes tests");
        for forbidden in [
            "\\u{2190}",
            "\\u{2192}",
            "\\u{2191}",
            "\\u{2026}",
            "\\u{2014}",
            "\\u{25a1}",
            "\\u{00d7}",
            ".child(\"+\")",
            ".child(\"×\")",
        ] {
            assert!(
                !production.contains(forbidden),
                "placeholder returned: {forbidden}"
            );
        }
    }

    #[test]
    fn extension_surfaces_are_backed_by_the_shared_eight_plugin_catalog() {
        let source = include_str!("chrome.rs");
        let production = source.split("#[cfg(test)]").next().unwrap();
        assert!(production.contains("folder-options-extensions-tab"));
        assert!(production.contains("folder-options-extensions-page"));
        assert!(production.contains("extension-author-"));
        assert!(production.contains("OpenExtensionAuthorWebsite"));
        assert!(production.contains("extension-community-"));
        assert!(production.contains("OpenExtensionCommunityWebsite"));
        assert!(production.contains("Release date："));
        assert!(production.contains("extension-command-lua-bulk-folder-button"));
        assert!(production.contains("view-extension-size-map"));
        assert!(
            production
                .matches(
                    "view_settings.mode == explorer_model::ViewMode::Details && !has_size_map_plan"
                )
                .count()
                >= 4,
            "Size Map must suppress the Details header, spacer, scrolling, and row chrome"
        );
        let menu = production
            .split("fn command_extensions_menu(")
            .nth(1)
            .expect("extensions menu exists")
            .split("\nfn ")
            .next()
            .expect("extensions menu has a bounded renderer");
        assert!(menu.contains(".w(px(400.0))"));
    }

    #[test]
    fn folder_options_window_keeps_its_footer_visible_and_scrolls_the_active_page() {
        let source = include_str!("chrome.rs");
        let production = source.split("#[cfg(test)]").next().unwrap();
        let dialog = production
            .split("fn folder_options_window_content(")
            .nth(1)
            .expect("folder options window content exists")
            .split("fn about_dialog(")
            .next()
            .expect("folder options dialog has a bounded renderer");
        assert!(dialog.contains(".id(\"folder-options-window-content\")"));
        assert!(dialog.contains(".id(\"folder-options-page\")"));
        assert!(dialog.contains(".overflow_y_scroll()"));
        assert!(dialog.contains(".track_scroll(&scroll)"));
        assert!(dialog.contains("cx.stop_propagation()"));
        assert!(!production.contains("folder-options-overlay"));
        for required in [
            "folder-options-general-tab",
            "folder-options-view-tab",
            "folder-options-extensions-tab",
            "folder-options-ok",
            "folder-options-cancel",
            "folder-options-apply",
        ] {
            assert!(
                dialog.contains(required),
                "missing minimum-size action: {required}"
            );
        }
        assert!(dialog.contains(".h(px(crate::layout::folder_options::FOOTER_HEIGHT.value()))\n                        .flex_none()"));

        let extensions_page = production
            .split("fn folder_options_extensions_page(")
            .nth(1)
            .expect("extensions page exists")
            .split("fn folder_options_general_page(")
            .next()
            .expect("extensions page has a bounded renderer");
        assert!(!extensions_page.contains(".overflow_y_scroll()"));
    }

    #[test]
    fn dynamic_numeric_columns_have_framed_progress_bars_and_aligned_values() {
        let source = include_str!("chrome.rs");
        let production = source.split("#[cfg(test)]").next().unwrap();
        for selector in [
            "folder-size-column-",
            "details_column_selector(\"code-lines-column\"",
        ] {
            let start = production.find(selector).expect("dynamic column cell");
            let local = &production[start..production.len().min(start + 4_500)];
            assert!(local.contains(".h_full()"), "{selector} height");
            assert!(
                local.contains(".text_right()"),
                "{selector} numeric alignment"
            );
        }
        for selector in ["folder-size-bar-track-", "\"code-lines-bar-track\""] {
            let start = production.find(selector).expect("progress bar track");
            let local = &production[start..production.len().min(start + 1_500)];
            assert!(local.contains(".border(px(1.0))"), "{selector} frame");
            assert!(
                local.contains(".border_color(colors.divider.to_gpui())"),
                "{selector} visible frame color"
            );
        }
    }

    #[test]
    fn code_lines_column_applies_to_folders_and_files() {
        let descriptor = crate::code_lines_column::code_lines_column_descriptor();
        assert_eq!(
            descriptor.applicability,
            explorer_model::ColumnApplicability::AllEntries
        );
    }

    #[test]
    fn extension_detail_cells_follow_registry_identity_order() {
        let folder = crate::folder_size_column::folder_size_column_descriptor();
        let rust = crate::code_lines_column::code_lines_column_descriptor();
        let lock = crate::code_lines_column::lock_owner_column_descriptor();
        let mut lua = rust.clone();
        lua.id = explorer_model::ColumnId::Extension {
            package_id: "lua-tokei-code-lines-column".to_owned(),
            column_id: crate::code_lines_column::CODE_LINES_COLUMN_ID.to_owned(),
        };
        lua.display_name = "Code lines".to_owned();

        let mut registry = explorer_model::ColumnRegistry::built_ins();
        for descriptor in [&rust, &folder, &lock, &lua] {
            let (owner, _) = descriptor.id.extension_parts().expect("extension identity");
            registry
                .replace_package(owner, [descriptor.clone()])
                .expect("register extension descriptor");
        }

        let ids = super::ordered_detail_extension_column_ids(
            &registry,
            Some(&folder.id),
            [&rust.id, &lock.id, &lua.id],
        );
        assert_eq!(
            ids,
            vec![
                lua.id.clone(),
                folder.id.clone(),
                lock.id.clone(),
                rust.id.clone(),
            ]
        );

        let lua_folder =
            super::ordered_detail_extension_column_ids(&registry, Some(&folder.id), [&lua.id]);
        assert_eq!(lua_folder, vec![lua.id.clone(), folder.id.clone()]);
        let folder_rust =
            super::ordered_detail_extension_column_ids(&registry, Some(&folder.id), [&rust.id]);
        assert_eq!(folder_rust, vec![folder.id.clone(), rust.id.clone()]);

        registry.unregister_package("lua-tokei-code-lines-column");
        let disabled = super::ordered_detail_extension_column_ids(
            &registry,
            Some(&folder.id),
            [&rust.id, &lock.id, &lua.id],
        );
        assert_eq!(
            disabled,
            vec![folder.id.clone(), lock.id.clone(), rust.id.clone()]
        );
        registry
            .replace_package("lua-tokei-code-lines-column", [lua.clone()])
            .expect("re-enable Lua descriptor");
        let reenabled = super::ordered_detail_extension_column_ids(
            &registry,
            Some(&folder.id),
            [&rust.id, &lock.id, &lua.id],
        );
        assert_eq!(reenabled, vec![lua.id, folder.id, lock.id, rust.id]);
    }

    #[test]
    fn extension_detail_projection_ignores_removed_and_unknown_runtimes() {
        let folder = crate::folder_size_column::folder_size_column_descriptor();
        let rust = crate::code_lines_column::code_lines_column_descriptor();
        let mut registry = explorer_model::ColumnRegistry::built_ins();
        registry
            .replace_package(
                crate::folder_size_column::FOLDER_SIZE_COLUMN_PACKAGE_ID,
                [folder.clone()],
            )
            .expect("register folder descriptor");

        let ids =
            super::ordered_detail_extension_column_ids(&registry, Some(&folder.id), [&rust.id]);
        assert_eq!(ids, vec![folder.id]);
    }

    #[test]
    fn about_command_and_dialog_expose_all_build_metadata_fields() {
        let source = include_str!("chrome.rs");
        let production = source.split("#[cfg(test)]").next().unwrap();
        for required in [
            "more-about",
            "about-dialog",
            "版本",
            "編譯日期",
            "Git hash",
            "作者",
            "about-ok",
        ] {
            assert!(production.contains(required), "missing {required}");
        }
    }

    #[test]
    fn browsing_address_has_separate_background_segment_and_chevron_actions() {
        let source = include_str!("chrome.rs");
        let production = source
            .split("#[cfg(test)]")
            .next()
            .expect("production source precedes tests");
        assert!(production.contains("ExplorerAction::EnterAddressEdit"));
        assert!(production.contains("ExplorerAction::ActivateBreadcrumbSegment"));
        assert!(production.contains("ExplorerAction::OpenBreadcrumbChildren"));
        assert!(production.contains("cx.stop_propagation()"));
        assert!(production.contains("fn breadcrumb_chevron_button("));
        assert!(production.contains(".aria_expanded(expanded)"));
        assert!(production.contains("列出 {segment_name} 的子資料夾，載入中"));
        assert!(production.contains("breadcrumb_location_id(&location)"));
        assert!(!production.contains("breadcrumb-child-{index}"));
        assert_eq!(production.matches("breadcrumb_shell_icon(").count(), 7);
        assert!(production.contains("active-tab-location-icon"));
        assert!(
            production.contains("select_breadcrumb_shell_icon(shell_icon, generic_shell_icon)")
        );
        assert!(production.contains("None => div().size(size).flex_none().into_any_element()"));
        assert!(!production.contains("fn breadcrumb_fallback_icon("));
        assert!(!production.contains("fn breadcrumb_location_fallback_icon("));
    }

    #[test]
    fn sort_and_view_menus_are_button_relative_deferred_overlays_with_typed_actions() {
        let source = include_str!("chrome.rs");
        let production = source
            .split("#[cfg(test)]")
            .next()
            .expect("production source precedes tests");
        assert!(production.contains("fn sort_menu("));
        assert!(production.contains("fn view_menu("));
        assert!(production.contains("fn semantic_button_with_popup("));
        assert!(production.contains("self.state.sort_menu_open().then(||"));
        assert!(production.contains("self.state.view_menu_open().then(||"));
        assert!(production.contains(".debug_selector(|| \"sort-menu\".to_owned())"));
        assert!(production.contains(".debug_selector(|| \"view-menu\".to_owned())"));
        assert!(production.contains(".top(px(layout.minimum_hit_target.value()))"));
        assert!(production.contains(".right_0()"));
        for menu_fn in [
            "fn command_extensions_menu(",
            "fn command_more_menu_v2(",
            "fn sort_menu(",
            "fn view_menu(",
        ] {
            let local = production
                .split(menu_fn)
                .nth(1)
                .unwrap_or_else(|| panic!("missing popup builder: {menu_fn}"))
                .split("\nfn ")
                .next()
                .expect("popup builder boundary");
            assert!(local.contains(".left_0()"), "{menu_fn} left anchor");
            assert!(local.contains(".right_0()"), "{menu_fn} right anchor");
            assert!(local.contains(".justify_center()"), "{menu_fn} centered");
        }
        assert!(!production.contains("px(-layout.minimum_hit_target.value() * 6.0)"));
        assert!(!production.contains("px(-layout.minimum_hit_target.value() * 4.0)"));
        for action in [
            "ExplorerAction::SetSortMenuFocus { index:",
            "ExplorerAction::SetViewMenuFocus { index",
            "ExplorerAction::SetMoreMenuFocus { index }",
        ] {
            assert!(
                production.contains(action),
                "missing pointer focus action: {action}"
            );
        }
        assert!(production.contains("item.bg(colors.selected_inactive.to_gpui())"));
        for action in [
            "SetColumnId(explorer_model::ColumnId::Name)",
            "SetColumnId(explorer_model::ColumnId::DateModified)",
            "SetColumnId(explorer_model::ColumnId::Type)",
            "SetColumnId(explorer_model::ColumnId::Size)",
            "SetSortDirection(explorer_model::SortDirection::Ascending)",
            "SetSortDirection(explorer_model::SortDirection::Descending)",
        ] {
            assert!(production.contains(action), "missing menu action: {action}");
        }
    }

    #[test]
    fn more_menu_omits_properties_and_recycle_bin_commands() {
        let source = include_str!("chrome.rs");
        let production = source
            .split("#[cfg(test)]")
            .next()
            .expect("production source precedes tests");
        let menu = production
            .split("fn command_more_menu_v2(")
            .nth(1)
            .expect("more menu builder")
            .split("\nfn ")
            .next()
            .expect("more menu builder boundary");

        for removed_id in ["more-properties", "more-restore", "more-empty-recycle-bin"] {
            assert!(!menu.contains(removed_id), "unexpected item: {removed_id}");
        }
    }

    #[test]
    fn every_command_popup_occludes_rows_and_owns_pointer_and_hover_hit_testing() {
        let source = include_str!("chrome.rs");
        let production = source
            .split("#[cfg(test)]")
            .next()
            .expect("production source precedes tests");
        for popup_id in [
            "command-new-popup",
            "command-more-popup",
            "command-extensions-popup",
            "sort-menu",
            "view-menu",
        ] {
            let id = format!(".id(\"{popup_id}\")");
            let start = production
                .find(&id)
                .unwrap_or_else(|| panic!("missing popup: {popup_id}"));
            let local = &production[start..production.len().min(start + 1_600)];
            assert!(local.contains(".role(Role::Menu)"), "{popup_id} role");
            assert!(local.contains(".occlude()"), "{popup_id} occlusion");
            assert!(
                local.contains("cx.stop_propagation()"),
                "{popup_id} pointer ownership"
            );
        }
        for item_builder in ["fn command_more_item(", "fn view_menu_item("] {
            let start = production
                .find(item_builder)
                .expect("shared menu item builder");
            let local = &production[start..production.len().min(start + 2_200)];
            assert!(local.contains(".role(Role::MenuItem)"));
            assert!(local.contains(".hover("));
            assert!(local.contains("cx.stop_propagation()"));
            assert!(local.contains("callback(&action, window, cx)"));
        }
    }

    #[test]
    fn item_right_click_owns_the_gesture_and_cannot_fall_through_to_background_menu() {
        let source = include_str!("chrome.rs");
        let production = source
            .split("#[cfg(test)]")
            .next()
            .expect("production source precedes tests");
        let ownership_marker = production
            .find("// The row owns the complete right-button gesture.")
            .expect("item right-button ownership contract");
        let start = production[..ownership_marker]
            .rfind(".on_mouse_down(MouseButton::Right")
            .expect("item right-button down handler");
        let end = production[start..]
            .find(".when_some(accessibility_activate")
            .map(|offset| start + offset)
            .expect("end of item pointer handlers");
        let handlers = &production[start..end];

        for (handler, callback) in [
            (".on_mouse_down(MouseButton::Right", "right_callback("),
            (".on_mouse_up(MouseButton::Right", "right_up_callback("),
            (".on_mouse_up_out(MouseButton::Right", "right_out_callback("),
        ] {
            let handler_start = handlers.find(handler).expect("right-button handler");
            let handler_body = &handlers[handler_start..];
            let stop = handler_body
                .find("cx.stop_propagation()")
                .expect("row handler stops propagation");
            let dispatch = handler_body.find(callback).expect("row handler dispatch");
            assert!(
                stop < dispatch,
                "{handler} must stop propagation before dispatching"
            );
        }

        assert_eq!(
            handlers.matches("cx.stop_propagation()").count(),
            3,
            "right down, right up, and right up-out must all remain row-owned"
        );

        let down_start = handlers
            .find(".on_mouse_down(MouseButton::Right")
            .expect("right-button down handler");
        let down_end = handlers[down_start..]
            .find(".on_mouse_move(")
            .map(|offset| down_start + offset)
            .expect("end of right-button down handler");
        let down = &handlers[down_start..down_end];
        let stable_gesture = down
            .find("ExplorerAction::BeginContextItemGesture")
            .expect("right-click captures a stable item identity and gesture candidate");
        assert!(
            down[stable_gesture..].contains("item_id: context_item_id.clone()"),
            "right-click must carry the hit identity instead of reconstructing it from a row index"
        );

        let out_start = handlers
            .find(".on_mouse_up_out(MouseButton::Right")
            .expect("right-button up-out handler");
        let out = &handlers[out_start..];
        let show = out
            .find("ExplorerAction::ShowContextMenu")
            .expect("up-out completes an item context-menu gesture after rerender");
        let cancel = out
            .find("ExplorerAction::CancelFileDrag")
            .expect("up-out clears the right-button drag candidate");
        assert!(
            show < cancel,
            "up-out must request the item menu before clearing its gesture candidate"
        );
    }

    #[test]
    fn background_right_click_stops_nested_host_bubbling_before_dispatch() {
        let source = include_str!("chrome.rs");
        let production = source
            .split("#[cfg(test)]")
            .next()
            .expect("production source precedes tests");
        let marker = production
            .find("// Nested scroll/file-view hosts can both participate")
            .expect("background right-button ownership contract");
        let handler_start = production[..marker]
            .rfind(".on_mouse_up(MouseButton::Right")
            .expect("background right-button handler");
        let handler = &production[handler_start..production.len().min(marker + 700)];
        let stop = handler
            .find("cx.stop_propagation()")
            .expect("background handler stops propagation");
        let dispatch = handler
            .find("menu_callback(")
            .expect("background handler dispatches one menu action");
        assert!(
            stop < dispatch,
            "background bubbling must stop before dispatch"
        );
        assert_eq!(
            handler.matches("menu_callback(").count(),
            1,
            "one physical background release submits exactly one request"
        );
    }

    #[test]
    fn preview_fallback_is_localized_live_and_exposes_one_retry_action_with_properties() {
        let source = include_str!("chrome.rs");
        let production = source
            .split("#[cfg(test)]")
            .next()
            .expect("production source precedes tests");
        for contract in [
            ".id(\"preview-live-status\")",
            ".role(Role::Status)",
            ".id(\"preview-file-properties\")",
            "format_explorer_size",
            "\"重試預覽\"",
            "Some(ExplorerAction::RetryExtensionBroker)",
        ] {
            assert!(
                production.contains(contract),
                "missing preview contract: {contract}"
            );
        }
        assert_eq!(production.matches("\"preview-broker-retry\"").count(), 1);
    }

    #[test]
    fn details_size_uses_adaptive_binary_units() {
        assert_eq!(format_explorer_size(0), "0 KB");
        assert_eq!(format_explorer_size(1), "1.0 KB");
        assert_eq!(format_explorer_size(1024), "1.0 KB");
        assert_eq!(format_explorer_size(1536), "1.5 KB");
        assert_eq!(format_explorer_size(5_427_537_920), "5.1 GB");
        assert_eq!(
            format_explorer_size(250 * 1024_u64.pow(3) + 512 * 1024_u64.pow(2)),
            "250.5 GB"
        );
    }

    #[test]
    fn caption_contract_uses_native_windows_non_client_areas() {
        assert_eq!(
            CAPTION_CONTROL_AREAS,
            [
                WindowControlArea::Drag,
                WindowControlArea::Min,
                WindowControlArea::Max,
                WindowControlArea::Close,
            ]
        );
    }

    #[test]
    fn selected_file_row_remains_visually_active_while_native_context_menu_owns_focus() {
        assert!(super::file_row_selection_active(true, false));
        assert!(super::file_row_selection_active(false, true));
        assert!(!super::file_row_selection_active(false, false));

        let source = include_str!("chrome.rs");
        let production = source
            .split("#[cfg(test)]")
            .next()
            .expect("production source precedes tests");
        assert!(production.contains(".border_color("));
        assert!(production.contains("if selection_active"));
    }

    #[test]
    fn file_row_visual_matches_explorer_and_preserves_accessible_theme_roles() {
        let high_contrast = ThemeTokens::windows_high_contrast(|role| match role {
            crate::theme::SystemColorRole::Window => crate::theme::Rgba8::opaque(0, 0, 0),
            crate::theme::SystemColorRole::WindowText => crate::theme::Rgba8::opaque(255, 255, 255),
            crate::theme::SystemColorRole::Highlight => crate::theme::Rgba8::opaque(0, 255, 0),
            _ => crate::theme::Rgba8::opaque(128, 128, 128),
        });
        for colors in [
            ThemeTokens::light().colors,
            ThemeTokens::dark().colors,
            high_contrast.colors,
        ] {
            let active = super::file_row_visual(colors, true, true);
            assert_eq!(active.selection_fill, Some(colors.file_row_selected_active));
            assert_eq!(active.hover_fill, None);
            assert_eq!(active.selection_border, Some(colors.file_row_focus_outline));
            assert_eq!(active.selection_border_width, 1);
            assert_eq!(active.text_color, colors.file_row_selected_text);

            let inactive = super::file_row_visual(colors, true, false);
            assert_eq!(
                inactive.selection_fill,
                Some(colors.file_row_selected_inactive)
            );
            assert_eq!(inactive.hover_fill, None);
            assert_eq!(inactive.selection_border, Some(colors.divider));
            assert_eq!(inactive.selection_border_width, 1);
            assert_eq!(inactive.text_color, colors.file_row_selected_text);

            let unselected = super::file_row_visual(colors, false, true);
            assert_eq!(unselected.selection_fill, None);
            assert_eq!(unselected.hover_fill, Some(colors.file_row_hover));
            assert_eq!(unselected.selection_border, None);
            assert_eq!(unselected.selection_border_width, 0);
            assert_eq!(unselected.text_color, colors.text_primary);
        }

        let light = ThemeTokens::light().colors;
        assert_eq!(
            light.file_row_hover,
            crate::theme::Rgba8::opaque(229, 243, 255)
        );
        assert_eq!(
            light.file_row_selected_active,
            crate::theme::Rgba8::opaque(204, 232, 255)
        );
        assert_eq!(
            light.file_row_focus_outline,
            crate::theme::Rgba8::opaque(0, 0, 0)
        );

        let dark = ThemeTokens::dark().colors;
        assert_ne!(dark.file_row_hover, light.file_row_hover);
        assert_ne!(dark.file_row_selected_active, dark.surface);
        assert_ne!(dark.file_row_selected_text, dark.file_row_selected_active);
    }

    #[test]
    fn bookmark_overflow_preserves_the_order_partition() {
        assert_eq!(super::bookmark_visible_count(5, 420.0), 2);
        let ordered = ["first", "second", "third", "fourth", "fifth"];
        let visible_count = super::bookmark_visible_count(ordered.len(), 420.0);
        assert_eq!(&ordered[..visible_count], &["first", "second"]);
        assert_eq!(&ordered[visible_count..], &["third", "fourth", "fifth"]);
    }

    #[test]
    fn bookmark_manager_tree_hides_descendants_until_ancestors_expand() {
        let mut bookmarks = explorer_model::Bookmarks::default();
        assert!(bookmarks.begin_add_folder("Parent".into(), None).changed());
        let parent = bookmarks.folders()[0].id;
        assert!(
            bookmarks
                .begin_add_folder("Child".into(), Some(parent))
                .changed()
        );
        let child = bookmarks.folders()[1].clone();
        let mut expanded = std::collections::HashSet::new();
        assert_eq!(
            super::bookmark_manager_folder_depth(&bookmarks, &child, &expanded),
            None
        );
        expanded.insert(parent);
        assert_eq!(
            super::bookmark_manager_folder_depth(&bookmarks, &child, &expanded),
            Some(1)
        );
    }

    #[test]
    fn bookmark_manager_rows_support_native_drag_reordering() {
        let production = include_str!("chrome.rs")
            .split("#[cfg(test)]")
            .next()
            .expect("production source");
        let manager = production
            .split("fn bookmark_manager(")
            .nth(1)
            .and_then(|source| source.split("fn bookmark_editor(").next())
            .expect("bookmark manager source");
        assert!(manager.contains(".on_drag("));
        assert!(manager.contains("element.on_drop(move |drag: &BookmarkDrag"));
        assert!(manager.contains("id: drag.id"));
        assert!(manager.contains("destination: sibling_index"));
        for required in [
            "bookmark-manager-toolbar",
            "管理 (O)⌄",
            "檢視 (V)⌄",
            "bookmark-manager-import",
            "bookmark-manager-backup",
            "ImportBookmarksFromClipboard",
            "BackupBookmarksToClipboard",
            "bookmark-manager-search-input",
            "bookmark-manager-tree",
            "所有書籤",
            "書籤工具列",
            "其他書籤",
            "bookmark-manager-columns",
            "名稱",
            "標籤",
            "網址",
            "bookmark-manager-details",
        ] {
            assert!(
                manager.contains(required),
                "missing Firefox library UI: {required}"
            );
        }
        for interactive in [
            "bookmark-manager-back",
            "bookmark-manager-forward",
            "bookmark-manager-manage-menu",
            "bookmark-manager-view-toggle",
            "bookmark-manager-transfer-menu",
            "bookmark-manager-all-bookmarks",
            "bookmark-manager-root-bookmarks",
            "bookmark-manager-detail-name-input",
            "bookmark-manager-detail-location-input",
            "bookmark-manager-menu-dismiss-overlay",
        ] {
            let control = manager
                .split(interactive)
                .nth(1)
                .unwrap_or_else(|| panic!("missing interactive manager control: {interactive}"));
            assert!(!control.is_empty());
        }
        for action in [
            "BookmarkManagerUiAction::Back",
            "BookmarkManagerUiAction::Forward",
            "BookmarkManagerUiAction::Navigate",
            "BookmarkManagerUiAction::SelectBookmark",
            "BookmarkManagerUiAction::ToggleMenu",
            "BookmarkManagerUiAction::Sort",
            "BookmarkManagerUiAction::ToggleDensity",
            "BookmarkManagerUiAction::DismissMenu",
        ] {
            assert!(
                manager.contains(action),
                "missing manager handler: {action}"
            );
        }
    }

    #[test]
    fn bookmark_toolbar_supports_native_folder_and_root_drops() {
        let source = include_str!("chrome.rs");
        assert!(source.contains("ExplorerAction::MoveBookmarkToFolder"));
        assert!(source.contains("parent_id: Some(folder_id)"));
        assert!(source.contains("parent_id: None"));
        assert!(source.contains(".on_drag("));
        assert!(source.contains("element.on_drop(move |drag: &BookmarkDrag"));
    }

    #[test]
    fn root_bookmark_toolbar_label_omits_root_directory_suffix() {
        assert_eq!(
            super::bookmark_toolbar_display_name("portable", None),
            "portable"
        );
        assert_eq!(
            super::bookmark_toolbar_display_name("portable", Some("工具")),
            "portable（工具）"
        );
    }

    #[test]
    fn left_click_folder_menu_is_content_only_and_management_stays_on_right_click() {
        let source = include_str!("chrome.rs");
        assert!(source.contains("bookmark-folder-menu-dismiss-overlay"));
        assert!(source.contains(".with_priority(155)"));
        let start = source
            .find(".when_some(active_folder_menu")
            .expect("folder menu");
        let end = source[start..]
            .find("enum BookmarkFolderMenuItem")
            .expect("folder menu end")
            + start;
        let menu = &source[start..end];
        assert!(menu.contains("bookmark-folder-child"));
        assert!(menu.contains("OpenBookmarkToolbarContextMenu"));
        assert!(!menu.contains("bookmark-folder-menu-rename"));
        assert!(!menu.contains("bookmark-folder-menu-add"));
        assert!(!menu.contains("bookmark-folder-menu-remove"));
    }

    #[test]
    fn bookmark_right_click_uses_the_folder_context_menu_visual_contract() {
        let source = include_str!("chrome.rs");
        let item = source
            .split("fn bookmark_context_menu(")
            .nth(1)
            .and_then(|source| source.split("fn remote_context_menu(").next())
            .expect("bookmark context menu");
        assert!(item.contains("bookmark-context-overlay"));
        assert!(item.contains("RequestRemoveBookmark"));
        assert!(item.contains("OpenBookmarkInNewTab"));
        for style in [
            ".min_w(px(252.0))",
            ".p(px(6.0))",
            ".rounded(px(6.0))",
            ".shadow_lg()",
        ] {
            assert!(
                item.contains(style),
                "missing shared folder-menu style: {style}"
            );
        }
        let root = include_str!("lib.rs");
        let open = root
            .split("if let ExplorerAction::OpenBookmarkContextMenu")
            .nth(1)
            .expect("open reducer");
        assert!(open.starts_with(" { id, x, y } = action {"));
        assert!(open.contains("open_bookmark_context_menu"));
        assert!(
            !open[..open.find("return;").expect("open branch return")]
                .contains("present_bookmark_action_window")
        );
    }

    #[test]
    fn bookmark_star_is_anchored_to_the_toolbar_left_edge() {
        let source = include_str!("chrome.rs");
        let toolbar = source
            .split("fn bookmark_bar(")
            .nth(1)
            .expect("bookmark toolbar exists")
            .split("fn bookmark_visible_count(")
            .next()
            .expect("bookmark toolbar has a bounded implementation");
        let star = toolbar
            .find(".id(\"bookmark-star-toggle\")")
            .expect("bookmark star exists");
        let bookmarks = toolbar
            .find(".children(visible.into_iter()")
            .expect("bookmark entries exist");
        assert!(star < bookmarks, "star must be the first toolbar control");
        assert!(toolbar.contains(".text_size(px(20.0))"));
        assert!(!toolbar.contains(".absolute()\n                .left(px("));
    }

    #[test]
    fn bookmarked_location_star_is_solid_and_focus_blue() {
        let source = include_str!("chrome.rs");
        let toolbar = source
            .split("fn bookmark_bar(")
            .nth(1)
            .and_then(|source| source.split("fn bookmark_visible_count(").next())
            .expect("bookmark toolbar source");
        assert!(toolbar.contains("let bookmarked = current_folder.is_some_and"));
        assert!(toolbar.contains("if bookmarked { \"★\" } else { \"☆\" }"));
        assert!(toolbar.contains("tokens.theme.colors.focus.to_gpui()"));
        assert!(toolbar.contains("ExplorerAction::ToggleCurrentFolderBookmark"));
        assert!(toolbar.contains("context_menu_coordinates(event.position(), window)"));
        assert!(toolbar.contains("screen_y: screen_y.saturating_add(18)"));
    }

    #[test]
    fn lua_bookmark_editor_uses_visible_token_styled_inputs() {
        let source = include_str!("chrome.rs");
        let editor = source
            .split("fn bookmark_editor(")
            .nth(1)
            .and_then(|section| {
                section
                    .split("fn session_reset_confirmation_dialog(")
                    .next()
            })
            .expect("bookmark editor source");
        for required in [
            "bookmark-name-input",
            "bookmark-payload-input",
            "資料夾路徑（可編輯）",
            ".bg(colors.control_fill.to_gpui())",
            ".text_color(input_text)",
            ".caret_color(input_caret.into())",
            ".border_color(colors.focus.to_gpui())",
            "payload_is_multiline",
            "if payload_is_multiline { 220.0 } else { 36.0 }",
            ".w_full()",
            ".h(px(32.0))",
            ".font_family(tokens.typography.family.primary)",
            ".text_size(px(16.0))",
            ".line_height(px(tokens.typography.address.line_height.value()))",
            ".rounded(px(4.0))",
        ] {
            assert!(
                editor.contains(required),
                "missing visible editor control contract: {required}"
            );
        }
    }

    #[test]
    fn bookmark_items_keep_all_right_click_hooks_with_the_shared_inline_menu() {
        let production = include_str!("chrome.rs")
            .split("#[cfg(test)]")
            .next()
            .expect("production source");
        assert_eq!(production.matches("OpenBookmarkContextMenu").count(), 5);
        assert!(production.contains("bookmark-context-overlay"));
        assert!(production.contains("fn bookmark_context_menu("));
    }

    #[test]
    fn bookmark_toolbar_context_exposes_root_folder_and_path_crud() {
        let production = include_str!("chrome.rs")
            .split("#[cfg(test)]")
            .next()
            .expect("production source");
        for required in [
            "OpenBookmarkToolbarContextMenu",
            "Bookmark toolbar context menu",
            "新增資料夾路徑書籤…",
            "新增檔案路徑書籤…",
            "重新命名資料夾…",
            "刪除資料夾",
            "ToggleBookmarkManager",
        ] {
            assert!(production.contains(required), "missing {required}");
        }
    }

    #[test]
    fn bookmark_folder_and_destination_surfaces_are_accessible_and_destructive_delete_confirms() {
        let source = include_str!("chrome.rs");
        for required in [
            "favorites-tree-heading",
            "favorite-folder-nav",
            "bookmark-folder-menu-add",
            "bookmark-folder-menu-rename",
            "bookmark-folder-menu-remove",
            "bookmark-destination-picker",
            "bookmark-editor-remove",
            "bookmark-folder-delete-dialog",
            "不會刪除磁碟上的檔案",
        ] {
            assert!(
                source.contains(required),
                "missing bookmark folder contract: {required}"
            );
        }
    }

    #[test]
    fn bookmark_editor_exposes_remove_only_for_existing_bookmarks() {
        let source = include_str!("chrome.rs");
        let editor = source
            .split("fn bookmark_editor(")
            .nth(1)
            .and_then(|source| source.split("fn bookmark_folder_editor(").next())
            .expect("bookmark editor source");
        assert!(editor.contains("bookmark-editor-remove"));
        assert!(editor.contains("移除書籤"));
        assert!(editor.contains("let is_new = editor.id.is_none()"));
        assert!(editor.contains(".when(!is_new"));
    }
}
#[test]
fn cache_budget_usage_text_reserves_unavailable_for_confirmed_failure() {
    use crate::folder_options_window::CacheUsageAvailabilityV1;

    assert_eq!(
        cache_budget_usage_text(CacheUsageAvailabilityV1::Pending, None, 512 * 1024 * 1024),
        "\u{2014} / 512.0 MB"
    );
    assert_eq!(
        cache_budget_usage_text(
            CacheUsageAvailabilityV1::Pending,
            Some(64 * 1024 * 1024),
            1024 * 1024 * 1024,
        ),
        "64.0 MB / 1.0 GB"
    );
    assert_eq!(
        cache_budget_usage_text(
            CacheUsageAvailabilityV1::Unavailable,
            Some(64 * 1024 * 1024),
            1024 * 1024 * 1024,
        ),
        "Unavailable / 1.0 GB"
    );
}
