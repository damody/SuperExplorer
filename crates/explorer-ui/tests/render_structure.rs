use explorer_ui::{
    ExplorerRoot,
    chrome::{
        ACTIVE_TAB_ID, ADDRESS_EDITOR_ID, COMMAND_BAR_ID, EXPLORER_WINDOW_ID, FILE_VIEW_HOST_ID,
        NAVIGATION_BAR_ID, NAVIGATION_DIVIDER_ID, NAVIGATION_PANE_ID, NEW_TAB_BUTTON_ID,
        SEARCH_BOX_ID, STATUS_BAR_ID,
    },
};
use gpui::{AppContext as _, TestAppContext, VisualTestContext, px, size};

#[gpui::test]
fn initial_render_contains_every_m1_region(cx: &mut TestAppContext) {
    let window = cx.open_window(size(px(1_120.0), px(720.0)), |_, _| ExplorerRoot::default());
    let any_window = window.into();
    cx.update_window(any_window, |_, window, cx| window.draw(cx).clear())
        .expect("test window remains available");

    let mut visual = VisualTestContext::from_window(any_window, cx);
    for selector in [
        EXPLORER_WINDOW_ID,
        COMMAND_BAR_ID,
        NAVIGATION_BAR_ID,
        ADDRESS_EDITOR_ID,
        SEARCH_BOX_ID,
        NAVIGATION_PANE_ID,
        NAVIGATION_DIVIDER_ID,
        FILE_VIEW_HOST_ID,
        STATUS_BAR_ID,
        ACTIVE_TAB_ID,
        NEW_TAB_BUTTON_ID,
    ] {
        let bounds = visual
            .debug_bounds(selector)
            .unwrap_or_else(|| panic!("rendered selector is missing: {selector}"));
        assert!(bounds.size.width > px(0.0), "zero width: {selector}");
        assert!(bounds.size.height > px(0.0), "zero height: {selector}");
    }
}
