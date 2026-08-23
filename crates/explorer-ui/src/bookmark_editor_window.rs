//! Dedicated transient bookmark editor window.

use std::rc::Rc;

use gpui::{
    App, Bounds, Context, FocusHandle, Focusable, IntoElement, Render, SharedString, Window,
    WindowBounds, WindowHandle, WindowOptions, div, prelude::*, px, size,
};
use gpui_elements::editable_text::EditableTextState;

use crate::{
    ExplorerRoot, UiTokens,
    actions::{ActionSource, ExplorerAction},
    chrome::{self, ActionCallback},
    state::AppViewState,
};

#[derive(Clone)]
pub struct BookmarkEditorWindowSnapshotV1 {
    pub state: AppViewState,
    pub name_input: gpui::Entity<EditableTextState>,
    pub payload_input: gpui::Entity<EditableTextState>,
}

pub fn bookmark_editor_window_options(cx: &App) -> WindowOptions {
    WindowOptions {
        window_bounds: Some(WindowBounds::Windowed(Bounds::centered(
            None,
            size(px(680.0), px(680.0)),
            cx,
        ))),
        titlebar: Some(gpui::TitlebarOptions {
            title: Some(SharedString::from("編輯書籤")),
            ..Default::default()
        }),
        kind: gpui::WindowKind::Normal,
        is_resizable: true,
        window_min_size: Some(size(px(520.0), px(520.0))),
        ..Default::default()
    }
}

pub struct BookmarkEditorWindow {
    tokens: UiTokens,
    owner: WindowHandle<ExplorerRoot>,
    snapshot: BookmarkEditorWindowSnapshotV1,
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
        focus_handle.focus(window, cx);
        let name = snapshot.name_input.clone();
        window.defer(cx, move |window, cx| {
            name.read(cx).focus_handle(cx).focus(window, cx);
        });
        Self {
            tokens,
            owner,
            snapshot,
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
        match owner.update(cx, |root, owner_window, cx| {
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
                Some(gpui::Entity::downgrade(&self.snapshot.name_input)),
                Some(gpui::Entity::downgrade(&self.snapshot.payload_input)),
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
    }
}
