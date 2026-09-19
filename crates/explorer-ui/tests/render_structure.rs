use explorer_ui::{
    ExplorerRoot,
    chrome::{
        ACTIVE_TAB_ID, ADDRESS_EDITOR_ID, COMMAND_BAR_ID, EXPLORER_WINDOW_ID, FILE_VIEW_HOST_ID,
        NAVIGATION_BAR_ID, NAVIGATION_DIVIDER_ID, NAVIGATION_PANE_ID, NEW_TAB_BUTTON_ID,
        SEARCH_BOX_ID, STATUS_BAR_ID, TAB_STRIP_ID,
    },
    layout,
};
use gpui::{AppContext as _, Modifiers, TestAppContext, VisualTestContext, px, size};

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
    let tab = visual
        .debug_bounds(ACTIVE_TAB_ID)
        .expect("active tab is rendered");
    assert!(
        tab.size.width <= px(layout::tabs::PREFERRED_WIDTH.value() + 0.5),
        "a single tab must not stretch past the preferred width, got {}",
        f32::from(tab.size.width)
    );
}

#[gpui::test]
fn crowded_tabs_shrink_below_preferred_width_and_stay_in_the_strip(cx: &mut TestAppContext) {
    let window = cx.open_window(size(px(900.0), px(720.0)), |_, _| ExplorerRoot::default());
    let any_window = window.into();
    cx.update_window(any_window, |_, window, cx| window.draw(cx).clear())
        .expect("test window remains available");
    let mut visual = VisualTestContext::from_window(any_window, cx);
    for _ in 0..5 {
        let plus = visual
            .debug_bounds(NEW_TAB_BUTTON_ID)
            .expect("new tab button remains visible");
        visual.simulate_click(plus.center(), Modifiers::default());
        cx.update_window(any_window, |_, window, cx| window.draw(cx).clear())
            .expect("test window remains available");
    }

    let strip = visual
        .debug_bounds(TAB_STRIP_ID)
        .expect("tab strip is rendered");
    let tab = visual
        .debug_bounds(ACTIVE_TAB_ID)
        .expect("active tab is rendered");
    let plus = visual
        .debug_bounds(NEW_TAB_BUTTON_ID)
        .expect("new tab button stays pinned after shrinking tabs");
    assert!(
        tab.size.width >= px(layout::tabs::MIN_WIDTH.value() - 0.5),
        "crowded tabs must not shrink below the floor, got {}",
        f32::from(tab.size.width)
    );
    assert!(
        tab.size.width <= px(layout::tabs::PREFERRED_WIDTH.value() + 0.5),
        "default tabs must not grow past the preferred width, got {}",
        f32::from(tab.size.width)
    );
    let plus_right = plus.origin.x + plus.size.width;
    assert!(
        plus_right <= strip.origin.x + strip.size.width + plus.size.width + px(8.0),
        "the new-tab button must stay pinned beside the scrolling tabs"
    );
}
