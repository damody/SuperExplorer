//! Dedicated, modeless Folder Options window.

use std::{
    rc::Rc,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};

use gpui::{
    App, Bounds, Context, FocusHandle, Focusable, IntoElement, KeyDownEvent, MouseButton,
    MouseDownEvent, MouseMoveEvent, Pixels, Render, Role, ScrollHandle, SharedString, Window,
    WindowBounds, WindowHandle, WindowOptions, div, point, prelude::*, px, size,
};

use crate::{
    ExplorerRoot, UiTokens,
    actions::{ActionSource, ExplorerAction, FolderOptionsPage},
    chrome::{self, ActionCallback},
    state::{ExtensionOptionV1, FolderOptionsDraft},
};
use gpui_elements::editable_text::{EditableTextState, StringStorage};

const INITIAL_WIDTH: f32 = 960.0;
const INITIAL_HEIGHT: f32 = 760.0;
const MINIMUM_WIDTH: f32 = 680.0;
const MINIMUM_HEIGHT: f32 = 480.0;

fn parse_cache_budget_memory_mb(
    value: &str,
    descriptor: explorer_model::CacheBudgetDescriptorV1,
) -> Option<u32> {
    value
        .trim()
        .parse::<u32>()
        .ok()
        .map(|value| descriptor.normalize(value))
}

/// Modeless window options. `WindowKind::Normal` is intentional: GPUI's
/// Windows `Dialog` kind disables its owner and would make Explorer modal.
pub fn folder_options_window_options(cx: &App) -> WindowOptions {
    WindowOptions {
        window_bounds: Some(WindowBounds::Windowed(Bounds::centered(
            None,
            size(px(INITIAL_WIDTH), px(INITIAL_HEIGHT)),
            cx,
        ))),
        titlebar: Some(gpui::TitlebarOptions {
            title: Some(SharedString::from("資料夾選項")),
            ..Default::default()
        }),
        kind: gpui::WindowKind::Normal,
        is_resizable: true,
        window_min_size: Some(size(px(MINIMUM_WIDTH), px(MINIMUM_HEIGHT))),
        ..Default::default()
    }
}

