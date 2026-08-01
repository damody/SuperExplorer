//! Deterministically generates the SDK bundle lock and manifest from the checked-out tree.
//!
//! This binary deliberately has no path or command-output overrides. Its only production
//! inputs are the repository containing this source file and trusted local tools (`rustc`,
//! `cargo`, and `git`). That keeps a generated bundle record tied to the checked-out source.

use std::{
    collections::BTreeMap,
    env,
    ffi::OsStr,
    fmt::Write as _,
    fs,
    io::Read,
    path::{Component, Path, PathBuf},
    process::Command,
};

use serde::Serialize;
use serde_json::Value;
use superexplorer_ui_abi_fingerprint::{artifact_bytes, production_fingerprint_from_lock};

const SCHEMA_VERSION: u32 = 1;
const GENERATED_FILES: [&str; 3] = [
    "sdk/sdk-lock.json",
    "sdk/bundle-manifest.json",
    "sdk/ui-abi-fingerprint.json",
];
type LockPackageKey = (String, String, Option<String>);
type LockChecksumMap = BTreeMap<LockPackageKey, Option<String>>;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct FileHash {
    path: String,
    sha256: String,
}

#[derive(Debug, Serialize)]
struct Toolchain {
    rustc_release: String,
    rustc_commit_hash: String,
    cargo_release: String,
    cargo_commit_hash: String,
    target: String,
}

#[derive(Debug, Serialize)]
struct GpuiSource {
    approved_snapshot_sha256: String,
    approved_snapshot: Value,
    repository: String,
    revision: String,
    tree: String,
}

#[derive(Debug, Clone, Serialize)]
struct ProtectedPackage {
    key: String,
    name: String,
    version: String,
    source: Option<String>,
    path: Option<String>,
    checksum: Option<String>,
    features: Vec<String>,
    dependencies: Vec<DependencyEdge>,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize)]
struct DependencyKind {
    kind: String,
    target: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize)]
struct DependencyEdge {
    name: String,
    to: String,
    dep_kinds: Vec<DependencyKind>,
}

#[derive(Debug, Serialize)]
struct AbiContract {
    schema_version: u64,
    fingerprint_algorithm: String,
    fingerprint: String,
}

#[derive(Debug, Serialize)]
struct SdkLock {
    schema_version: u32,
    bundle_id: String,
    inventory_root_sha256: String,
    toolchain: Toolchain,
    manifests: Vec<FileHash>,
    gpui: GpuiSource,
    protected_dependency_graph: Vec<ProtectedPackage>,
    protected_dependency_contract: Value,
    release_profiles: BTreeMap<String, Value>,
    build_policy: Value,
    abi: AbiContract,
    sdk_public_source_hashes: Vec<FileHash>,
    inventory: Vec<FileHash>,
}

#[derive(Debug, Serialize)]
struct BundleManifest {
    schema_version: u32,
    bundle_id: String,
    inventory_root_sha256: String,
    sdk_lock_sha256: String,
    files: Vec<FileHash>,
    generated_artifacts: Vec<FileHash>,
}

/// Runs the only two production operations. Neither operation accepts a caller-supplied
/// repository root, executable, metadata JSON, or output location.
///
/// # Errors
///
/// Returns an error when the requested operation is invalid or when any trusted input,
/// tool invocation, generated record, or filesystem operation cannot be verified.
pub fn run(arguments: impl IntoIterator<Item = String>) -> Result<(), String> {
    let arguments = arguments.into_iter().collect::<Vec<_>>();
    let root = repository_root()?;
    match arguments.as_slice() {
        [command] if command == "generate" => write_generated(&root),
        [command] if command == "verify" => verify_generated(&root),
        _ => Err("usage: superexplorer-bundle-generator <generate|verify>".to_owned()),
    }
}

fn repository_root() -> Result<PathBuf, String> {
    let source = Path::new(env!("CARGO_MANIFEST_DIR"));
    source
        .ancestors()
        .nth(3)
        .map(Path::to_path_buf)
        .filter(|root| root.join("Cargo.toml").is_file() && root.join("sdk").is_dir())
        .ok_or_else(|| "could not resolve repository root from bundle generator source".to_owned())
}

