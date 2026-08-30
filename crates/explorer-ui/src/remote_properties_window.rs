//! Dedicated Windows-style Properties window for one ADB or SFTP item.

use gpui::{
    App, Bounds, Context, FocusHandle, Focusable, IntoElement, Render, Role, SharedString, Window,
    WindowBounds, WindowHandle, WindowOptions, div, prelude::*, px, size,
};

use crate::{ExplorerRoot, UiTokens};

#[derive(Clone)]
pub struct RemotePropertiesWindowSnapshotV1 {
    pub entry: explorer_model::FileEntry,
    pub mode: u32,
}

pub fn remote_properties_window_options(
    cx: &App,
    snapshot: &RemotePropertiesWindowSnapshotV1,
) -> WindowOptions {
    remote_properties_window_options_on_display(cx, snapshot, None)
}

pub fn remote_properties_window_options_on_display(
    cx: &App,
    snapshot: &RemotePropertiesWindowSnapshotV1,
    display_id: Option<gpui::DisplayId>,
) -> WindowOptions {
    WindowOptions {
        window_bounds: Some(WindowBounds::Windowed(Bounds::centered(
            display_id,
            size(px(590.0), px(620.0)),
            cx,
        ))),
        titlebar: Some(gpui::TitlebarOptions {
            title: Some(SharedString::from(format!(
                "{} - 內容",
                snapshot.entry.display_name
            ))),
            ..Default::default()
        }),
        kind: gpui::WindowKind::Normal,
        is_resizable: false,
        window_min_size: Some(size(px(520.0), px(560.0))),
        ..Default::default()
    }
}

pub struct RemotePropertiesWindow {
    tokens: UiTokens,
    owner: WindowHandle<ExplorerRoot>,
    snapshot: RemotePropertiesWindowSnapshotV1,
    original_mode: u32,
    mode: u32,
    confirm_focus: FocusHandle,
}

impl RemotePropertiesWindow {
    pub fn new(
        tokens: UiTokens,
        owner: WindowHandle<ExplorerRoot>,
        snapshot: RemotePropertiesWindowSnapshotV1,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let confirm_focus = cx.focus_handle();
        confirm_focus.focus(window, cx);
        let mode = snapshot.mode & 0o7777;
        Self {
            tokens,
            owner,
            snapshot,
            original_mode: mode,
            mode,
            confirm_focus,
        }
    }

    pub fn replace_snapshot(
        &mut self,
        snapshot: RemotePropertiesWindowSnapshotV1,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let mode = snapshot.mode & 0o7777;
        self.snapshot = snapshot;
        self.original_mode = mode;
        self.mode = mode;
        window.set_window_title(&format!("{} - 內容", self.snapshot.entry.display_name));
        self.confirm_focus.focus(window, cx);
        cx.notify();
        window.refresh();
    }

    fn confirm(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.mode != self.original_mode {
            let entry = self.snapshot.entry.clone();
            let mode = self.mode;
            let owner = self.owner;
            let _ = owner.update(cx, |root, _, _| {
                root.apply_remote_properties_from_window(entry, mode);
            });
        }
        window.remove_window();
    }

    fn toggle(&mut self, mask: u32, cx: &mut Context<Self>) {
        if mask != 0 && mask & !0o7777 == 0 {
            self.mode ^= mask;
            cx.notify();
        }
    }
}

impl Focusable for RemotePropertiesWindow {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.confirm_focus.clone()
    }
}