/// Immutable handoff used while the owner window is still dispatching the
/// command that creates this native window. It avoids a nested owner-window
/// read during GPUI's immediate first render.
#[derive(Clone)]
pub struct FolderOptionsWindowSnapshotV1 {
    pub draft: FolderOptionsDraft,
    pub extensions: Vec<ExtensionOptionV1>,
    pub cache_usage: CacheUsageSnapshotV1,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct CacheUsageSnapshotV1 {
    pub icon_memory_bytes: u64,
    pub icon_memory_limit: u64,
    pub base_icon_memory_bytes: u64,
    pub base_icon_memory_limit: u64,
    pub thumbnail_memory_bytes: u64,
    pub thumbnail_memory_limit: u64,
    pub icon_gpu_bytes: u64,
    pub icon_gpu_limit: u64,
    pub icon_gpu_entries: u64,
    pub thumbnail_gpu_bytes: u64,
    pub thumbnail_gpu_limit: u64,
    pub thumbnail_gpu_entries: u64,
    pub bc7_gpu_supported: Option<bool>,
    pub extension_memory_bytes: Option<u64>,
    pub extension_memory_limit: Option<u64>,
    pub extension_memory_entries: Option<u64>,
    pub icon_disk_bytes: Option<u64>,
    pub icon_disk_limit: Option<u64>,
    pub thumbnail_disk_bytes: Option<u64>,
    pub thumbnail_disk_limit: Option<u64>,
    pub extension_disk_bytes: Option<u64>,
    pub extension_disk_limit: Option<u64>,
    pub mft_disk_bytes: Option<u64>,
    pub mft_disk_limit: Option<u64>,
    pub mft_volume_index_memory_bytes: Option<u64>,
    pub mft_volume_index_memory_limit: Option<u64>,
    pub mft_file_data_memory_bytes: Option<u64>,
    pub mft_file_data_memory_limit: Option<u64>,
    pub mft_aggregate_memory_bytes: Option<u64>,
    pub mft_aggregate_memory_limit: Option<u64>,
    pub mft_service_bytes: Option<u64>,
    pub mft_service_limit: Option<u64>,
    pub mft_service_entries: Option<u64>,
    pub mft_service_hits: Option<u64>,
    pub mft_service_misses: Option<u64>,
    pending_mask: u16,
    unavailable_mask: u16,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CacheUsageAvailabilityV1 {
    Pending,
    Available,
    Unavailable,
}

impl CacheUsageSnapshotV1 {
    fn telemetry_bit(id: explorer_model::CacheTelemetryIdV1) -> u16 {
        1_u16 << (id as u16)
    }

    fn set_availability(
        &mut self,
        id: explorer_model::CacheTelemetryIdV1,
        availability: CacheUsageAvailabilityV1,
    ) {
        let bit = Self::telemetry_bit(id);
        self.pending_mask &= !bit;
        self.unavailable_mask &= !bit;
        match availability {
            CacheUsageAvailabilityV1::Pending => self.pending_mask |= bit,
            CacheUsageAvailabilityV1::Unavailable => self.unavailable_mask |= bit,
            CacheUsageAvailabilityV1::Available => {}
        }
    }

    pub fn availability(&self, id: explorer_model::CacheTelemetryIdV1) -> CacheUsageAvailabilityV1 {
        let bit = Self::telemetry_bit(id);
        if self.unavailable_mask & bit != 0 {
            CacheUsageAvailabilityV1::Unavailable
        } else if self.pending_mask & bit != 0 {
            CacheUsageAvailabilityV1::Pending
        } else {
            CacheUsageAvailabilityV1::Available
        }
    }

    pub fn background_sample(
        service: Option<&dyn explorer_model::ExplorerService>,
        cancelled: &AtomicBool,
    ) -> Self {
        if cancelled.load(Ordering::Acquire) {
            return Self::default();
        }
        let mut sample = Self::default();
        if let Some(service) = service {
            let telemetry = service.cache_telemetry_snapshot();
            sample.apply_host_telemetry(&telemetry);
        }
        sample
    }

    fn apply_host_telemetry(&mut self, telemetry: &explorer_model::CacheTelemetrySnapshotV1) {
        for entry in telemetry.entries() {
            self.set_availability(
                entry.id,
                match entry.availability {
                    explorer_model::CacheTelemetryAvailabilityV1::Available(_) => {
                        CacheUsageAvailabilityV1::Available
                    }
                    explorer_model::CacheTelemetryAvailabilityV1::Pending => {
                        CacheUsageAvailabilityV1::Pending
                    }
                    explorer_model::CacheTelemetryAvailabilityV1::Unavailable => {
                        CacheUsageAvailabilityV1::Unavailable
                    }
                },
            );
        }
        if let Some(entry) =
            telemetry.entry(explorer_model::CacheTelemetryIdV1::ExtensionColumnsMemory)
            && let explorer_model::CacheTelemetryAvailabilityV1::Available(value) =
                entry.availability
        {
            self.extension_memory_bytes = Some(value.bytes);
            self.extension_memory_limit = value.limit_bytes;
            self.extension_memory_entries = Some(value.entry_count);
        }
        for (id, target, limit_target) in [
            (
                explorer_model::CacheTelemetryIdV1::IconsDisk,
                &mut self.icon_disk_bytes,
                Some(&mut self.icon_disk_limit),
            ),
            (
                explorer_model::CacheTelemetryIdV1::ThumbnailsDisk,
                &mut self.thumbnail_disk_bytes,
                Some(&mut self.thumbnail_disk_limit),
            ),
            (
                explorer_model::CacheTelemetryIdV1::ExtensionColumnsDisk,
                &mut self.extension_disk_bytes,
                Some(&mut self.extension_disk_limit),
            ),
            (
                explorer_model::CacheTelemetryIdV1::MftPersistedIndex,
                &mut self.mft_disk_bytes,
                Some(&mut self.mft_disk_limit),
            ),
            (
                explorer_model::CacheTelemetryIdV1::MftVolumeIndexMemory,
                &mut self.mft_volume_index_memory_bytes,
                Some(&mut self.mft_volume_index_memory_limit),
            ),
            (
                explorer_model::CacheTelemetryIdV1::MftFileDataMemory,
                &mut self.mft_file_data_memory_bytes,
                Some(&mut self.mft_file_data_memory_limit),
            ),
            (
                explorer_model::CacheTelemetryIdV1::MftAggregateMemory,
                &mut self.mft_aggregate_memory_bytes,
                Some(&mut self.mft_aggregate_memory_limit),
            ),
        ] {
            if let Some(entry) = telemetry.entry(id)
                && let explorer_model::CacheTelemetryAvailabilityV1::Available(value) =
                    entry.availability
            {
                *target = Some(value.bytes);
                if let Some(limit_target) = limit_target {
                    *limit_target = value.limit_bytes;
                }
            }
        }
        if let Some(entry) = telemetry.entry(explorer_model::CacheTelemetryIdV1::MftServiceLru)
            && let explorer_model::CacheTelemetryAvailabilityV1::Available(value) =
                entry.availability
        {
            self.mft_service_bytes = Some(value.bytes);
            self.mft_service_limit = value.limit_bytes;
            self.mft_service_entries = Some(value.entry_count);
            if let Some(counters) = value.counters {
                self.mft_service_hits = Some(counters.hits);
                self.mft_service_misses = Some(counters.misses);
            }
        }
    }

    fn retain_pending_from(&mut self, previous: Self) {
        macro_rules! retain {
            ($id:expr, $($field:ident),+ $(,)?) => {
                if self.availability($id) == CacheUsageAvailabilityV1::Pending {
                    $(if self.$field.is_none() { self.$field = previous.$field; })+
                }
            };
        }
        retain!(
            explorer_model::CacheTelemetryIdV1::ExtensionColumnsMemory,
            extension_memory_bytes,
            extension_memory_limit,
            extension_memory_entries
        );
        retain!(
            explorer_model::CacheTelemetryIdV1::IconsDisk,
            icon_disk_bytes,
            icon_disk_limit
        );
        retain!(
            explorer_model::CacheTelemetryIdV1::ThumbnailsDisk,
            thumbnail_disk_bytes,
            thumbnail_disk_limit
        );
        retain!(
            explorer_model::CacheTelemetryIdV1::ExtensionColumnsDisk,
            extension_disk_bytes,
            extension_disk_limit
        );
        retain!(
            explorer_model::CacheTelemetryIdV1::MftPersistedIndex,
            mft_disk_bytes,
            mft_disk_limit
        );
        retain!(
            explorer_model::CacheTelemetryIdV1::MftVolumeIndexMemory,
            mft_volume_index_memory_bytes,
            mft_volume_index_memory_limit
        );
        retain!(
            explorer_model::CacheTelemetryIdV1::MftFileDataMemory,
            mft_file_data_memory_bytes,
            mft_file_data_memory_limit
        );
        retain!(
            explorer_model::CacheTelemetryIdV1::MftAggregateMemory,
            mft_aggregate_memory_bytes,
            mft_aggregate_memory_limit
        );
        retain!(
            explorer_model::CacheTelemetryIdV1::MftServiceLru,
            mft_service_bytes,
            mft_service_limit,
            mft_service_entries,
            mft_service_hits,
            mft_service_misses
        );
    }
}

struct CacheUsageSamplerV1 {
    in_flight: AtomicBool,
    cancelled: AtomicBool,
    latest: Mutex<CacheUsageSnapshotV1>,
}

impl CacheUsageSamplerV1 {
    fn new(initial: CacheUsageSnapshotV1) -> Self {
        Self {
            in_flight: AtomicBool::new(false),
            cancelled: AtomicBool::new(false),
            latest: Mutex::new(initial),
        }
    }

    fn sample(
        &self,
        service: Option<&dyn explorer_model::ExplorerService>,
    ) -> CacheUsageSnapshotV1 {
        if self.cancelled.load(Ordering::Acquire)
            || self
                .in_flight
                .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
                .is_err()
        {
            return self
                .latest
                .lock()
                .map_or_else(|_| CacheUsageSnapshotV1::default(), |latest| *latest);
        }
        let previous = self
            .latest
            .lock()
            .map_or_else(|_| CacheUsageSnapshotV1::default(), |latest| *latest);
        let mut sampled = CacheUsageSnapshotV1::background_sample(service, &self.cancelled);
        sampled.retain_pending_from(previous);
        if !self.cancelled.load(Ordering::Acquire)
            && let Ok(mut latest) = self.latest.lock()
        {
            *latest = sampled;
        }
        self.in_flight.store(false, Ordering::Release);
        sampled
    }

    fn cancel(&self) {
        self.cancelled.store(true, Ordering::Release);
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct ScrollbarDragV1 {
    page: FolderOptionsPage,
    grab_offset_y: f32,
}

/// Root entity for the independent Folder Options native window.
pub struct FolderOptionsWindow {
    tokens: UiTokens,
    owner: WindowHandle<ExplorerRoot>,
    focus_handle: FocusHandle,
    general_scroll: ScrollHandle,
    view_scroll: ScrollHandle,
    extensions_scroll: ScrollHandle,
    scrollbar_drag: Option<ScrollbarDragV1>,
    snapshot: FolderOptionsWindowSnapshotV1,
    cache_budget_inputs: Vec<gpui::Entity<EditableTextState>>,
    cache_budget_input_baseline: explorer_model::CacheBudgetSettingsV1,
    cache_usage_sampler: Arc<CacheUsageSamplerV1>,
}

impl Drop for FolderOptionsWindow {
    fn drop(&mut self) {
        self.cache_usage_sampler.cancel();
    }
}

impl FolderOptionsWindow {
    pub fn new(
        tokens: UiTokens,
        owner: WindowHandle<ExplorerRoot>,
        snapshot: FolderOptionsWindowSnapshotV1,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let focus_handle = cx.focus_handle();
        focus_handle.focus(window, cx);
        let cache_budget_input_baseline = snapshot.draft.settings.cache_budgets;
        let cache_budget_inputs = explorer_model::CACHE_BUDGET_DESCRIPTORS_V1
            .into_iter()
            .map(|descriptor| {
                let value = snapshot.draft.settings.cache_budgets.get(descriptor.id);
                cx.new(|cx| EditableTextState::new(StringStorage::from(value.to_string()), cx))
            })
            .collect::<Vec<_>>();
        let cache_usage_sampler = Arc::new(CacheUsageSamplerV1::new(snapshot.cache_usage));
        let refresh_sampler = Arc::clone(&cache_usage_sampler);
        cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor().timer(Duration::from_secs(1)).await;
                let service = this
                    .update(cx, |this, _| this.owner)
                    .ok()
                    .and_then(|owner| {
                        owner
                            .update(cx, |root, _, _| root.service_for_cache_telemetry())
                            .ok()
                    })
                    .flatten();
                let sampler = Arc::clone(&refresh_sampler);
                let disk = cx
                    .background_executor()
                    .spawn(async move { sampler.sample(service.as_deref()) })
                    .await;
                if this
                    .update(cx, |this, cx| {
                        let owner = this.owner;
                        if let Ok(usage) =
                            owner.update(cx, |root, _, _| root.cache_usage_snapshot(disk))
                        {
                            this.snapshot.cache_usage = usage;
                            cx.notify();
                        }
                    })
                    .is_err()
                {
                    break;
                }
            }
        })
        .detach();
        Self {
            tokens,
            owner,
            focus_handle,
            general_scroll: ScrollHandle::new(),
            view_scroll: ScrollHandle::new(),
            extensions_scroll: ScrollHandle::new(),
            scrollbar_drag: None,
            snapshot,
            cache_budget_inputs,
            cache_budget_input_baseline,
            cache_usage_sampler,
        }
    }

    fn scroll_for_page(&self, page: FolderOptionsPage) -> &ScrollHandle {
        match page {
            FolderOptionsPage::General => &self.general_scroll,
            FolderOptionsPage::View => &self.view_scroll,
            FolderOptionsPage::Extensions => &self.extensions_scroll,
        }
    }

    fn stop_drag(&mut self) {
        self.scrollbar_drag = None;
    }

    fn update_drag(&mut self, pointer_y: Pixels) -> bool {
        let Some(drag) = self.scrollbar_drag else {
            return false;
        };
        let handle = self.scroll_for_page(drag.page);
        let bounds = handle.bounds();
        let viewport = f32::from(bounds.size.height).max(0.0);
        let maximum = f32::from(handle.max_offset().y).max(0.0);
        let pointer_local_y = f32::from(pointer_y - bounds.top());
        let Some(target) = crate::interaction::scrollbar_target_offset(
            viewport,
            maximum,
            self.tokens.layout.minimum_hit_target.value(),
            pointer_local_y,
            drag.grab_offset_y,
        ) else {
            return false;
        };
        let offset = handle.offset();
        handle.set_offset(point(offset.x, px(-target)));
        true
    }

    fn keyboard_scroll(&self, page: FolderOptionsPage, key: &str) -> bool {
        let handle = self.scroll_for_page(page);
        let viewport = f32::from(handle.bounds().size.height).max(0.0);
        let maximum = f32::from(handle.max_offset().y).max(0.0);
        if viewport <= 0.0 {
            return false;
        }
        let current = (-f32::from(handle.offset().y)).clamp(0.0, maximum);
        let target = match key {
            "pageup" => current - viewport,
            "pagedown" => current + viewport,
            "home" => 0.0,
            "end" => maximum,
            _ => return false,
        }
        .clamp(0.0, maximum);
        let offset = handle.offset();
        handle.set_offset(point(offset.x, px(-target)));
        true
    }

    fn close_with_action(
        &mut self,
        action: ExplorerAction,
        source: ActionSource,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.stop_drag();
        let owner = self.owner;
        if let Err(error) = owner.update(cx, |root, owner_window, cx| {
            root.dispatch_folder_options_action(action, source, owner_window, cx);
        }) {
            tracing::warn!(%error, "Folder Options owner window is unavailable");
        }
        window.remove_window();
    }

    fn scrollbar(
        &self,
        page: FolderOptionsPage,
        handle: ScrollHandle,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let colors = self.tokens.theme.colors;
        let bounds = handle.bounds();
        let viewport = f32::from(bounds.size.height).max(0.0);
        let maximum = f32::from(handle.max_offset().y).max(0.0);
        let current = (-f32::from(handle.offset().y)).clamp(0.0, maximum);
        let minimum_thumb = self.tokens.layout.minimum_hit_target.value();
        let track_width = self.tokens.layout.content_spacing.value() * 1.5;
        let thumb_width = (track_width - self.tokens.layout.focus_stroke.value() * 2.0).max(8.0);
        let thumb_height =
            crate::interaction::scrollbar_thumb_height(viewport, maximum, minimum_thumb)
                .unwrap_or(viewport.max(1.0));
        let thumb_top = if maximum > 0.0 {
            current / maximum * (viewport - thumb_height).max(0.0)
        } else {
            0.0
        };
        let header_height = self.tokens.layout.address_bar_height.value()
            + self.tokens.layout.title_tab_height.value();
        let footer_height = crate::layout::folder_options::FOOTER_HEIGHT.value();
        let click_handle = handle.clone();
        div()
            .id("folder-options-scrollbar")
            .debug_selector(|| "folder-options-scrollbar".to_owned())
            .role(Role::ScrollBar)
            .aria_label("資料夾選項垂直捲動列")
            .aria_numeric_value(f64::from(current))
            .aria_min_numeric_value(0.0)
            .aria_max_numeric_value(f64::from(maximum))
            .absolute()
            .top(px(header_height))
            .right_0()
            .bottom(px(footer_height))
            .w(px(track_width))
            .bg(colors.surface.to_gpui())
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, event: &MouseDownEvent, window, cx| {
                    let bounds = click_handle.bounds();
                    let viewport = f32::from(bounds.size.height).max(0.0);
                    let maximum = f32::from(click_handle.max_offset().y).max(0.0);
                    if viewport <= 0.0 {
                        cx.stop_propagation();
                        return;
                    }
                    let current = (-f32::from(click_handle.offset().y)).clamp(0.0, maximum);
                    let thumb_height = crate::interaction::scrollbar_thumb_height(
                        viewport,
                        maximum,
                        minimum_thumb,
                    )
                    .unwrap_or(viewport);
                    let thumb_top = if maximum > 0.0 {
                        current / maximum * (viewport - thumb_height).max(0.0)
                    } else {
                        0.0
                    };
                    let pointer = f32::from(event.position.y - bounds.top());
                    if maximum > 0.0 && pointer >= thumb_top && pointer <= thumb_top + thumb_height
                    {
                        this.scrollbar_drag = Some(ScrollbarDragV1 {
                            page,
                            grab_offset_y: pointer - thumb_top,
                        });
                    } else if maximum > 0.0 {
                        let target = if pointer < thumb_top {
                            current - viewport
                        } else {
                            current + viewport
                        }
                        .clamp(0.0, maximum);
                        let offset = click_handle.offset();
                        click_handle.set_offset(point(offset.x, px(-target)));
                    }
                    cx.stop_propagation();
                    cx.notify();
                    window.refresh();
                }),
            )
            .child(
                div()
                    .absolute()
                    .top(px(thumb_top))
                    .right(px((track_width - thumb_width) / 2.0))
                    .w(px(thumb_width))
                    .h(px(thumb_height))
                    .rounded(px(self.tokens.layout.corner_radius.value()))
                    .bg(if maximum > 0.0 {
                        colors.text_disabled.to_gpui()
                    } else {
                        colors.divider.to_gpui()
                    })
                    .when(maximum > 0.0, |thumb| {
                        thumb.hover(|style| style.bg(colors.text_secondary.to_gpui()))
                    }),
            )
    }
}

