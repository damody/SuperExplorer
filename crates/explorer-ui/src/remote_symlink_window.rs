//! Dedicated editor for creating one Linux symbolic link on ADB or SFTP.

use gpui::{
    App, Bounds, Context, FocusHandle, Focusable, IntoElement, Render, Role, SharedString, Window,
    WindowBounds, WindowHandle, WindowOptions, div, prelude::*, px, size,
};
use gpui_elements::editable_text::{EditableTextState, StringStorage, text_input};

use crate::{ExplorerRoot, UiTokens};

#[derive(Clone, Debug)]
pub struct RemoteSymlinkWindowSnapshotV1 {
    pub session_id: u64,
    pub parent: explorer_model::LocationDescriptor,
    pub name: String,
    pub target: String,
}

#[derive(Clone, Debug)]
pub enum RemoteSymlinkWindowUpdateV1 {
    Open(RemoteSymlinkWindowSnapshotV1),
    Failed { session_id: u64, message: String },
    Close { session_id: u64 },
}

pub fn remote_symlink_window_options(cx: &App) -> WindowOptions {
    remote_symlink_window_options_on_display(cx, None)
}

pub fn remote_symlink_window_options_on_display(
    cx: &App,
    display_id: Option<gpui::DisplayId>,
) -> WindowOptions {
    let width = display_id
        .and_then(|id| cx.find_display(id))
        .or_else(|| cx.primary_display())
        .map_or(760.0, |display| {
            (f32::from(display.bounds().size.width) * 0.8).max(640.0)
        });
    WindowOptions {
        window_bounds: Some(WindowBounds::Windowed(Bounds::centered(
            display_id,
            size(px(width), px(390.0)),
            cx,
        ))),
        titlebar: Some(gpui::TitlebarOptions {
            title: Some(SharedString::from("新增捷徑")),
            ..Default::default()
        }),
        kind: gpui::WindowKind::Normal,
        is_resizable: false,
        window_min_size: Some(size(px(640.0), px(350.0))),
        ..Default::default()
    }
}

pub fn validate_remote_symlink_input(name: &str, target: &str) -> Result<(), &'static str> {
    if name.trim().is_empty()
        || matches!(name, "." | "..")
        || name.contains(['/', '\\', '\0', '\r', '\n'])
    {
        return Err("捷徑名稱必須是目前資料夾內的一個有效名稱。");
    }
    if target.is_empty() || target.contains(['\0', '\r', '\n']) {
        return Err("請輸入有效的目標路徑。");
    }
    Ok(())
}

pub struct RemoteSymlinkWindow {
    tokens: UiTokens,
    owner: WindowHandle<ExplorerRoot>,
    snapshot: RemoteSymlinkWindowSnapshotV1,
    name_input: gpui::Entity<EditableTextState>,
    target_input: gpui::Entity<EditableTextState>,
    focus_handle: FocusHandle,
    busy: bool,
    error: Option<String>,
}

impl RemoteSymlinkWindow {
    pub fn new(
        tokens: UiTokens,
        owner: WindowHandle<ExplorerRoot>,
        snapshot: RemoteSymlinkWindowSnapshotV1,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let name_input =
            cx.new(|cx| EditableTextState::new(StringStorage::from(snapshot.name.clone()), cx));
        let target_input =
            cx.new(|cx| EditableTextState::new(StringStorage::from(snapshot.target.clone()), cx));
        name_input.update(cx, EditableTextState::select_document);
        let name_for_focus = name_input.clone();
        window.defer(cx, move |window, cx| {
            name_for_focus.read(cx).focus_handle(cx).focus(window, cx);
        });
        let close_owner = owner;
        let close_session_id = snapshot.session_id;
        window.on_window_should_close(cx, move |_, cx| {
            let _ = close_owner.update(cx, move |root, _, _| {
                root.cancel_remote_symlink_session(close_session_id);
            });
            true
        });
        Self {
            tokens,
            owner,
            snapshot,
            name_input,
            target_input,
            focus_handle: cx.focus_handle(),
            busy: false,
            error: None,
        }
    }

    pub fn replace_snapshot(
        &mut self,
        snapshot: RemoteSymlinkWindowSnapshotV1,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.snapshot = snapshot.clone();
        self.name_input =
            cx.new(|cx| EditableTextState::new(StringStorage::from(snapshot.name.clone()), cx));
        self.target_input =
            cx.new(|cx| EditableTextState::new(StringStorage::from(snapshot.target), cx));
        self.name_input
            .update(cx, EditableTextState::select_document);
        self.busy = false;
        self.error = None;
        let name_for_focus = self.name_input.clone();
        window.defer(cx, move |window, cx| {
            name_for_focus.read(cx).focus_handle(cx).focus(window, cx);
        });
        cx.notify();
    }

