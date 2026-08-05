//! Dedicated, modeless Folder Options window.

use std::rc::Rc;

use gpui::{
    App, Bounds, Context, FocusHandle, Focusable, IntoElement, KeyDownEvent, MouseButton,
    MouseDownEvent, MouseMoveEvent, Pixels, Render, Role, ScrollHandle, SharedString, Window,
    WindowBounds, WindowHandle, WindowOptions, div, point, prelude::*, px, size,
};

use crate::{
    ExplorerRoot, UiTokens,
    actions::{ActionSource, ExplorerAction, FolderOptionsPage},
    chrome::{self, ActionCallback},
    state::{ExtensionOptionV1, FolderOptionsDraft},
};

const INITIAL_WIDTH: f32 = 960.0;
const INITIAL_HEIGHT: f32 = 760.0;
const MINIMUM_WIDTH: f32 = 680.0;
const MINIMUM_HEIGHT: f32 = 480.0;

/// Modeless window options. `WindowKind::Normal` is intentional: GPUI's
/// Windows `Dialog` kind disables its owner and would make Explorer modal.
pub fn folder_options_window_options(cx: &App) -> WindowOptions {
    WindowOptions {
        window_bounds: Some(WindowBounds::Windowed(Bounds::centered(
            None,
            size(px(INITIAL_WIDTH), px(INITIAL_HEIGHT)),
            cx,
        ))),
        titlebar: Some(gpui::TitlebarOptions {
            title: Some(SharedString::from("資料夾選項")),
            ..Default::default()
        }),
        kind: gpui::WindowKind::Normal,
        is_resizable: true,
        window_min_size: Some(size(px(MINIMUM_WIDTH), px(MINIMUM_HEIGHT))),
        ..Default::default()
    }
}

/// Immutable handoff used while the owner window is still dispatching the
/// command that creates this native window. It avoids a nested owner-window
/// read during GPUI's immediate first render.
#[derive(Clone)]
pub struct FolderOptionsWindowSnapshotV1 {
    pub draft: FolderOptionsDraft,
    pub extensions: Vec<ExtensionOptionV1>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct ScrollbarDragV1 {
    page: FolderOptionsPage,
    grab_offset_y: f32,
}

/// Root entity for the independent Folder Options native window.
pub struct FolderOptionsWindow {
    tokens: UiTokens,
    owner: WindowHandle<ExplorerRoot>,
    focus_handle: FocusHandle,
    general_scroll: ScrollHandle,
    view_scroll: ScrollHandle,
    extensions_scroll: ScrollHandle,
    scrollbar_drag: Option<ScrollbarDragV1>,
    snapshot: FolderOptionsWindowSnapshotV1,
}

impl FolderOptionsWindow {
    pub fn new(
        tokens: UiTokens,
        owner: WindowHandle<ExplorerRoot>,
        snapshot: FolderOptionsWindowSnapshotV1,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let focus_handle = cx.focus_handle();
        focus_handle.focus(window, cx);
        Self {
            tokens,
            owner,
            focus_handle,
            general_scroll: ScrollHandle::new(),
            view_scroll: ScrollHandle::new(),
            extensions_scroll: ScrollHandle::new(),
            scrollbar_drag: None,
            snapshot,
        }
    }

    fn scroll_for_page(&self, page: FolderOptionsPage) -> &ScrollHandle {
        match page {
            FolderOptionsPage::General => &self.general_scroll,
            FolderOptionsPage::View => &self.view_scroll,
            FolderOptionsPage::Extensions => &self.extensions_scroll,
        }
    }

    fn stop_drag(&mut self) {
        self.scrollbar_drag = None;
    }

    fn update_drag(&mut self, pointer_y: Pixels) -> bool {
        let Some(drag) = self.scrollbar_drag else {
            return false;
        };
        let handle = self.scroll_for_page(drag.page);
        let bounds = handle.bounds();
        let viewport = f32::from(bounds.size.height).max(0.0);
        let maximum = f32::from(handle.max_offset().y).max(0.0);
        let pointer_local_y = f32::from(pointer_y - bounds.top());
        let Some(target) = crate::interaction::scrollbar_target_offset(
            viewport,
            maximum,
            self.tokens.layout.minimum_hit_target.value(),
            pointer_local_y,
            drag.grab_offset_y,
        ) else {
            return false;
        };
        let offset = handle.offset();
        handle.set_offset(point(offset.x, px(-target)));
        true
    }

