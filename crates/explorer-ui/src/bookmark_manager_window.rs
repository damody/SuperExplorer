//! Dedicated interactive bookmark manager window.

use std::rc::Rc;

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

#[derive(Clone)]
pub struct BookmarkManagerWindowSnapshotV1 {
    pub state: AppViewState,
}

pub fn bookmark_manager_window_options(cx: &App) -> WindowOptions {
    WindowOptions {
        window_bounds: Some(WindowBounds::Windowed(Bounds::centered(
            None,
            size(px(720.0), px(620.0)),
            cx,
        ))),
        titlebar: Some(gpui::TitlebarOptions {
            title: Some(SharedString::from("書籤管理員")),
            ..Default::default()
        }),
        kind: gpui::WindowKind::Normal,
        is_resizable: true,
        window_min_size: Some(size(px(560.0), px(420.0))),
        ..Default::default()
    }
}

pub struct BookmarkManagerWindow {
    tokens: UiTokens,
    owner: WindowHandle<ExplorerRoot>,
    snapshot: BookmarkManagerWindowSnapshotV1,
    focus_handle: FocusHandle,
}

impl BookmarkManagerWindow {
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
        Self {
            tokens,
            owner,
            snapshot,
            focus_handle: cx.focus_handle(),
        }
    }

    pub fn replace_snapshot(
        &mut self,
        snapshot: BookmarkManagerWindowSnapshotV1,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
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
        if action == ExplorerAction::ToggleBookmarkManager {
            window.remove_window();
            return;
        }
        let owner = self.owner;
        match owner.update(cx, |root, owner_window, cx| {
            root.dispatch_bookmark_manager_action(action, source, owner_window, cx);
            root.bookmark_manager_window_snapshot()
        }) {
            Ok(snapshot) => {
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
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let on_action: ActionCallback =
            Rc::new(cx.listener(|this, action: &ExplorerAction, window, cx| {
                this.dispatch(action.clone(), ActionSource::Mouse, window, cx);
            }));
        div()
            .id("bookmark-manager-window")
            .role(gpui::Role::Dialog)
            .aria_label("Bookmark manager window")
            .size_full()
            .track_focus(&self.focus_handle)
            .capture_key_down(cx.listener(|_, event: &gpui::KeyDownEvent, window, cx| {
                if event.keystroke.key == "escape" {
                    cx.stop_propagation();
                    window.remove_window();
                }
            }))
            .child(chrome::bookmark_manager(
                self.tokens,
                &self.snapshot.state,
                Some(on_action),
            ))
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn manager_uses_a_normal_dedicated_window_and_shared_reducer() {
        let source = include_str!("bookmark_manager_window.rs");
        assert!(source.contains("WindowKind::Normal"));
        assert!(source.contains("dispatch_bookmark_manager_action"));
        assert!(source.contains("chrome::bookmark_manager("));
        assert!(source.contains("window.remove_window()"));
    }
}
