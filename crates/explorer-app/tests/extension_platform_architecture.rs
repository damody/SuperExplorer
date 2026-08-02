//! Dependency-direction guard for the public extension seam.

const WORKSPACE_MANIFEST: &str = include_str!("../../../Cargo.toml");
const APP_MANIFEST: &str = include_str!("../Cargo.toml");
const APPLICATION_COMPOSITION: &str = include_str!("../src/application.rs");

#[test]
fn public_extension_crates_have_one_way_dependencies_and_a_host_composition_root() {
    for member in [
        "crates/explorer-extension-api",
        "crates/explorer-extension-ui-api",
        "crates/explorer-extension-host",
    ] {
        assert!(
            WORKSPACE_MANIFEST.contains(member),
            "missing workspace member: {member}"
        );
    }

    let metadata = cargo_metadata();
    assert_dependency_allowlist(&metadata, "explorer-extension-api", &["abi_stable"]);
    assert_dependency_allowlist(
        &metadata,
        "explorer-extension-ui-api",
        &["explorer-extension-api"],
    );
    assert_dependency_allowlist(
        &metadata,
        "explorer-extension-host",
        &[
            "abi_stable",
            "explorer-extension-api",
            "explorer-extension-ui-api",
        ],
    );
    assert!(
        APP_MANIFEST
            .contains("explorer-extension-host = { path = \"../explorer-extension-host\" }")
    );
    assert!(
        APPLICATION_COMPOSITION
            .contains("extension_host: Option<explorer_extension_host::ExtensionHost>")
    );
    assert!(APPLICATION_COMPOSITION.contains("extension_host.start();"));
    assert!(APPLICATION_COMPOSITION.contains("extension_host.shutdown();"));
    for package in [
        "explorer-ui",
        "explorer-extension-protocol",
        "explorer-extension-broker",
    ] {
        assert_no_dependency(&metadata, package, "explorer-extension-host");
    }
}

fn cargo_metadata() -> serde_json::Value {
    let repository = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("canonical repository root");
    let output = std::process::Command::new(env!("CARGO"))
        .args([
            "metadata",
            "--format-version",
            "1",
            "--no-deps",
            "--locked",
            "--offline",
        ])
        .current_dir(repository)
        .output()
        .expect("run cargo metadata");
    assert!(
        output.status.success(),
        "cargo metadata failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).expect("parse cargo metadata")
}

fn package<'a>(metadata: &'a serde_json::Value, name: &str) -> &'a serde_json::Value {
    metadata["packages"]
        .as_array()
        .expect("metadata packages")
        .iter()
        .find(|package| package["name"] == name)
        .unwrap_or_else(|| panic!("missing metadata package: {name}"))
}

fn assert_dependency_allowlist(metadata: &serde_json::Value, name: &str, allowed: &[&str]) {
    let actual = package(metadata, name)["dependencies"]
        .as_array()
        .expect("package dependencies")
        .iter()
        .map(|dependency| {
            dependency["name"]
                .as_str()
                .expect("dependency package name")
        })
        .collect::<std::collections::BTreeSet<_>>();
    let expected = allowed
        .iter()
        .copied()
        .collect::<std::collections::BTreeSet<_>>();
    assert_eq!(
        actual, expected,
        "{name} dependency surface changed; public extension crates require an explicit allowlist review"
    );
}

fn assert_no_dependency(metadata: &serde_json::Value, package_name: &str, forbidden: &str) {
    let has_forbidden = package(metadata, package_name)["dependencies"]
        .as_array()
        .expect("package dependencies")
        .iter()
        .any(|dependency| dependency["name"] == forbidden);
    assert!(
        !has_forbidden,
        "{package_name} must not depend on {forbidden}"
    );
}
