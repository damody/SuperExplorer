use std::path::Path;

#[test]
fn cargo_package_and_windows_binary_names_are_intentionally_distinct() {
    let manifest = include_str!("../Cargo.toml");
    assert!(manifest.contains("name = \"explorer-app\""));
    assert!(manifest.contains("[[bin]]"));
    assert!(manifest.contains("name = \"SuperExplorer\""));
    assert!(manifest.contains("path = \"src/main.rs\""));
}

#[test]
fn version_resource_exposes_superexplorer_identity() {
    let resource = include_str!("../app.rc");
    for expected in [
        "VALUE \"FileDescription\", \"SuperExplorer\\0\"",
        "VALUE \"InternalName\", \"SuperExplorer\\0\"",
        "VALUE \"OriginalFilename\", \"SuperExplorer.exe\\0\"",
        "VALUE \"ProductName\", \"SuperExplorer\\0\"",
    ] {
        assert!(
            resource.contains(expected),
            "missing VERSIONINFO: {expected}"
        );
    }
}

#[test]
fn cargo_exposes_the_renamed_binary_to_integration_tests() {
    let binary = Path::new(env!("CARGO_BIN_EXE_SuperExplorer"));
    assert_eq!(
        binary.file_name().and_then(|name| name.to_str()),
        Some("SuperExplorer.exe")
    );
}

#[test]
fn production_consumers_do_not_require_the_legacy_binary_name() {
    let workspace = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("workspace root");
    let files = [
        "build/build_install.lua",
        "installer/SuperExplorer.nsi",
        "scripts/finalize_windows_artifact.ps1",
        "uitest/manifest.json",
    ];
    for relative in files {
        let path = workspace.join(relative);
        let text = std::fs::read_to_string(&path)
            .unwrap_or_else(|error| panic!("read {}: {error}", path.display()));
        assert!(
            !text.contains("explorer-app.exe"),
            "legacy production binary reference remains in {}",
            path.display()
        );
    }
}

#[test]
fn product_rename_preserves_all_persisted_data_roots() {
    let workspace = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("workspace root");
    for relative in [
        "crates/explorer-app/src/session_store.rs",
        "crates/explorer-common/src/diagnostics.rs",
        "crates/explorer-search/src/local_index.rs",
        "crates/explorer-shell-win/src/icon_disk_cache.rs",
        "crates/explorer-shell-win/src/thumbnail.rs",
    ] {
        let path = workspace.join(relative);
        let text = std::fs::read_to_string(&path)
            .unwrap_or_else(|error| panic!("read {}: {error}", path.display()));
        assert!(
            text.contains("RustGpuiExplorer"),
            "persisted compatibility root changed in {}",
            path.display()
        );
    }
}
