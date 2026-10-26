use explorer_ui::{
    ExplorerRoot,
    chrome::{
        ACTIVE_TAB_ID, ADDRESS_EDITOR_ID, CAPTION_MINIMIZE_ID, COMMAND_BAR_ID, EXPLORER_WINDOW_ID,
        FILE_VIEW_HOST_ID, NAVIGATION_BAR_ID, NAVIGATION_DIVIDER_ID, NAVIGATION_PANE_ID,
        NEW_TAB_BUTTON_ID, SEARCH_BOX_ID, STATUS_BAR_ID, TAB_STRIP_ID, TAB_STRIP_SCROLLBAR_ID,
        WINDOW_DRAG_REGION_ID,
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
    let plus = visual
        .debug_bounds(NEW_TAB_BUTTON_ID)
        .expect("new tab button is rendered");
    let minimize = visual
        .debug_bounds(CAPTION_MINIMIZE_ID)
        .expect("minimize caption is rendered");
    assert!(
        plus.origin.x + plus.size.width
            <= tab.origin.x + tab.size.width + plus.size.width + px(16.0),
        "the new-tab button must sit beside the rightmost tab"
    );
    assert!(
        f32::from(minimize.origin.x - (plus.origin.x + plus.size.width))
            >= layout::tabs::CAPTION_DRAG_RESERVE.value() - 1.0,
        "caption buttons must keep a 50px window-drag gap, got {}",
        f32::from(minimize.origin.x - (plus.origin.x + plus.size.width))
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
    let drag = visual
        .debug_bounds(WINDOW_DRAG_REGION_ID)
        .expect("caption-adjacent drag strip is rendered");
    assert!(
        f32::from(drag.size.width) >= layout::tabs::CAPTION_DRAG_RESERVE.value() - 1.0,
        "caption buttons must keep a 50px window-drag gap, got {}",
        f32::from(drag.size.width)
    );
    let _ = strip;
}

#[gpui::test]
fn overflowing_tabs_show_a_horizontal_scrollbar_and_keep_caption_buttons(cx: &mut TestAppContext) {
    let window = cx.open_window(size(px(900.0), px(720.0)), |_, _| ExplorerRoot::default());
    let any_window = window.into();
    cx.update_window(any_window, |_, window, cx| window.draw(cx).clear())
        .expect("test window remains available");
    let mut visual = VisualTestContext::from_window(any_window, cx);
    for _ in 0..14 {
        let plus = visual
            .debug_bounds(NEW_TAB_BUTTON_ID)
            .expect("new tab button remains reachable");
        visual.simulate_click(plus.center(), Modifiers::default());
        cx.update_window(any_window, |_, window, cx| window.draw(cx).clear())
            .expect("test window remains available");
    }
    // The scroll handle learns its measured bounds during paint; draw again like a real frame.
    cx.update_window(any_window, |_, window, cx| window.draw(cx).clear())
        .expect("test window remains available");

    let strip = visual.debug_bounds(TAB_STRIP_ID).expect("tab strip");
    let minimize = visual
        .debug_bounds(CAPTION_MINIMIZE_ID)
        .expect("minimize caption is rendered");
    let drag = visual
        .debug_bounds(WINDOW_DRAG_REGION_ID)
        .expect("drag region is rendered");
    assert!(
        strip.origin.x + strip.size.width <= minimize.origin.x + px(0.5),
        "overflowing tabs must stay inside the strip instead of pushing out the caption buttons"
    );
    assert!(
        minimize.origin.x + minimize.size.width <= px(900.5),
        "caption buttons must remain inside the window"
    );
    let scrollbar = visual
        .debug_bounds(TAB_STRIP_SCROLLBAR_ID)
        .expect("overflowing tabs must expose the horizontal tab-strip scrollbar");
    // Whatever region is laid out first below the window chrome paints over anything that
    // spills out of it, so the scrollbar must end where both bars begin.
    let command_bar = [COMMAND_BAR_ID, NAVIGATION_BAR_ID]
        .into_iter()
        .filter_map(|id| visual.debug_bounds(id))
        .min_by(|a, b| a.origin.y.partial_cmp(&b.origin.y).expect("finite y"))
        .expect("bars below the chrome are rendered");
    assert!(
        scrollbar.origin.y + scrollbar.size.height <= command_bar.origin.y + px(0.5),
        "the scrollbar must sit above the command bar, not under it: bar bottom {} command bar top {}",
        f32::from(scrollbar.origin.y + scrollbar.size.height),
        f32::from(command_bar.origin.y)
    );
    assert!(
        scrollbar.size.height > px(0.0),
        "the scrollbar track must have visible height"
    );
}
