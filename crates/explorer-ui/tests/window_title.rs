use explorer_ui::ExplorerRoot;
use gpui::{AppContext as _, TestAppContext, VisualTestContext, px, size};

#[gpui::test]
fn rendered_root_sets_native_title_from_active_path(cx: &mut TestAppContext) {
    let window = cx.open_window(size(px(1_120.0), px(720.0)), |_, _| ExplorerRoot::default());
    let any_window = window.into();
    cx.update_window(any_window, |_, window, cx| window.draw(cx).clear())
        .expect("test window remains available");

    let mut visual = VisualTestContext::from_window(any_window, cx);
    assert_eq!(visual.window_title().as_deref(), Some(r"C:\"));
}