fn write_generated(root: &Path) -> Result<(), String> {
    let (lock, mut manifest) = generate(root)?;
    let lock_bytes = json_file_bytes(&lock, &root.join("sdk/sdk-lock.json"))?;
    fs::write(root.join("sdk/sdk-lock.json"), &lock_bytes)
        .map_err(|error| format!("could not write sdk/sdk-lock.json: {error}"))?;
    let artifact =
        production_fingerprint_from_lock(&serde_json::to_value(&lock).map_err(|error| {
            format!("could not serialize SDK lock for UI fingerprint: {error}")
        })?)?;
    let artifact_bytes = artifact_bytes(&artifact)?;
    let artifact_path = "sdk/ui-abi-fingerprint.json";
    fs::write(root.join(artifact_path), &artifact_bytes)
        .map_err(|error| format!("could not write {artifact_path}: {error}"))?;
    manifest.generated_artifacts = vec![FileHash {
        path: artifact_path.to_owned(),
        sha256: sha256_hex(&artifact_bytes),
    }];
    write_json(&root.join("sdk/bundle-manifest.json"), &manifest)
}

fn verify_generated(root: &Path) -> Result<(), String> {
    let (lock, mut manifest) = generate(root)?;
    let artifact =
        production_fingerprint_from_lock(&serde_json::to_value(&lock).map_err(|error| {
            format!("could not serialize SDK lock for UI fingerprint: {error}")
        })?)?;
    let artifact_bytes = artifact_bytes(&artifact)?;
    let artifact_path = "sdk/ui-abi-fingerprint.json";
    manifest.generated_artifacts = vec![FileHash {
        path: artifact_path.to_owned(),
        sha256: sha256_hex(&artifact_bytes),
    }];
    verify_json(&root.join("sdk/sdk-lock.json"), &lock)?;
    verify_bytes(&root.join(artifact_path), &artifact_bytes)?;
    verify_json(&root.join("sdk/bundle-manifest.json"), &manifest)
}

fn generate(root: &Path) -> Result<(SdkLock, BundleManifest), String> {
    let inventory = collect_inventory(root)?;
    let inventory_root_sha256 = inventory_hash(&inventory);
    let manifests = [
        "Cargo.toml",
        "rust-toolchain.toml",
        "sdk/Cargo.toml",
        "sdk/rust-toolchain.toml",
    ]
    .into_iter()
    .map(|path| file_hash(root, path))
    .collect::<Result<Vec<_>, _>>()?;
    let policy = read_json(&root.join("sdk/build-policy.json"))?;
    let gpui_snapshot = read_json(&root.join("sdk/snapshot/approved-gpui.json"))?;
    let gpui_snapshot_sha256 = hash_file(&root.join("sdk/snapshot/approved-gpui.json"))?;
    let gpui = read_gpui(root, gpui_snapshot, gpui_snapshot_sha256)?;
    let protected_dependency_graph = cargo_graph(root)?;
    let protected_dependency_contract =
        read_json(&root.join("sdk/snapshot/protected-dependency-closure.json"))?;
    validate_protected_dependency_contract(&protected_dependency_contract)?;
    let release_profiles = release_profiles(root)?;
    let abi = abi_contract(&policy, &protected_dependency_graph)?;
    let toolchain = parse_toolchain(
        &command_output("rustc", &["-Vv"], root)?,
        &command_output("cargo", &["-Vv"], root)?,
    )?;
    let sdk_public_source_hashes = inventory
        .iter()
        .filter(|entry| entry.path.starts_with("sdk/src/"))
        .cloned()
        .collect::<Vec<_>>();
    let bundle_identity = serde_json::json!({
        "inventory_root_sha256": inventory_root_sha256,
        "toolchain": toolchain,
        "manifests": manifests,
        "gpui": gpui,
        "protected_dependency_graph": protected_dependency_graph,
        "protected_dependency_contract": protected_dependency_contract,
        "release_profiles": release_profiles,
        "build_policy": policy,
        "abi": abi,
        "sdk_public_source_hashes": sdk_public_source_hashes,
    });
    let bundle_id = bundle_id_from_identity(&bundle_identity)?;
    let lock = SdkLock {
        schema_version: SCHEMA_VERSION,
        bundle_id: bundle_id.clone(),
        inventory_root_sha256: inventory_root_sha256.clone(),
        toolchain,
        manifests,
        gpui,
        protected_dependency_graph,
        protected_dependency_contract,
        release_profiles,
        build_policy: policy,
        abi,
        sdk_public_source_hashes,
        inventory: inventory.clone(),
    };
    let sdk_lock_sha256 = sha256_hex(&json_file_bytes(&lock, &root.join("sdk/sdk-lock.json"))?);
    let manifest = BundleManifest {
        schema_version: SCHEMA_VERSION,
        bundle_id,
        inventory_root_sha256,
        sdk_lock_sha256,
        files: inventory,
        generated_artifacts: Vec::new(),
    };
    Ok((lock, manifest))
}

