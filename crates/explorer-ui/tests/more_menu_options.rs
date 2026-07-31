#[test]
fn more_menu_matches_explorer_order_and_button_relative_anchor() {
    let chrome = include_str!("../src/chrome.rs");
    let ids = [
        "more-undo",
        "more-compress-zip",
        "more-add-favorite",
        "more-copy-path",
        "more-select-all",
        "more-select-none",
        "more-invert-selection",
        "more-properties",
        "more-options",
    ];
    let mut previous = 0;
    for id in ids {
        let position = chrome
            .find(id)
            .unwrap_or_else(|| panic!("missing More command: {id}"));
        assert!(position > previous, "More command order changed at {id}");
        previous = position;
    }
    assert!(chrome.contains(".top(px(tokens.layout.minimum_hit_target.value()))"));
    assert!(chrome.contains(".right_0()"));
    assert!(chrome.contains(".with_priority(140)"));
}

#[test]
fn labeled_other_and_extensions_controls_keep_order_and_popup_contracts() {
    let chrome = include_str!("../src/chrome.rs");
    let view = chrome.find("command-view").expect("View command");
    let other = chrome
        .find("command-more-menu")
        .expect("labeled Other command");
    let extensions = chrome
        .find("command-extensions-menu")
        .expect("Extensions command");
    assert!(view < other && other < extensions);
    for contract in [
        "其它",
        "擴充功能",
        "command-extensions-popup",
        "extensions-refresh-tortoisegit",
        "更新 TortoiseGit 狀態",
        "沒有可用的擴充功能",
    ] {
        assert!(
            chrome.contains(contract),
            "missing toolbar contract: {contract}"
        );
    }
}

#[test]
fn folder_options_has_general_and_view_but_deliberately_no_search_page() {
    let chrome = include_str!("../src/chrome.rs");
    assert!(chrome.contains("folder-options-general-tab"));
    assert!(chrome.contains("folder-options-view-tab"));
    assert!(!chrome.contains("folder-options-search-tab"));
    for id in [
        "folder-option-checkboxes",
        "folder-option-extensions",
        "folder-option-hidden",
        "folder-option-compact",
        "folder-option-details-pane",
        "folder-option-preview-pane",
        "folder-options-ok",
        "folder-options-cancel",
        "folder-options-apply",
    ] {
        assert!(chrome.contains(id), "missing Folder Options control: {id}");
    }
}

#[test]
fn more_commands_use_typed_selection_clipboard_and_shell_boundaries() {
    let actions = include_str!("../src/actions.rs");
    let state = include_str!("../src/state.rs");
    let root = include_str!("../src/lib.rs");
    for action in [
        "UndoCurrentFolder",
        "CompressSelectedToZip",
        "AddSelectedToFavorites",
        "CopySelectedPaths",
        "ShowPropertiesSelected",
        "SelectAllItems",
        "ClearSelection",
        "InvertSelection",
    ] {
        assert!(actions.contains(action), "missing typed action: {action}");
    }
    assert!(state.contains("\"Windows.CompressToZip\""));
    assert!(state.contains("toggle_selected_quick_access"));
    assert!(state.contains("requested_verb: Some(\"undo\".to_owned())"));
    assert!(root.contains("ClipboardItem::new_string(text)"));
}
