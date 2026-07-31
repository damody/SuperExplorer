use std::{fs, path::Path};

#[test]
fn every_workspace_production_target_enables_panic_api_denials() {
    let workspace = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("workspace root");
    let targets = [
        "crates/explorer-app/src/lib.rs",
        "crates/explorer-app/src/main.rs",
        "crates/explorer-common/src/lib.rs",
        "crates/explorer-jobs/src/lib.rs",
        "crates/explorer-model/src/lib.rs",
        "crates/explorer-search/src/lib.rs",
        "crates/explorer-shell-win/src/lib.rs",
        "crates/explorer-test-support/src/lib.rs",
        "crates/explorer-ui/src/lib.rs",
        "crates/explorer-uitest/src/lib.rs",
        "crates/explorer-uitest/src/main.rs",
    ];
    let required = [
        "not(test)",
        "clippy::unwrap_used",
        "clippy::expect_used",
        "clippy::panic",
        "clippy::todo",
        "clippy::unimplemented",
    ];

    for relative in targets {
        let source = fs::read_to_string(workspace.join(relative)).expect("read production root");
        for lint in required {
            assert!(
                source.contains(lint),
                "{relative} does not enforce {lint} for production builds"
            );
        }
    }
}