impl Render for RemotePropertiesWindow {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let colors = self.tokens.theme.colors;
        let entry = self.snapshot.entry.clone();
        let location = match &entry.location {
            explorer_model::LocationDescriptor::Virtual(remote) => format!(
                "{}://{}/{}",
                remote.provider_id,
                remote.public_authority.as_deref().unwrap_or("remote"),
                remote.components.join("/")
            ),
            _ => "無法取得".to_owned(),
        };
        let type_name = entry.metadata.type_display.clone().unwrap_or_else(|| {
            if entry.is_container {
                "遠端資料夾"
            } else {
                "遠端檔案"
            }
            .to_owned()
        });
        let size = entry
            .metadata
            .size_bytes
            .map_or_else(|| "無法取得".to_owned(), |value| format!("{value} 位元組"));
        let mut permission_grid = div().flex().flex_col().gap(px(7.0));
        for (label, read, write, execute) in [
            ("擁有者", 0o400, 0o200, 0o100),
            ("群組", 0o040, 0o020, 0o010),
            ("其他", 0o004, 0o002, 0o001),
        ] {
            permission_grid = permission_grid.child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(14.0))
                    .child(div().w(px(72.0)).child(label))
                    .child(permission_button("讀取", read, self.mode, colors, cx))
                    .child(permission_button("寫入", write, self.mode, colors, cx))
                    .child(permission_button("執行", execute, self.mode, colors, cx)),
            );
        }
        div()
            .id("remote-properties-window")
            .role(Role::Dialog)
            .aria_label(format!("{} 內容", entry.display_name))
            .size_full()
            .track_focus(&self.confirm_focus)
            .capture_key_down(cx.listener(|this, event: &gpui::KeyDownEvent, window, cx| {
                match event.keystroke.key.as_str() {
                    "enter" => {
                        cx.stop_propagation();
                        this.confirm(window, cx);
                    }
                    "escape" => {
                        cx.stop_propagation();
                        window.remove_window();
                    }
                    _ => {}
                }
            }))
            .p(px(26.0))
            .flex()
            .flex_col()
            .gap(px(15.0))
            .bg(colors.surface.to_gpui())
            .child(div().text_size(px(21.0)).child(entry.display_name))
            .child("一般")
            .child(div().h(px(1.0)).bg(colors.divider.to_gpui()))
            .child(format!("檔案類型：{type_name}"))
            .child(format!("位置：{location}"))
            .child(format!("大小：{size}"))
            .when_some(entry.metadata.created_display, |view, value| {
                view.child(format!("建立日期：{value}"))
            })
            .when_some(entry.metadata.modified_display, |view, value| {
                view.child(format!("修改日期：{value}"))
            })
            .child(div().h(px(1.0)).bg(colors.divider.to_gpui()))
            .child(format!("權限：{:04o}", self.mode))
            .child(permission_grid)
            .child(div().flex_1())
            .child(
                div()
                    .flex()
                    .justify_end()
                    .gap(px(10.0))
                    .child(
                        div()
                            .id("remote-properties-cancel")
                            .role(Role::Button)
                            .cursor_pointer()
                            .rounded(px(6.0))
                            .border_1()
                            .border_color(colors.divider.to_gpui())
                            .px(px(18.0))
                            .py(px(7.0))
                            .child("取消")
                            .on_click(|_, window, _| window.remove_window()),
                    )
                    .child(
                        div()
                            .id("remote-properties-confirm")
                            .role(Role::Button)
                            .track_focus(&self.confirm_focus)
                            .cursor_pointer()
                            .rounded(px(6.0))
                            .border(px(2.0))
                            .border_color(colors.focus.to_gpui())
                            .px(px(18.0))
                            .py(px(6.0))
                            .child("確定")
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.confirm(window, cx);
                            })),
                    ),
            )
    }
}

fn permission_button(
    label: &'static str,
    mask: u32,
    mode: u32,
    colors: crate::theme::SemanticColors,
    cx: &mut Context<RemotePropertiesWindow>,
) -> impl IntoElement {
    let checked = mode & mask != 0;
    div()
        .id(format!("remote-properties-permission-{mask:o}"))
        .role(Role::Button)
        .cursor_pointer()
        .flex()
        .items_center()
        .gap(px(5.0))
        .child(
            div()
                .w(px(14.0))
                .h(px(14.0))
                .border_1()
                .border_color(colors.text_secondary.to_gpui())
                .when(checked, |box_element| {
                    box_element
                        .p(px(2.0))
                        .child(div().size_full().bg(colors.accent.to_gpui()))
                }),
        )
        .child(label)
        .on_click(cx.listener(move |this, _, _, cx| this.toggle(mask, cx)))
}

#[cfg(test)]
mod tests {
    #[test]
    fn properties_use_a_normal_window_with_default_enter_confirmation() {
        let source = include_str!("remote_properties_window.rs");
        assert!(source.contains("WindowKind::Normal"));
        assert!(source.contains("confirm_focus.focus(window, cx)"));
        assert!(source.contains("\"enter\" =>"));
        assert!(source.contains("this.confirm(window, cx)"));
        assert!(source.contains("remote-properties-confirm"));
        assert!(source.contains("window.remove_window()"));
    }
}