fn read_gpui(root: &Path, snapshot: Value, snapshot_hash: String) -> Result<GpuiSource, String> {
    let source = snapshot
        .get("source")
        .and_then(Value::as_object)
        .ok_or_else(|| "approved GPUI snapshot has no source object".to_owned())?;
    let required = |key: &str| {
        source
            .get(key)
            .and_then(Value::as_str)
            .map(str::to_owned)
            .ok_or_else(|| format!("approved GPUI snapshot has no source.{key}"))
    };
    let repository = required("repository")?;
    let revision = command_output("git", &["-C", "vendor/gpui-ce", "rev-parse", "HEAD"], root)?;
    let tree = command_output(
        "git",
        &["-C", "vendor/gpui-ce", "rev-parse", "HEAD^{tree}"],
        root,
    )?;
    if source.get("revision").and_then(Value::as_str) != Some(revision.as_str())
        || source.get("tree").and_then(Value::as_str) != Some(tree.as_str())
    {
        return Err(
            "approved GPUI snapshot does not match checked-out GPUI revision/tree".to_owned(),
        );
    }
    Ok(GpuiSource {
        approved_snapshot_sha256: snapshot_hash,
        approved_snapshot: snapshot,
        repository,
        revision,
        tree,
    })
}

fn cargo_graph(root: &Path) -> Result<Vec<ProtectedPackage>, String> {
    let metadata = command_output(
        "cargo",
        &[
            "metadata",
            "--locked",
            "--format-version",
            "1",
            "--manifest-path",
            "sdk/Cargo.toml",
        ],
        root,
    )?;
    let metadata = serde_json::from_str::<Value>(&metadata)
        .map_err(|error| format!("cargo metadata returned invalid JSON: {error}"))?;
    let lock = fs::read_to_string(root.join("sdk/Cargo.lock"))
        .map_err(|error| format!("could not read sdk/Cargo.lock: {error}"))?
        .parse::<toml::Value>()
        .map_err(|error| format!("could not parse sdk/Cargo.lock: {error}"))?;
    let checksums = lock_checksums(&lock)?;
    let packages = metadata
        .get("packages")
        .and_then(Value::as_array)
        .ok_or_else(|| "cargo metadata has no packages array".to_owned())?;
    let mut packages_by_id = BTreeMap::new();
    for package in packages {
        let id = json_string(package, "id")?;
        let name = json_string(package, "name")?;
        let version = json_string(package, "version")?;
        let source = package
            .get("source")
            .and_then(Value::as_str)
            .map(str::to_owned);
        let path = package
            .get("manifest_path")
            .and_then(Value::as_str)
            .map(Path::new)
            .and_then(|path| path.parent())
            .and_then(|path| relative_path(root, path).ok());
        let key = package_key(&name, &version, source.as_deref(), path.as_deref());
        packages_by_id.insert(id, (key, name, version, source, path));
    }
    let nodes = metadata
        .pointer("/resolve/nodes")
        .and_then(Value::as_array)
        .ok_or_else(|| "cargo metadata has no resolved nodes".to_owned())?;
    let mut graph = Vec::with_capacity(nodes.len());
    for node in nodes {
        let package_id = json_string(node, "id")?;
        let (key, name, version, source, path) =
            packages_by_id.get(&package_id).cloned().ok_or_else(|| {
                format!("resolved package {package_id} is absent from package metadata")
            })?;
        let mut features = node
            .get("features")
            .and_then(Value::as_array)
            .ok_or_else(|| format!("resolved package {key} has no features"))?
            .iter()
            .map(|feature| feature.as_str().map(str::to_owned))
            .collect::<Option<Vec<_>>>()
            .ok_or_else(|| format!("resolved package {key} has a non-string feature"))?;
        features.sort();
        let mut dependencies = node
            .get("deps")
            .and_then(Value::as_array)
            .ok_or_else(|| format!("resolved package {key} has no dependency edges"))?
            .iter()
            .map(|dependency| {
                let name = json_string(dependency, "name")?;
                let package_id = json_string(dependency, "pkg")?;
                let to = packages_by_id
                    .get(&package_id)
                    .map(|package| package.0.clone())
                    .ok_or_else(|| format!("resolved package {key} has an unknown dependency"))?;
                let mut dep_kinds = dependency
                    .get("dep_kinds")
                    .and_then(Value::as_array)
                    .ok_or_else(|| format!("dependency {name} of {key} has no dep_kinds"))?
                    .iter()
                    .map(|kind_entry| {
                        let kind = match kind_entry.get("kind") {
                            Some(Value::Null) | None => "normal".to_owned(),
                            Some(Value::String(kind)) => kind.clone(),
                            _ => {
                                return Err(format!("dependency {name} of {key} has invalid kind"));
                            }
                        };
                        let target = match kind_entry.get("target") {
                            Some(Value::Null) | None => None,
                            Some(Value::String(target)) => Some(target.clone()),
                            _ => {
                                return Err(format!(
                                    "dependency {name} of {key} has invalid target"
                                ));
                            }
                        };
                        Ok(DependencyKind { kind, target })
                    })
                    .collect::<Result<Vec<_>, String>>()?;
                dep_kinds.sort();
                Ok::<DependencyEdge, String>(DependencyEdge {
                    name,
                    to,
                    dep_kinds,
                })
            })
            .collect::<Result<Vec<_>, _>>()?;
        dependencies.sort();
        let checksum = checksums
            .get(&(name.clone(), version.clone(), source.clone()))
            .cloned()
            .flatten();
        if source
            .as_deref()
            .is_some_and(|source| source.starts_with("registry+"))
            && checksum.is_none()
        {
            return Err(format!(
                "registry package {key} has no canonical lock checksum"
            ));
        }
        graph.push(ProtectedPackage {
            key,
            name,
            version,
            source,
            path,
            checksum,
            features,
            dependencies,
        });
    }
    graph.sort_by(|left, right| left.key.cmp(&right.key));
    Ok(graph)
}

