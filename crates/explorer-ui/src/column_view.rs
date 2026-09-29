//! Finder-style local column strip. Rows are realized only for the visible range of each column.

use std::{cell::Cell, collections::HashMap, ops::Range, rc::Rc, sync::Arc};

use explorer_i18n::Catalog;
use explorer_model::{ColumnFault, ColumnPhase, DirectoryState, FileEntry, ShellItemId};
use gpui::{
    App, IntoElement, MouseButton, ObjectFit, RenderImage, RenderOnce, Role, SharedString, Window,
    canvas, div, img, prelude::*, px,
};
use gpui_elements::editable_text::{EditableTextState, text_input};

use crate::{
    UiTokens, actions::ExplorerAction, chrome::ActionCallback, file_view::DirectoryPresentation,
};

/// Extra viewports of rows realized above and below the clip.
pub(crate) const COLUMN_ROW_OVERSCAN_VIEWPORTS: usize = 2;

/// Icon loads for one frame. Visible rows are requested before overscan.
pub(crate) const COLUMN_ICON_CANDIDATE_LIMIT: usize = 96;

pub const COLUMN_ROW_HEIGHT: f32 = explorer_model::COLUMN_ROW_HEIGHT;

/// One filename line. The font size stays the inherited 12px file-row size.
const COLUMN_FILENAME_LINE_HEIGHT: f32 = 16.0;
const COLUMN_FILENAME_MAX_LINES: usize = 2;
const _: () = assert!(COLUMN_FILENAME_LINE_HEIGHT * 2.0 + 4.0 == COLUMN_ROW_HEIGHT);

/// `SetColumnScroll` has no viewport field. This index publishes the measured
/// list height instead of moving a column. Real columns never use it.
pub(crate) const COLUMN_LIST_VIEWPORT_SLOT: usize = usize::MAX;

#[derive(Clone, Debug)]
pub struct ColumnRowModel {
    pub id: ShellItemId,
    pub name: String,
    pub location: explorer_model::LocationDescriptor,
    pub is_container: bool,
    pub metadata: explorer_model::FileEntryMetadata,
    pub selected: bool,
    pub branch_selected: bool,
}

#[derive(Clone, Debug)]
pub struct ColumnPaneModel {
    pub index: usize,
    pub title: String,
    /// Directory this column lists. Background drops use it, not the navigated folder.
    pub directory: explorer_model::LocationDescriptor,
    pub width: f32,
    pub vertical_offset: f32,
    /// Filtered row count. Scroll clamps use this, not `rows.len()`.
    pub row_count: usize,
    /// Projection index of `rows[0]`.
    pub row_origin: usize,
    /// Visible rows plus overscan. Not the whole directory.
    pub rows: Vec<ColumnRowModel>,
    /// Rename target when it sits outside `rows`.
    pub pinned_row: Option<(usize, ColumnRowModel)>,
    pub status: ColumnPaneStatus,
    /// Readable failure text for [`ColumnPaneStatus::Error`]. Never an empty-folder claim.
    pub status_detail: Option<String>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ColumnPaneStatus {
    Ready,
    Loading,
    Empty,
    Error(ColumnFault),
}

#[derive(Clone, Debug)]
pub struct ColumnStripModel {
    pub panes: Vec<ColumnPaneModel>,
    pub active: usize,
    pub horizontal_offset: f32,
    pub preview_visible: bool,
    pub preview_width: f32,
    /// Measured file-surface width. Zero until layout has run.
    pub file_surface_width: f32,
    /// Measured list viewport height. Zero until the strip has been laid out.
    pub list_viewport_height: f32,
    pub preview_title: String,
    pub preview_detail: String,
    pub preview_status: String,
    pub preview_route: ColumnPreviewRoute,
}

pub fn pane_status(phase: &ColumnPhase) -> ColumnPaneStatus {
    match phase {
        ColumnPhase::Pending | ColumnPhase::Loading => ColumnPaneStatus::Loading,
        ColumnPhase::Ready(snapshot) if snapshot.entries().is_empty() => ColumnPaneStatus::Empty,
        ColumnPhase::Ready(_) => ColumnPaneStatus::Ready,
        ColumnPhase::Empty => ColumnPaneStatus::Empty,
        ColumnPhase::Error(fault) => ColumnPaneStatus::Error(*fault),
    }
}

/// Ancestor listing status. An open request stays Loading even after a partial `Ready` batch.
pub fn auxiliary_column_status(phase: &ColumnPhase, request_open: bool) -> ColumnPaneStatus {
    if request_open
        && matches!(
            phase,
            ColumnPhase::Pending | ColumnPhase::Loading | ColumnPhase::Ready(_)
        )
    {
        return ColumnPaneStatus::Loading;
    }
    pane_status(phase)
}

/// Current-column status from the tab directory reducer. `Error` is never reported as empty.
pub fn directory_column_status(state: &DirectoryState) -> (ColumnPaneStatus, Option<String>) {
    match state {
        DirectoryState::Idle | DirectoryState::Loading { .. } => (ColumnPaneStatus::Loading, None),
        DirectoryState::Ready(snapshot) if snapshot.entries().is_empty() => {
            (ColumnPaneStatus::Empty, None)
        }
        DirectoryState::Ready(_) => (ColumnPaneStatus::Ready, None),
        DirectoryState::Error { error, .. } => {
            let message = error.user_message.trim();
            let detail = (!message.is_empty()).then(|| message.to_owned());
            (ColumnPaneStatus::Error(ColumnFault::Inaccessible), detail)
        }
    }
}

pub(crate) fn column_status_offers_refresh(status: ColumnPaneStatus) -> bool {
    matches!(status, ColumnPaneStatus::Empty | ColumnPaneStatus::Error(_))
}

pub(crate) fn column_level_accessible_name(
    catalog: Catalog,
    index: usize,
    title: &str,
    status: &str,
) -> String {
    let level = format!("{} {} {}", catalog.t("a11y-column-level"), index + 1, title);
    if status.is_empty() {
        level
    } else {
        format!("{level} {status}")
    }
}

pub(crate) fn column_status_text(
    catalog: Catalog,
    status: ColumnPaneStatus,
    detail: Option<&str>,
) -> String {
    if let ColumnPaneStatus::Error(_) = status
        && let Some(detail) = detail.map(str::trim).filter(|text| !text.is_empty())
    {
        let empty_claim = catalog.t("column-empty");
        if detail != empty_claim {
            return detail.to_owned();
        }
    }
    match status {
        ColumnPaneStatus::Ready => String::new(),
        ColumnPaneStatus::Loading => catalog.t("column-loading"),
        ColumnPaneStatus::Empty => catalog.t("column-empty"),
        ColumnPaneStatus::Error(ColumnFault::Cycle) => catalog.t("column-cycle"),
        ColumnPaneStatus::Error(_) => catalog.t("column-error"),
    }
}

pub fn rows_from_presentation(
    presentation: &DirectoryPresentation,
    rows: Range<usize>,
    selected: &[ShellItemId],
    branch_child: Option<&ShellItemId>,
) -> Vec<ColumnRowModel> {
    let mut materialized = Vec::with_capacity(rows.len());
    for index in rows {
        let Some((_, entry)) = presentation.entry(index) else {
            continue;
        };
        materialized.push(ColumnRowModel {
            id: entry.id.clone(),
            name: entry.display_name.clone(),
            location: entry.location.clone(),
            is_container: entry.is_container,
            metadata: entry.metadata.clone(),
            selected: selected.iter().any(|id| id == &entry.id),
            branch_selected: branch_child == Some(&entry.id),
        });
    }
    materialized
}

pub(crate) fn column_row_indexes(origin: usize, len: usize, pinned: Option<usize>) -> Vec<usize> {
    let mut indexes = Vec::with_capacity(len.saturating_add(usize::from(pinned.is_some())));
    for offset in 0..len {
        indexes.push(origin.saturating_add(offset));
    }
    if let Some(pinned) = pinned
        && let Err(position) = indexes.binary_search(&pinned)
    {
        indexes.insert(position, pinned);
    }
    indexes
}

fn ordered_column_rows(
    origin: usize,
    rows: Vec<ColumnRowModel>,
    pinned: Option<(usize, ColumnRowModel)>,
) -> Vec<(usize, ColumnRowModel)> {
    let pinned_index = pinned.as_ref().map(|(index, _)| *index);
    let indexes = column_row_indexes(origin, rows.len(), pinned_index);
    let mut pool = HashMap::with_capacity(indexes.len());
    for (offset, row) in rows.into_iter().enumerate() {
        pool.insert(origin.saturating_add(offset), row);
    }
    if let Some((index, row)) = pinned {
        pool.entry(index).or_insert(row);
    }
    indexes
        .into_iter()
        .filter_map(|index| pool.remove(&index).map(|row| (index, row)))
        .collect()
}

#[allow(
    clippy::cast_precision_loss,
    reason = "a column gap is a row count times the fixed row height"
)]
fn column_gap_height(rows: usize) -> f32 {
    ((rows as f64) * f64::from(COLUMN_ROW_HEIGHT))
        .round()
        .clamp(0.0, f64::from(u32::MAX)) as f32
}

fn column_row_gap(rows: usize) -> gpui::AnyElement {
    div()
        .w_full()
        .h(px(column_gap_height(rows)))
        .flex_none()
        .into_any_element()
}

pub fn offline_placeholder(entry: &FileEntry) -> bool {
    const OFFLINE: u32 = 0x1000;
    const RECALL_ON_DATA_ACCESS: u32 = 0x0040_0000;
    entry.metadata.filesystem_attributes & (OFFLINE | RECALL_ON_DATA_ACCESS) != 0
}

#[derive(IntoElement)]
pub struct ColumnStrip {
    tokens: UiTokens,
    model: ColumnStripModel,
    catalog: Catalog,
    viewport_height: f32,
    icons: HashMap<explorer_model::ShellIconKey, Arc<RenderImage>>,
    icon_dpi: u16,
    icon_logical: u16,
    on_action: Option<ActionCallback>,
    preview_texture: Option<Arc<RenderImage>>,
    preview_failed: bool,
    preview_broker: Option<&'static str>,
    preview_handler: ColumnHandlerPhase,
    preview_pane_height: f32,
    preview_content: Option<crate::preview_content::PreviewContent>,
    rename_editor: Option<explorer_model::RenameEditorState>,
    rename_input: Option<gpui::WeakEntity<EditableTextState>>,
}

impl ColumnStrip {
    pub fn new(
        tokens: UiTokens,
        model: ColumnStripModel,
        catalog: Catalog,
        viewport_height: f32,
        icons: HashMap<explorer_model::ShellIconKey, Arc<RenderImage>>,
        icon_dpi: u16,
        icon_logical: u16,
        on_action: Option<ActionCallback>,
    ) -> Self {
        Self {
            tokens,
            model,
            catalog,
            viewport_height,
            icons,
            icon_dpi,
            icon_logical,
            on_action,
            preview_texture: None,
            preview_failed: false,
            preview_broker: None,
            preview_handler: ColumnHandlerPhase::Inactive,
            preview_pane_height: 0.0,
            preview_content: None,
            rename_editor: None,
            rename_input: None,
        }
    }

    #[must_use]
    pub fn with_rename(
        mut self,
        editor: Option<explorer_model::RenameEditorState>,
        input: Option<gpui::WeakEntity<EditableTextState>>,
    ) -> Self {
        self.rename_editor = editor;
        self.rename_input = input;
        self
    }

    #[must_use]
    pub fn with_preview(
        mut self,
        texture: Option<Arc<RenderImage>>,
        failed: bool,
        broker: Option<&'static str>,
        handler: ColumnHandlerPhase,
        pane_height: f32,
    ) -> Self {
        self.preview_texture = texture;
        self.preview_failed = failed;
        self.preview_broker = broker;
        self.preview_handler = handler;
        self.preview_pane_height = pane_height;
        self
    }

    pub(crate) fn with_content(
        mut self,
        content: Option<crate::preview_content::PreviewContent>,
    ) -> Self {
        self.preview_content = content;
        self
    }
}

/// Which way a column-strip wheel event should move.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ColumnWheelAxis {
    Horizontal,
    Vertical,
}

/// Horizontal trackpad movement, or Shift held with a wheel, scrolls the strip.
/// A vertical wheel without Shift stays inside the column under the pointer.
pub fn column_wheel_axis(delta_x: f32, delta_y: f32, shift: bool) -> (ColumnWheelAxis, f32) {
    let x = if delta_x.is_finite() { delta_x } else { 0.0 };
    let y = if delta_y.is_finite() { delta_y } else { 0.0 };
    if shift || (x.abs() > 0.0 && x.abs() >= y.abs()) {
        let delta = if x.abs() >= y.abs() && x.abs() > 0.0 {
            x
        } else {
            y
        };
        (ColumnWheelAxis::Horizontal, delta)
    } else {
        (ColumnWheelAxis::Vertical, y)
    }
}

/// Usable list height after the overlaid horizontal scrollbar, if that track is showing.
pub fn column_list_viewport_height(bounds_height: f32, scrollbar_obstruction: f32) -> f32 {
    if !bounds_height.is_finite() || bounds_height <= 0.0 {
        return 0.0;
    }
    let obstruction = if scrollbar_obstruction.is_finite() {
        scrollbar_obstruction.max(0.0)
    } else {
        0.0
    };
    (bounds_height - obstruction).max(0.0)
}

pub fn column_vertical_wheel_offset(
    vertical_offset: f32,
    delta: f32,
    row_count: usize,
    viewport_height: f32,
) -> f32 {
    let proposed = if vertical_offset.is_finite() && delta.is_finite() {
        vertical_offset - delta
    } else if vertical_offset.is_finite() {
        vertical_offset.max(0.0)
    } else {
        0.0
    };
    explorer_model::ColumnBranch::clamp_vertical_offset(
        proposed,
        row_count,
        COLUMN_ROW_HEIGHT,
        viewport_height,
    )
}

