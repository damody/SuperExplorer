//! Native transient transfer-center window presentation.

use std::{collections::HashSet, rc::Rc, time::Duration};

use gpui::{
    App, Bounds, Context, FocusHandle, Focusable, Pixels, Render, ScrollHandle, Window,
    WindowBounds, WindowHandle, WindowOptions, point, prelude::*, px,
};

use crate::{
    ExplorerRoot, UiTokens,
    actions::ActionSource,
    chrome::{ActionCallback, transfer_center_panel},
};

pub const TRANSFER_WINDOW_WIDTH: f32 = 520.0;
pub const TRANSFER_WINDOW_HEIGHT: f32 = 560.0;

#[derive(Clone, Copy, Debug)]
pub struct TransferWindowRequestV1 {
    pub visible: bool,
    pub owner_hwnd: u64,
    pub owner_bounds: Bounds<Pixels>,
}

#[derive(Clone)]
pub struct TransferWindowSnapshotV1 {
    pub tokens: UiTokens,
    pub records: Vec<explorer_model::OperationRecord>,
    pub cancelling_ids: HashSet<explorer_common::RequestId>,
}

impl PartialEq for TransferWindowSnapshotV1 {
    fn eq(&self, other: &Self) -> bool {
        self.tokens == other.tokens
            && self.cancelling_ids == other.cancelling_ids
            && self.records.len() == other.records.len()
            && self
                .records
                .iter()
                .zip(&other.records)
                .all(|(left, right)| {
                    left.id == right.id
                        && left.request == right.request
                        && left.phase == right.phase
                        && left.progress == right.progress
                        && left.terminal == right.terminal
                        && left.bytes_per_second().map(f64::to_bits)
                            == right.bytes_per_second().map(f64::to_bits)
                })
    }
}

pub fn transfer_window_bounds(owner: Bounds<Pixels>) -> Bounds<Pixels> {
    let width = px(TRANSFER_WINDOW_WIDTH);
    let height = px(TRANSFER_WINDOW_HEIGHT).min(owner.size.height);
    Bounds::new(
        point(
            (owner.right() - width).max(owner.left()),
            (owner.top() + px(112.0)).min((owner.bottom() - height).max(owner.top())),
        ),
        gpui::size(width, height),
    )
}

pub fn transfer_window_options(bounds: Bounds<Pixels>) -> WindowOptions {
    WindowOptions {
        window_bounds: Some(WindowBounds::Windowed(bounds)),
        titlebar: None,
        focus: true,
        show: true,
        kind: gpui::WindowKind::PopUp,
        is_movable: false,
        is_resizable: false,
        is_minimizable: false,
        window_min_size: Some(bounds.size),
        ..Default::default()
    }
}

pub struct TransferCenterWindow {
    owner: WindowHandle<ExplorerRoot>,
    focus_handle: FocusHandle,
    scroll_handle: ScrollHandle,
    snapshot: Option<TransferWindowSnapshotV1>,
}

