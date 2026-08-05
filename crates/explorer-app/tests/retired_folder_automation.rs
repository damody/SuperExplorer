#[test]
fn production_composition_does_not_reference_retired_folder_automation() {
    let production_sources = [
        include_str!("../src/application.rs"),
        include_str!("../src/lib.rs"),
        include_str!("../../explorer-ui/src/lib.rs"),
        include_str!("../../explorer-automation/src/lib.rs"),
    ]
    .join("\n");

    for retired_symbol in [
        "AutomationComposition",
        "FolderScriptHandle",
        "enter_directory",
        ".explorer.lua",
    ] {
        assert!(
            !production_sources.contains(retired_symbol),
            "retired automatic folder-script reference remains: {retired_symbol}"
        );
    }
}
