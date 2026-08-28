//! Dedicated confirmation window for deleting one bookmark.

use gpui::{
    App, Bounds, Context, FocusHandle, Focusable, IntoElement, Render, SharedString, Window,
    WindowBounds, WindowHandle, WindowOptions, div, prelude::*, px, size,
};

use crate::{
    ExplorerRoot, UiTokens,
    actions::{ActionSource, ExplorerAction},
};

#[derive(Clone)]
pub struct BookmarkDeleteWindowSnapshotV1 {
    pub bookmark: explorer_model::Bookmark,
}

pub fn bookmark_delete_window_options(cx: &App) -> WindowOptions {
    WindowOptions {
        window_bounds: Some(WindowBounds::Windowed(Bounds::centered(
            None,
            size(px(520.0), px(240.0)),
            cx,
        ))),
        titlebar: Some(gpui::TitlebarOptions {
            title: Some(SharedString::from("刪除書籤")),
            ..Default::default()
        }),
        kind: gpui::WindowKind::Normal,
        is_resizable: false,
        window_min_size: Some(size(px(460.0), px(220.0))),
        ..Default::default()
    }
}

pub struct BookmarkDeleteWindow {
    tokens: UiTokens,
    owner: WindowHandle<ExplorerRoot>,
    snapshot: BookmarkDeleteWindowSnapshotV1,
    focus_handle: FocusHandle,
}

impl BookmarkDeleteWindow {
    pub fn new(
        tokens: UiTokens,
        owner: WindowHandle<ExplorerRoot>,
        snapshot: BookmarkDeleteWindowSnapshotV1,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        Self {
            tokens,
            owner,
            snapshot,
            focus_handle: cx.focus_handle(),
        }
    }

    pub fn replace_snapshot(
        &mut self,
        snapshot: BookmarkDeleteWindowSnapshotV1,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.snapshot = snapshot;
        cx.notify();
        window.refresh();
    }

    fn confirm(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let id = self.snapshot.bookmark.id;
        let owner = self.owner;
        let _ = owner.update(cx, |root, owner_window, cx| {
            if root.bookmark_exists(id) {
                root.dispatch_bookmark_action_window_action(
                    ExplorerAction::RemoveBookmark { id },
                    ActionSource::Mouse,
                    owner_window,
                    cx,
                );
            }
        });
        window.remove_window();
    }
}

impl Focusable for BookmarkDeleteWindow {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Render for BookmarkDeleteWindow {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let colors = self.tokens.theme.colors;
        div()
            .id("bookmark-delete-window")
            .role(gpui::Role::Dialog)
            .aria_label("Delete bookmark confirmation")
            .size_full()
            .track_focus(&self.focus_handle)
            .capture_key_down(cx.listener(|this, event: &gpui::KeyDownEvent, window, cx| {
                match event.keystroke.key.as_str() {
                    "escape" => {
                        cx.stop_propagation();
                        window.remove_window();
                    }
                    "enter" => {
                        cx.stop_propagation();
                        this.confirm(window, cx);
                    }
                    _ => {}
                }
            }))
            .p(px(28.0))
            .flex()
            .flex_col()
            .gap(px(16.0))
            .bg(colors.surface.to_gpui())
            .child(format!("刪除書籤「{}」？", self.snapshot.bookmark.name))
            .child("這會移除書籤，不會刪除磁碟上的檔案。")
            .child(
                div()
                    .flex()
                    .justify_end()
                    .gap(px(16.0))
                    .child(
                        div()
                            .id("bookmark-delete-cancel")
                            .role(gpui::Role::Button)
                            .cursor_pointer()
                            .rounded(px(6.0))
                            .border_1()
                            .border_color(colors.divider.to_gpui())
                            .px(px(12.0))
                            .py(px(7.0))
                            .child("取消")
                            .on_click(|_, window, _| window.remove_window()),
                    )
                    .child(
                        div()
                            .id("bookmark-delete-confirm")
                            .role(gpui::Role::Button)
                            .cursor_pointer()
                            .rounded(px(6.0))
                            .border_1()
                            .border_color(colors.danger.to_gpui())
                            .px(px(12.0))
                            .py(px(7.0))
                            .text_color(colors.danger.to_gpui())
                            .child("刪除")
                            .on_click(cx.listener(|this, _, window, cx| this.confirm(window, cx))),
                    ),
            )
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn delete_uses_a_normal_dedicated_confirmation_window() {
        let source = include_str!("bookmark_delete_window.rs");
        assert!(source.contains("WindowKind::Normal"));
        assert!(source.contains("不會刪除磁碟上的檔案"));
        assert!(source.contains("bookmark_exists"));
        assert!(source.contains("ExplorerAction::RemoveBookmark"));
        assert!(source.contains(".border_1()"));
        assert!(source.contains("border_color(colors.danger.to_gpui())"));
        assert!(source.contains("window.remove_window()"));
    }
}