    fn keyboard_scroll(&self, page: FolderOptionsPage, key: &str) -> bool {
        let handle = self.scroll_for_page(page);
        let viewport = f32::from(handle.bounds().size.height).max(0.0);
        let maximum = f32::from(handle.max_offset().y).max(0.0);
        if viewport <= 0.0 {
            return false;
        }
        let current = (-f32::from(handle.offset().y)).clamp(0.0, maximum);
        let target = match key {
            "pageup" => current - viewport,
            "pagedown" => current + viewport,
            "home" => 0.0,
            "end" => maximum,
            _ => return false,
        }
        .clamp(0.0, maximum);
        let offset = handle.offset();
        handle.set_offset(point(offset.x, px(-target)));
        true
    }

    fn close_with_action(
        &mut self,
        action: ExplorerAction,
        source: ActionSource,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.stop_drag();
        let owner = self.owner;
        if let Err(error) = owner.update(cx, |root, owner_window, cx| {
            root.dispatch_folder_options_action(action, source, owner_window, cx);
        }) {
            tracing::warn!(%error, "Folder Options owner window is unavailable");
        }
        window.remove_window();
    }

    fn scrollbar(
        &self,
        page: FolderOptionsPage,
        handle: ScrollHandle,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let colors = self.tokens.theme.colors;
        let bounds = handle.bounds();
        let viewport = f32::from(bounds.size.height).max(0.0);
        let maximum = f32::from(handle.max_offset().y).max(0.0);
        let current = (-f32::from(handle.offset().y)).clamp(0.0, maximum);
        let minimum_thumb = self.tokens.layout.minimum_hit_target.value();
        let track_width = self.tokens.layout.content_spacing.value() * 1.5;
        let thumb_width = (track_width - self.tokens.layout.focus_stroke.value() * 2.0).max(8.0);
        let thumb_height =
            crate::interaction::scrollbar_thumb_height(viewport, maximum, minimum_thumb)
                .unwrap_or(viewport.max(1.0));
        let thumb_top = if maximum > 0.0 {
            current / maximum * (viewport - thumb_height).max(0.0)
        } else {
            0.0
        };
        let header_height = self.tokens.layout.address_bar_height.value()
            + self.tokens.layout.title_tab_height.value();
        let footer_height = crate::layout::folder_options::FOOTER_HEIGHT.value();
        let click_handle = handle.clone();
        div()
            .id("folder-options-scrollbar")
            .debug_selector(|| "folder-options-scrollbar".to_owned())
            .role(Role::ScrollBar)
            .aria_label("資料夾選項垂直捲動列")
            .aria_numeric_value(f64::from(current))
            .aria_min_numeric_value(0.0)
            .aria_max_numeric_value(f64::from(maximum))
            .absolute()
            .top(px(header_height))
            .right_0()
            .bottom(px(footer_height))
            .w(px(track_width))
            .bg(colors.surface.to_gpui())
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, event: &MouseDownEvent, window, cx| {
                    let bounds = click_handle.bounds();
                    let viewport = f32::from(bounds.size.height).max(0.0);
                    let maximum = f32::from(click_handle.max_offset().y).max(0.0);
                    if viewport <= 0.0 {
                        cx.stop_propagation();
                        return;
                    }
                    let current = (-f32::from(click_handle.offset().y)).clamp(0.0, maximum);
                    let thumb_height = crate::interaction::scrollbar_thumb_height(
                        viewport,
                        maximum,
                        minimum_thumb,
                    )
                    .unwrap_or(viewport);
                    let thumb_top = if maximum > 0.0 {
                        current / maximum * (viewport - thumb_height).max(0.0)
                    } else {
                        0.0
                    };
                    let pointer = f32::from(event.position.y - bounds.top());
                    if maximum > 0.0 && pointer >= thumb_top && pointer <= thumb_top + thumb_height
                    {
                        this.scrollbar_drag = Some(ScrollbarDragV1 {
                            page,
                            grab_offset_y: pointer - thumb_top,
                        });
                    } else if maximum > 0.0 {
                        let target = if pointer < thumb_top {
                            current - viewport
                        } else {
                            current + viewport
                        }
                        .clamp(0.0, maximum);
                        let offset = click_handle.offset();
                        click_handle.set_offset(point(offset.x, px(-target)));
                    }
                    cx.stop_propagation();
                    cx.notify();
                    window.refresh();
                }),
            )
            .child(
                div()
                    .absolute()
                    .top(px(thumb_top))
                    .right(px((track_width - thumb_width) / 2.0))
                    .w(px(thumb_width))
                    .h(px(thumb_height))
                    .rounded(px(self.tokens.layout.corner_radius.value()))
                    .bg(if maximum > 0.0 {
                        colors.text_disabled.to_gpui()
                    } else {
                        colors.divider.to_gpui()
                    })
                    .when(maximum > 0.0, |thumb| {
                        thumb.hover(|style| style.bg(colors.text_secondary.to_gpui()))
                    }),
            )
    }
}