impl Focusable for FolderOptionsWindow {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Render for FolderOptionsWindow {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if !window.is_window_active() {
            self.stop_drag();
        }
        let draft = self.snapshot.draft.clone();
        let extensions = self.snapshot.extensions.clone();
        let page = draft.page;
        let scroll = self.scroll_for_page(page).clone();
        let scrollbar = self.scrollbar(page, scroll.clone(), cx).into_any_element();
        let owner = self.owner;
        let on_action: ActionCallback = Rc::new(cx.listener(
            move |this, action: &ExplorerAction, window, cx| {
                let close = matches!(
                    action,
                    ExplorerAction::CloseFolderOptions | ExplorerAction::ConfirmFolderOptions
                );
                let cache_budgets = matches!(
                    action,
                    ExplorerAction::ApplyFolderOptions | ExplorerAction::ConfirmFolderOptions
                )
                .then(|| {
                    let mut budgets = this.snapshot.draft.settings.cache_budgets;
                    for (descriptor, input) in explorer_model::CACHE_BUDGET_DESCRIPTORS_V1
                        .into_iter()
                        .zip(&this.cache_budget_inputs)
                    {
                        if let Some(value) =
                            parse_cache_budget_memory_mb(input.read(cx).as_str(), descriptor)
                            && value != this.cache_budget_input_baseline.get(descriptor.id)
                        {
                            budgets.set(descriptor.id, value);
                        }
                    }
                    budgets.normalized()
                });
                match owner.update(cx, |root, owner_window, cx| {
                    if let Some(budgets) = cache_budgets {
                        tracing::info!(
                            mft_lru_mb = budgets.mft_lru_mb,
                            "Folder Options cache budgets committed from editors"
                        );
                        root.dispatch_folder_options_action(
                            ExplorerAction::SetFolderOptionCacheBudgets(budgets),
                            ActionSource::Keyboard,
                            owner_window,
                            cx,
                        );
                    }
                    root.dispatch_folder_options_action(
                        action.clone(),
                        ActionSource::Mouse,
                        owner_window,
                        cx,
                    );
                    root.state
                        .folder_options()
                        .map(|draft| FolderOptionsWindowSnapshotV1 {
                            draft,
                            extensions: root.state.extensions().to_vec(),
                            cache_usage: root.cache_usage_snapshot(this.snapshot.cache_usage),
                        })
                }) {
                    Ok(Some(snapshot)) => this.snapshot = snapshot,
                    Ok(None) if !close => {
                        tracing::warn!(
                            "Folder Options draft disappeared during a non-closing action"
                        );
                    }
                    Ok(None) => {}
                    Err(error) => {
                        tracing::warn!(%error, "Folder Options action owner is unavailable");
                    }
                }
                if let ExplorerAction::SetFolderOptionCacheBudgets(budgets) = action {
                    tracing::info!(
                        mft_lru_mb = budgets.mft_lru_mb,
                        "Folder Options cache budgets updated"
                    );
                    for (descriptor, input) in explorer_model::CACHE_BUDGET_DESCRIPTORS_V1
                        .into_iter()
                        .zip(&this.cache_budget_inputs)
                    {
                        let value = budgets.get(descriptor.id).to_string();
                        input.update(cx, |state, cx| state.emplace(&value, cx));
                    }
                }
                if let Some(budgets) = cache_budgets {
                    for (descriptor, input) in explorer_model::CACHE_BUDGET_DESCRIPTORS_V1
                        .into_iter()
                        .zip(&this.cache_budget_inputs)
                    {
                        let value = budgets.get(descriptor.id).to_string();
                        input.update(cx, |state, cx| state.emplace(&value, cx));
                    }
                    this.cache_budget_input_baseline = budgets;
                }
                this.stop_drag();
                if close {
                    window.remove_window();
                } else {
                    cx.notify();
                    window.refresh();
                }
            },
        ));

