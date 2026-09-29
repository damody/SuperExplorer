#[test]
fn wrapped_icon_items_use_fixed_cells_and_single_line_labels() {
    let chrome = include_str!("../src/chrome.rs");
    assert!(chrome.contains("let explicit_row_width = file_row_explicit_width("));
    assert!(chrome.contains("element.w(px(width)).min_w(px(width))"));
    assert!(chrome.contains(".when(explicit_row_width.is_none(), Styled::w_full)"));
    assert!(chrome.contains(".when(spatial_metrics.stacked, |element|"));
    let name = chrome
        .rsplit_once(".id(\"file-row-name\")")
        .unwrap()
        .1
        .split(".child(display_name)")
        .next()
        .unwrap();
    assert!(name.contains("STACKED_ICON_LABEL_HEIGHT"));
    assert!(name.contains(".mb(px(label_gap))"));
    assert!(name.contains(".whitespace_nowrap()"));
    assert!(name.contains(".text_ellipsis()"));
    assert!(!name.contains(".line_clamp("));
    assert!(!name.contains("selected"));
}

#[test]
fn renderer_marquee_and_keyboard_share_spatial_grid_metrics() {
    let chrome = include_str!("../src/chrome.rs");
    let state = include_str!("../src/state.rs");
    let root = include_str!("../src/lib.rs");
    assert!(
        chrome.contains(
            "spatial_grid_metrics_with_registry(&view_settings, &column_registry, layout)"
        )
    );
    assert!(chrome.contains("let spatial_layout = spatial_grid_layout("));
    assert!(state.contains("crate::chrome::spatial_grid_metrics(&settings, layout)"));
    assert!(state.contains("crate::chrome::spatial_grid_layout(metrics, viewport_width"));
    assert!(root.contains("chrome::spatial_grid_metrics(settings, layout)"));
    assert!(root.contains("chrome::spatial_grid_columns("));
    assert!(root.contains("chrome::spatial_grid_layout("));
}