impl Focusable for FolderOptionsWindow {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Render for FolderOptionsWindow {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if !window.is_window_active() {
            self.stop_drag();
        }
        let draft = self.snapshot.draft.clone();
        let extensions = self.snapshot.extensions.clone();
        let page = draft.page;
        let scroll = self.scroll_for_page(page).clone();
        let scrollbar = self.scrollbar(page, scroll.clone(), cx).into_any_element();
        let owner = self.owner;
        let on_action: ActionCallback = Rc::new(cx.listener(
            move |this, action: &ExplorerAction, window, cx| {
                let close = matches!(
                    action,
                    ExplorerAction::CloseFolderOptions | ExplorerAction::ConfirmFolderOptions
                );
                match owner.update(cx, |root, owner_window, cx| {
                    root.dispatch_folder_options_action(
                        action.clone(),
                        ActionSource::Mouse,
                        owner_window,
                        cx,
                    );
                    root.state
                        .folder_options()
                        .map(|draft| FolderOptionsWindowSnapshotV1 {
                            draft,
                            extensions: root.state.extensions().to_vec(),
                        })
                }) {
                    Ok(Some(snapshot)) => this.snapshot = snapshot,
                    Ok(None) if !close => {
                        tracing::warn!(
                            "Folder Options draft disappeared during a non-closing action"
                        );
                    }
                    Ok(None) => {}
                    Err(error) => {
                        tracing::warn!(%error, "Folder Options action owner is unavailable");
                    }
                }
                this.stop_drag();
                if close {
                    window.remove_window();
                } else {
                    cx.notify();
                    window.refresh();
                }
            },
        ));

        div()
            .id("folder-options-window")
            .debug_selector(|| "folder-options-window".to_owned())
            .role(Role::Dialog)
            .aria_label("資料夾選項")
            .size_full()
            .relative()
            .track_focus(&self.focus_handle)
            .on_mouse_move(cx.listener(|this, event: &MouseMoveEvent, window, cx| {
                if this.update_drag(event.position.y) {
                    cx.stop_propagation();
                    cx.notify();
                    window.refresh();
                }
            }))
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(|this, _, _, cx| {
                    if this.scrollbar_drag.take().is_some() {
                        cx.stop_propagation();
                        cx.notify();
                    }
                }),
            )
            .on_mouse_up_out(
                MouseButton::Left,
                cx.listener(|this, _, _, cx| {
                    if this.scrollbar_drag.take().is_some() {
                        cx.stop_propagation();
                        cx.notify();
                    }
                }),
            )
            .on_key_down(cx.listener(move |this, event: &KeyDownEvent, window, cx| {
                if event.keystroke.key == "escape" {
                    cx.stop_propagation();
                    this.close_with_action(
                        ExplorerAction::CloseFolderOptions,
                        ActionSource::Keyboard,
                        window,
                        cx,
                    );
                } else if this.keyboard_scroll(page, event.keystroke.key.as_str()) {
                    cx.stop_propagation();
                    cx.notify();
                    window.refresh();
                }
            }))
            .child(chrome::folder_options_window_content(
                self.tokens,
                draft,
                extensions,
                scroll,
                scrollbar,
                Some(on_action),
            ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn window_options_are_modeless_resizable_and_bounded() {
        let cx = gpui::TestAppContext::single();
        let app = cx.app.borrow();
        let options = folder_options_window_options(&app);
        assert_eq!(options.kind, gpui::WindowKind::Normal);
        assert!(options.is_resizable);
        assert_eq!(options.window_min_size, Some(size(px(680.0), px(480.0))));
    }
}
