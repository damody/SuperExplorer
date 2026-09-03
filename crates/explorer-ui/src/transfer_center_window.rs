//! Native transient transfer-center window presentation.

use std::{collections::HashSet, rc::Rc, time::Duration};

use gpui::{
    App, Bounds, Context, Pixels, Render, Window, WindowBounds, WindowHandle, WindowOptions,
    prelude::*,
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

pub fn transfer_window_bounds(owner: Bounds<Pixels>) -> Bounds<Pixels> {
    let width = gpui::px(TRANSFER_WINDOW_WIDTH);
    let height = gpui::px(TRANSFER_WINDOW_HEIGHT).min(owner.size.height);
    Bounds::new(
        gpui::point(
            (owner.right() - width).max(owner.left()),
            (owner.top() + gpui::px(112.0)).min((owner.bottom() - height).max(owner.top())),
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
}

impl TransferCenterWindow {
    pub fn new(
        owner: WindowHandle<ExplorerRoot>,
        on_deactivate: Rc<dyn Fn(&mut App)>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
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
                    .update(cx, |_, cx| {
                        poll_deactivate(cx);
                        cx.notify();
                    })
                    .is_err()
                {
                    break;
                }
            }
        })
        .detach();
        Self { owner }
    }
}

impl Render for TransferCenterWindow {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let snapshot = self
            .owner
            .update(cx, |root, _, _| root.transfer_window_snapshot())
            .ok();
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
        let content = snapshot.map_or_else(
            || gpui::div().into_any_element(),
            |snapshot| {
                transfer_center_panel(
                    snapshot.tokens,
                    snapshot.records,
                    &snapshot.cancelling_ids,
                    Some(on_action),
                )
                .into_any_element()
            },
        );
        let owner = self.owner;
        gpui::div()
            .size_full()
            .on_key_down(move |event, _, cx| {
                if event.keystroke.key == "escape" {
                    let _ = owner.update(cx, |root, owner_window, cx| {
                        root.dispatch_transfer_window_action(
                            crate::actions::ExplorerAction::CloseTransferPanel,
                            ActionSource::Keyboard,
                            owner_window,
                            cx,
                        );
                    });
                }
            })
            .child(content)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn transfer_window_anchors_to_owner_top_right_without_exceeding_owner() {
        let owner = Bounds::new(
            gpui::point(gpui::px(100.0), gpui::px(50.0)),
            gpui::size(gpui::px(1200.0), gpui::px(800.0)),
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