        div()
            .id("folder-options-window")
            .debug_selector(|| "folder-options-window".to_owned())
            .role(Role::Dialog)
            .aria_label("資料夾選項")
            .size_full()
            .relative()
            .track_focus(&self.focus_handle)
            .on_mouse_move(cx.listener(|this, event: &MouseMoveEvent, window, cx| {
                if this.update_drag(event.position.y) {
                    cx.stop_propagation();
                    cx.notify();
                    window.refresh();
                }
            }))
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(|this, _, _, cx| {
                    if this.scrollbar_drag.take().is_some() {
                        cx.stop_propagation();
                        cx.notify();
                    }
                }),
            )
            .on_mouse_up_out(
                MouseButton::Left,
                cx.listener(|this, _, _, cx| {
                    if this.scrollbar_drag.take().is_some() {
                        cx.stop_propagation();
                        cx.notify();
                    }
                }),
            )
            .on_key_down(cx.listener(move |this, event: &KeyDownEvent, window, cx| {
                if event.keystroke.key == "escape" {
                    cx.stop_propagation();
                    this.close_with_action(
                        ExplorerAction::CloseFolderOptions,
                        ActionSource::Keyboard,
                        window,
                        cx,
                    );
                } else if this.keyboard_scroll(page, event.keystroke.key.as_str()) {
                    cx.stop_propagation();
                    cx.notify();
                    window.refresh();
                }
            }))
            .child(chrome::folder_options_window_content(
                self.tokens,
                draft,
                extensions,
                scroll,
                scrollbar,
                Some(on_action),
                self.cache_budget_inputs
                    .iter()
                    .map(gpui::Entity::downgrade)
                    .collect(),
                self.snapshot.cache_usage,
            ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cache_budget_input_accepts_arbitrary_values_and_clamps_per_row() {
        let descriptor =
            explorer_model::cache_budget_descriptor(explorer_model::CacheBudgetIdV1::MftLru);
        assert_eq!(
            parse_cache_budget_memory_mb("4096", descriptor),
            Some(4_096)
        );
        assert_eq!(
            parse_cache_budget_memory_mb("4097", descriptor),
            Some(4_097)
        );
        assert_eq!(
            parse_cache_budget_memory_mb("16384", descriptor),
            Some(16_384)
        );
        assert_eq!(
            parse_cache_budget_memory_mb("16385", descriptor),
            Some(16_384)
        );
        assert_eq!(
            parse_cache_budget_memory_mb(" 2048 ", descriptor),
            Some(2_048)
        );
        assert_eq!(parse_cache_budget_memory_mb("0", descriptor), Some(128));
        assert_eq!(parse_cache_budget_memory_mb("", descriptor), None);
        assert_eq!(parse_cache_budget_memory_mb("512 MB", descriptor), None);
    }

    #[test]
    fn window_options_are_modeless_resizable_and_bounded() {
        let cx = gpui::TestAppContext::single();
        let app = cx.app.borrow();
        let options = folder_options_window_options(&app);
        assert_eq!(options.kind, gpui::WindowKind::Normal);
        assert!(options.is_resizable);
        assert_eq!(options.window_min_size, Some(size(px(680.0), px(480.0))));
    }

    #[test]
    fn disk_sampler_reentry_reuses_latest_completed_snapshot() {
        let expected = CacheUsageSnapshotV1 {
            icon_disk_bytes: Some(41),
            ..CacheUsageSnapshotV1::default()
        };
        let sampler = CacheUsageSamplerV1::new(expected);
        sampler.in_flight.store(true, Ordering::Release);
        assert_eq!(sampler.sample(None), expected);
        assert!(sampler.in_flight.load(Ordering::Acquire));
    }

    #[test]
    fn host_snapshot_maps_disk_values_without_directory_scanning() {
        let available = |id, category, bytes| explorer_model::CacheTelemetryEntryV1 {
            id,
            category,
            availability: explorer_model::CacheTelemetryAvailabilityV1::Available(
                explorer_model::CacheTelemetryValueV1 {
                    bytes,
                    limit_bytes: None,
                    entry_count: 1,
                    counters: None,
                },
            ),
        };
        let telemetry = explorer_model::CacheTelemetrySnapshotV1::new(vec![
            available(
                explorer_model::CacheTelemetryIdV1::IconsDisk,
                explorer_model::CacheTelemetryCategoryV1::Disk,
                11,
            ),
            available(
                explorer_model::CacheTelemetryIdV1::ThumbnailsDisk,
                explorer_model::CacheTelemetryCategoryV1::Disk,
                22,
            ),
            available(
                explorer_model::CacheTelemetryIdV1::ExtensionColumnsDisk,
                explorer_model::CacheTelemetryCategoryV1::Disk,
                33,
            ),
            available(
                explorer_model::CacheTelemetryIdV1::MftPersistedIndex,
                explorer_model::CacheTelemetryCategoryV1::MftService,
                44,
            ),
            available(
                explorer_model::CacheTelemetryIdV1::MftVolumeIndexMemory,
                explorer_model::CacheTelemetryCategoryV1::MftService,
                55,
            ),
            available(
                explorer_model::CacheTelemetryIdV1::MftFileDataMemory,
                explorer_model::CacheTelemetryCategoryV1::MftService,
                66,
            ),
            available(
                explorer_model::CacheTelemetryIdV1::MftAggregateMemory,
                explorer_model::CacheTelemetryCategoryV1::MftService,
                77,
            ),
        ])
        .unwrap();
        let mut snapshot = CacheUsageSnapshotV1::default();
        snapshot.apply_host_telemetry(&telemetry);
        assert_eq!(snapshot.icon_disk_bytes, Some(11));
        assert_eq!(snapshot.thumbnail_disk_bytes, Some(22));
        assert_eq!(snapshot.extension_disk_bytes, Some(33));
        assert_eq!(snapshot.mft_disk_bytes, Some(44));
        assert_eq!(snapshot.mft_volume_index_memory_bytes, Some(55));
        assert_eq!(snapshot.mft_file_data_memory_bytes, Some(66));
        assert_eq!(snapshot.mft_aggregate_memory_bytes, Some(77));
    }

    #[test]
    fn pending_sample_retains_last_success_and_unavailable_does_not_claim_pending() {
        let id = explorer_model::CacheTelemetryIdV1::MftServiceLru;
        let available = explorer_model::CacheTelemetrySnapshotV1::new(vec![
            explorer_model::CacheTelemetryEntryV1 {
                id,
                category: explorer_model::CacheTelemetryCategoryV1::MftService,
                availability: explorer_model::CacheTelemetryAvailabilityV1::Available(
                    explorer_model::CacheTelemetryValueV1 {
                        bytes: 41,
                        limit_bytes: Some(512),
                        entry_count: 3,
                        counters: Some(explorer_model::CacheTelemetryCountersV1 {
                            hits: 5,
                            misses: 7,
                        }),
                    },
                ),
            },
        ])
        .unwrap();
        let mut previous = CacheUsageSnapshotV1::default();
        previous.apply_host_telemetry(&available);

        let pending = explorer_model::CacheTelemetrySnapshotV1::new(vec![
            explorer_model::CacheTelemetryEntryV1 {
                id,
                category: explorer_model::CacheTelemetryCategoryV1::MftService,
                availability: explorer_model::CacheTelemetryAvailabilityV1::Pending,
            },
        ])
        .unwrap();
        let mut next = CacheUsageSnapshotV1::default();
        next.apply_host_telemetry(&pending);
        next.retain_pending_from(previous);
        assert_eq!(next.availability(id), CacheUsageAvailabilityV1::Pending);
        assert_eq!(next.mft_service_bytes, Some(41));
        assert_eq!(next.mft_service_limit, Some(512));
        assert_eq!(next.mft_service_hits, Some(5));

        let unavailable = explorer_model::CacheTelemetrySnapshotV1::new(vec![
            explorer_model::CacheTelemetryEntryV1 {
                id,
                category: explorer_model::CacheTelemetryCategoryV1::MftService,
                availability: explorer_model::CacheTelemetryAvailabilityV1::Unavailable,
            },
        ])
        .unwrap();
        next.apply_host_telemetry(&unavailable);
        assert_eq!(next.availability(id), CacheUsageAvailabilityV1::Unavailable);

        let recovered = explorer_model::CacheTelemetrySnapshotV1::new(vec![
            explorer_model::CacheTelemetryEntryV1 {
                id,
                category: explorer_model::CacheTelemetryCategoryV1::MftService,
                availability: explorer_model::CacheTelemetryAvailabilityV1::Available(
                    explorer_model::CacheTelemetryValueV1 {
                        bytes: 99,
                        limit_bytes: Some(2_048),
                        entry_count: 4,
                        counters: None,
                    },
                ),
            },
        ])
        .unwrap();
        next.apply_host_telemetry(&recovered);
        assert_eq!(next.availability(id), CacheUsageAvailabilityV1::Available);
        assert_eq!(next.mft_service_bytes, Some(99));
        assert_eq!(next.mft_service_limit, Some(2_048));
    }
}
