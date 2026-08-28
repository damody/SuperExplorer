//! Dedicated transient bookmark editor window.

use std::rc::Rc;

use gpui::{
    App, Bounds, Context, FocusHandle, Focusable, IntoElement, Render, SharedString, Window,
    WindowBounds, WindowHandle, WindowOptions, div, prelude::*, px, size,
};
use gpui_elements::editable_text::{EditableTextState, StringStorage};

use crate::{
    ExplorerRoot, UiTokens,
    actions::{ActionSource, ExplorerAction},
    chrome::{self, ActionCallback},
    state::AppViewState,
};

#[derive(Clone)]
pub struct BookmarkEditorWindowSnapshotV1 {
    pub state: AppViewState,
}

fn bookmark_editor_width(display_width: f32) -> f32 {
    (display_width * 0.8).max(640.0)
}

pub fn bookmark_editor_window_options(cx: &App) -> WindowOptions {
    let width = cx.primary_display().map_or(760.0, |display| {
        bookmark_editor_width(f32::from(display.bounds().size.width))
    });
    WindowOptions {
        window_bounds: Some(WindowBounds::Windowed(Bounds::centered(
            None,
            size(px(width), px(560.0)),
            cx,
        ))),
        titlebar: Some(gpui::TitlebarOptions {
            title: Some(SharedString::from("編輯書籤")),
            ..Default::default()
        }),
        kind: gpui::WindowKind::Normal,
        is_resizable: true,
        window_min_size: Some(size(px(640.0), px(460.0))),
        ..Default::default()
    }
}

pub struct BookmarkEditorWindow {
    tokens: UiTokens,
    owner: WindowHandle<ExplorerRoot>,
    snapshot: BookmarkEditorWindowSnapshotV1,
    name_input: gpui::Entity<EditableTextState>,
    payload_input: gpui::Entity<EditableTextState>,
    focus_handle: FocusHandle,
    was_active: bool,
}

const fn should_cancel_on_activation_loss(was_active: bool, active: bool) -> bool {
    was_active && !active
}

impl BookmarkEditorWindow {
    pub fn new(
        tokens: UiTokens,
        owner: WindowHandle<ExplorerRoot>,
        snapshot: BookmarkEditorWindowSnapshotV1,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let focus_handle = cx.focus_handle();
        let editor = snapshot
            .state
            .bookmark_editor()
            .cloned()
            .expect("bookmark editor snapshot requires a draft");
        let payload = editor.target.editable_payload();
        let name_input = cx.new(|cx| EditableTextState::new(StringStorage::from(editor.name), cx));
        let payload_input = cx.new(|cx| EditableTextState::new(StringStorage::from(payload), cx));
        let close_owner = owner;
        window.on_window_should_close(cx, move |_, cx| {
            let _ = close_owner.update(cx, |root, owner_window, cx| {
                if root.bookmark_editor_window_snapshot().is_some() {
                    root.dispatch_bookmark_editor_action(
                        ExplorerAction::CancelBookmarkEditor,
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
            payload_input,
            focus_handle,
            was_active: false,
        }
    }

    fn dispatch(
        &mut self,
        action: ExplorerAction,
        source: ActionSource,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let owner = self.owner;
        let input_values = (action == ExplorerAction::SaveBookmarkEditor).then(|| {
            (
                self.name_input.read(cx).as_str().to_owned(),
                Some(self.payload_input.read(cx).as_str().to_owned()),
            )
        });
        match owner.update(cx, |root, owner_window, cx| {
            if let Some((name, payload)) = input_values {
                root.update_bookmark_editor_name_from_window(name);
                if let Some(payload) = payload {
                    root.update_bookmark_editor_payload_from_window(payload);
                }
            }
            root.dispatch_bookmark_editor_action(action, source, owner_window, cx);
            root.bookmark_editor_window_snapshot()
        }) {
            Ok(Some(snapshot)) => {
                self.snapshot = snapshot;
                cx.notify();
                window.refresh();
            }
            Ok(None) => window.remove_window(),
            Err(error) => {
                tracing::warn!(%error, "Bookmark editor owner window is unavailable");
                window.remove_window();
            }
        }
    }
}

impl Focusable for BookmarkEditorWindow {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Render for BookmarkEditorWindow {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let active = window.is_window_active();
        if active {
            self.was_active = true;
        } else if should_cancel_on_activation_loss(self.was_active, active) {
            self.dispatch(
                ExplorerAction::CancelBookmarkEditor,
                ActionSource::Programmatic,
                window,
                cx,
            );
        }
        let on_action: ActionCallback =
            Rc::new(cx.listener(|this, action: &ExplorerAction, window, cx| {
                this.dispatch(action.clone(), ActionSource::Mouse, window, cx);
            }));
        div()
            .id("bookmark-editor-window")
            .role(gpui::Role::Dialog)
            .aria_label("Bookmark editor window")
            .size_full()
            .track_focus(&self.focus_handle)
            .capture_key_down(cx.listener(|this, event: &gpui::KeyDownEvent, window, cx| {
                if event.keystroke.key == "escape" {
                    cx.stop_propagation();
                    this.dispatch(
                        ExplorerAction::CancelBookmarkEditor,
                        ActionSource::Keyboard,
                        window,
                        cx,
                    );
                }
            }))
            .child(chrome::bookmark_editor(
                self.tokens,
                &self.snapshot.state,
                Some(gpui::Entity::downgrade(&self.name_input)),
                Some(gpui::Entity::downgrade(&self.payload_input)),
                Some(on_action),
            ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn editor_cancels_only_after_an_active_window_loses_focus() {
        assert!(!should_cancel_on_activation_loss(false, false));
        assert!(!should_cancel_on_activation_loss(false, true));
        assert!(!should_cancel_on_activation_loss(true, true));
        assert!(should_cancel_on_activation_loss(true, false));
    }

    #[test]
    fn dedicated_window_contract_uses_normal_window_and_shared_editor_content() {
        let source = include_str!("bookmark_editor_window.rs");
        assert!(source.contains("WindowKind::Normal"));
        assert!(source.contains("chrome::bookmark_editor("));
        assert!(source.contains("CancelBookmarkEditor"));
        assert!(source.contains("window.remove_window()"));
        assert!(source.contains("editor.target.editable_payload()"));
        assert!(source.contains("bookmark_editor_width"));
        assert_eq!(bookmark_editor_width(1920.0), 1536.0);
        assert_eq!(bookmark_editor_width(800.0), 640.0);
    }
}
