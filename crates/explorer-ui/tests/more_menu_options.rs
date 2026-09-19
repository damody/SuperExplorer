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
        "more-theme",
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
    assert!(chrome.contains("more-theme"));
    assert!(chrome.contains("more-theme-menu"));
    assert!(chrome.contains(".top(px(tokens.layout.minimum_hit_target.value()))"));
    assert!(chrome.contains(".right_0()"));
    assert!(chrome.contains(".with_priority(140)"));
}

#[test]
fn more_theme_submenu_opens_to_the_right_of_the_more_menu() {
    let chrome = include_str!("../src/chrome.rs");
    let production = chrome
        .split("#[cfg(test)]")
        .next()
        .expect("production source precedes tests");
    let more_menu = production
        .split("fn command_more_menu_v2(")
        .nth(1)
        .expect("missing command_more_menu_v2")
        .split("\nfn ")
        .next()
        .expect("command_more_menu_v2 boundary");
    assert!(
        more_menu.contains(".w(px(tokens.layout.address_min_width.value()))"),
        "more menu width must stay the commands column so opening Theme does not shift it left"
    );
    assert!(
        !more_menu.contains(".flex_row()"),
        "theme pane must not join the commands column in a growing flex row"
    );
    assert!(
        more_menu.contains("more_theme_submenu("),
        "more menu must host the theme pane as a sibling"
    );
    let local = production
        .split("fn more_theme_submenu(")
        .nth(1)
        .expect("missing more_theme_submenu")
        .split("\nfn ")
        .next()
        .expect("more_theme_submenu boundary");
    assert!(
        local.contains(".absolute()"),
        "theme pane must overlay to the right without changing the more menu box"
    );
    assert!(
        local.contains(".left(px(layout.address_min_width.value()))"),
        "theme flyout must open to the right of the more menu"
    );
    assert!(
        local.contains("more_theme_submenu_offset(layout)"),
        "theme flyout must align with the Theme row, not the top of the more menu"
    );
    assert!(
        !local.contains(".left(px(-layout.address_min_width.value()))"),
        "theme flyout must not open to the left of the more menu"
    );
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
        "menu-other",
        "menu-extensions",
        "command-extensions-popup",
        "extensions-refresh-tortoisegit",
        "menu-refresh-tortoisegit",
        "menu-no-extensions",
        "menu-theme",
    ] {
        assert!(
            chrome.contains(contract),
            "missing toolbar contract: {contract}"
        );
    }
}

#[test]
fn handoff_command_sits_immediately_after_extensions() {
    let chrome = include_str!("../src/chrome.rs");
    let production = chrome
        .split("#[cfg(test)]")
        .next()
        .expect("production source precedes tests");
    let extensions = production
        .find("command-extensions-menu")
        .expect("Extensions command");
    let handoff = production
        .find("command-handoff-file-explorer")
        .expect("Handoff to File Explorer command");
    let transfers = production
        .find("command-transfer-center")
        .expect("Transfer center command");
    assert!(
        extensions < handoff && handoff < transfers,
        "handoff button must sit between Extensions and the transfer-center spacer group"
    );
    let button = &production[handoff..transfers];
    assert!(
        button.contains("menu-open-in-file-explorer"),
        "visible handoff label missing"
    );
    assert!(
        button.contains("HandoffToFileExplorer"),
        "handoff action missing"
    );
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