impl TransferCenterWindow {
    pub fn new(
        owner: WindowHandle<ExplorerRoot>,
        on_deactivate: Rc<dyn Fn(&mut App)>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let focus_handle = cx.focus_handle();
        focus_handle.focus(window, cx);
        let initial_snapshot = owner
            .update(cx, |root, _, _| root.transfer_window_snapshot())
            .ok();
        let activation_deactivate = Rc::clone(&on_deactivate);
        cx.observe_window_activation(window, move |_, window, cx| {
            if window.is_window_active() {
                return;
            }
            let callback = Rc::clone(&activation_deactivate);
            cx.spawn(async move |_, cx| {
                cx.background_executor()
                    .timer(Duration::from_millis(75))
                    .await;
                let _ = cx.update(|cx| callback(cx));
            })
            .detach();
        })
        .detach();
        let poll_deactivate = Rc::clone(&on_deactivate);
        cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor()
                    // Sample between the 200 ms provider publication ticks so
                    // a just-published native ADB snapshot is painted promptly.
                    .timer(Duration::from_millis(100))
                    .await;
                if this
                    .update(cx, |this, cx| {
                        poll_deactivate(cx);
                        let next_snapshot = this
                            .owner
                            .update(cx, |root, _, _| root.transfer_window_snapshot())
                            .ok();
                        if next_snapshot != this.snapshot {
                            this.snapshot = next_snapshot;
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
            owner,
            focus_handle,
            scroll_handle: ScrollHandle::new(),
            snapshot: initial_snapshot,
        }
    }

    fn keyboard_scroll(&self, key: &str) -> bool {
        let viewport = f32::from(self.scroll_handle.bounds().size.height).max(0.0);
        let maximum = f32::from(self.scroll_handle.max_offset().y).max(0.0);
        if !viewport.is_finite() || !maximum.is_finite() || viewport <= 0.0 {
            return false;
        }
        let current = (-f32::from(self.scroll_handle.offset().y)).clamp(0.0, maximum);
        let target = match key {
            "up" | "arrowup" => current - 48.0,
            "down" | "arrowdown" => current + 48.0,
            "pageup" => current - viewport,
            "pagedown" => current + viewport,
            "home" => 0.0,
            "end" => maximum,
            _ => return false,
        }
        .clamp(0.0, maximum);
        let offset = self.scroll_handle.offset();
        self.scroll_handle.set_offset(point(offset.x, px(-target)));
        true
    }
}

impl Focusable for TransferCenterWindow {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Render for TransferCenterWindow {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if window.is_window_active() && window.focused(cx).is_none() {
            self.focus_handle.focus(window, cx);
        }
        if let Ok(next_snapshot) = self
            .owner
            .update(cx, |root, _, _| root.transfer_window_snapshot())
        {
            self.snapshot = Some(next_snapshot);
        }
        let snapshot = self.snapshot.clone();
        let owner = self.owner;
        let on_action: ActionCallback = Rc::new(move |action, _, cx| {
            let action = action.clone();
            let _ = owner.update(cx, |root, owner_window, cx| {
                root.dispatch_transfer_window_action(
                    action,
                    ActionSource::Programmatic,
                    owner_window,
                    cx,
                );
            });
        });
        let catalog = self
            .owner
            .update(cx, |root, _, _| root.catalog())
            .unwrap_or_else(|_| explorer_i18n::Catalog::new(explorer_i18n::AppLocale::ZhTw));
        let content = snapshot.map_or_else(
            || gpui::div().into_any_element(),
            |snapshot| {
                transfer_center_panel(
                    snapshot.tokens,
                    catalog,
                    snapshot.records,
                    &snapshot.cancelling_ids,
                    Some(on_action),
                )
                .into_any_element()
            },
        );
        gpui::div()
            .id("transfer-center-window")
            .size_full()
            .track_focus(&self.focus_handle)
            .capture_key_down(cx.listener(|this, event: &gpui::KeyDownEvent, window, cx| {
                let key = event.keystroke.key.as_str();
                if key == "escape" {
                    cx.stop_propagation();
                    let owner = this.owner;
                    let _ = owner.update(cx, |root, owner_window, cx| {
                        root.dispatch_transfer_window_action(
                            crate::actions::ExplorerAction::CloseTransferPanel,
                            ActionSource::Keyboard,
                            owner_window,
                            cx,
                        );
                    });
                } else if this.keyboard_scroll(key) {
                    cx.stop_propagation();
                    cx.notify();
                    window.refresh();
                }
            }))
            .child(
                gpui::div()
                    .id("transfer-center-list")
                    .size_full()
                    .overflow_y_scroll()
                    .track_scroll(&self.scroll_handle)
                    .child(content),
            )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn transfer_window_anchors_to_owner_top_right_without_exceeding_owner() {
        let owner = Bounds::new(
            point(px(100.0), px(50.0)),
            gpui::size(px(1200.0), px(800.0)),
        );
        let bounds = transfer_window_bounds(owner);
        assert_eq!(bounds.right(), owner.right());
        assert!(bounds.top() >= owner.top());
        assert!(bounds.bottom() <= owner.bottom());
        let options = transfer_window_options(bounds);
        assert_eq!(options.kind, gpui::WindowKind::PopUp);
        assert!(options.titlebar.is_none());
    }
}
