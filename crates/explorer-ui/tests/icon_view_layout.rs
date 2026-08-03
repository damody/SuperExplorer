#[test]
fn wrapped_icon_items_use_fixed_cells_and_mode_specific_label_flow() {
    let chrome = include_str!("../src/chrome.rs");
    assert!(chrome.contains("element.w(px(item_width)).min_w(px(item_width))"));
    assert!(chrome.contains(".when(!wrapped_view, Styled::w_full)"));
    assert!(chrome.contains(".when(spatial_metrics.stacked, |element|"));
    assert!(chrome.contains(".line_clamp(stacked_icon_label_lines(selected))"));
    assert!(chrome.contains(".whitespace_normal()"));
    assert!(chrome.contains(".text_ellipsis()"));
    assert!(chrome.contains("name.whitespace_nowrap().text_ellipsis()"));
}

#[test]
fn renderer_marquee_and_keyboard_share_spatial_grid_metrics() {
    let chrome = include_str!("../src/chrome.rs");
    let state = include_str!("../src/state.rs");
    let root = include_str!("../src/lib.rs");
    assert!(chrome.contains("let spatial_metrics = spatial_grid_metrics(&view_settings, layout)"));
    assert!(chrome.contains("let spatial_layout = spatial_grid_layout("));
    assert!(state.contains("crate::chrome::spatial_grid_metrics(&settings, layout)"));
    assert!(state.contains("crate::chrome::spatial_grid_layout(metrics, viewport_width"));
    assert!(root.contains("chrome::spatial_grid_metrics(settings, layout)"));
    assert!(root.contains("chrome::spatial_grid_columns("));
    assert!(root.contains("chrome::spatial_grid_layout("));
}