pub fn column_wheel_action(
    delta_x: f32,
    delta_y: f32,
    shift: bool,
    column_index: Option<usize>,
    vertical_offset: f32,
    horizontal_offset: f32,
    vertical_rows: Option<(usize, f32)>,
) -> Option<ExplorerAction> {
    let (axis, delta) = column_wheel_axis(delta_x, delta_y, shift);
    if delta == 0.0 {
        return None;
    }
    match axis {
        ColumnWheelAxis::Horizontal => Some(ExplorerAction::SetColumnHorizontalOffset {
            offset: horizontal_offset - delta,
        }),
        ColumnWheelAxis::Vertical => {
            let offset = match vertical_rows {
                Some((row_count, viewport_height)) => {
                    column_vertical_wheel_offset(vertical_offset, delta, row_count, viewport_height)
                }
                None => (vertical_offset - delta).max(0.0),
            };
            column_index.map(|column_index| ExplorerAction::SetColumnScroll {
                column_index,
                offset,
            })
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ColumnScrollbarMetrics {
    pub visible: bool,
    pub maximum: f32,
    pub offset: f32,
    pub thumb_width: f32,
    pub thumb_left: f32,
}

pub fn column_scrollbar_metrics(
    content_width: f32,
    viewport: f32,
    offset: f32,
    minimum_thumb: f32,
) -> ColumnScrollbarMetrics {
    let viewport = if viewport.is_finite() {
        viewport.max(0.0)
    } else {
        0.0
    };
    let content = if content_width.is_finite() {
        content_width.max(0.0)
    } else {
        0.0
    };
    let maximum = (content - viewport).max(0.0);
    if viewport <= 1.0 || maximum <= 0.0 {
        return ColumnScrollbarMetrics {
            visible: false,
            maximum: 0.0,
            offset: 0.0,
            thumb_width: 0.0,
            thumb_left: 0.0,
        };
    }
    let offset = if offset.is_finite() {
        offset.clamp(0.0, maximum)
    } else {
        0.0
    };
    let thumb_width =
        crate::interaction::scrollbar_thumb_height(viewport, maximum, minimum_thumb.max(1.0))
            .unwrap_or(viewport)
            .min(viewport);
    let travel = (viewport - thumb_width).max(1.0);
    ColumnScrollbarMetrics {
        visible: true,
        maximum,
        offset,
        thumb_width,
        thumb_left: offset / maximum * travel,
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ColumnScrollbarHit {
    Thumb { grab_offset: f32 },
    Page { offset: f32 },
}

pub fn column_scrollbar_hit(
    metrics: &ColumnScrollbarMetrics,
    viewport: f32,
    pointer_x: f32,
) -> Option<ColumnScrollbarHit> {
    if !metrics.visible || !pointer_x.is_finite() {
        return None;
    }
    if pointer_x >= metrics.thumb_left && pointer_x <= metrics.thumb_left + metrics.thumb_width {
        return Some(ColumnScrollbarHit::Thumb {
            grab_offset: (pointer_x - metrics.thumb_left).max(0.0),
        });
    }
    let page = if pointer_x < metrics.thumb_left {
        metrics.offset - viewport
    } else {
        metrics.offset + viewport
    };
    Some(ColumnScrollbarHit::Page {
        offset: page.clamp(0.0, metrics.maximum),
    })
}

impl RenderOnce for ColumnStrip {
    fn render(self, _window: &mut Window, _cx: &mut App) -> impl IntoElement {
        let colors = self.tokens.theme.colors;
        let catalog = self.catalog;
        let on_action = self.on_action;
        let fallback_height = if self.viewport_height.is_finite() {
            self.viewport_height.max(0.0)
        } else {
            0.0
        };
        let height = if self.model.list_viewport_height > 1.0 {
            self.model.list_viewport_height
        } else {
            fallback_height
        };
        let surface_known = self.model.file_surface_width > 1.0;
        let (strip_span, preview_width) = if surface_known {
            explorer_model::column_layout_spans(
                self.model.file_surface_width,
                self.model.preview_width,
                self.model.preview_visible,
            )
        } else if self.model.preview_visible {
            (0.0, self.model.preview_width)
        } else {
            (0.0, 0.0)
        };
        let show_preview = self.model.preview_visible && preview_width > 1.0;
        let offset = self.model.horizontal_offset;
        let preview_route = self.model.preview_route;
        let preview_texture = self.preview_texture;
        let preview_failed = self.preview_failed;
        let preview_broker = self.preview_broker;
        let preview_handler = self.preview_handler;
        let preview_pane_height = self.preview_pane_height;
        let preview_content = self.preview_content;
        let icons = self.icons;
        let icon_dpi = self.icon_dpi;
        let icon_logical = self.icon_logical;
        let theme = match self.tokens.theme.mode {
            crate::theme::ThemeMode::Light => explorer_model::ShellIconTheme::Light,
            crate::theme::ThemeMode::Dark => explorer_model::ShellIconTheme::Dark,
        };
        let content_width = self.model.panes.iter().map(|pane| pane.width).sum::<f32>();
        let minimum_thumb = self.tokens.layout.minimum_hit_target.value();
        let scrollbar = column_scrollbar_metrics(content_width, strip_span, offset, minimum_thumb);
        let track_height = self.tokens.layout.content_spacing.value() * 1.5;
        let scrollbar_block = if scrollbar.visible {
            track_height.max(12.0)
        } else {
            0.0
        };
        let measured_list = Rc::new(Cell::new(0.0_f32));
        let published_viewport = self.model.list_viewport_height;
        let rename_editor = self.rename_editor;
        let rename_input = self.rename_input;
        div()
            .id("column-view")
            .debug_selector(|| "column-view".to_owned())
            .role(Role::TabPanel)
            .aria_label(catalog.t("a11y-column-view"))
            .flex()
            .flex_row()
            .h_full()
            .w_full()
            .overflow_hidden()
            .bg(colors.surface.to_gpui())
            .child(
                div()
                    .id("column-strip")
                    .relative()
                    .flex()
                    .flex_row()
                    .h_full()
                    .flex_1()
                    .min_w(px(0.0))
                    .overflow_hidden()
                    .child(
                        canvas(
                            {
                                let measured_list = Rc::clone(&measured_list);
                                let on_action = on_action.clone();
                                move |bounds, window, cx| {
                                    let next = column_list_viewport_height(
                                        f32::from(bounds.size.height),
                                        scrollbar_block,
                                    );
                                    if next > 1.0 {
                                        measured_list.set(next);
                                    }
                                    if next > 1.0 && (next - published_viewport).abs() > 0.5 {
                                        let Some(callback) = on_action.clone() else {
                                            return;
                                        };
                                        window.defer(cx, move |window, cx| {
                                            callback(
                                                &ExplorerAction::SetColumnScroll {
                                                    column_index: COLUMN_LIST_VIEWPORT_SLOT,
                                                    offset: next,
                                                },
                                                window,
                                                cx,
                                            );
                                        });
                                    }
                                }
                            },
                            |_, _, _, _| {},
                        )
                        .absolute()
                        .inset_0()
                        .size_full(),
                    )
                    .child(div().flex().flex_row().h_full().ml(px(-offset)).children(
                        self.model.panes.into_iter().map(|pane| {
                            column_pane(
                                pane,
                                height,
                                published_viewport,
                                Rc::clone(&measured_list),
                                offset,
                                self.tokens,
                                catalog,
                                icons.clone(),
                                icon_dpi,
                                icon_logical,
                                theme,
                                on_action.clone(),
                                rename_editor.clone(),
                                rename_input.clone(),
                            )
                        }),
                    ))
                    .when(scrollbar.visible, |strip| {
                        strip.child(column_horizontal_scrollbar(
                            catalog,
                            self.tokens,
                            content_width,
                            strip_span,
                            offset,
                            minimum_thumb,
                            track_height,
                            on_action.clone(),
                        ))
                    }),
            )
            .when(show_preview, |element| {
                element.child(column_preview(
                    self.tokens,
                    catalog,
                    preview_width,
                    offset,
                    self.model.preview_title,
                    self.model.preview_detail,
                    preview_route,
                    preview_texture,
                    preview_failed,
                    preview_broker,
                    preview_handler,
                    preview_pane_height,
                    on_action,
                    preview_content,
                ))
            })
    }
}

fn column_horizontal_scrollbar(
    catalog: Catalog,
    tokens: UiTokens,
    content_width: f32,
    viewport: f32,
    offset: f32,
    minimum_thumb: f32,
    track_height: f32,
    on_action: Option<ActionCallback>,
) -> gpui::AnyElement {
    let colors = tokens.theme.colors;
    let metrics = column_scrollbar_metrics(content_width, viewport, offset, minimum_thumb);
    let track_bounds = Rc::new(Cell::new(None::<(f32, f32)>));
    let thumb_height = (track_height - tokens.layout.focus_stroke.value() * 2.0).max(8.0);
    div()
        .id("column-horizontal-scrollbar")
        .debug_selector(|| "column-horizontal-scrollbar".to_owned())
        .role(Role::ScrollBar)
        .aria_label(catalog.t("chrome-file-view-hscroll"))
        .aria_numeric_value(f64::from(metrics.offset))
        .aria_min_numeric_value(0.0)
        .aria_max_numeric_value(f64::from(metrics.maximum))
        .absolute()
        .left_0()
        .right_0()
        .bottom_0()
        .h(px(track_height.max(12.0)))
        .bg(colors.surface.to_gpui())
        .child(
            canvas(
                {
                    let track_bounds = Rc::clone(&track_bounds);
                    move |bounds, _window, _cx| {
                        track_bounds.set(Some((
                            f32::from(bounds.origin.x),
                            f32::from(bounds.size.width),
                        )));
                    }
                },
                |_, _, _, _| {},
            )
            .absolute()
            .inset_0()
            .size_full(),
        )
        .on_scroll_wheel({
            let callback = on_action.clone();
            move |event, window, cx| {
                if event.modifiers.control {
                    return;
                }
                let (delta_x, delta_y) = match event.delta {
                    gpui::ScrollDelta::Pixels(delta) => (f32::from(delta.x), f32::from(delta.y)),
                    gpui::ScrollDelta::Lines(delta) => {
                        (delta.x * COLUMN_ROW_HEIGHT, delta.y * COLUMN_ROW_HEIGHT)
                    }
                };
                let Some(action) =
                    column_wheel_action(delta_x, delta_y, true, None, 0.0, offset, None)
                else {
                    return;
                };
                if let Some(callback) = callback.clone() {
                    callback(&action, window, cx);
                    cx.stop_propagation();
                }
            }
        })
        .when_some(on_action, |bar, callback| {
            let track_bounds = Rc::clone(&track_bounds);
            bar.on_mouse_down(MouseButton::Left, move |event, window, cx| {
                cx.stop_propagation();
                let Some((track_left, track_width)) = track_bounds.get() else {
                    return;
                };
                if track_width <= 1.0 {
                    return;
                }
                let pointer = f32::from(event.position.x) - track_left;
                let metrics =
                    column_scrollbar_metrics(content_width, track_width, offset, minimum_thumb);
                let Some(hit) = column_scrollbar_hit(&metrics, track_width, pointer) else {
                    return;
                };
                let action = match hit {
                    ColumnScrollbarHit::Thumb { grab_offset } => {
                        ExplorerAction::BeginColumnHorizontalScroll {
                            track_left,
                            track_width,
                            grab_offset,
                            minimum_thumb,
                        }
                    }
                    ColumnScrollbarHit::Page { offset } => {
                        ExplorerAction::SetColumnHorizontalOffset { offset }
                    }
                };
                callback(&action, window, cx);
            })
        })
        .child(
            div()
                .id("column-horizontal-scrollbar-thumb")
                .absolute()
                .left(px(metrics.thumb_left))
                .bottom(px((track_height.max(12.0) - thumb_height) / 2.0))
                .w(px(metrics.thumb_width))
                .h(px(thumb_height))
                .rounded(px(tokens.layout.corner_radius.value()))
                .bg(colors.text_disabled.to_gpui())
                .hover(|style| style.bg(colors.text_secondary.to_gpui())),
        )
        .into_any_element()
}

fn column_pane(
    mut pane: ColumnPaneModel,
    _viewport_height: f32,
    published_viewport: f32,
    measured_viewport: Rc<Cell<f32>>,
    horizontal_offset: f32,
    tokens: UiTokens,
    catalog: Catalog,
    icons: HashMap<explorer_model::ShellIconKey, Arc<RenderImage>>,
    icon_dpi: u16,
    icon_logical: u16,
    theme: explorer_model::ShellIconTheme,
    on_action: Option<ActionCallback>,
    rename_editor: Option<explorer_model::RenameEditorState>,
    rename_input: Option<gpui::WeakEntity<EditableTextState>>,
) -> gpui::AnyElement {
    let colors = tokens.theme.colors;
    let row_count = pane.row_count;
    let ordered = ordered_column_rows(
        pane.row_origin,
        std::mem::take(&mut pane.rows),
        pane.pinned_row.take(),
    );
    let mut rendered_rows = Vec::with_capacity(ordered.len().saturating_mul(2));
    let mut cursor = 0usize;
    for (row_index, row) in ordered {
        let gap = row_index.saturating_sub(cursor);
        if gap > 0 {
            rendered_rows.push(column_row_gap(gap));
        }
        rendered_rows.push(column_row(
            pane.index,
            row,
            tokens,
            catalog,
            &icons,
            icon_dpi,
            icon_logical,
            theme,
            on_action.clone(),
            rename_editor.clone(),
            rename_input.clone(),
        ));
        cursor = row_index.saturating_add(1);
    }
    let index = pane.index;
    let width = pane.width;
    let vertical_offset = pane.vertical_offset;
    let status = column_status_text(catalog, pane.status, pane.status_detail.as_deref());
    let offers_refresh = column_status_offers_refresh(pane.status);
    let directory = pane.directory.clone();
    let title = pane.title.clone();
    let list_name = column_level_accessible_name(catalog, index, &title, &status);
    let scroll_action = on_action.clone();
    let resize_action = on_action.clone();
    div()
        .id(SharedString::from(format!("column-level-{index}")))
        .debug_selector({
            let title = title.clone();
            move || format!("column-level-{index}:{title}")
        })
        .relative()
        .role(Role::List)
        .aria_label(list_name)
        .w(px(width))
        .h_full()
        .flex_none()
        .flex()
        .flex_col()
        .overflow_hidden()
        .border_r(px(1.0))
        .border_color(colors.divider.to_gpui())
        .on_scroll_wheel(move |event, window, cx| {
            if event.modifiers.control {
                return;
            }
            let (delta_x, delta_y) = match event.delta {
                gpui::ScrollDelta::Pixels(delta) => (f32::from(delta.x), f32::from(delta.y)),
                gpui::ScrollDelta::Lines(delta) => {
                    (delta.x * COLUMN_ROW_HEIGHT, delta.y * COLUMN_ROW_HEIGHT)
                }
            };
            let live = measured_viewport.get();
            let viewport = if live > 1.0 {
                live
            } else if published_viewport > 1.0 {
                published_viewport
            } else {
                0.0
            };
            let Some(action) = column_wheel_action(
                delta_x,
                delta_y,
                event.modifiers.shift,
                Some(index),
                vertical_offset,
                horizontal_offset,
                Some((row_count, viewport)),
            ) else {
                return;
            };
            let Some(callback) = scroll_action.clone() else {
                return;
            };
            if matches!(action, ExplorerAction::SetColumnScroll { .. })
                && live > 1.0
                && (live - published_viewport).abs() > 0.5
            {
                callback(
                    &ExplorerAction::SetColumnScroll {
                        column_index: COLUMN_LIST_VIEWPORT_SLOT,
                        offset: live,
                    },
                    window,
                    cx,
                );
            }
            callback(&action, window, cx);
            cx.stop_propagation();
        })
        .child(
            // Gaps stand in for rows that were not built. The negative margin scrolls.
            div()
                .w_full()
                .flex_1()
                .min_h(px(0.0))
                .flex()
                .flex_col()
                .mt(px(-vertical_offset))
                .children(rendered_rows)
                .child(column_background(index, directory, on_action.clone())),
        )
        .children(column_status_elements(
            index,
            pane.status,
            status,
            offers_refresh,
            colors,
            catalog,
            on_action.clone(),
        ))
        .child({
            let hit = tokens.layout.divider_width.value().max(6.0);
            div()
                .id(SharedString::from(format!("column-divider-{index}")))
                .debug_selector(move || format!("column-divider-{index}"))
                .role(Role::Splitter)
                .aria_label(catalog.t("a11y-column-divider"))
                .aria_numeric_value(f64::from(width))
                .aria_min_numeric_value(f64::from(explorer_model::COLUMN_WIDTH_MIN))
                .aria_max_numeric_value(f64::from(explorer_model::COLUMN_WIDTH_MAX))
                .absolute()
                .right_0()
                .top_0()
                .bottom_0()
                .w(px(hit))
                .cursor_col_resize()
                .bg(colors.divider.to_gpui())
                .hover(move |style| style.bg(colors.focus.to_gpui()))
                .active(move |style| style.bg(colors.accent.to_gpui()))
                .when_some(resize_action, |divider, callback| {
                    divider.on_mouse_down(MouseButton::Left, move |event, window, cx| {
                        cx.stop_propagation();
                        callback(
                            &ExplorerAction::BeginColumnWidthResize {
                                column_index: index,
                                pointer_x: f32::from(event.position.x),
                            },
                            window,
                            cx,
                        );
                    })
                })
        })
        .into_any_element()
}

pub(crate) fn negotiate_column_drop(
    paths: &gpui::ExternalPaths,
    destination: &explorer_model::LocationDescriptor,
) -> explorer_model::DragEffect {
    // `negotiate_external_paths(..., None)` is always `DragEffect::None`. A column
    // drop has to name the directory under the pointer or the transfer is discarded.
    crate::chrome::negotiate_external_paths(paths, true, Some(destination))
}

/// Rewrites the OLE effect only while the pointer is inside this column target.
///
/// `None` means leave the effect alone. A drag over another column must not be
/// overwritten by a row the pointer has already left. `Some(DragEffect::None)`
/// is a real rejection of the directory under the pointer.
pub(crate) fn column_drag_hover_effect(
    paths: &gpui::ExternalPaths,
    destination: &explorer_model::LocationDescriptor,
    pointer_inside: bool,
) -> Option<explorer_model::DragEffect> {
    pointer_inside.then(|| negotiate_column_drop(paths, destination))
}

fn column_background(
    column_index: usize,
    directory: explorer_model::LocationDescriptor,
    on_action: Option<ActionCallback>,
) -> gpui::AnyElement {
    div()
        .id(SharedString::from(format!(
            "column-background-{column_index}"
        )))
        .flex_1()
        .w_full()
        .min_h(px(0.0))
        .when_some(on_action, |element, callback| {
            let menu = Rc::clone(&callback);
            let drop_callback = Rc::clone(&callback);
            let accept_directory = directory.clone();
            let drop_directory = directory;
            element
                .on_mouse_down(MouseButton::Right, move |event, window, cx| {
                    cx.stop_propagation();
                    let (owner_window, x, y) =
                        crate::chrome::context_menu_coordinates(event.position, window);
                    menu(
                        &ExplorerAction::ShowContextMenu {
                            item_id: None,
                            column_index: Some(column_index),
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
                .on_mouse_up(MouseButton::Right, |_, _, cx| cx.stop_propagation())
                .can_drop(move |value, _, _| {
                    value
                        .downcast_ref::<gpui::ExternalPaths>()
                        .is_some_and(|paths| {
                            negotiate_column_drop(paths, &accept_directory)
                                != explorer_model::DragEffect::None
                        })
                })
                .on_drag_move::<gpui::ExternalPaths>({
                    let hover_directory = drop_directory.clone();
                    move |event, _, cx| {
                        // Capture runs parent-first. An ancestor that is not under the pointer
                        // must not replace this directory's effect, and a hit must stop later
                        // columns from replacing it.
                        if column_drag_hover_effect(
                            event.drag(cx),
                            &hover_directory,
                            event.bounds.contains(&event.event.position),
                        )
                        .is_some()
                        {
                            cx.stop_propagation();
                        }
                    }
                })
                .on_drop(move |paths: &gpui::ExternalPaths, window, cx| {
                    cx.stop_propagation();
                    let effect = negotiate_column_drop(paths, &drop_directory);
                    drop_callback(
                        &ExplorerAction::DropOnColumn {
                            column_index,
                            folder_id: None,
                            paths: paths.paths().to_vec(),
                            effect,
                            right_button: paths.drop_metadata().right_button,
                            allowed: crate::chrome::external_transfer_effects(paths),
                        },
                        window,
                        cx,
                    );
                })
        })
        .into_any_element()
}

fn column_debug_key(kind: &str, column_index: usize, id: &ShellItemId) -> String {
    let mut key = format!("{kind}-{column_index}-");
    for byte in id.provider_bytes() {
        key.push_str(&format!("{byte:02x}"));
    }
    key
}

fn column_row(
    column_index: usize,
    row: ColumnRowModel,
    tokens: UiTokens,
    catalog: Catalog,
    icons: &HashMap<explorer_model::ShellIconKey, Arc<RenderImage>>,
    icon_dpi: u16,
    icon_logical: u16,
    theme: explorer_model::ShellIconTheme,
    on_action: Option<ActionCallback>,
    rename_editor: Option<explorer_model::RenameEditorState>,
    rename_input: Option<gpui::WeakEntity<EditableTextState>>,
) -> gpui::AnyElement {
    let colors = tokens.theme.colors;
    let selected = row.selected || row.branch_selected;
    let name = row.name.clone();
    let is_container = row.is_container;
    let icon = column_row_icon(&row, icons, icon_dpi, icon_logical, theme);
    let item_id = row.id.clone();
    let icon_id = column_debug_key("column-row-icon", column_index, &item_id);
    let location = row.location.clone();
    let renaming = rename_editor.filter(|editor| editor.item.id == item_id);
    let open = ExplorerAction::OpenColumnItem {
        column_index,
        item_id: item_id.clone(),
        location: location.clone(),
        is_container,
    };
    let row_id = format!(
        "column-row-{column_index}-{:02x?}",
        item_id.provider_bytes()
    );
    div()
        .id(SharedString::from(row_id))
        .debug_selector({
            let id = item_id.clone();
            move || column_debug_key("column-row", column_index, &id)
        })
        .role(Role::ListItem)
        .aria_label(format!("{} {name}", catalog.t("a11y-column-row")))
        .aria_selected(selected)
        .h(px(COLUMN_ROW_HEIGHT))
        .min_h(px(COLUMN_ROW_HEIGHT))
        .max_h(px(COLUMN_ROW_HEIGHT))
        .w_full()
        .flex_none()
        .overflow_hidden()
        .flex()
        .items_center()
        .px(px(8.0))
        .when(selected, |element| {
            element.bg(colors.selected_active.to_gpui())
        })
        .hover(|style| style.bg(colors.selected_inactive.to_gpui()))
        .when_some(on_action, |element, callback| {
            let open_action = open;
            let press_id = item_id.clone();
            let press_location = location.clone();
            let menu_down = item_id.clone();
            let menu_up = item_id.clone();
            let menu_out = item_id.clone();
            let drop_id = item_id.clone();
            let move_callback = Rc::clone(&callback);
            let up_callback = Rc::clone(&callback);
            let right_callback = Rc::clone(&callback);
            let right_up_callback = Rc::clone(&callback);
            let right_out_callback = Rc::clone(&callback);
            let drop_callback = Rc::clone(&callback);
            element
                .on_mouse_down(MouseButton::Left, move |event, window, cx| {
                    cx.stop_propagation();
                    if event.click_count >= 2 && !is_container {
                        callback(&open_action, window, cx);
                        return;
                    }
                    callback(
                        &ExplorerAction::PressColumnRow {
                            column_index,
                            item_id: press_id.clone(),
                            location: press_location.clone(),
                            is_container,
                            x: f32::from(event.position.x),
                            y: f32::from(event.position.y),
                            shift: event.modifiers.shift,
                            control: event.modifiers.control || event.modifiers.platform,
                        },
                        window,
                        cx,
                    );
                })
                .on_mouse_down(MouseButton::Right, move |event, window, cx| {
                    cx.stop_propagation();
                    right_callback(
                        &ExplorerAction::BeginContextItemGesture {
                            column_index: Some(column_index),
                            item_id: menu_down.clone(),
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
                    cx.stop_propagation();
                    up_callback(&ExplorerAction::FinishColumnPress, window, cx);
                    up_callback(&ExplorerAction::CancelFileDrag, window, cx);
                })
                .on_mouse_up(MouseButton::Right, move |event, window, cx| {
                    cx.stop_propagation();
                    let (owner_window, x, y) =
                        crate::chrome::context_menu_coordinates(event.position, window);
                    right_up_callback(
                        &ExplorerAction::ShowContextMenu {
                            item_id: Some(menu_up.clone()),
                            column_index: Some(column_index),
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
                    cx.stop_propagation();
                    let (owner_window, x, y) =
                        crate::chrome::context_menu_coordinates(event.position, window);
                    right_out_callback(
                        &ExplorerAction::ShowContextMenu {
                            item_id: Some(menu_out.clone()),
                            column_index: Some(column_index),
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
                .when(is_container, |element| {
                    let folder_id = drop_id.clone();
                    let accept_folder = location.clone();
                    let drop_folder = location.clone();
                    element
                        .can_drop(move |value, _, _| {
                            value
                                .downcast_ref::<gpui::ExternalPaths>()
                                .is_some_and(|paths| {
                                    negotiate_column_drop(paths, &accept_folder)
                                        != explorer_model::DragEffect::None
                                })
                        })
                        .on_drag_move::<gpui::ExternalPaths>({
                            let hover_folder = drop_folder.clone();
                            move |event, _, cx| {
                                if column_drag_hover_effect(
                                    event.drag(cx),
                                    &hover_folder,
                                    event.bounds.contains(&event.event.position),
                                )
                                .is_some()
                                {
                                    cx.stop_propagation();
                                }
                            }
                        })
                        .on_drop(move |paths: &gpui::ExternalPaths, window, cx| {
                            cx.stop_propagation();
                            let effect = negotiate_column_drop(paths, &drop_folder);
                            drop_callback(
                                &ExplorerAction::DropOnColumn {
                                    column_index,
                                    folder_id: Some(folder_id.clone()),
                                    paths: paths.paths().to_vec(),
                                    effect,
                                    right_button: paths.drop_metadata().right_button,
                                    allowed: crate::chrome::external_transfer_effects(paths),
                                },
                                window,
                                cx,
                            );
                        })
                })
        })
        .child(column_row_icon_element(&icon_id, &name, is_container, icon))
        .child(if let Some(editor) = renaming {
            column_rename_editor(editor, rename_input, tokens)
        } else {
            div()
                .debug_selector({
                    let id = item_id.clone();
                    move || column_debug_key("column-filename", column_index, &id)
                })
                .flex_1()
                .min_w(px(0.0))
                .max_h(px(COLUMN_FILENAME_LINE_HEIGHT * 2.0))
                .overflow_hidden()
                .line_height(px(COLUMN_FILENAME_LINE_HEIGHT))
                .whitespace_normal()
                .text_ellipsis()
                .line_clamp(COLUMN_FILENAME_MAX_LINES)
                .text_color(colors.text_primary.to_gpui())
                .child(column_filename_text(name))
                .into_any_element()
        })
        .child({
            let chevron_id = item_id.clone();
            div()
                .debug_selector(move || {
                    column_debug_key("column-chevron", column_index, &chevron_id)
                })
                .flex_none()
                .text_color(colors.text_secondary.to_gpui())
                .child(if is_container { "›" } else { "" })
        })
        .into_any_element()
}

fn column_rename_editor(
    editor: explorer_model::RenameEditorState,
    input: Option<gpui::WeakEntity<EditableTextState>>,
    tokens: UiTokens,
) -> gpui::AnyElement {
    let colors = tokens.theme.colors;
    div()
        .id("column-inline-rename")
        .debug_selector(|| "column-inline-rename".to_owned())
        .flex_1()
        .min_w(px(0.0))
        // One 16px line plus the previous 4px inset. The row is taller so a
        // filename can wrap; the editor stays a single line inside that row.
        .h(px(COLUMN_FILENAME_LINE_HEIGHT + 4.0))
        .max_h(px(COLUMN_ROW_HEIGHT))
        .overflow_hidden()
        .flex()
        .items_center()
        .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
        .on_mouse_up(MouseButton::Left, |_, _, cx| cx.stop_propagation())
        .on_mouse_down(MouseButton::Right, |_, _, cx| cx.stop_propagation())
        .child(if let Some(input) = input {
            text_input("inline-rename-editor")
                .state(input)
                .multiline(false)
                .w_full()
                .min_w(px(0.0))
                .h(px(COLUMN_FILENAME_LINE_HEIGHT + 2.0))
                .px(px(4.0))
                .text_size(px(tokens.typography.file_row.size.value()))
                .line_height(px(COLUMN_FILENAME_LINE_HEIGHT))
                .whitespace_nowrap()
                .text_ellipsis()
                .overflow_hidden()
                .bg(colors.control_fill.to_gpui())
                .text_color(colors.text_primary.to_gpui())
                .border(px(1.0))
                .border_color(colors.focus.to_gpui())
                .into_any_element()
        } else {
            div()
                .debug_selector(|| "column-inline-rename-text".to_owned())
                .w_full()
                .min_w(px(0.0))
                .line_height(px(COLUMN_FILENAME_LINE_HEIGHT))
                .whitespace_nowrap()
                .text_ellipsis()
                .overflow_hidden()
                .px(px(4.0))
                .border(px(1.0))
                .border_color(colors.focus.to_gpui())
                .child(editor.buffer)
                .into_any_element()
        })
        .into_any_element()
}

fn column_row_icon(
    row: &ColumnRowModel,
    icons: &HashMap<explorer_model::ShellIconKey, Arc<RenderImage>>,
    icon_dpi: u16,
    icon_logical: u16,
    theme: explorer_model::ShellIconTheme,
) -> Option<Arc<RenderImage>> {
    let entry = FileEntry {
        id: row.id.clone(),
        display_name: row.name.clone(),
        location: row.location.clone(),
        is_container: row.is_container,
        metadata: row.metadata.clone(),
    };
    let key = crate::navigation_pane::file_icon_key_for_size(&entry, theme, icon_dpi, icon_logical);
    if let Some(texture) = icons.get(&key) {
        return Some(Arc::clone(texture));
    }
    if let Some(texture) = icons.iter().find_map(|(candidate, texture)| {
        (candidate.item_id.as_ref() == Some(&row.id)).then(|| Arc::clone(texture))
    }) {
        return Some(texture);
    }
    row.is_container
        .then(|| {
            icons.iter().find_map(|(candidate, texture)| {
                crate::navigation_pane::is_generic_breadcrumb_folder_icon_key(candidate)
                    .then(|| Arc::clone(texture))
            })
        })
        .flatten()
}

fn column_row_icon_element(
    element_id: &str,
    name: &str,
    is_container: bool,
    icon: Option<Arc<RenderImage>>,
) -> gpui::AnyElement {
    const ICON: f32 = 16.0;
    let icon_selector = element_id.to_owned();
    div()
        .id(SharedString::from(element_id.to_owned()))
        .debug_selector(move || icon_selector)
        .role(Role::Image)
        .aria_label(format!("column-icon {name}"))
        .w(px(ICON))
        .h(px(ICON))
        .min_w(px(ICON))
        .mr(px(6.0))
        .flex_none()
        .flex()
        .items_center()
        .justify_center()
        .child(match icon {
            Some(texture) => img(texture)
                .w(px(ICON))
                .h(px(ICON))
                .flex_none()
                .object_fit(ObjectFit::Contain)
                .into_any_element(),
            None => column_icon_fallback(is_container).into_any_element(),
        })
        .into_any_element()
}

fn column_icon_fallback(is_container: bool) -> gpui::Div {
    let palette = crate::theme::NavigationIconPalette::WINDOWS_11;
    if is_container {
        div()
            .w(px(14.0))
            .h(px(11.0))
            .flex_none()
            .rounded(px(1.5))
            .bg(palette.folder.to_gpui())
    } else {
        div()
            .w(px(11.0))
            .h(px(14.0))
            .flex_none()
            .rounded(px(1.5))
            .bg(palette.documents.to_gpui())
    }
}

/// Broker Preview Handler lifecycle as seen by the integrated column slot.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ColumnHandlerPhase {
    Inactive,
    Loading,
    Ready,
    Failed,
}

pub fn column_handler_phase(lifecycle: &explorer_model::PreviewLifecycle) -> ColumnHandlerPhase {
    match lifecycle {
        explorer_model::PreviewLifecycle::Visible { .. } => ColumnHandlerPhase::Ready,
        explorer_model::PreviewLifecycle::Failed { .. }
        | explorer_model::PreviewLifecycle::Fallback { .. } => ColumnHandlerPhase::Failed,
        explorer_model::PreviewLifecycle::Closed => ColumnHandlerPhase::Inactive,
        explorer_model::PreviewLifecycle::Idle
        | explorer_model::PreviewLifecycle::Debouncing { .. }
        | explorer_model::PreviewLifecycle::Loading { .. }
        | explorer_model::PreviewLifecycle::Unloading { .. } => ColumnHandlerPhase::Loading,
    }
}

/// What the integrated preview slot draws for one route and the current texture outcome.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ColumnPreviewChrome {
    pub show_image: bool,
    pub host_handler: bool,
    pub show_retry: bool,
    pub status_id: &'static str,
}

pub fn column_preview_chrome(
    route: ColumnPreviewRoute,
    has_texture: bool,
    failed: bool,
    handler: ColumnHandlerPhase,
) -> ColumnPreviewChrome {
    match route {
        ColumnPreviewRoute::None => ColumnPreviewChrome {
            show_image: false,
            host_handler: false,
            show_retry: false,
            status_id: "status-preview-select-one",
        },
        ColumnPreviewRoute::FolderSummary => ColumnPreviewChrome {
            show_image: false,
            host_handler: false,
            show_retry: false,
            status_id: "column-preview-folder",
        },
        ColumnPreviewRoute::Multiple => ColumnPreviewChrome {
            show_image: false,
            host_handler: false,
            show_retry: false,
            status_id: "column-preview-multiple",
        },
        ColumnPreviewRoute::PlainText => ColumnPreviewChrome {
            show_image: false,
            host_handler: false,
            show_retry: false,
            status_id: "status-preview-loading",
        },
        ColumnPreviewRoute::OfflineBlocked => ColumnPreviewChrome {
            show_image: false,
            host_handler: false,
            show_retry: false,
            status_id: "column-preview-offline",
        },
        ColumnPreviewRoute::ImageThumbnail if failed || !has_texture => ColumnPreviewChrome {
            show_image: false,
            host_handler: false,
            show_retry: failed,
            status_id: if failed {
                "status-preview-failed"
            } else {
                "status-preview-loading"
            },
        },
        ColumnPreviewRoute::ImageThumbnail => ColumnPreviewChrome {
            show_image: true,
            host_handler: false,
            show_retry: false,
            status_id: "chrome-preview-image-loaded",
        },
        ColumnPreviewRoute::PreviewHandler => {
            let failed_handler = failed || matches!(handler, ColumnHandlerPhase::Failed);
            let ready = !failed_handler && matches!(handler, ColumnHandlerPhase::Ready);
            ColumnPreviewChrome {
                show_image: false,
                host_handler: true,
                show_retry: failed_handler,
                status_id: if failed_handler {
                    "status-preview-failed"
                } else if ready {
                    "column-preview-handler-ready"
                } else {
                    "status-preview-loading"
                },
            }
        }
    }
}

/// Fitted image decision for the integrated slot. Zero or non-finite slots draw no image.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct IntegratedPreviewFrame {
    pub show_image: bool,
    pub host_handler: bool,
    pub show_retry: bool,
    pub status_id: &'static str,
    pub image_width: f32,
    pub image_height: f32,
}

pub fn preview_content_slot(pane_width: f32, pane_height: f32) -> (f32, f32) {
    const PAD_X: f32 = 24.0;
    const RESERVED_Y: f32 = 120.0;
    let width = if pane_width.is_finite() {
        (pane_width - PAD_X).max(0.0)
    } else {
        0.0
    };
    let height = if pane_height.is_finite() {
        (pane_height - RESERVED_Y).max(0.0)
    } else {
        0.0
    };
    (width, height)
}

pub fn integrated_preview_frame(
    route: ColumnPreviewRoute,
    source_width: u32,
    source_height: u32,
    has_texture: bool,
    failed: bool,
    handler: ColumnHandlerPhase,
    slot_width: f32,
    slot_height: f32,
) -> IntegratedPreviewFrame {
    let chrome = column_preview_chrome(route, has_texture, failed, handler);
    let (image_width, image_height) = if chrome.show_image {
        preview_aspect_fit(source_width, source_height, slot_width, slot_height)
    } else {
        (0.0, 0.0)
    };
    IntegratedPreviewFrame {
        show_image: chrome.show_image && image_width > 0.0 && image_height > 0.0,
        host_handler: chrome.host_handler,
        show_retry: chrome.show_retry,
        status_id: chrome.status_id,
        image_width,
        image_height,
    }
}

fn texture_pixel_size(texture: &RenderImage) -> (u32, u32) {
    let size = texture.size(0);
    (u32::from(size.width), u32::from(size.height))
}

fn column_preview(
    tokens: UiTokens,
    catalog: Catalog,
    width: f32,
    horizontal_offset: f32,
    title: String,
    detail: String,
    route: ColumnPreviewRoute,
    texture: Option<Arc<RenderImage>>,
    failed: bool,
    broker: Option<&'static str>,
    handler: ColumnHandlerPhase,
    pane_height: f32,
    on_action: Option<ActionCallback>,
    content: Option<crate::preview_content::PreviewContent>,
) -> impl IntoElement {
    let colors = tokens.theme.colors;
    let (source_width, source_height) = texture
        .as_ref()
        .map(|texture| texture_pixel_size(texture))
        .unwrap_or((0, 0));
    let (slot_width, mut slot_height) = preview_content_slot(width, pane_height);
    let has_image_info = route == ColumnPreviewRoute::ImageThumbnail && content.is_some();
    if has_image_info {
        slot_height = (slot_height - (pane_height * 0.4).min(240.0)).max(0.0);
    }
    let frame = integrated_preview_frame(
        route,
        source_width,
        source_height,
        texture.is_some(),
        failed,
        handler,
        slot_width,
        slot_height,
    );
    let native_content = route == ColumnPreviewRoute::PlainText || has_image_info;
    let status = match content.as_ref() {
        Some(crate::preview_content::PreviewContent::Failed(message)) if native_content => {
            message.clone()
        }
        _ => catalog.t(frame.status_id),
    };
    let image_label = catalog.t("chrome-preview-image-loaded");
    let retry_label = catalog.t("menu-retry-preview");
    div()
        .id("column-preview")
        .debug_selector(|| "column-preview".to_owned())
        .role(Role::Complementary)
        .aria_label(catalog.t("a11y-column-preview"))
        .relative()
        .overflow_hidden()
        .w(px(width))
        .h_full()
        .flex_none()
        .flex()
        .flex_col()
        .gap(px(8.0))
        .p(px(12.0))
        .border_l(px(1.0))
        .border_color(colors.divider.to_gpui())
        .bg(colors.surface.to_gpui())
        .on_scroll_wheel({
            let callback = on_action.clone();
            move |event, window, cx| {
                if event.modifiers.control {
                    return;
                }
                let (delta_x, delta_y) = match event.delta {
                    gpui::ScrollDelta::Pixels(delta) => (f32::from(delta.x), f32::from(delta.y)),
                    gpui::ScrollDelta::Lines(delta) => {
                        (delta.x * COLUMN_ROW_HEIGHT, delta.y * COLUMN_ROW_HEIGHT)
                    }
                };
                if native_content && !event.modifiers.shift && delta_y.abs() >= delta_x.abs() {
                    return;
                }
                let Some(action) = column_wheel_action(
                    delta_x,
                    delta_y,
                    event.modifiers.shift,
                    None,
                    0.0,
                    horizontal_offset,
                    None,
                ) else {
                    return;
                };
                if !matches!(action, ExplorerAction::SetColumnHorizontalOffset { .. }) {
                    return;
                }
                if let Some(callback) = callback.clone() {
                    callback(&action, window, cx);
                    cx.stop_propagation();
                }
            }
        })
        .child(
            div()
                .id("column-preview-divider")
                .role(Role::Splitter)
                .aria_label(catalog.t("a11y-resize-side-pane"))
                .aria_numeric_value(f64::from(width))
                .aria_min_numeric_value(f64::from(explorer_model::COLUMN_PREVIEW_WIDTH_MIN))
                .aria_max_numeric_value(f64::from(explorer_model::COLUMN_PREVIEW_WIDTH_MAX))
                .absolute()
                .left_0()
                .top_0()
                .bottom_0()
                .w(px(tokens.layout.divider_width.value().max(6.0)))
                .cursor_col_resize()
                .when_some(on_action.clone(), |divider, callback| {
                    divider.on_mouse_down(MouseButton::Left, move |event, window, cx| {
                        cx.stop_propagation();
                        callback(
                            &ExplorerAction::BeginColumnPreviewResize {
                                pointer_x: f32::from(event.position.x),
                            },
                            window,
                            cx,
                        );
                    })
                }),
        )
        .child(
            div()
                .id("column-preview-name")
                .role(Role::Label)
                .aria_label(title.clone())
                .flex_none()
                .line_height(px(18.0))
                .line_clamp(2)
                .max_h(px(36.0))
                .overflow_hidden()
                .child(title),
        )
        .child(
            div()
                .id("column-preview-detail")
                .flex_none()
                .line_clamp(2)
                .text_color(colors.text_secondary.to_gpui())
                .child(detail),
        )
        .when_some(texture.filter(|_| frame.show_image), |pane, texture| {
            pane.child(
                div()
                    .id("column-preview-image")
                    .role(Role::Image)
                    .aria_label(image_label)
                    .relative()
                    .w_full()
                    .flex_1()
                    .min_h(px(0.0))
                    .overflow_hidden()
                    .flex()
                    .items_center()
                    .justify_center()
                    .child(
                        img(texture)
                            .w(px(frame.image_width))
                            .h(px(frame.image_height))
                            .max_w_full()
                            .max_h_full()
                            .flex_none()
                            .object_fit(ObjectFit::Contain),
                    ),
            )
        })
        .when(
            !frame.show_image && route != ColumnPreviewRoute::PlainText,
            |pane| {
                pane.child(
                    div()
                        .id("column-preview-status")
                        .role(Role::Status)
                        .aria_label(status.clone())
                        .relative()
                        .w_full()
                        .flex_1()
                        .min_h(px(0.0))
                        .overflow_hidden()
                        .child(status)
                        .when(frame.host_handler, |host| {
                            host.child(
                                div()
                                    .id("column-preview-host")
                                    .debug_selector(|| "column-preview-host".to_owned())
                                    .absolute()
                                    .inset_0()
                                    .size_full()
                                    .overflow_hidden()
                                    .child(crate::chrome::preview_host_boundary_probe(
                                        on_action.clone(),
                                    )),
                            )
                        }),
                )
            },
        )
        .when(
            route == ColumnPreviewRoute::PlainText || has_image_info,
            |pane| {
                pane.child(native_preview_content(
                    content,
                    route == ColumnPreviewRoute::ImageThumbnail,
                    pane_height,
                    colors,
                    catalog,
                ))
            },
        )
        .when(frame.show_retry, |pane| {
            pane.child(
                div()
                    .id("column-preview-retry")
                    .role(Role::Button)
                    .aria_label(retry_label.clone())
                    .flex_none()
                    .px(px(8.0))
                    .py(px(4.0))
                    .child(retry_label)
                    .when_some(on_action.clone(), |button, callback| {
                        button.on_mouse_down(MouseButton::Left, move |_, window, cx| {
                            cx.stop_propagation();
                            callback(&ExplorerAction::RetryExtensionBroker, window, cx);
                        })
                    }),
            )
        })
        .when(frame.host_handler, |pane| {
            pane.when_some(broker, |pane, broker| {
                pane.child(
                    div()
                        .id("column-preview-broker")
                        .role(Role::Status)
                        .aria_label(broker)
                        .child(broker),
                )
            })
        })
}

fn native_preview_content(
    content: Option<crate::preview_content::PreviewContent>,
    image: bool,
    pane_height: f32,
    colors: crate::theme::SemanticColors,
    catalog: Catalog,
) -> impl IntoElement {
    use crate::preview_content::PreviewContent;
    let mut body = div()
        .flex()
        .flex_col()
        .gap(px(6.0))
        .text_size(px(13.0))
        .line_height(px(18.0));
    match content {
        Some(PreviewContent::Text {
            text,
            truncated,
            encoding,
        }) => {
            let count = text.chars().count();
            body = body
                .child(
                    div()
                        .text_color(colors.text_secondary.to_gpui())
                        .child(format!("{encoding} · {count} 字")),
                )
                .child(
                    div()
                        .id("column-preview-plain-text")
                        .debug_selector(|| "column-preview-plain-text".to_owned())
                        .text_color(colors.text_primary.to_gpui())
                        .child(if text.is_empty() {
                            "（空白檔案）".to_owned()
                        } else {
                            text
                        }),
                );
            if truncated {
                body = body.child(
                    div()
                        .id("column-preview-text-limit")
                        .text_color(colors.text_secondary.to_gpui())
                        .child("僅顯示前 3000 字"),
                );
            }
        }
        Some(PreviewContent::Image {
            width,
            height,
            bits_per_pixel,
            color,
            size,
            exif,
            metadata_note,
        }) => {
            let size = size.map_or_else(
                || "未知".to_owned(),
                |size| format!("{size} bytes ({:.2} MiB)", size as f64 / 1_048_576.0),
            );
            body = body
                .child(div().child(format!("尺寸：{width} × {height} px")))
                .child(div().child(format!("檔案大小：{size}")))
                .child(div().child(format!("色彩深度：{bits_per_pixel} bits/pixel")))
                .child(div().child(format!("色彩格式：{color}")))
                .child(div().child("EXIF"));
            for (tag, value) in exif {
                body = body.child(div().child(format!("{tag}：{value}")));
            }
            if let Some(note) = metadata_note {
                body = body.child(
                    div()
                        .text_color(colors.text_secondary.to_gpui())
                        .child(note),
                );
            }
        }
        Some(PreviewContent::Failed(message)) => {
            body = body.child(div().child(message));
        }
        _ => {
            body = body.child(div().child(catalog.t("status-preview-loading")));
        }
    }
    div()
        .id("column-preview-native-content")
        .debug_selector(|| "column-preview-native-content".to_owned())
        .role(Role::Group)
        .aria_label(if image {
            "圖片資訊與 EXIF"
        } else {
            "純文字預覽"
        })
        .w_full()
        .min_h(px(0.0))
        .overflow_y_scroll()
        .when(image, |pane| {
            pane.h(px((pane_height * 0.4).clamp(48.0, 240.0)))
                .flex_none()
        })
        .when(!image, |pane| pane.flex_1())
        .child(body)
}

fn column_status_elements(
    index: usize,
    status: ColumnPaneStatus,
    status_text: String,
    offers_refresh: bool,
    colors: crate::theme::SemanticColors,
    catalog: Catalog,
    on_action: Option<ActionCallback>,
) -> Vec<gpui::AnyElement> {
    if status == ColumnPaneStatus::Ready || status_text.is_empty() {
        return Vec::new();
    }
    let mut elements = vec![
        div()
            .id(SharedString::from(format!("column-status-{index}")))
            .role(Role::Status)
            .aria_label(status_text.clone())
            .px(px(8.0))
            .py(px(8.0))
            .text_color(colors.text_secondary.to_gpui())
            .child(status_text)
            .into_any_element(),
    ];
    if offers_refresh {
        let retry = catalog.t("menu-retry");
        elements.push(
            div()
                .id(SharedString::from(format!("column-retry-{index}")))
                .role(Role::Button)
                .aria_label(retry.clone())
                .flex_none()
                .px(px(8.0))
                .py(px(4.0))
                .text_color(colors.accent.to_gpui())
                .child(retry)
                .when_some(on_action, |button, callback| {
                    button.on_mouse_down(MouseButton::Left, move |_, window, cx| {
                        cx.stop_propagation();
                        // Same command as F5. There is no separate column file service.
                        callback(&ExplorerAction::Refresh, window, cx);
                    })
                })
                .into_any_element(),
        );
    }
    elements
}

pub fn probe_drive_kind(path: &std::path::Path) -> explorer_model::DriveKind {
    crate::drive_probe::probe_drive_kind(path)
}

/// How the integrated column preview should use the existing preview services.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ColumnPreviewRoute {
    None,
    FolderSummary,
    Multiple,
    OfflineBlocked,
    ImageThumbnail,
    PlainText,
    PreviewHandler,
}

pub fn column_preview_route(entries: &[FileEntry]) -> ColumnPreviewRoute {
    match entries {
        [] => ColumnPreviewRoute::None,
        [entry] if entry.is_container => ColumnPreviewRoute::FolderSummary,
        [entry] if offline_placeholder(entry) => ColumnPreviewRoute::OfflineBlocked,
        [entry] if previewable_image(&entry.location) => ColumnPreviewRoute::ImageThumbnail,
        [entry] if crate::preview_content::plain_text(&entry.location) => {
            ColumnPreviewRoute::PlainText
        }
        [_] => ColumnPreviewRoute::PreviewHandler,
        _ => ColumnPreviewRoute::Multiple,
    }
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

/// Aspect-preserving size inside the integrated preview host. Zero inputs collapse to zero.
pub fn preview_aspect_fit(
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

pub fn column_accessible_names(
    level: usize,
    folder: &str,
    item: &str,
    kind: &str,
) -> (String, String, String) {
    (
        format!("Column {} {folder}", level + 1),
        format!("Item {item} {kind}"),
        "Integrated preview".to_owned(),
    )
}

fn column_filename_text(name: String) -> gpui::StyledText {
    let text = gpui::StyledText::new(name);
    #[cfg(test)]
    FILENAME_LAYOUT_PROBE.with(|probe| {
        if let Some(records) = probe.borrow().as_ref() {
            records.borrow_mut().push(text.layout().clone());
        }
    });
    text
}

#[cfg(test)]
thread_local! {
    static FILENAME_LAYOUT_PROBE: std::cell::RefCell<Option<Rc<std::cell::RefCell<Vec<gpui::TextLayout>>>>> =
        const { std::cell::RefCell::new(None) };
}

#[cfg(test)]
mod tests {
    use super::{
        COLUMN_ROW_HEIGHT, ColumnPreviewRoute, ColumnRowModel, ColumnScrollbarHit,
        column_accessible_names, column_list_viewport_height, column_preview_chrome,
        column_preview_route, column_row_icon, column_scrollbar_hit, column_scrollbar_metrics,
        column_vertical_wheel_offset, column_wheel_action, preview_aspect_fit,
    };
    use crate::actions::ExplorerAction;
    use explorer_model::{FileEntry, FileEntryMetadata, LocationDescriptor, ShellItemId};
    use gpui::{MouseButton, div, prelude::*, px};
    use std::{cell::RefCell, rc::Rc};

    fn entry(name: &str, container: bool, attributes: u32) -> FileEntry {
        FileEntry {
            id: ShellItemId::from_provider_bytes(name.as_bytes()).expect("id"),
            display_name: name.to_owned(),
            location: LocationDescriptor::file_system(format!(r"C:\fixture\{name}")),
            is_container: container,
            metadata: FileEntryMetadata {
                filesystem_attributes: attributes,
                ..FileEntryMetadata::default()
            },
        }
    }

    #[test]
    fn preview_route_uses_thumbnail_handler_or_blocks_offline_hydration() {
        assert_eq!(column_preview_route(&[]), ColumnPreviewRoute::None);
        assert_eq!(
            column_preview_route(&[entry("folder", true, 0)]),
            ColumnPreviewRoute::FolderSummary
        );
        assert_eq!(
            column_preview_route(&[entry("photo.jpg", false, 0)]),
            ColumnPreviewRoute::ImageThumbnail
        );
        assert_eq!(
            column_preview_route(&[entry("notes.txt", false, 0)]),
            ColumnPreviewRoute::PlainText
        );
        assert_eq!(
            column_preview_route(&[entry("cloud.docx", false, 0x1000)]),
            ColumnPreviewRoute::OfflineBlocked
        );
        assert_eq!(
            column_preview_route(&[entry("a.txt", false, 0), entry("b.txt", false, 0)]),
            ColumnPreviewRoute::Multiple
        );
    }

    #[test]
    fn preview_chrome_shows_an_image_only_for_a_ready_thumbnail() {
        let image = column_preview_chrome(
            ColumnPreviewRoute::ImageThumbnail,
            true,
            false,
            super::ColumnHandlerPhase::Inactive,
        );
        assert!(image.show_image);
        assert!(!image.host_handler);
        assert!(!image.show_retry);
        assert_eq!(image.status_id, "chrome-preview-image-loaded");
        let (width, height) = preview_aspect_fit(400, 200, 180.0, 120.0);
        assert!(width <= 180.0 && height <= 120.0);
        assert!((width / height - 2.0).abs() < 0.01);

        let loading = column_preview_chrome(
            ColumnPreviewRoute::ImageThumbnail,
            false,
            false,
            super::ColumnHandlerPhase::Inactive,
        );
        assert!(!loading.show_image);
        assert!(!loading.show_retry);
        assert_eq!(loading.status_id, "status-preview-loading");
        let failed = column_preview_chrome(
            ColumnPreviewRoute::ImageThumbnail,
            true,
            true,
            super::ColumnHandlerPhase::Inactive,
        );
        assert!(!failed.show_image);
        assert!(!failed.host_handler);
        assert!(failed.show_retry);
        assert_eq!(failed.status_id, "status-preview-failed");

        for route in [
            ColumnPreviewRoute::None,
            ColumnPreviewRoute::FolderSummary,
            ColumnPreviewRoute::Multiple,
            ColumnPreviewRoute::OfflineBlocked,
            ColumnPreviewRoute::PlainText,
        ] {
            let chrome =
                column_preview_chrome(route, true, false, super::ColumnHandlerPhase::Ready);
            assert!(!chrome.show_image, "{route:?} must not paint a stale image");
            assert!(
                !chrome.host_handler,
                "{route:?} must not host a preview handler"
            );
            assert!(!chrome.show_retry, "{route:?} must not offer retry");
            if route == ColumnPreviewRoute::PlainText {
                assert_eq!(
                    chrome.status_id, "status-preview-loading",
                    "plain text waits for its bounded reader"
                );
            } else {
                assert_ne!(chrome.status_id, "status-preview-loading");
            }
        }

        let handler = column_preview_chrome(
            ColumnPreviewRoute::PreviewHandler,
            false,
            false,
            super::ColumnHandlerPhase::Loading,
        );
        assert!(handler.host_handler);
        assert!(!handler.show_image);
        assert_eq!(handler.status_id, "status-preview-loading");
        let handler_ready = column_preview_chrome(
            ColumnPreviewRoute::PreviewHandler,
            true,
            false,
            super::ColumnHandlerPhase::Ready,
        );
        assert!(handler_ready.host_handler);
        assert!(!handler_ready.show_image);
        assert!(!handler_ready.show_retry);
        assert_eq!(handler_ready.status_id, "column-preview-handler-ready");
        let handler_failed = column_preview_chrome(
            ColumnPreviewRoute::PreviewHandler,
            false,
            true,
            super::ColumnHandlerPhase::Failed,
        );
        assert!(handler_failed.host_handler);
        assert!(handler_failed.show_retry);
        assert_eq!(handler_failed.status_id, "status-preview-failed");
    }

    #[test]
    fn integrated_preview_frame_fits_a_jpeg_and_refuses_false_loading() {
        use super::{
            ColumnHandlerPhase, IntegratedPreviewFrame, column_handler_phase,
            integrated_preview_frame, preview_content_slot,
        };
        use explorer_model::PreviewLifecycle;

        let (slot_width, slot_height) = preview_content_slot(360.0, 480.0);
        assert!(slot_width > 0.0 && slot_height > 0.0);
        assert!(slot_width < 360.0 && slot_height < 480.0);
        let frame = integrated_preview_frame(
            ColumnPreviewRoute::ImageThumbnail,
            400,
            200,
            true,
            false,
            ColumnHandlerPhase::Inactive,
            slot_width,
            slot_height,
        );
        assert!(frame.show_image);
        assert!(!frame.host_handler);
        assert_eq!(frame.status_id, "chrome-preview-image-loaded");
        assert!(frame.image_width <= slot_width && frame.image_height <= slot_height);
        assert!((frame.image_width / frame.image_height - 2.0).abs() < 0.01);

        let states = [
            (
                ColumnPreviewRoute::None,
                true,
                false,
                ColumnHandlerPhase::Inactive,
                "status-preview-select-one",
            ),
            (
                ColumnPreviewRoute::FolderSummary,
                true,
                false,
                ColumnHandlerPhase::Ready,
                "column-preview-folder",
            ),
            (
                ColumnPreviewRoute::Multiple,
                true,
                false,
                ColumnHandlerPhase::Loading,
                "column-preview-multiple",
            ),
            (
                ColumnPreviewRoute::OfflineBlocked,
                true,
                false,
                ColumnHandlerPhase::Failed,
                "column-preview-offline",
            ),
            (
                ColumnPreviewRoute::ImageThumbnail,
                false,
                false,
                ColumnHandlerPhase::Inactive,
                "status-preview-loading",
            ),
            (
                ColumnPreviewRoute::ImageThumbnail,
                true,
                true,
                ColumnHandlerPhase::Inactive,
                "status-preview-failed",
            ),
        ];
        for (route, has_texture, failed, handler, status_id) in states {
            let observed: IntegratedPreviewFrame = integrated_preview_frame(
                route,
                400,
                200,
                has_texture,
                failed,
                handler,
                slot_width,
                slot_height,
            );
            assert!(
                !observed.show_image,
                "{route:?} must not keep a stale image"
            );
            assert!(!observed.host_handler, "{route:?} must not host a handler");
            assert_eq!(observed.status_id, status_id, "{route:?}");
            assert_eq!(observed.image_width, 0.0);
            assert_eq!(observed.image_height, 0.0);
            if matches!(
                route,
                ColumnPreviewRoute::None
                    | ColumnPreviewRoute::FolderSummary
                    | ColumnPreviewRoute::Multiple
                    | ColumnPreviewRoute::OfflineBlocked
            ) {
                assert_ne!(observed.status_id, "status-preview-loading");
                assert!(!observed.show_retry);
            }
        }

        let loading_handler = integrated_preview_frame(
            ColumnPreviewRoute::PreviewHandler,
            0,
            0,
            false,
            false,
            ColumnHandlerPhase::Loading,
            slot_width,
            slot_height,
        );
        assert!(loading_handler.host_handler);
        assert!(!loading_handler.show_image);
        assert_eq!(loading_handler.status_id, "status-preview-loading");
        let ready_handler = integrated_preview_frame(
            ColumnPreviewRoute::PreviewHandler,
            400,
            200,
            true,
            false,
            ColumnHandlerPhase::Ready,
            slot_width,
            slot_height,
        );
        assert!(ready_handler.host_handler);
        assert!(!ready_handler.show_image);
        assert_ne!(ready_handler.status_id, "status-preview-loading");
        assert_eq!(ready_handler.status_id, "column-preview-handler-ready");
        let failed_handler = integrated_preview_frame(
            ColumnPreviewRoute::PreviewHandler,
            0,
            0,
            false,
            false,
            ColumnHandlerPhase::Failed,
            slot_width,
            slot_height,
        );
        assert!(failed_handler.show_retry);
        assert_eq!(failed_handler.status_id, "status-preview-failed");
        assert_eq!(
            integrated_preview_frame(
                ColumnPreviewRoute::ImageThumbnail,
                0,
                10,
                true,
                false,
                ColumnHandlerPhase::Inactive,
                slot_width,
                slot_height,
            )
            .show_image,
            false,
            "a zero-size texture must not paint an image"
        );

        assert_eq!(
            column_handler_phase(&PreviewLifecycle::Closed),
            ColumnHandlerPhase::Inactive
        );
        assert_eq!(
            column_handler_phase(&PreviewLifecycle::Loading {
                generation: explorer_model::Generation::default(),
            }),
            ColumnHandlerPhase::Loading
        );
        assert_eq!(
            column_handler_phase(&PreviewLifecycle::Visible {
                generation: explorer_model::Generation::default(),
            }),
            ColumnHandlerPhase::Ready
        );
        assert_eq!(
            column_handler_phase(&PreviewLifecycle::Failed {
                generation: explorer_model::Generation::default(),
                retryable: true,
            }),
            ColumnHandlerPhase::Failed
        );

        let source = include_str!("column_view.rs");
        let production = source.split("#cfg(test)").next().unwrap_or(source);
        assert!(production.contains("integrated_preview_frame("));
        assert!(production.contains("object_fit(ObjectFit::Contain)"));
        assert!(production.contains("column-preview-host"));
        assert!(production.contains("preview_host_boundary_probe"));
        assert!(!production.contains("preview-side-pane"));
    }

    #[test]
    fn preview_aspect_fit_preserves_ratio_inside_the_host() {
        let (width, height) = preview_aspect_fit(400, 200, 100.0, 100.0);
        assert!((width - 100.0).abs() < 0.01);
        assert!((height - 50.0).abs() < 0.01);
        assert!(width <= 100.0 && height <= 100.0);
        assert_eq!(preview_aspect_fit(0, 10, 100.0, 100.0), (0.0, 0.0));
        let (level, row, preview) = column_accessible_names(2, "Docs", "photo.jpg", "File");
        assert!(level.contains("3") && level.contains("Docs"));
        assert!(row.contains("photo.jpg"));
        assert!(!preview.is_empty());
    }

    #[test]
    fn preview_host_client_rect_stays_in_window_client_pixels_across_scale_and_clip() {
        use crate::chrome::{PreviewHostClientRect, preview_host_client_rect};

        let client = preview_host_client_rect(900.0, 140.0, 280.0, 360.0, 1.0, 1280.0, 800.0)
            .expect("100% preview slot");
        assert_eq!(
            client,
            PreviewHostClientRect {
                left_physical: 900,
                top_physical: 140,
                width_physical: 280,
                height_physical: 360,
                dpi: 96,
            }
        );
        // A screen origin of (40, 40) must not be subtracted from client bounds.
        assert_ne!(client.left_physical, 900 - 40);
        assert_ne!(client.top_physical, 140 - 40);

        let wider = preview_host_client_rect(820.0, 140.0, 360.0, 360.0, 1.0, 1280.0, 800.0)
            .expect("dragged preview width");
        assert_eq!(wider.left_physical, 820);
        assert_eq!(wider.width_physical, 360);
        assert!(wider.width_physical > client.width_physical);

        let scaled = preview_host_client_rect(900.0, 140.0, 280.0, 360.0, 1.5, 1280.0, 800.0)
            .expect("150% dpi");
        assert_eq!(
            scaled,
            PreviewHostClientRect {
                left_physical: 1350,
                top_physical: 210,
                width_physical: 420,
                height_physical: 540,
                dpi: 144,
            }
        );
        let dpi_125 = preview_host_client_rect(200.0, 80.0, 100.0, 40.0, 1.25, 1280.0, 800.0)
            .expect("125% dpi");
        assert_eq!(dpi_125.dpi, 120);
        assert_eq!(dpi_125.left_physical, 250);
        assert_eq!(dpi_125.width_physical, 125);

        let clipped = preview_host_client_rect(-40.0, 10.0, 100.0, 40.0, 1.0, 1280.0, 800.0)
            .expect("negative client origin is clipped, not shifted onto the columns");
        assert_eq!(clipped.left_physical, 0);
        assert_eq!(clipped.width_physical, 60);

        assert!(preview_host_client_rect(0.0, 0.0, 0.0, 10.0, 1.0, 100.0, 100.0).is_none());
        assert!(preview_host_client_rect(0.0, 0.0, 10.0, 10.0, f32::NAN, 100.0, 100.0).is_none());

        let chrome = include_str!("chrome.rs");
        let probe = chrome
            .split("pub(crate) fn preview_host_boundary_probe")
            .nth(1)
            .expect("probe")
            .split("pub struct CommandBar")
            .next()
            .expect("probe body");
        assert!(probe.contains("preview_host_client_rect"));
        assert!(
            !probe.contains("window.bounds()"),
            "the host probe must not treat window.bounds() as a client origin"
        );
    }

    #[test]
    fn column_row_icon_uses_the_file_view_key_then_item_id_then_generic_folder() {
        use explorer_model::ShellIconTheme;
        use std::sync::Arc;

        let row = ColumnRowModel {
            id: ShellItemId::from_provider_bytes(b"docs").expect("id"),
            name: "Docs".to_owned(),
            location: LocationDescriptor::file_system(r"C:\fixture\Docs"),
            is_container: true,
            metadata: FileEntryMetadata::default(),
            selected: false,
            branch_selected: false,
        };
        let entry = FileEntry {
            id: row.id.clone(),
            display_name: row.name.clone(),
            location: row.location.clone(),
            is_container: row.is_container,
            metadata: row.metadata.clone(),
        };
        let presentation =
            crate::navigation_pane::file_icon_key_for_size(&entry, ShellIconTheme::Light, 96, 20);
        let specific = Arc::new(gpui::RenderImage::new(smallvec::SmallVec::<
            [image::Frame; 1],
        >::new()));
        let other = Arc::new(gpui::RenderImage::new(smallvec::SmallVec::<
            [image::Frame; 1],
        >::new()));
        let generic = Arc::new(gpui::RenderImage::new(smallvec::SmallVec::<
            [image::Frame; 1],
        >::new()));
        let mut icons = std::collections::HashMap::new();
        icons.insert(presentation.clone(), Arc::clone(&specific));
        assert!(Arc::ptr_eq(
            &column_row_icon(&row, &icons, 96, 20, ShellIconTheme::Light).expect("specific"),
            &specific
        ));

        icons.clear();
        let mut mismatched = presentation.clone();
        mismatched.size_bucket = mismatched.size_bucket.saturating_add(8);
        icons.insert(mismatched, Arc::clone(&other));
        assert!(Arc::ptr_eq(
            &column_row_icon(&row, &icons, 96, 20, ShellIconTheme::Light).expect("item"),
            &other
        ));

        icons.clear();
        let generic_key = crate::navigation_pane::generic_breadcrumb_folder_icon_key(
            ShellIconTheme::Light,
            96,
            1,
        );
        icons.insert(generic_key, Arc::clone(&generic));
        assert!(Arc::ptr_eq(
            &column_row_icon(&row, &icons, 96, 20, ShellIconTheme::Light).expect("generic"),
            &generic
        ));
    }

    #[test]
    fn column_view_rows_wire_context_menu_drag_drop_and_inline_rename() {
        let source = include_str!("column_view.rs");
        let production = source.split("#[cfg(test)]").next().unwrap_or(source);
        for required in [
            "PressColumnRow",
            "BeginContextItemGesture",
            "ShowContextMenu",
            "DropOnColumn",
            "negotiate_column_drop",
            "column_drag_hover_effect",
            "on_drag_move::<gpui::ExternalPaths>",
            "column-background-",
            "column-inline-rename",
            "inline-rename-editor",
            "FinishColumnPress",
        ] {
            assert!(
                production.contains(required),
                "column row command path missing {required}"
            );
        }
        assert!(
            !production.contains("negotiate_external_paths(paths, true, None)"),
            "column drop must name the directory under the pointer"
        );
    }

    #[test]
    fn column_drop_names_the_folder_under_the_pointer() {
        let paths = gpui::ExternalPaths::with_metadata(
            [std::path::PathBuf::from(r"C:\alpha\notes.txt")]
                .into_iter()
                .collect(),
            gpui::ExternalDropMetadata {
                allowed: gpui::ExternalDropEffects {
                    copy: true,
                    move_item: true,
                    link: false,
                },
                ..gpui::ExternalDropMetadata::default()
            },
        );
        let folder = LocationDescriptor::file_system(r"C:\alpha\sibling");
        let parent = LocationDescriptor::file_system(r"C:\alpha");
        assert_eq!(
            crate::chrome::negotiate_external_paths(&paths, true, None),
            explorer_model::DragEffect::None,
            "omitting the destination cancels the drop"
        );
        assert_eq!(
            super::negotiate_column_drop(&paths, &folder),
            explorer_model::DragEffect::Move
        );
        assert_eq!(
            super::negotiate_column_drop(&paths, &parent),
            explorer_model::DragEffect::None,
            "moving a file onto its current parent is not a transfer"
        );
        let elsewhere = gpui::ExternalPaths::with_metadata(
            [std::path::PathBuf::from(r"C:\alpha\beta\deep.txt")]
                .into_iter()
                .collect(),
            gpui::ExternalDropMetadata {
                allowed: gpui::ExternalDropEffects {
                    copy: true,
                    move_item: true,
                    link: false,
                },
                ..gpui::ExternalDropMetadata::default()
            },
        );
        assert_eq!(
            super::negotiate_column_drop(&elsewhere, &parent),
            explorer_model::DragEffect::Move,
            "a file from another column can move into this column directory"
        );
    }

    #[test]
    fn column_status_text_distinguishes_empty_loading_and_error_and_retry_uses_refresh() {
        let catalog = explorer_i18n::Catalog::new(explorer_i18n::AppLocale::En);
        let empty = catalog.t("column-empty");
        let denied = "Access is denied.";
        assert_eq!(
            super::column_status_text(catalog, super::ColumnPaneStatus::Empty, None),
            empty
        );
        assert_eq!(
            super::column_status_text(catalog, super::ColumnPaneStatus::Loading, None),
            catalog.t("column-loading")
        );
        assert_eq!(
            super::column_status_text(catalog, super::ColumnPaneStatus::Ready, None),
            ""
        );
        let error = super::column_status_text(
            catalog,
            super::ColumnPaneStatus::Error(explorer_model::ColumnFault::Inaccessible),
            Some(denied),
        );
        assert_eq!(error, denied);
        assert_ne!(error, empty);
        let generic = super::column_status_text(
            catalog,
            super::ColumnPaneStatus::Error(explorer_model::ColumnFault::Inaccessible),
            Some(empty.as_str()),
        );
        assert_eq!(generic, catalog.t("column-error"));
        assert_ne!(generic, empty);
        assert!(super::column_status_offers_refresh(
            super::ColumnPaneStatus::Empty
        ));
        assert!(super::column_status_offers_refresh(
            super::ColumnPaneStatus::Error(explorer_model::ColumnFault::Inaccessible)
        ));
        assert!(!super::column_status_offers_refresh(
            super::ColumnPaneStatus::Loading
        ));
        assert!(!super::column_status_offers_refresh(
            super::ColumnPaneStatus::Ready
        ));
        let named = super::column_level_accessible_name(catalog, 1, "empty", &empty);
        assert!(named.contains("empty"));
        assert!(named.contains(&empty));
        assert!(named.contains(&catalog.t("a11y-column-level")));

        let mut snapshot = explorer_model::DirectorySnapshot::default();
        assert_eq!(
            super::directory_column_status(&explorer_model::DirectoryState::Ready(
                snapshot.clone()
            ))
            .0,
            super::ColumnPaneStatus::Empty
        );
        let _ = snapshot.upsert(entry("keep.txt", false, 0));
        assert_eq!(
            super::directory_column_status(&explorer_model::DirectoryState::Ready(
                snapshot.clone()
            ))
            .0,
            super::ColumnPaneStatus::Ready
        );
        assert_eq!(
            super::directory_column_status(&explorer_model::DirectoryState::Idle).0,
            super::ColumnPaneStatus::Loading
        );
        let request = explorer_model::RequestContext::new(
            explorer_model::TabId::new(),
            explorer_model::Generation::default(),
        );
        let (loading, _) =
            super::directory_column_status(&explorer_model::DirectoryState::Loading {
                request,
                snapshot: snapshot.clone(),
                seen: std::collections::HashSet::new(),
            });
        assert_eq!(loading, super::ColumnPaneStatus::Loading);
        let (failed, detail) =
            super::directory_column_status(&explorer_model::DirectoryState::Error {
                error: explorer_common::ExplorerError::new(
                    explorer_common::ExplorerErrorKind::Authorization,
                    "enumerate",
                    true,
                    denied,
                    "0x80070005",
                ),
                previous: snapshot,
            });
        assert_eq!(
            failed,
            super::ColumnPaneStatus::Error(explorer_model::ColumnFault::Inaccessible)
        );
        assert_eq!(detail.as_deref(), Some(denied));
        assert_ne!(detail.as_deref(), Some(empty.as_str()));

        let partial = explorer_model::DirectorySnapshot::default();
        assert_eq!(
            super::auxiliary_column_status(&explorer_model::ColumnPhase::Pending, false),
            super::ColumnPaneStatus::Loading
        );
        assert_eq!(
            super::auxiliary_column_status(&explorer_model::ColumnPhase::Loading, true),
            super::ColumnPaneStatus::Loading
        );
        assert_eq!(
            super::auxiliary_column_status(&explorer_model::ColumnPhase::Empty, false),
            super::ColumnPaneStatus::Empty
        );
        assert_eq!(
            super::auxiliary_column_status(&explorer_model::ColumnPhase::Ready(partial), true),
            super::ColumnPaneStatus::Loading
        );
        assert_eq!(
            super::auxiliary_column_status(
                &explorer_model::ColumnPhase::Error(explorer_model::ColumnFault::Inaccessible),
                false
            ),
            super::ColumnPaneStatus::Error(explorer_model::ColumnFault::Inaccessible)
        );

        let source = include_str!("column_view.rs");
        let production = source.split("#[cfg(test)]").next().unwrap_or(source);
        for required in [
            "column-status-",
            "column-retry-",
            "Role::Status",
            "Role::Button",
            "ExplorerAction::Refresh",
            "column_level_accessible_name(",
            "column_status_text(",
        ] {
            assert!(
                production.contains(required),
                "empty/error column chrome missing {required}"
            );
        }
    }

    #[test]
    fn column_surface_exposes_stable_accessible_roles() {
        let source = include_str!("column_view.rs");
        let production = source.split("#cfg(test)").next().unwrap_or(source);
        for required in [
            "Role::List",
            "Role::ListItem",
            "Role::Image",
            "Role::Splitter",
            "Role::Complementary",
            "a11y-column-view",
            "a11y-column-preview",
            "column-preview",
        ] {
            assert!(production.contains(required), "{required}");
        }
        assert!(!production.contains("preview-side-pane"));
    }

    #[test]
    fn column_wheel_keeps_vertical_scroll_and_routes_shift_or_trackpad_horizontally() {
        assert_eq!(
            column_wheel_action(0.0, -24.0, false, Some(1), 40.0, 100.0, None),
            Some(ExplorerAction::SetColumnScroll {
                column_index: 1,
                offset: 64.0,
            })
        );
        assert_eq!(
            column_wheel_action(0.0, -24.0, true, Some(1), 40.0, 100.0, None),
            Some(ExplorerAction::SetColumnHorizontalOffset { offset: 124.0 })
        );
        assert_eq!(
            column_wheel_action(30.0, -4.0, false, Some(2), 10.0, 80.0, None),
            Some(ExplorerAction::SetColumnHorizontalOffset { offset: 50.0 })
        );
        assert_eq!(
            column_wheel_action(0.0, -24.0, true, None, 0.0, 10.0, None),
            Some(ExplorerAction::SetColumnHorizontalOffset { offset: 34.0 })
        );
        assert!(column_wheel_action(0.0, 0.0, true, Some(0), 0.0, 0.0, None).is_none());
    }

    #[test]
    fn column_vertical_wheel_cannot_scroll_an_empty_or_short_list_into_blank() {
        let row = COLUMN_ROW_HEIGHT;
        assert_eq!(column_list_viewport_height(40.0, 0.0), 40.0);
        assert_eq!(column_list_viewport_height(30.0, 16.0), 14.0);
        assert_eq!(column_list_viewport_height(10.0, 16.0), 0.0);
        assert_eq!(column_list_viewport_height(f32::NAN, 0.0), 0.0);
        assert_eq!(column_list_viewport_height(-4.0, 0.0), 0.0);
        assert_eq!(column_vertical_wheel_offset(0.0, -400.0, 0, 48.0), 0.0);
        assert_eq!(column_vertical_wheel_offset(12.0, -400.0, 1, 80.0), 0.0);
        assert_eq!(
            column_vertical_wheel_offset(0.0, -10_000.0, 3, 8.0),
            (3.0 * row - 8.0).max(0.0)
        );
        assert_eq!(
            column_wheel_action(0.0, -400.0, false, Some(0), 0.0, 0.0, Some((0, 48.0))),
            Some(ExplorerAction::SetColumnScroll {
                column_index: 0,
                offset: 0.0,
            })
        );
        assert_eq!(
            column_wheel_action(0.0, -400.0, false, Some(1), 0.0, 0.0, Some((1, row * 4.0))),
            Some(ExplorerAction::SetColumnScroll {
                column_index: 1,
                offset: 0.0,
            })
        );
        assert_eq!(
            column_wheel_action(0.0, -10_000.0, false, Some(2), 0.0, 20.0, Some((10, 8.0))),
            Some(ExplorerAction::SetColumnScroll {
                column_index: 2,
                offset: 10.0 * row - 8.0,
            })
        );
        let source = include_str!("column_view.rs");
        let production = source.split("#[cfg(test)]").next().unwrap_or(source);
        for required in [
            "column_list_viewport_height(",
            "COLUMN_LIST_VIEWPORT_SLOT",
            "mt(px(-vertical_offset))",
            "column_vertical_wheel_offset(",
        ] {
            assert!(
                production.contains(required),
                "column list scroll path missing {required}"
            );
        }
    }

    #[test]
    fn column_scrollbar_pages_and_drags_inside_a_narrow_strip() {
        let hidden = column_scrollbar_metrics(200.0, 240.0, 0.0, 16.0);
        assert!(!hidden.visible);
        let metrics = column_scrollbar_metrics(960.0, 140.0, 0.0, 16.0);
        assert!(metrics.visible);
        assert!(metrics.maximum > 0.0);
        assert!(metrics.thumb_width < 140.0);
        assert!(matches!(
            column_scrollbar_hit(&metrics, 140.0, metrics.thumb_left + 1.0),
            Some(ColumnScrollbarHit::Thumb { .. })
        ));
        assert_eq!(
            column_scrollbar_hit(
                &metrics,
                140.0,
                metrics.thumb_left + metrics.thumb_width + 4.0
            ),
            Some(ColumnScrollbarHit::Page { offset: 140.0 })
        );
        assert_eq!(
            column_scrollbar_hit(&metrics, 140.0, -4.0),
            Some(ColumnScrollbarHit::Page { offset: 0.0 })
        );
        let source = include_str!("column_view.rs");
        let production = source.split("#[cfg(test)]").next().unwrap_or(source);
        for required in [
            "column_wheel_action(",
            "SetColumnHorizontalOffset",
            "BeginColumnPreviewResize",
            "BeginColumnHorizontalScroll",
            "column-horizontal-scrollbar",
            "column-preview-divider",
            "SetColumnScroll",
        ] {
            assert!(
                production.contains(required),
                "column strip input path missing {required}"
            );
        }
    }

    #[test]
    fn column_row_indexes_keep_spacer_gaps_and_a_distant_rename_row() {
        assert_eq!(super::column_row_indexes(0, 0, None), Vec::<usize>::new());
        assert_eq!(super::column_row_indexes(10, 3, None), vec![10, 11, 12]);
        assert_eq!(
            super::column_row_indexes(10, 3, Some(11)),
            vec![10, 11, 12],
            "a rename target already inside the window is not repeated"
        );
        assert_eq!(
            super::column_row_indexes(180, 3, Some(0)),
            vec![0, 180, 181, 182]
        );
        let mut cursor = 0usize;
        let mut gaps = Vec::new();
        for index in super::column_row_indexes(180, 3, Some(0)) {
            gaps.push(index - cursor);
            cursor = index + 1;
        }
        assert_eq!(gaps, vec![0, 179, 0, 0]);
        assert_eq!(
            gaps.iter().sum::<usize>() + 4,
            183,
            "gaps plus realized rows reach the last index"
        );
    }

    struct ColumnFilenameProbe {
        tokens: crate::UiTokens,
        model: super::ColumnStripModel,
        rename: Option<explorer_model::RenameEditorState>,
    }

    impl Render for ColumnFilenameProbe {
        fn render(
            &mut self,
            _window: &mut gpui::Window,
            _cx: &mut Context<Self>,
        ) -> impl IntoElement {
            div()
                .size_full()
                .font_family(self.tokens.typography.family.primary)
                .text_size(px(self.tokens.typography.file_row.size.value()))
                // Not 16. A filename that forgets its own line height inherits this and
                // no longer fits the 36px row.
                .line_height(px(22.0))
                .child(
                    super::ColumnStrip::new(
                        self.tokens,
                        self.model.clone(),
                        explorer_i18n::Catalog::new(explorer_i18n::AppLocale::En),
                        200.0,
                        std::collections::HashMap::new(),
                        96,
                        16,
                        None,
                    )
                    .with_rename(self.rename.clone(), None),
                )
        }
    }

    fn edge_right(bounds: gpui::Bounds<gpui::Pixels>) -> f32 {
        f32::from(bounds.origin.x) + f32::from(bounds.size.width)
    }

    fn edge_bottom(bounds: gpui::Bounds<gpui::Pixels>) -> f32 {
        f32::from(bounds.origin.y) + f32::from(bounds.size.height)
    }

    fn mid_y(bounds: gpui::Bounds<gpui::Pixels>) -> f32 {
        f32::from(bounds.origin.y) + f32::from(bounds.size.height) / 2.0
    }

    fn inside(outer: gpui::Bounds<gpui::Pixels>, inner: gpui::Bounds<gpui::Pixels>) -> bool {
        f32::from(inner.origin.x) >= f32::from(outer.origin.x) - 0.5
            && f32::from(inner.origin.y) >= f32::from(outer.origin.y) - 0.5
            && edge_right(inner) <= edge_right(outer) + 0.5
            && edge_bottom(inner) <= edge_bottom(outer) + 0.5
    }

    #[gpui::test]
    fn wrapped_column_filenames_stay_inside_adjacent_rows(cx: &mut gpui::TestAppContext) {
        use explorer_model::{ItemDescriptor, RenameEditorState};

        let short_id = ShellItemId::from_provider_bytes(b"s").expect("id");
        let wrapped_id = ShellItemId::from_provider_bytes(b"w").expect("id");
        let folder_id = ShellItemId::from_provider_bytes(b"n").expect("id");
        let rename_id = ShellItemId::from_provider_bytes(b"r").expect("id");
        assert_eq!(
            super::column_debug_key("column-row", 0, &short_id),
            "column-row-0-73"
        );
        assert_eq!(
            super::column_debug_key("column-filename", 0, &wrapped_id),
            "column-filename-0-77"
        );
        assert_eq!(
            super::column_debug_key("column-chevron", 0, &folder_id),
            "column-chevron-0-6e"
        );
        assert_eq!(
            super::column_debug_key("column-row-icon", 0, &rename_id),
            "column-row-icon-0-72"
        );
        let wrapped_name =
            "Quarterly Financial Report Final Approved by Finance and Legal September 2026.pdf";
        let rename_name = format!("{wrapped_name} draft copy");
        let mut rows = vec![
            filename_row(short_id.clone(), "Notes.txt", false),
            filename_row(wrapped_id.clone(), wrapped_name, false),
            filename_row(folder_id.clone(), "Plans", true),
            filename_row(rename_id.clone(), &rename_name, false),
        ];
        for index in 0..8 {
            let id =
                ShellItemId::from_provider_bytes(format!("f{index}").into_bytes()).expect("id");
            rows.push(filename_row(id, &format!("extra-{index}.txt"), false));
        }
        let row_count = rows.len();
        let directory = LocationDescriptor::file_system(r"C:\docs");
        let rename = RenameEditorState::begin(
            ItemDescriptor {
                id: rename_id.clone(),
                location: directory.clone(),
            },
            rename_name,
            false,
        );
        let model = super::ColumnStripModel {
            panes: vec![super::ColumnPaneModel {
                index: 0,
                title: "Docs".to_owned(),
                directory,
                width: 180.0,
                vertical_offset: 0.0,
                row_count,
                row_origin: 0,
                rows,
                pinned_row: None,
                status: super::ColumnPaneStatus::Ready,
                status_detail: None,
            }],
            active: 0,
            horizontal_offset: 0.0,
            preview_visible: false,
            preview_width: 280.0,
            file_surface_width: 420.0,
            list_viewport_height: 200.0,
            preview_title: String::new(),
            preview_detail: String::new(),
            preview_status: String::new(),
            preview_route: ColumnPreviewRoute::None,
        };
        let window = cx.open_window(gpui::size(px(420.0), px(200.0)), |_, _| {
            ColumnFilenameProbe {
                tokens: crate::UiTokens::default(),
                model,
                rename: Some(rename),
            }
        });
        let any_window = window.into();
        cx.update_window(any_window, |_, window, cx| window.draw(cx).clear())
            .expect("test window remains available");
        let mut visual = gpui::VisualTestContext::from_window(any_window, cx);
        let read = |visual: &mut gpui::VisualTestContext, selector: &'static str| {
            visual
                .debug_bounds(selector)
                .unwrap_or_else(|| panic!("missing selector {selector}"))
        };
        let short_row = read(&mut visual, "column-row-0-73");
        let wrapped_row = read(&mut visual, "column-row-0-77");
        let folder_row = read(&mut visual, "column-row-0-6e");
        let rename_row = read(&mut visual, "column-row-0-72");
        let short_label = read(&mut visual, "column-filename-0-73");
        let wrapped_label = read(&mut visual, "column-filename-0-77");
        let folder_label = read(&mut visual, "column-filename-0-6e");
        let short_icon = read(&mut visual, "column-row-icon-0-73");
        let wrapped_icon = read(&mut visual, "column-row-icon-0-77");
        let folder_chevron = read(&mut visual, "column-chevron-0-6e");
        let rename_editor = read(&mut visual, "column-inline-rename");
        let rename_text = read(&mut visual, "column-inline-rename-text");

        for (name, row) in [
            ("short", short_row),
            ("wrapped", wrapped_row),
            ("folder", folder_row),
            ("rename", rename_row),
        ] {
            assert!(
                (f32::from(row.size.height) - COLUMN_ROW_HEIGHT).abs() <= 0.5,
                "{name} row height {} collapsed below the shared {}",
                f32::from(row.size.height),
                COLUMN_ROW_HEIGHT
            );
            assert!(
                (f32::from(row.size.width) - 180.0).abs() <= 2.0,
                "{name} row width {}",
                f32::from(row.size.width)
            );
        }
        assert!(
            (f32::from(wrapped_row.origin.y) - f32::from(short_row.origin.y) - COLUMN_ROW_HEIGHT)
                .abs()
                <= 0.5,
            "wrapped row does not start on the next fixed boundary: short {:?} wrapped {:?}",
            short_row,
            wrapped_row
        );
        assert!(
            (f32::from(folder_row.origin.y) - f32::from(wrapped_row.origin.y) - COLUMN_ROW_HEIGHT)
                .abs()
                <= 0.5,
            "folder row collided with the wrapped filename: wrapped {:?} folder {:?}",
            wrapped_row,
            folder_row
        );
        assert!(
            (f32::from(rename_row.origin.y) - f32::from(folder_row.origin.y) - COLUMN_ROW_HEIGHT)
                .abs()
                <= 0.5
        );

        let short_h = f32::from(short_label.size.height);
        let wrapped_h = f32::from(wrapped_label.size.height);
        let folder_h = f32::from(folder_label.size.height);
        assert!(
            (short_h - super::COLUMN_FILENAME_LINE_HEIGHT).abs() <= 1.0,
            "single-line filename should be one 16px line, got {short_h}"
        );
        assert!(
            (folder_h - super::COLUMN_FILENAME_LINE_HEIGHT).abs() <= 1.0,
            "folder filename should be one 16px line, got {folder_h}"
        );
        assert!(
            wrapped_h > super::COLUMN_FILENAME_LINE_HEIGHT + 8.0,
            "long PDF name did not wrap, height {wrapped_h}"
        );
        assert!(
            wrapped_h <= super::COLUMN_FILENAME_LINE_HEIGHT * 2.0 + 0.5,
            "wrapped PDF name exceeded two 16px lines, height {wrapped_h}"
        );
        assert!(
            inside(short_row, short_label),
            "short label {:?} escaped {:?}",
            short_label,
            short_row
        );
        assert!(
            inside(wrapped_row, wrapped_label),
            "wrapped label {:?} escaped {:?}",
            wrapped_label,
            wrapped_row
        );
        assert!(
            inside(folder_row, folder_label),
            "folder label {:?} escaped {:?}",
            folder_label,
            folder_row
        );
        assert!(
            edge_bottom(wrapped_label) <= f32::from(folder_row.origin.y) + 0.5,
            "wrapped label bottom {} entered the next row at {}",
            edge_bottom(wrapped_label),
            f32::from(folder_row.origin.y)
        );
        assert!((mid_y(short_label) - mid_y(short_row)).abs() <= 1.0);
        assert!((mid_y(wrapped_label) - mid_y(wrapped_row)).abs() <= 1.0);
        assert!((mid_y(short_icon) - mid_y(short_row)).abs() <= 1.0);
        assert!((mid_y(wrapped_icon) - mid_y(wrapped_row)).abs() <= 1.0);
        assert!(
            (mid_y(folder_chevron) - mid_y(folder_row)).abs() <= 1.0,
            "chevron {:?} is not centered on {:?}",
            folder_chevron,
            folder_row
        );
        assert!(
            inside(rename_row, rename_editor),
            "rename editor {:?} escaped {:?}",
            rename_editor,
            rename_row
        );
        let editor_h = f32::from(rename_editor.size.height);
        let rename_text_h = f32::from(rename_text.size.height);
        assert!(
            editor_h < COLUMN_ROW_HEIGHT,
            "rename editor filled more than the row: {editor_h}"
        );
        assert!(
            (editor_h - (super::COLUMN_FILENAME_LINE_HEIGHT + 4.0)).abs() <= 0.5,
            "rename editor should stay a single-line field, got {editor_h}"
        );
        assert!(
            rename_text_h <= super::COLUMN_FILENAME_LINE_HEIGHT + 4.0,
            "rename text wrapped past one line, height {rename_text_h}"
        );
        assert!(
            inside(rename_editor, rename_text),
            "rename text {:?} escaped {:?}",
            rename_text,
            rename_editor
        );
        assert!((mid_y(rename_text) - mid_y(rename_row)).abs() <= 1.0);
    }

    fn filename_row(id: ShellItemId, name: &str, is_container: bool) -> ColumnRowModel {
        ColumnRowModel {
            id,
            name: name.to_owned(),
            location: LocationDescriptor::file_system(format!(r"C:\docs\{name}")),
            is_container,
            metadata: FileEntryMetadata::default(),
            selected: false,
            branch_selected: false,
        }
    }

    struct FilenameLayoutProbe(Rc<RefCell<Vec<gpui::TextLayout>>>);

    impl FilenameLayoutProbe {
        fn install() -> Self {
            let records = Rc::new(RefCell::new(Vec::new()));
            super::FILENAME_LAYOUT_PROBE.with(|slot| {
                assert!(slot.borrow().is_none());
                *slot.borrow_mut() = Some(Rc::clone(&records));
            });
            Self(records)
        }

        fn take(&self) -> Vec<gpui::TextLayout> {
            let mut records = std::mem::take(&mut *self.0.borrow_mut());
            // Input simulation may render before our explicit draw. Keep the latest
            // complete frame, whose four rows are all inside the viewport.
            if records.len() > 4 {
                records.drain(..records.len() - 4);
            }
            records
        }
    }

    impl Drop for FilenameLayoutProbe {
        fn drop(&mut self) {
            super::FILENAME_LAYOUT_PROBE.with(|slot| *slot.borrow_mut() = None);
        }
    }

    fn assert_filename_layout(layout: &gpui::TextLayout, original: &str, truncated: bool) {
        let text = layout.text();
        let wrapped = layout.wrapped_text();
        assert!(wrapped.lines().count() <= 2, "{original}: {wrapped:?}");
        assert!(f32::from(layout.bounds().size.height) <= 32.5);
        if truncated {
            assert!(
                text.ends_with('…'),
                "missing ellipsis for {original}: {text:?}"
            );
            assert!(original.len() > text.len());
            let bounds = layout.bounds();
            let ellipsis = layout
                .position_for_index(text.rfind('…').expect("ellipsis"))
                .expect("ellipsis has a shaped position");
            assert!(ellipsis.x >= bounds.left() && ellipsis.x < bounds.right());
            assert!(ellipsis.y >= bounds.top() && ellipsis.y < bounds.bottom());
        } else {
            assert_eq!(text, original);
        }
    }

    const LONG_APK_NAME: &str =
        "mdtq8u1e1nr1eu2op15f0nghacapp4qio8e4eu1nre4t4e5t875dj33fed88f9bd9.apk";
    const UUID_IMAGE_NAME: &str =
        "grok-image-33bf8e83-3299-4e5f-869a-bf1784b98669_upscayl_2x_upscayl-standard-4x.jpg";
    const CJK_NAME: &str = "這是一個非常長的檔案名稱用來確認中文字換行之後超過兩行能夠正確截斷並且不會影響下一個檔案以及其他語言的測試檔案與資料夾顯示結果.pdf";

    struct ColumnResizeProbe {
        state: crate::state::AppViewState,
        actions: Vec<ExplorerAction>,
    }

    impl Render for ColumnResizeProbe {
        fn render(&mut self, _: &mut gpui::Window, cx: &mut Context<Self>) -> impl IntoElement {
            let tokens = crate::UiTokens::default();
            let callback: crate::chrome::ActionCallback =
                Rc::new(cx.listener(|this, action: &ExplorerAction, _, cx| {
                    match action {
                        ExplorerAction::BeginColumnWidthResize {
                            column_index,
                            pointer_x,
                        } => {
                            this.state
                                .begin_column_width_resize(*column_index, *pointer_x);
                        }
                        ExplorerAction::UpdateColumnWidthResize { pointer_x } => {
                            this.state.update_column_width_resize(*pointer_x);
                        }
                        ExplorerAction::EndColumnWidthResize => {
                            this.state.end_column_width_resize();
                        }
                        _ => return,
                    }
                    this.actions.push(action.clone());
                    cx.notify();
                }));
            let mut model = self.state.column_strip_model().expect("column model");
            model.active = 0;
            model.horizontal_offset = 0.0;
            model.preview_visible = false;
            model.file_surface_width = 1000.0;
            model.list_viewport_height = 200.0;
            let names = ["Notes.txt", LONG_APK_NAME, UUID_IMAGE_NAME, CJK_NAME];
            let pane = &mut model.panes[0];
            pane.title = "Names".to_owned();
            pane.rows = names
                .iter()
                .enumerate()
                .map(|(index, name)| {
                    filename_row(
                        ShellItemId::from_provider_bytes(vec![index as u8]).expect("id"),
                        name,
                        false,
                    )
                })
                .collect();
            pane.row_count = names.len();
            pane.row_origin = 0;
            pane.status = super::ColumnPaneStatus::Ready;
            pane.status_detail = None;
            div()
                .size_full()
                .relative()
                .font_family(tokens.typography.family.primary)
                .text_size(px(tokens.typography.file_row.size.value()))
                .line_height(px(22.0))
                .child(super::ColumnStrip::new(
                    tokens,
                    model,
                    explorer_i18n::Catalog::new(explorer_i18n::AppLocale::En),
                    200.0,
                    std::collections::HashMap::new(),
                    96,
                    16,
                    Some(callback.clone()),
                ))
                .when(self.state.column_width_resize_active(), |element| {
                    element.child(crate::chrome::pointer_drag_capture_listener(
                        Some(callback),
                        None,
                        false,
                        false,
                        false,
                        false,
                        true,
                        false,
                        0.0,
                        0.0,
                        None,
                        1000.0,
                    ))
                })
        }
    }

    #[gpui::test]
    fn column_view_long_names_truncate_and_divider_drag_survives_rerenders(
        cx: &mut gpui::TestAppContext,
    ) {
        let probe = FilenameLayoutProbe::install();
        let mut state = crate::state::AppViewState::with_initial_location(
            explorer_model::HistoryEntry::new(LocationDescriptor::file_system(r"C:\docs"), "Docs"),
        );
        state.set_drive_kind('C', explorer_model::DriveKind::Fixed);
        state.set_view_mode(explorer_model::ViewMode::Columns);
        state.ensure_column_branch();
        state.set_column_file_viewport_width(1000.0);
        state.set_column_list_viewport_height(200.0);
        let window = cx.open_window(gpui::size(px(1000.0), px(200.0)), |_, _| {
            ColumnResizeProbe {
                state,
                actions: Vec::new(),
            }
        });
        let mut visual = gpui::VisualTestContext::from_window(window.into(), cx);
        visual.update(|window, cx| window.draw(cx).clear());
        let names = ["Notes.txt", LONG_APK_NAME, UUID_IMAGE_NAME, CJK_NAME];
        let layouts = probe.take();
        assert_eq!(layouts.len(), names.len());
        for (layout, name) in layouts.iter().zip(names) {
            assert_filename_layout(layout, name, name != "Notes.txt");
        }
        let divider = visual.debug_bounds("column-divider-0").expect("divider");
        assert!(f32::from(divider.size.width) >= 6.0);
        let start = gpui::point(divider.center().x, px(120.0));
        let width = |visual: &gpui::VisualTestContext| {
            window
                .read_with(visual, |view, _| view.state.view_settings().column_width)
                .expect("window")
        };
        visual.simulate_mouse_down(start, MouseButton::Left, gpui::Modifiers::default());
        visual.update(|window, cx| window.draw(cx).clear());
        assert!(
            window
                .read_with(&visual, |view, _| view.state.column_width_resize_active())
                .unwrap()
        );
        probe.take();
        visual.simulate_mouse_move(
            start - gpui::point(px(40.0), px(0.0)),
            Some(MouseButton::Left),
            gpui::Modifiers::default(),
        );
        visual.update(|window, cx| window.draw(cx).clear());
        assert_eq!(width(&visual), 200);
        let narrow = probe.take();
        for (layout, name) in narrow.iter().zip(names) {
            assert_filename_layout(layout, name, name != "Notes.txt");
        }
        let narrow_text = narrow[1].text();
        visual.simulate_mouse_move(
            start + gpui::point(px(180.0), px(0.0)),
            Some(MouseButton::Left),
            gpui::Modifiers::default(),
        );
        visual.update(|window, cx| window.draw(cx).clear());
        assert_eq!(
            width(&visual),
            420,
            "global capture continues outside the narrow divider after rerender"
        );
        let wide = probe.take();
        assert!(
            wide[1].text().len() > narrow_text.len(),
            "widening recalculates truncation"
        );
        assert_eq!(wide[0].text(), "Notes.txt");
        for layout in &wide {
            assert!(layout.wrapped_text().lines().count() <= 2);
        }
        for selector in [
            "column-row-0-00",
            "column-row-0-01",
            "column-row-0-02",
            "column-row-0-03",
        ] {
            let row = visual.debug_bounds(selector).expect("row");
            assert!((f32::from(row.size.height) - COLUMN_ROW_HEIGHT).abs() < 0.5);
        }
        let outside = gpui::point(px(900.0), px(150.0));
        visual.simulate_mouse_up(outside, MouseButton::Left, gpui::Modifiers::default());
        visual.update(|window, cx| window.draw(cx).clear());
        assert!(
            !window
                .read_with(&visual, |view, _| view.state.column_width_resize_active())
                .unwrap()
        );
        visual.simulate_mouse_move(
            gpui::point(px(50.0), px(150.0)),
            None,
            gpui::Modifiers::default(),
        );
        visual.update(|window, cx| window.draw(cx).clear());
        assert_eq!(
            width(&visual),
            420,
            "move after outside release does not resize"
        );
        let actions = window
            .read_with(&visual, |view, _| view.actions.clone())
            .unwrap();
        assert!(matches!(
            actions.first(),
            Some(ExplorerAction::BeginColumnWidthResize {
                column_index: 0,
                ..
            })
        ));
        assert!(matches!(
            actions.last(),
            Some(ExplorerAction::EndColumnWidthResize)
        ));
    }

    struct NativePreviewProbe {
        content: crate::preview_content::PreviewContent,
        route: ColumnPreviewRoute,
    }

    impl Render for NativePreviewProbe {
        fn render(&mut self, _: &mut gpui::Window, _: &mut Context<Self>) -> impl IntoElement {
            div().size_full().child(super::column_preview(
                crate::UiTokens::default(),
                explorer_i18n::Catalog::new(explorer_i18n::AppLocale::En),
                320.0,
                0.0,
                "preview.md".to_owned(),
                String::new(),
                self.route,
                None,
                false,
                None,
                super::ColumnHandlerPhase::Inactive,
                600.0,
                None,
                Some(self.content.clone()),
            ))
        }
    }

    #[gpui::test]
    fn column_preview_native_text_and_image_information_render_inside_scrollable_slots(
        cx: &mut gpui::TestAppContext,
    ) {
        let text = "# Raw markdown\n**plain text**\n"
            .repeat(100)
            .chars()
            .take(3_000)
            .collect::<String>();
        let window = cx.open_window(gpui::size(px(320.0), px(600.0)), |_, _| {
            NativePreviewProbe {
                content: crate::preview_content::PreviewContent::Text {
                    text,
                    truncated: true,
                    encoding: "UTF-8".to_owned(),
                },
                route: ColumnPreviewRoute::PlainText,
            }
        });
        let mut visual = gpui::VisualTestContext::from_window(window.into(), cx);
        visual.update(|window, cx| window.draw(cx).clear());
        let pane = visual.debug_bounds("column-preview").unwrap();
        let slot = visual
            .debug_bounds("column-preview-native-content")
            .unwrap();
        let body = visual.debug_bounds("column-preview-plain-text").unwrap();
        assert!(slot.left() >= pane.left() && slot.right() <= pane.right());
        assert!(slot.top() >= pane.top() && slot.bottom() <= pane.bottom());
        assert!(
            body.size.height > slot.size.height,
            "long plain text must scroll instead of expanding the pane"
        );
        assert!(
            visual.debug_bounds("column-preview-host").is_none(),
            "text must not activate a native handler"
        );
        window
            .update(&mut visual, |view, _, cx| {
                view.route = ColumnPreviewRoute::ImageThumbnail;
                view.content = crate::preview_content::PreviewContent::Image {
                    width: 800,
                    height: 600,
                    bits_per_pixel: 24,
                    color: "Rgb8".to_owned(),
                    size: Some(123_456),
                    exif: vec![("Make".to_owned(), "Camera".to_owned())],
                    metadata_note: None,
                };
                cx.notify();
            })
            .unwrap();
        visual.update(|window, cx| window.draw(cx).clear());
        let info = visual
            .debug_bounds("column-preview-native-content")
            .unwrap();
        assert!(f32::from(info.size.height) <= 240.0);
        assert!(info.bottom() <= pane.bottom());
        assert!(
            visual.debug_bounds("column-preview-plain-text").is_none(),
            "old text content is removed on image selection"
        );
    }
}