fn lock_checksums(lock: &toml::Value) -> Result<LockChecksumMap, String> {
    let packages = lock
        .get("package")
        .and_then(toml::Value::as_array)
        .ok_or_else(|| "sdk/Cargo.lock has no package array".to_owned())?;
    let mut checksums = BTreeMap::new();
    for package in packages {
        let name = package
            .get("name")
            .and_then(toml::Value::as_str)
            .ok_or_else(|| "lock package has no name".to_owned())?
            .to_owned();
        let version = package
            .get("version")
            .and_then(toml::Value::as_str)
            .ok_or_else(|| "lock package has no version".to_owned())?
            .to_owned();
        let checksum = package
            .get("checksum")
            .and_then(toml::Value::as_str)
            .map(str::to_owned);
        let source = package
            .get("source")
            .and_then(toml::Value::as_str)
            .map(str::to_owned);
        checksums.insert((name, version, source), checksum);
    }
    Ok(checksums)
}

fn validate_protected_dependency_contract(contract: &Value) -> Result<(), String> {
    if contract.get("schema_version").and_then(Value::as_u64) != Some(2)
        || contract.get("algorithm").and_then(Value::as_str) != Some("normalized-package-edges-v2")
    {
        return Err("protected dependency contract schema/algorithm drifted".to_owned());
    }
    let digest = contract
        .get("edge_digest")
        .and_then(Value::as_str)
        .ok_or_else(|| "protected dependency contract has no edge digest".to_owned())?;
    if digest.len() != 64 || !digest.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err("protected dependency contract edge digest is not SHA-256".to_owned());
    }
    Ok(())
}

fn release_profiles(root: &Path) -> Result<BTreeMap<String, Value>, String> {
    ["Cargo.toml", "sdk/Cargo.toml"]
        .into_iter()
        .map(|path| {
            let document = fs::read_to_string(root.join(path))
                .map_err(|error| format!("could not read {path}: {error}"))?
                .parse::<toml::Value>()
                .map_err(|error| format!("could not parse {path}: {error}"))?;
            let profile = document
                .get("profile")
                .and_then(toml::Value::as_table)
                .and_then(|profiles| profiles.get("release"))
                .ok_or_else(|| format!("{path} has no profile.release"))?;
            let value = serde_json::to_value(profile)
                .map_err(|error| format!("could not serialize {path} release profile: {error}"))?;
            Ok((path.to_owned(), value))
        })
        .collect()
}

fn abi_contract(policy: &Value, graph: &[ProtectedPackage]) -> Result<AbiContract, String> {
    let schema_version = policy
        .get("abi_schema_version")
        .and_then(Value::as_u64)
        .ok_or_else(|| "SDK build policy has no abi_schema_version".to_owned())?;
    let fingerprint_algorithm = policy
        .get("fingerprint_algorithm")
        .and_then(Value::as_str)
        .ok_or_else(|| "SDK build policy has no fingerprint_algorithm".to_owned())?
        .to_owned();
    let abi_stable = graph
        .iter()
        .filter(|package| package.name == "abi_stable")
        .collect::<Vec<_>>();
    if abi_stable.len() != 1 {
        return Err(
            "protected dependency graph must resolve exactly one abi_stable package".to_owned(),
        );
    }
    let fingerprint_input = serde_json::json!({
        "schema_version": schema_version,
        "algorithm": fingerprint_algorithm,
        "abi_stable": abi_stable[0],
    });
    Ok(AbiContract {
        schema_version,
        fingerprint_algorithm,
        fingerprint: sha256_hex(&canonical_json(&fingerprint_input)?),
    })
}

