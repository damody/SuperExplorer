use std::path::Path;

use serde::Deserialize;

use crate::{AUTOMATION_API_VERSION, AUTOMATION_EVENT_NAMES, EVENT_SCHEMA_VERSION, LuaVm};

const EXAMPLES: &[(&str, &str)] = &[
    (
        "01_hotkey_queue.lua",
        include_str!("../../../automation-sdk/examples/01_hotkey_queue.lua"),
    ),
    (
        "02_watch_files.lua",
        include_str!("../../../automation-sdk/examples/02_watch_files.lua"),
    ),
    (
        "03_cli_delete.lua",
        include_str!("../../../automation-sdk/examples/03_cli_delete.lua"),
    ),
    (
        "04_deepseek_txt.lua",
        include_str!("../../../automation-sdk/examples/04_deepseek_txt.lua"),
    ),
    (
        "05_clipboard_timing.lua",
        include_str!("../../../automation-sdk/examples/05_clipboard_timing.lua"),
    ),
];

#[derive(Deserialize)]
struct DocumentedCatalog {
    api_version: String,
    event_schema_version: u16,
    events: Vec<String>,
}

#[test]
fn all_documented_examples_compile_and_register() {
    for (name, source) in EXAMPLES {
        let mut vm = LuaVm::new().expect("VM");
        vm.register(source, Path::new(name))
            .expect("example registration");
        assert!(vm.registration().is_some());
    }
}

#[test]
fn machine_catalog_exactly_matches_runtime_contract() {
    let documented: DocumentedCatalog =
        serde_json::from_str(include_str!("../../../automation-sdk/EVENT_CATALOG.json"))
            .expect("event catalog JSON");
    assert_eq!(documented.api_version, AUTOMATION_API_VERSION);
    assert_eq!(documented.event_schema_version, EVENT_SCHEMA_VERSION);
    assert_eq!(
        documented
            .events
            .iter()
            .map(String::as_str)
            .collect::<Vec<_>>(),
        AUTOMATION_EVENT_NAMES
    );
    let types = include_str!("../../../automation-sdk/types/explorer-automation.lua");
    assert!(types.contains(AUTOMATION_API_VERSION));
}
