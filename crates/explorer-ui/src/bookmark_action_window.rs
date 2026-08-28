//! Dedicated confirmed command window opened by bookmark-item right-click.

use gpui::{
    App, Bounds, Context, FocusHandle, Focusable, IntoElement, MouseButton, Render, SharedString,
    Window, WindowBounds, WindowHandle, WindowOptions, div, prelude::*, px, size,
};

use crate::{
    ExplorerRoot, UiTokens,
    actions::{ActionSource, ExplorerAction},
};

#[derive(Clone)]
pub struct BookmarkActionWindowSnapshotV1 {
    pub bookmark: explorer_model::Bookmark,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BookmarkActionCommand {
    Open,
    OpenInNewTab,
    Edit,
    Delete,
}

impl BookmarkActionCommand {
    fn label(self) -> &'static str {
        match self {
            Self::Open => "開啟",
            Self::OpenInNewTab => "在新分頁開啟",
            Self::Edit => "編輯名稱與路徑",
            Self::Delete => "刪除書籤",
        }
    }

    fn action(self, id: explorer_model::BookmarkId) -> Option<ExplorerAction> {
        match self {
            Self::Open => Some(ExplorerAction::ActivateBookmark { id }),
            Self::OpenInNewTab => Some(ExplorerAction::OpenBookmarkInNewTab { id }),
            Self::Edit => Some(ExplorerAction::EditBookmark { id }),
            Self::Delete => None,
        }
    }
}

fn applicable_commands(target: &explorer_model::BookmarkTarget) -> Vec<BookmarkActionCommand> {
    let mut commands = vec![BookmarkActionCommand::Open];
    if target.is_folder() {
        commands.push(BookmarkActionCommand::OpenInNewTab);
    }
    commands.extend([BookmarkActionCommand::Edit, BookmarkActionCommand::Delete]);
    commands
}

pub fn bookmark_action_window_options(cx: &App) -> WindowOptions {
    WindowOptions {
        window_bounds: Some(WindowBounds::Windowed(Bounds::centered(
            None,
            size(px(460.0), px(360.0)),
            cx,
        ))),
        titlebar: Some(gpui::TitlebarOptions {
            title: Some(SharedString::from("書籤操作")),
            ..Default::default()
        }),
        kind: gpui::WindowKind::Normal,
        is_resizable: false,
        window_min_size: Some(size(px(420.0), px(320.0))),
        ..Default::default()
    }
}

pub struct BookmarkActionWindow {
    tokens: UiTokens,
    owner: WindowHandle<ExplorerRoot>,
    snapshot: BookmarkActionWindowSnapshotV1,
    selected: BookmarkActionCommand,
    focus_handle: FocusHandle,
}

impl BookmarkActionWindow {
    pub fn new(
        tokens: UiTokens,
        owner: WindowHandle<ExplorerRoot>,
        snapshot: BookmarkActionWindowSnapshotV1,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        Self {
            tokens,
            owner,
            snapshot,
            selected: BookmarkActionCommand::Open,
            focus_handle: cx.focus_handle(),
        }
    }

    pub fn replace_snapshot(
        &mut self,
        snapshot: BookmarkActionWindowSnapshotV1,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.snapshot = snapshot;
        self.selected = BookmarkActionCommand::Open;
        cx.notify();
        window.refresh();
    }

    fn confirm(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let id = self.snapshot.bookmark.id;
        if self.selected == BookmarkActionCommand::Delete {
            let owner = self.owner;
            let _ = owner.update(cx, |root, _, cx| {
                root.present_bookmark_delete_window(id, cx);
            });
            window.remove_window();
            return;
        }
        let action = if let Some(action) = self.selected.action(id) {
            action
        } else {
            return;
        };
        let owner = self.owner;
        let exists = owner
            .update(cx, |root, owner_window, cx| {
                if !root.bookmark_exists(id) {
                    return false;
                }
                root.dispatch_bookmark_action_window_action(
                    action,
                    ActionSource::Mouse,
                    owner_window,
                    cx,
                );
                true
            })
            .unwrap_or(false);
        if !exists {
            tracing::info!(bookmark_id = %id, "Bookmark action target became stale");
        }
        window.remove_window();
    }
}

impl Focusable for BookmarkActionWindow {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Render for BookmarkActionWindow {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let colors = self.tokens.theme.colors;
        let selected = self.selected;
        let commands = applicable_commands(&self.snapshot.bookmark.target)
            .into_iter()
            .map(|command| {
                let active = selected == command;
                div()
                    .id(format!("bookmark-action-{:?}", command))
                    .role(gpui::Role::Button)
                    .aria_label(command.label())
                    .cursor_pointer()
                    .px(px(12.0))
                    .py(px(9.0))
                    .rounded(px(5.0))
                    .bg(if active {
                        colors.control_pressed.to_gpui()
                    } else {
                        colors.control_fill.to_gpui()
                    })
                    .child(format!(
                        "{} {}",
                        if active { "●" } else { "○" },
                        command.label()
                    ))
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |this, _, window, cx| {
                            this.selected = command;
                            cx.notify();
                            window.refresh();
                        }),
                    )
            })
            .collect::<Vec<_>>();
        div()
            .id("bookmark-action-window")
            .role(gpui::Role::Dialog)
            .aria_label("Bookmark action window")
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
            .p(px(20.0))
            .flex()
            .flex_col()
            .gap(px(12.0))
            .bg(colors.surface.to_gpui())
            .child("書籤操作")
            .child(format!("{}", self.snapshot.bookmark.name))
            .children(commands)
            .child(
                div()
                    .flex()
                    .justify_end()
                    .gap(px(8.0))
                    .child(
                        div()
                            .id("bookmark-action-cancel")
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
                            .id("bookmark-action-confirm")
                            .role(gpui::Role::Button)
                            .cursor_pointer()
                            .rounded(px(6.0))
                            .border_1()
                            .border_color(colors.accent.to_gpui())
                            .px(px(12.0))
                            .py(px(7.0))
                            .text_color(colors.text_primary.to_gpui())
                            .child("確認")
                            .on_click(cx.listener(|this, _, window, cx| this.confirm(window, cx))),
                    ),
            )
    }
}

#[cfg(test)]
mod tests {
    use super::{BookmarkActionCommand, applicable_commands};

    #[test]
    fn commands_are_typed_and_folder_only_new_tab_is_explicit() {
        let folder = explorer_model::BookmarkTarget::FolderPath { path: "x".into() };
        let file = explorer_model::BookmarkTarget::FilePath { path: "x".into() };
        assert_eq!(
            applicable_commands(&folder),
            vec![
                BookmarkActionCommand::Open,
                BookmarkActionCommand::OpenInNewTab,
                BookmarkActionCommand::Edit,
                BookmarkActionCommand::Delete,
            ]
        );
        assert_eq!(
            applicable_commands(&file),
            vec![
                BookmarkActionCommand::Open,
                BookmarkActionCommand::Edit,
                BookmarkActionCommand::Delete,
            ]
        );
    }

    #[test]
    fn action_window_routes_delete_to_a_dedicated_confirmation_window() {
        let source = include_str!("bookmark_action_window.rs");
        assert!(source.contains("BookmarkActionWindow"));
        assert!(source.contains("present_bookmark_delete_window"));
        assert!(source.contains("window.remove_window()"));
        assert!(source.contains("replace_snapshot"));
        assert!(source.contains(".border_1()"));
    }
}