fn collect_inventory(root: &Path) -> Result<Vec<FileHash>, String> {
    let mut inventory = Vec::new();
    for relative_root in ["sdk", "vendor/gpui-ce"] {
        collect_directory(root, &root.join(relative_root), &mut inventory)?;
    }
    inventory.sort_by(|left, right| left.path.cmp(&right.path));
    if inventory
        .windows(2)
        .any(|entries| entries[0].path == entries[1].path)
    {
        return Err("inventory contains duplicate paths".to_owned());
    }
    Ok(inventory)
}

fn collect_directory(
    root: &Path,
    directory: &Path,
    inventory: &mut Vec<FileHash>,
) -> Result<(), String> {
    for entry in fs::read_dir(directory)
        .map_err(|error| format!("could not list {}: {error}", directory.display()))?
    {
        let entry = entry.map_err(|error| format!("could not read directory entry: {error}"))?;
        let file_type = entry
            .file_type()
            .map_err(|error| format!("could not read file type: {error}"))?;
        let path = entry.path();
        if is_git_metadata(&path) {
            continue;
        }
        if file_type.is_symlink() {
            return Err(format!("inventory refuses symlink {}", path.display()));
        }
        if file_type.is_dir() {
            if !excluded_build_directory(root, &path)? {
                collect_directory(root, &path, inventory)?;
            }
        } else if file_type.is_file() {
            let relative = relative_path(root, &path)?;
            if !GENERATED_FILES.contains(&relative.as_str()) {
                inventory.push(FileHash {
                    sha256: hash_file(&path)?,
                    path: relative,
                });
            }
        }
    }
    Ok(())
}

fn is_git_metadata(path: &Path) -> bool {
    path.file_name() == Some(OsStr::new(".git"))
}

fn excluded_build_directory(root: &Path, path: &Path) -> Result<bool, String> {
    let relative = relative_path(root, path)?;
    let components = relative.split('/').collect::<Vec<_>>();
    Ok(matches!(
        components.as_slice(),
        ["sdk", "target"]
            | ["sdk", "fixtures" | "tools", _, "target"]
            | ["vendor", "gpui-ce", "target"]
    ))
}

fn file_hash(root: &Path, relative: &str) -> Result<FileHash, String> {
    Ok(FileHash {
        path: relative.to_owned(),
        sha256: hash_file(&root.join(relative))?,
    })
}