    pub fn apply_failure(&mut self, session_id: u64, message: String, cx: &mut Context<Self>) {
        if self.snapshot.session_id == session_id {
            self.busy = false;
            self.error = Some(message);
            cx.notify();
        }
    }

    pub const fn session_id(&self) -> u64 {
        self.snapshot.session_id
    }

    fn submit(&mut self, cx: &mut Context<Self>) {
        if self.busy {
            return;
        }
        let name = self.name_input.read(cx).as_str().to_owned();
        let target = self.target_input.read(cx).as_str().to_owned();
        if let Err(error) = validate_remote_symlink_input(&name, &target) {
            self.error = Some(error.to_owned());
            cx.notify();
            return;
        }
        let owner = self.owner;
        match owner.update(cx, |root, _, _| {
            root.submit_remote_symlink_from_window(
                self.snapshot.session_id,
                self.snapshot.parent.clone(),
                name,
                target,
            )
        }) {
            Ok(Ok(())) => {
                self.busy = true;
                self.error = None;
            }
            Ok(Err(error)) => self.error = Some(error),
            Err(_) => self.error = Some("主視窗已關閉，無法建立捷徑。".to_owned()),
        }
        cx.notify();
    }

    fn cancel(&mut self, cx: &mut Context<Self>) {
        let session_id = self.snapshot.session_id;
        let _ = self.owner.update(cx, move |root, _, _| {
            root.cancel_remote_symlink_session(session_id);
        });
    }
}

impl Focusable for RemoteSymlinkWindow {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Render for RemoteSymlinkWindow {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let colors = self.tokens.theme.colors;
        let submit = cx.listener(|this, _, _, cx| this.submit(cx));
        let cancel = cx.listener(|this, _, window, cx| {
            this.cancel(cx);
            window.remove_window();
        });
        div()
            .id("remote-symlink-window")
            .role(Role::Dialog)
            .aria_label("新增遠端捷徑")
            .track_focus(&self.focus_handle)
            .size_full()
            .p(px(26.0))
            .flex()
            .flex_col()
            .gap(px(12.0))
            .bg(colors.surface.to_gpui())
            .child(div().text_size(px(22.0)).child("新增捷徑"))
            .child("捷徑名稱")
            .child(
                text_input("remote-symlink-name-input")
                    .aria_label("捷徑名稱")
                    .state(gpui::Entity::downgrade(&self.name_input))
                    .multiline(false)
                    .caret_blink_interval_500ms()
                    .w_full()
                    .h(px(38.0))
                    .px(px(9.0))
                    .bg(colors.control_fill.to_gpui())
                    .border_1()
                    .border_color(colors.focus.to_gpui()),
            )
            .child("目標路徑")
            .child(
                text_input("remote-symlink-target-input")
                    .aria_label("目標路徑")
                    .state(gpui::Entity::downgrade(&self.target_input))
                    .multiline(false)
                    .caret_blink_interval_500ms()
                    .w_full()
                    .h(px(38.0))
                    .px(px(9.0))
                    .bg(colors.control_fill.to_gpui())
                    .border_1()
                    .border_color(colors.focus.to_gpui()),
            )
            .when_some(self.error.clone(), |view, error| {
                view.child(
                    div()
                        .id("remote-symlink-error")
                        .aria_label(error.clone())
                        .text_color(colors.danger.to_gpui())
                        .child(error),
                )
            })
            .child(div().flex_1())
            .child(
                div()
                    .flex()
                    .justify_end()
                    .gap(px(10.0))
                    .child(
                        div()
                            .id("remote-symlink-cancel")
                            .role(Role::Button)
                            .aria_label("取消新增捷徑")
                            .cursor_pointer()
                            .px(px(18.0))
                            .py(px(8.0))
                            .child("取消")
                            .on_click(cancel),
                    )
                    .child(
                        div()
                            .id("remote-symlink-create")
                            .role(Role::Button)
                            .aria_label("建立遠端捷徑")
                            .px(px(18.0))
                            .py(px(8.0))
                            .bg(colors.accent.to_gpui())
                            .text_color(colors.text_primary.to_gpui())
                            .when(!self.busy, |button| {
                                button.cursor_pointer().on_click(submit)
                            })
                            .child(if self.busy { "建立中…" } else { "建立" }),
                    ),
            )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validation_rejects_escape_names_and_keeps_dangling_targets() {
        for name in ["", " ", ".", "..", "a/b", "a\\b", "bad\0name"] {
            assert!(validate_remote_symlink_input(name, "missing").is_err());
        }
        for target in ["missing", "../missing target", "/absolute/missing"] {
            assert!(validate_remote_symlink_input("link", target).is_ok());
        }
    }
}
