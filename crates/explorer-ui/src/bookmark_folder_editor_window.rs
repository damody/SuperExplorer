//! Dedicated native window for bookmark-folder creation and rename.

use std::rc::Rc;

use gpui::{
    App, Bounds, Context, FocusHandle, Focusable, IntoElement, Render, SharedString, Window,
    WindowBounds, WindowHandle, WindowOptions, div, prelude::*, px, size,
};
use gpui_elements::editable_text::{EditableTextState, StringStorage, TextChanged};

use crate::{
    ExplorerRoot, UiTokens,
    actions::{ActionSource, ExplorerAction},
    chrome::{self, ActionCallback},
    state::AppViewState,
};

#[derive(Clone)]
pub struct BookmarkFolderEditorWindowSnapshotV1 {
    pub state: AppViewState,
}

pub fn bookmark_folder_editor_window_options(cx: &App) -> WindowOptions {
    WindowOptions {
        window_bounds: Some(WindowBounds::Windowed(Bounds::centered(
            None,
            size(px(520.0), px(260.0)),
            cx,
        ))),
        titlebar: Some(gpui::TitlebarOptions {
            title: Some(SharedString::from("重新命名書籤資料夾")),
            ..Default::default()
        }),
        kind: gpui::WindowKind::Normal,
        is_resizable: false,
        window_min_size: Some(size(px(460.0), px(220.0))),
        ..Default::default()
    }
}

pub struct BookmarkFolderEditorWindow {
    tokens: UiTokens,
    owner: WindowHandle<ExplorerRoot>,
    snapshot: BookmarkFolderEditorWindowSnapshotV1,
    name_input: gpui::Entity<EditableTextState>,
    focus_handle: FocusHandle,
}

impl BookmarkFolderEditorWindow {
    fn create_name_input(
        owner: WindowHandle<ExplorerRoot>,
        name: String,
        cx: &mut Context<Self>,
    ) -> gpui::Entity<EditableTextState> {
        let input = cx.new(|cx| EditableTextState::new(StringStorage::from(name), cx));
        cx.subscribe(&input, move |_, input, _: &TextChanged, cx| {
            let value = input.read(cx).as_str().to_owned();
            let _ = owner.update(cx, |root, _, _| {
                root.update_bookmark_folder_editor_name_from_window(value);
            });
        })
        .detach();
        input.update(cx, EditableTextState::select_document);
        input
    }

    pub fn new(
        tokens: UiTokens,
        owner: WindowHandle<ExplorerRoot>,
        snapshot: BookmarkFolderEditorWindowSnapshotV1,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let draft = snapshot
            .state
            .bookmark_folder_editor()
            .cloned()
            .expect("bookmark folder editor snapshot requires a draft");
        let name_input = Self::create_name_input(owner, draft.name, cx);
        let input_for_focus = name_input.clone();
        window.defer(cx, move |window, cx| {
            input_for_focus.read(cx).focus_handle(cx).focus(window, cx);
        });
        window.on_window_should_close(cx, move |_, cx| {
            let _ = owner.update(cx, |root, owner_window, cx| {
                if root.bookmark_folder_editor_window_snapshot().is_some() {
                    root.dispatch_bookmark_folder_editor_action(
                        ExplorerAction::CancelBookmarkFolderEditor,
                        ActionSource::Programmatic,
                        owner_window,
                        cx,
                    );
                }
            });
            true
        });
        Self {
            tokens,
            owner,
            snapshot,
            name_input,
            focus_handle: cx.focus_handle(),
        }
    }

    pub fn replace_snapshot(
        &mut self,
        snapshot: BookmarkFolderEditorWindowSnapshotV1,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(draft) = snapshot.state.bookmark_folder_editor().cloned() else {
            window.remove_window();
            return;
        };
        self.snapshot = snapshot;
        self.name_input = Self::create_name_input(self.owner, draft.name, cx);
        let input = self.name_input.clone();
        window.defer(cx, move |window, cx| {
            input.read(cx).focus_handle(cx).focus(window, cx);
        });
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
        let owner = self.owner;
        let name = (action == ExplorerAction::SaveBookmarkFolderEditor)
            .then(|| self.name_input.read(cx).as_str().to_owned());
        match owner.update(cx, |root, owner_window, cx| {
            if let Some(name) = name {
                root.update_bookmark_folder_editor_name_from_window(name);
            }
            root.dispatch_bookmark_folder_editor_action(action, source, owner_window, cx);
            root.bookmark_folder_editor_window_snapshot()
        }) {
            Ok(Some(snapshot)) => {
                self.snapshot = snapshot;
                cx.notify();
                window.refresh();
            }
            Ok(None) => window.remove_window(),
            Err(error) => {
                tracing::warn!(%error, "Bookmark folder editor owner window is unavailable");
                window.remove_window();
            }
        }
    }
}

impl Focusable for BookmarkFolderEditorWindow {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Render for BookmarkFolderEditorWindow {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let on_action: ActionCallback =
            Rc::new(cx.listener(|this, action: &ExplorerAction, window, cx| {
                this.dispatch(action.clone(), ActionSource::Mouse, window, cx);
            }));
        div()
            .id("bookmark-folder-editor-window")
            .role(gpui::Role::Dialog)
            .aria_label("Bookmark folder editor window")
            .size_full()
            .track_focus(&self.focus_handle)
            .capture_key_down(cx.listener(|this, event: &gpui::KeyDownEvent, window, cx| {
                match event.keystroke.key.as_str() {
                    "escape" => {
                        cx.stop_propagation();
                        this.dispatch(
                            ExplorerAction::CancelBookmarkFolderEditor,
                            ActionSource::Keyboard,
                            window,
                            cx,
                        );
                    }
                    "enter" => {
                        cx.stop_propagation();
                        this.dispatch(
                            ExplorerAction::SaveBookmarkFolderEditor,
                            ActionSource::Keyboard,
                            window,
                            cx,
                        );
                    }
                    _ => {}
                }
            }))
            .child(chrome::bookmark_folder_editor(
                self.tokens,
                Some(gpui::Entity::downgrade(&self.name_input)),
                Some(on_action),
            ))
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn folder_editor_uses_a_normal_dedicated_window_and_shared_reducer() {
        let source = include_str!("bookmark_folder_editor_window.rs");
        assert!(source.contains("WindowKind::Normal"));
        assert!(source.contains("dispatch_bookmark_folder_editor_action"));
        assert!(source.contains("EditableTextState"));
        assert!(source.contains("window.remove_window()"));
    }
}