fn hash_file(path: &Path) -> Result<String, String> {
    let mut file = fs::File::open(path)
        .map_err(|error| format!("could not read {}: {error}", path.display()))?;
    let mut hasher = Sha256::default();
    let mut buffer = [0_u8; 8 * 1024];
    loop {
        let read = file
            .read(&mut buffer)
            .map_err(|error| format!("could not hash {}: {error}", path.display()))?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Ok(hasher.finish_hex())
}

fn inventory_hash(inventory: &[FileHash]) -> String {
    let mut hasher = Sha256::default();
    for entry in inventory {
        hasher.update(entry.path.as_bytes());
        hasher.update(&[0]);
        hasher.update(entry.sha256.as_bytes());
        hasher.update(b"\n");
    }
    hasher.finish_hex()
}

fn relative_path(root: &Path, path: &Path) -> Result<String, String> {
    let relative = path
        .strip_prefix(root)
        .map_err(|_| format!("path outside repository: {}", path.display()))?;
    if relative
        .components()
        .any(|component| !matches!(component, Component::Normal(_)))
    {
        return Err(format!(
            "path is not a stable relative path: {}",
            path.display()
        ));
    }
    let stable = relative.to_string_lossy().replace('\\', "/");
    if stable.is_empty() || stable.contains(':') || stable.starts_with('/') {
        return Err(format!("path is not portable: {}", path.display()));
    }
    Ok(stable)
}

fn package_key(name: &str, version: &str, source: Option<&str>, path: Option<&str>) -> String {
    let origin = source.or(path).unwrap_or("unresolved-origin");
    format!("{name}@{version}#{origin}")
}

fn json_string(value: &Value, key: &str) -> Result<String, String> {
    value
        .get(key)
        .and_then(Value::as_str)
        .map(str::to_owned)
        .ok_or_else(|| format!("JSON object has no string {key}"))
}

fn read_json(path: &Path) -> Result<Value, String> {
    let contents = fs::read_to_string(path)
        .map_err(|error| format!("could not read {}: {error}", path.display()))?;
    serde_json::from_str(&contents)
        .map_err(|error| format!("could not parse {}: {error}", path.display()))
}

fn command_output(program: &str, arguments: &[&str], root: &Path) -> Result<String, String> {
    let output = Command::new(program)
        .args(arguments)
        .current_dir(root)
        .output()
        .map_err(|error| format!("could not invoke {program}: {error}"))?;
    if !output.status.success() {
        return Err(format!(
            "{program} {} failed with {}: {}",
            arguments.join(" "),
            output.status,
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    String::from_utf8(output.stdout)
        .map_err(|error| format!("{program} emitted non-UTF-8 output: {error}"))
        .map(|output| output.replace("\r\n", "\n").trim_end().to_owned())
}

fn parse_toolchain(rustc: &str, cargo: &str) -> Result<Toolchain, String> {
    let field = |text: &str, key: &str| {
        text.lines()
            .find_map(|line| line.strip_prefix(&format!("{key}: ")))
            .filter(|value| !value.is_empty())
            .map(str::to_owned)
            .ok_or_else(|| format!("tool output has no {key}"))
    };
    let rustc_release = field(rustc, "release")?;
    let rustc_commit_hash = field(rustc, "commit-hash")?;
    let cargo_release = field(cargo, "release")?;
    let cargo_commit_hash = field(cargo, "commit-hash")?;
    let target = field(rustc, "host")?;
    if target != "x86_64-pc-windows-msvc" {
        return Err(format!("unsupported toolchain target {target}"));
    }
    Ok(Toolchain {
        rustc_release,
        rustc_commit_hash,
        cargo_release,
        cargo_commit_hash,
        target,
    })
}

fn canonical_json<T: Serialize>(value: &T) -> Result<Vec<u8>, String> {
    serde_json::to_vec(value)
        .map_err(|error| format!("could not serialize canonical JSON: {error}"))
}

fn bundle_id_from_identity(identity: &Value) -> Result<String, String> {
    Ok(format!(
        "sdk-{}",
        sha256_hex(&canonical_json(identity)?)[..24].to_owned()
    ))
}

fn write_json<T: Serialize>(path: &Path, value: &T) -> Result<(), String> {
    let bytes = json_file_bytes(value, path)?;
    fs::write(path, bytes).map_err(|error| format!("could not write {}: {error}", path.display()))
}

fn json_file_bytes<T: Serialize>(value: &T, path: &Path) -> Result<Vec<u8>, String> {
    let mut bytes = serde_json::to_vec_pretty(value)
        .map_err(|error| format!("could not serialize {}: {error}", path.display()))?;
    bytes.push(b'\n');
    Ok(bytes)
}

fn verify_json<T: Serialize>(path: &Path, value: &T) -> Result<(), String> {
    let expected = json_file_bytes(value, path)?;
    verify_bytes(path, &expected)
}

fn verify_bytes(path: &Path, expected: &[u8]) -> Result<(), String> {
    let actual =
        fs::read(path).map_err(|error| format!("could not read {}: {error}", path.display()))?;
    if actual == expected {
        Ok(())
    } else {
        Err(format!(
            "{} is stale; rerun bundle generator",
            path.display()
        ))
    }
}

struct Sha256 {
    state: [u32; 8],
    buffer: [u8; 64],
    buffered_bytes: usize,
    length_bits: u64,
}

impl Default for Sha256 {
    fn default() -> Self {
        Self {
            state: [
                0x6a09_e667,
                0xbb67_ae85,
                0x3c6e_f372,
                0xa54f_f53a,
                0x510e_527f,
                0x9b05_688c,
                0x1f83_d9ab,
                0x5be0_cd19,
            ],
            buffer: [0; 64],
            buffered_bytes: 0,
            length_bits: 0,
        }
    }
}

impl Sha256 {
    fn update(&mut self, bytes: &[u8]) {
        self.length_bits = self.length_bits.wrapping_add((bytes.len() as u64) * 8);
        let mut remaining = bytes;
        if self.buffered_bytes != 0 {
            let copied = (64 - self.buffered_bytes).min(remaining.len());
            self.buffer[self.buffered_bytes..self.buffered_bytes + copied]
                .copy_from_slice(&remaining[..copied]);
            self.buffered_bytes += copied;
            remaining = &remaining[copied..];
            if self.buffered_bytes < 64 {
                return;
            }
            if self.buffered_bytes == 64 {
                self.process_block(self.buffer);
                self.buffered_bytes = 0;
            }
        }
        while remaining.len() >= 64 {
            let block = remaining[..64]
                .try_into()
                .expect("block is exactly 64 bytes");
            self.process_block(block);
            remaining = &remaining[64..];
        }
        self.buffer[..remaining.len()].copy_from_slice(remaining);
        self.buffered_bytes = remaining.len();
    }

    fn finish_hex(mut self) -> String {
        let original_length = self.length_bits;
        self.buffer[self.buffered_bytes] = 0x80;
        self.buffered_bytes += 1;
        if self.buffered_bytes > 56 {
            self.buffer[self.buffered_bytes..].fill(0);
            self.process_block(self.buffer);
            self.buffered_bytes = 0;
        }
        self.buffer[self.buffered_bytes..56].fill(0);
        self.buffer[56..].copy_from_slice(&original_length.to_be_bytes());
        self.process_block(self.buffer);
        let mut output = String::with_capacity(64);
        for word in self.state {
            write!(output, "{word:08x}").expect("writing into a string cannot fail");
        }
        output
    }

    #[allow(clippy::many_single_char_names)]
    fn process_block(&mut self, block: [u8; 64]) {
        const K: [u32; 64] = [
            0x428a_2f98,
            0x7137_4491,
            0xb5c0_fbcf,
            0xe9b5_dba5,
            0x3956_c25b,
            0x59f1_11f1,
            0x923f_82a4,
            0xab1c_5ed5,
            0xd807_aa98,
            0x1283_5b01,
            0x2431_85be,
            0x550c_7dc3,
            0x72be_5d74,
            0x80de_b1fe,
            0x9bdc_06a7,
            0xc19b_f174,
            0xe49b_69c1,
            0xefbe_4786,
            0x0fc1_9dc6,
            0x240c_a1cc,
            0x2de9_2c6f,
            0x4a74_84aa,
            0x5cb0_a9dc,
            0x76f9_88da,
            0x983e_5152,
            0xa831_c66d,
            0xb003_27c8,
            0xbf59_7fc7,
            0xc6e0_0bf3,
            0xd5a7_9147,
            0x06ca_6351,
            0x1429_2967,
            0x27b7_0a85,
            0x2e1b_2138,
            0x4d2c_6dfc,
            0x5338_0d13,
            0x650a_7354,
            0x766a_0abb,
            0x81c2_c92e,
            0x9272_2c85,
            0xa2bf_e8a1,
            0xa81a_664b,
            0xc24b_8b70,
            0xc76c_51a3,
            0xd192_e819,
            0xd699_0624,
            0xf40e_3585,
            0x106a_a070,
            0x19a4_c116,
            0x1e37_6c08,
            0x2748_774c,
            0x34b0_bcb5,
            0x391c_0cb3,
            0x4ed8_aa4a,
            0x5b9c_ca4f,
            0x682e_6ff3,
            0x748f_82ee,
            0x78a5_636f,
            0x84c8_7814,
            0x8cc7_0208,
            0x90be_fffa,
            0xa450_6ceb,
            0xbef9_a3f7,
            0xc671_78f2,
        ];
        let mut words = [0_u32; 64];
        for (index, chunk) in block.chunks_exact(4).enumerate().take(16) {
            words[index] = u32::from_be_bytes(chunk.try_into().expect("word is four bytes"));
        }
        for index in 16..64 {
            let sigma0 = words[index - 15].rotate_right(7)
                ^ words[index - 15].rotate_right(18)
                ^ (words[index - 15] >> 3);
            let sigma1 = words[index - 2].rotate_right(17)
                ^ words[index - 2].rotate_right(19)
                ^ (words[index - 2] >> 10);
            words[index] = words[index - 16]
                .wrapping_add(sigma0)
                .wrapping_add(words[index - 7])
                .wrapping_add(sigma1);
        }
        let [mut a, mut b, mut c, mut d, mut e, mut f, mut g, mut h] = self.state;
        for (constant, word) in K.into_iter().zip(words) {
            let sigma1 = e.rotate_right(6) ^ e.rotate_right(11) ^ e.rotate_right(25);
            let choice = (e & f) ^ ((!e) & g);
            let temporary1 = h
                .wrapping_add(sigma1)
                .wrapping_add(choice)
                .wrapping_add(constant)
                .wrapping_add(word);
            let sigma0 = a.rotate_right(2) ^ a.rotate_right(13) ^ a.rotate_right(22);
            let majority = (a & b) ^ (a & c) ^ (b & c);
            let temporary2 = sigma0.wrapping_add(majority);
            h = g;
            g = f;
            f = e;
            e = d.wrapping_add(temporary1);
            d = c;
            c = b;
            b = a;
            a = temporary1.wrapping_add(temporary2);
        }
        self.state[0] = self.state[0].wrapping_add(a);
        self.state[1] = self.state[1].wrapping_add(b);
        self.state[2] = self.state[2].wrapping_add(c);
        self.state[3] = self.state[3].wrapping_add(d);
        self.state[4] = self.state[4].wrapping_add(e);
        self.state[5] = self.state[5].wrapping_add(f);
        self.state[6] = self.state[6].wrapping_add(g);
        self.state[7] = self.state[7].wrapping_add(h);
    }
}

fn sha256_hex(bytes: &[u8]) -> String {
    let mut hasher = Sha256::default();
    hasher.update(bytes);
    hasher.finish_hex()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sha256_matches_standard_test_vector() {
        assert_eq!(
            sha256_hex(b""),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
        assert_eq!(
            sha256_hex(b"abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
        assert_eq!(
            sha256_hex(&vec![b'a'; 1_000]),
            "41edece42d63e8d9bf515a9ba6932e1c20cbc9f5a5d134645adb5db1b9737ea3"
        );
    }

    #[test]
    fn toolchain_parser_uses_only_required_fields() {
        let rustc = "rustc 1\nrelease: 1.97.1\ncommit-hash: rust\nhost: x86_64-pc-windows-msvc\nOS: ignored";
        let cargo = "cargo 1\nrelease: 1.97.1\ncommit-hash: cargo\nlibgit2: ignored";
        let first = parse_toolchain(rustc, cargo).unwrap();
        let second = parse_toolchain(
            &format!("{rustc}\nextra: changed"),
            &format!("{cargo}\nextra: changed"),
        )
        .unwrap();
        assert_eq!(
            canonical_json(&first).unwrap(),
            canonical_json(&second).unwrap()
        );
        assert_ne!(
            first.rustc_commit_hash,
            parse_toolchain(&rustc.replace("rust", "other"), cargo)
                .unwrap()
                .rustc_commit_hash
        );
    }

    #[test]
    fn sha256_is_independent_of_stream_chunk_boundaries() {
        let payload = vec![b'a'; 1_000];
        let expected = sha256_hex(&payload);
        let mut streamed = Sha256::default();
        for byte in &payload {
            streamed.update(std::slice::from_ref(byte));
        }
        assert_eq!(streamed.finish_hex(), expected);
    }

    #[test]
    fn inventory_root_and_bundle_identity_change_for_one_file_hash() {
        let before = vec![FileHash {
            path: "sdk/src/lib.rs".to_owned(),
            sha256: sha256_hex(b"before"),
        }];
        let after = vec![FileHash {
            path: "sdk/src/lib.rs".to_owned(),
            sha256: sha256_hex(b"after"),
        }];
        assert_ne!(inventory_hash(&before), inventory_hash(&after));
        let before_id =
            sha256_hex(&canonical_json(&serde_json::json!({"inventory": before})).unwrap());
        let after_id =
            sha256_hex(&canonical_json(&serde_json::json!({"inventory": after})).unwrap());
        assert_ne!(before_id, after_id);
    }

    #[test]
    fn bundle_identity_changes_for_a_toolchain_commit_change() {
        let before = serde_json::json!({
            "inventory_root_sha256": "fixed",
            "toolchain": {"rustc_vv": "commit-hash: abc", "cargo_vv": "commit-hash: def"},
        });
        let after = serde_json::json!({
            "inventory_root_sha256": "fixed",
            "toolchain": {"rustc_vv": "commit-hash: changed", "cargo_vv": "commit-hash: def"},
        });
        assert_ne!(
            bundle_id_from_identity(&before).unwrap(),
            bundle_id_from_identity(&after).unwrap()
        );
    }

    #[test]
    fn inventory_paths_are_portable() {
        let root = Path::new("D:/repo");
        assert_eq!(
            relative_path(root, Path::new("D:/repo/sdk/src/lib.rs")).unwrap(),
            "sdk/src/lib.rs"
        );
        assert!(relative_path(root, Path::new("D:/elsewhere/file")).is_err());
    }

    #[test]
    fn only_explicit_sdk_build_output_directories_are_excluded() {
        let root = Path::new("D:/repo");
        assert!(
            excluded_build_directory(root, Path::new("D:/repo/sdk/tools/tool/target")).unwrap()
        );
        assert!(
            excluded_build_directory(root, Path::new("D:/repo/vendor/gpui-ce/target")).unwrap()
        );
        assert!(
            !excluded_build_directory(
                root,
                Path::new("D:/repo/sdk/vendor/cargo-sources/cc/src/target")
            )
            .unwrap()
        );
        assert!(is_git_metadata(Path::new("D:/repo/vendor/gpui-ce/.git")));
        assert!(!is_git_metadata(Path::new(
            "D:/repo/vendor/gpui-ce/.gitignore"
        )));
    }
}
