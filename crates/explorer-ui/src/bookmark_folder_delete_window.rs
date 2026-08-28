//! Dedicated confirmation window for deleting one bookmark folder.

use gpui::{
    App, Bounds, Context, FocusHandle, Focusable, IntoElement, Render, SharedString, Window,
    WindowBounds, WindowHandle, WindowOptions, div, prelude::*, px, size,
};

use crate::{
    ExplorerRoot, UiTokens,
    actions::{ActionSource, ExplorerAction},
};

#[derive(Clone)]
pub struct BookmarkFolderDeleteWindowSnapshotV1 {
    pub id: explorer_model::BookmarkFolderId,
    pub name: String,
    pub descendant_count: usize,
}

pub fn bookmark_folder_delete_window_options(cx: &App) -> WindowOptions {
    WindowOptions {
        window_bounds: Some(WindowBounds::Windowed(Bounds::centered(
            None,
            size(px(560.0), px(250.0)),
            cx,
        ))),
        titlebar: Some(gpui::TitlebarOptions {
            title: Some(SharedString::from("刪除書籤資料夾")),
            ..Default::default()
        }),
        kind: gpui::WindowKind::Normal,
        is_resizable: false,
        window_min_size: Some(size(px(500.0), px(230.0))),
        ..Default::default()
    }
}

pub struct BookmarkFolderDeleteWindow {
    tokens: UiTokens,
    owner: WindowHandle<ExplorerRoot>,
    snapshot: BookmarkFolderDeleteWindowSnapshotV1,
    focus_handle: FocusHandle,
}

impl BookmarkFolderDeleteWindow {
    pub fn new(
        tokens: UiTokens,
        owner: WindowHandle<ExplorerRoot>,
        snapshot: BookmarkFolderDeleteWindowSnapshotV1,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        window.on_window_should_close(cx, move |_, cx| {
            let _ = owner.update(cx, |root, owner_window, cx| {
                root.dispatch_bookmark_folder_delete_window_action(
                    ExplorerAction::CancelRemoveBookmarkFolder,
                    ActionSource::Programmatic,
                    owner_window,
                    cx,
                );
            });
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
        snapshot: BookmarkFolderDeleteWindowSnapshotV1,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.snapshot = snapshot;
        cx.notify();
        window.refresh();
    }

    fn dispatch_and_close(
        &mut self,
        action: ExplorerAction,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let owner = self.owner;
        let _ = owner.update(cx, |root, owner_window, cx| {
            root.dispatch_bookmark_folder_delete_window_action(
                action,
                ActionSource::Mouse,
                owner_window,
                cx,
            );
        });
        window.remove_window();
    }
}

impl Focusable for BookmarkFolderDeleteWindow {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Render for BookmarkFolderDeleteWindow {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let colors = self.tokens.theme.colors;
        div()
            .id("bookmark-folder-delete-window")
            .role(gpui::Role::Dialog)
            .aria_label("Delete bookmark folder confirmation")
            .size_full()
            .track_focus(&self.focus_handle)
            .capture_key_down(cx.listener(|this, event: &gpui::KeyDownEvent, window, cx| {
                match event.keystroke.key.as_str() {
                    "escape" => {
                        cx.stop_propagation();
                        this.dispatch_and_close(
                            ExplorerAction::CancelRemoveBookmarkFolder,
                            window,
                            cx,
                        );
                    }
                    "enter" => {
                        cx.stop_propagation();
                        this.dispatch_and_close(
                            ExplorerAction::ConfirmRemoveBookmarkFolder,
                            window,
                            cx,
                        );
                    }
                    _ => {}
                }
            }))
            .p(px(28.0))
            .flex()
            .flex_col()
            .gap(px(16.0))
            .bg(colors.surface.to_gpui())
            .child(format!("刪除書籤資料夾「{}」？", self.snapshot.name))
            .child(format!(
                "這會移除資料夾以及其中 {} 個項目，不會刪除磁碟上的檔案。",
                self.snapshot.descendant_count
            ))
            .child(
                div()
                    .flex()
                    .justify_end()
                    .gap(px(16.0))
                    .child(
                        div()
                            .id("bookmark-folder-delete-window-cancel")
                            .role(gpui::Role::Button)
                            .cursor_pointer()
                            .rounded(px(6.0))
                            .border_1()
                            .border_color(colors.divider.to_gpui())
                            .px(px(12.0))
                            .py(px(7.0))
                            .child("取消")
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.dispatch_and_close(
                                    ExplorerAction::CancelRemoveBookmarkFolder,
                                    window,
                                    cx,
                                );
                            })),
                    )
                    .child(
                        div()
                            .id("bookmark-folder-delete-window-confirm")
                            .role(gpui::Role::Button)
                            .cursor_pointer()
                            .rounded(px(6.0))
                            .border_1()
                            .border_color(colors.danger.to_gpui())
                            .px(px(12.0))
                            .py(px(7.0))
                            .text_color(colors.danger.to_gpui())
                            .child("刪除")
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.dispatch_and_close(
                                    ExplorerAction::ConfirmRemoveBookmarkFolder,
                                    window,
                                    cx,
                                );
                            })),
                    ),
            )
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn folder_delete_uses_a_normal_dedicated_confirmation_window() {
        let source = include_str!("bookmark_folder_delete_window.rs");
        assert!(source.contains("WindowKind::Normal"));
        assert!(source.contains("ConfirmRemoveBookmarkFolder"));
        assert!(source.contains("CancelRemoveBookmarkFolder"));
        assert!(source.contains("不會刪除磁碟上的檔案"));
        assert!(source.contains(".border_1()"));
        assert!(source.contains("border_color(colors.danger.to_gpui())"));
    }
}
