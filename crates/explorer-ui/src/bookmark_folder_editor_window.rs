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

impl BookmarkFolderEditorWindowSnapshotV1 {
    pub fn is_new_folder(&self) -> bool {
        self.state
            .bookmark_folder_editor()
            .is_some_and(|editor| editor.id.is_none())
    }
}

pub fn bookmark_folder_editor_window_options(
    cx: &App,
    title: impl Into<SharedString>,
) -> WindowOptions {
    let _ = title;
    WindowOptions {
        window_bounds: Some(WindowBounds::Windowed(Bounds::centered(
            None,
            size(px(420.0), px(248.0)),
            cx,
        ))),
        titlebar: None,
        kind: gpui::WindowKind::PopUp,
        focus: true,
        show: true,
        is_resizable: false,
        is_minimizable: false,
        window_min_size: Some(size(px(380.0), px(220.0))),
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
        let draft = snapshot.state.bookmark_folder_editor().cloned();
        if draft.is_none() {
            tracing::error!("bookmark folder editor window opened without a draft");
            window.remove_window();
        }
        let draft = draft.unwrap_or(crate::state::BookmarkFolderEditorDraft {
            id: None,
            parent_id: None,
            name: String::new(),
            token: 0,
        });
        let token = draft.token;
        let name_input = Self::create_name_input(owner, draft.name, cx);
        let input_for_focus = name_input.clone();
        window.defer(cx, move |window, cx| {
            input_for_focus.read(cx).focus_handle(cx).focus(window, cx);
        });
        window.on_window_should_close(cx, move |_, cx| {
            let _ = owner.update(cx, |root, owner_window, cx| {
                if root
                    .bookmark_folder_editor_window_snapshot()
                    .is_some_and(|snapshot| {
                        snapshot
                            .state
                            .bookmark_folder_editor()
                            .is_some_and(|editor| editor.token == token)
                    })
                {
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
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let catalog = self.snapshot.state.catalog();
        let is_new = self
            .snapshot
            .state
            .bookmark_folder_editor()
            .is_some_and(|editor| editor.id.is_none());
        let title = catalog.t(if is_new {
            "dialog-new-bookmark-toolbar-folder"
        } else {
            "dialog-rename-bookmark-folder"
        });
        window.set_window_title(&title);
        let on_action: ActionCallback =
            Rc::new(cx.listener(|this, action: &ExplorerAction, window, cx| {
                this.dispatch(action.clone(), ActionSource::Mouse, window, cx);
            }));
        div()
            .id("bookmark-folder-editor-window")
            .role(gpui::Role::Dialog)
            .aria_label(catalog.t("a11y-bookmark-folder-editor"))
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
                catalog,
                Some(gpui::Entity::downgrade(&self.name_input)),
                is_new,
                Some(on_action),
            ))
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn folder_editor_uses_a_normal_dedicated_window_and_shared_reducer() {
        let source = include_str!("bookmark_folder_editor_window.rs")
            .split("#[cfg(test)]")
            .next()
            .expect("production source");
        assert!(source.contains("WindowKind::PopUp"));
        assert!(source.contains("titlebar: None"));
        assert!(source.contains("dispatch_bookmark_folder_editor_action"));
        assert!(source.contains("EditableTextState"));
        assert!(source.contains("window.remove_window()"));
        assert!(
            source.contains("tracing::error!"),
            "missing drafts must log instead of panicking"
        );
        assert!(
            !source.contains(".expect("),
            "folder editor must not panic on a missing draft"
        );
        assert!(
            source.contains("editor.token == token"),
            "closing one editor window must not cancel a newer editor session"
        );
        assert!(
            source.contains("size(px(420.0), px(248.0))"),
            "folder editor must leave room for unclipped save/cancel buttons"
        );
    }
}
