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
            "base64",
            "explorer-extension-api",
            "explorer-extension-ui-api",
            "libloading",
            "ring",
            "semver",
            "serde",
            "serde_json",
            "sha2",
            "tempfile",
            "thiserror",
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

#[test]
fn extension_host_normal_dependency_closure_has_no_steam_crates() {
    let metadata = cargo_metadata_with_dependencies();
    assert_no_forbidden_in_normal_dependency_closure(
        &metadata,
        "explorer-extension-host",
        &["steam", "steamworks"],
    );
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

fn cargo_metadata_with_dependencies() -> serde_json::Value {
    let repository = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("canonical repository root");
    let output = std::process::Command::new(env!("CARGO"))
        .args(["metadata", "--format-version", "1", "--locked", "--offline"])
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

fn assert_no_forbidden_in_normal_dependency_closure(
    metadata: &serde_json::Value,
    root_name: &str,
    forbidden_fragments: &[&str],
) {
    let packages = metadata["packages"].as_array().expect("metadata packages");
    let package_names = packages
        .iter()
        .map(|package| {
            (
                package["id"].as_str().expect("package id").to_owned(),
                package["name"].as_str().expect("package name").to_owned(),
            )
        })
        .collect::<std::collections::BTreeMap<_, _>>();
    let root_id = packages
        .iter()
        .find(|package| package["name"] == root_name)
        .and_then(|package| package["id"].as_str())
        .unwrap_or_else(|| panic!("missing metadata package: {root_name}"))
        .to_owned();
    let resolve_nodes = metadata["resolve"]["nodes"]
        .as_array()
        .expect("metadata resolve nodes");
    let node_dependencies = resolve_nodes
        .iter()
        .map(|node| {
            let dependencies = node["deps"]
                .as_array()
                .expect("resolve node dependencies")
                .iter()
                .filter(|dependency| {
                    dependency["dep_kinds"]
                        .as_array()
                        .map(|kinds| {
                            kinds
                                .iter()
                                .any(|kind| kind["kind"].is_null() || kind["kind"] == "normal")
                        })
                        .unwrap_or(true)
                })
                .filter_map(|dependency| dependency["pkg"].as_str().map(str::to_owned))
                .collect::<Vec<_>>();
            (
                node["id"].as_str().expect("resolve node id").to_owned(),
                dependencies,
            )
        })
        .collect::<std::collections::BTreeMap<_, _>>();

    let mut pending = vec![root_id];
    let mut visited = std::collections::BTreeSet::new();
    while let Some(package_id) = pending.pop() {
        if !visited.insert(package_id.clone()) {
            continue;
        }
        let package_name = package_names
            .get(&package_id)
            .unwrap_or_else(|| panic!("missing package for resolve id: {package_id}"));
        let lowered = package_name.to_ascii_lowercase();
        assert!(
            !forbidden_fragments
                .iter()
                .any(|fragment| lowered.contains(fragment)),
            "{root_name} normal dependency closure contains forbidden crate {package_name}"
        );
        if let Some(dependencies) = node_dependencies.get(&package_id) {
            pending.extend(dependencies.iter().cloned());
        }
    }
}
