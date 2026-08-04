use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    io::{Read, Write},
    path::{Path, PathBuf},
    process::Command,
};
use superexplorer_ui_abi_fingerprint::sha256_hex;
use toml::Value as TomlValue;

const MAX_PAYLOADS: usize = 128;
const MAX_PAYLOAD_BYTES: u64 = 64 * 1024 * 1024;
const MAX_TOTAL_PAYLOAD_BYTES: u64 = 512 * 1024 * 1024;
const MAX_MANIFEST_BYTES: u64 = 1024 * 1024;
// These mirror the production PackageManifestV1 and canonical ZIP importer.
const HOST_MAX_RUNTIME_PAYLOADS: usize = 128;
const HOST_MAX_RUNTIME_PATH_BYTES: usize = 1_024;
const HOST_MAX_RUNTIME_MANIFEST_BYTES: usize = 256 * 1024;
const HOST_MAX_CANONICAL_ZIP_BYTES: u64 = 512 * 1024 * 1024;
const CANONICAL_ZIP_LOCAL_HEADER_BYTES: u64 = 30;
const CANONICAL_ZIP_CENTRAL_HEADER_BYTES: u64 = 46;
const CANONICAL_ZIP_END_OF_CENTRAL_DIRECTORY_BYTES: u64 = 22;
const MAX_CARGO_FILE_BYTES: u64 = 4 * 1024 * 1024;
const ABI_STABLE_ROOT_MODULE_LOADER_EXPORT: &str = "_1as_0lib_1header_0root_bmodule_bloader";
const ROOT_MODULE_CONTRACT_NAMESPACE_V1: u32 = 0x5345_0001;
const ROOT_MODULE_CONTRACT_VALUE_V1: u64 = 1;
const PUBLIC_SDK_CONTRACT_DEPENDENCY: &str = "explorer-extension-api";
const PUBLIC_SDK_CONTRACT_VERSION: &str = "=1.2.0";
const PUBLIC_UI_SDK_CONTRACT_DEPENDENCY: &str = "explorer-extension-ui-api";
const PUBLIC_UI_SDK_CONTRACT_VERSION: &str = "=1.2.0";
const TRUSTED_CARGO_PATH_ENV: &str = "SUPEREXPLORER_TRUSTED_CARGO";
const TRUSTED_CARGO_SHA256_ENV: &str = "SUPEREXPLORER_TRUSTED_CARGO_SHA256";
const TRUSTED_RUSTC_PATH_ENV: &str = "SUPEREXPLORER_TRUSTED_RUSTC";
const TRUSTED_RUSTC_SHA256_ENV: &str = "SUPEREXPLORER_TRUSTED_RUSTC_SHA256";
const CRATES_IO_REGISTRY_SOURCE: &str = "registry+https://github.com/rust-lang/crates.io-index";

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Manifest {
    schema_version: u32,
    package: Package,
    publisher: Publisher,
    sdk: Sdk,
    rust: RustPlugin,
    features: Vec<Feature>,
    contributions: Vec<Contribution>,
    payloads: Vec<Payload>,
    #[serde(default)]
    private_dependencies: Vec<PrivateDependency>,
    verification: Verification,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Package {
    id: String,
    version: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Publisher {
    id: String,
    display_name: String,
    contacts: Vec<Contact>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Contact {
    kind: String,
    value: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Sdk {
    bundle_id: String,
    target: String,
    abi_schema: u32,
    gpui: bool,
    ui_abi_fingerprint: Option<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RustPlugin {
    crate_name: String,
    entrypoint: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Feature {
    id: String,
    capabilities: Vec<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Contribution {
    id: String,
    feature_id: String,
    kind: String,
    capabilities: Vec<String>,
    payload: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Payload {
    path: String,
    size: u64,
    sha256: String,
    kind: String,
}

#[derive(Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct PrivateDependency {
    name: String,
    version: String,
    path: String,
    tree_sha256: String,
    provenance: PrivateDependencyProvenance,
}

#[derive(Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct PrivateDependencyProvenance {
    source: String,
    crate_sha256: String,
    license_expression: String,
    license_hashes: BTreeMap<String, String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Verification {
    requirements: Vec<RequirementEvidence>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RequirementEvidence {
    requirement_id: String,
    evidence: Evidence,
}

#[derive(Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct Evidence {
    unit: Vec<String>,
    integration: Vec<String>,
    uitest: Vec<String>,
    security: Vec<String>,
    docs: Vec<String>,
}

#[derive(Clone, Debug, Serialize, Eq, Ord, PartialEq, PartialOrd)]
pub struct Diagnostic {
    pub code: String,
    pub severity: String,
    pub phase: String,
    pub path: String,
    pub message: String,
}

#[derive(Serialize)]
pub struct Report {
    pub schema_version: u32,
    pub valid: bool,
    pub diagnostics: Vec<Diagnostic>,
}

struct ExpectedSdk {
    repository_root: PathBuf,
    bundle_id: String,
    target: String,
    abi_schema: u32,
    rustc_release: String,
    rustc_commit_hash: String,
    rustc_sha256: String,
    cargo_release: String,
    cargo_commit_hash: String,
    cargo_sha256: String,
    ui_abi_fingerprint: String,
    gpui_repository: String,
    gpui_revision: String,
    gpui_packages: BTreeSet<String>,
    protected_graph: BTreeMap<String, ProtectedPackage>,
    release_profile: ReleaseProfilePolicy,
    gates: BTreeMap<String, Evidence>,
}

#[derive(Clone)]
struct ReleaseProfilePolicy {
    panic: String,
    lto: String,
    codegen_units: i64,
    strip: Option<Value>,
    overflow_checks: Option<bool>,
}

#[derive(Clone, Deserialize)]
struct ProtectedPackage {
    key: String,
    name: String,
    version: String,
    source: Option<String>,
    path: Option<String>,
    checksum: Option<String>,
    #[serde(default)]
    features: Vec<String>,
    #[serde(default)]
    dependencies: Vec<ProtectedDependency>,
}

#[derive(Clone, Deserialize)]
struct ProtectedDependency {
    name: String,
    to: String,
    #[serde(default)]
    dep_kinds: Vec<ProtectedDependencyKind>,
}

#[derive(Clone, Deserialize)]
struct ProtectedDependencyKind {
    kind: String,
    target: Option<String>,
}

#[derive(Clone, Deserialize)]
struct CargoMetadata {
    packages: Vec<CargoMetadataPackage>,
    resolve: Option<CargoMetadataResolve>,
}

#[derive(Clone, Deserialize)]
struct CargoMetadataPackage {
    id: String,
    name: String,
    version: String,
    source: Option<String>,
    manifest_path: String,
}

#[derive(Clone, Deserialize)]
struct CargoMetadataResolve {
    root: Option<String>,
    nodes: Vec<CargoMetadataNode>,
}

#[derive(Clone, Deserialize)]
struct CargoMetadataNode {
    id: String,
    #[serde(default)]
    deps: Vec<CargoMetadataDependency>,
    #[serde(default)]
    features: Vec<String>,
}

#[derive(Clone, Deserialize)]
struct CargoMetadataDependency {
    name: String,
    pkg: String,
    #[serde(default)]
    dep_kinds: Vec<CargoMetadataDependencyKind>,
}

#[derive(Clone, Deserialize)]
struct CargoMetadataDependencyKind {
    kind: Option<String>,
    target: Option<String>,
}

#[derive(Clone)]
struct CargoLockPackage {
    name: String,
    version: String,
    source: Option<String>,
    checksum: Option<String>,
}

#[derive(Clone)]
struct InputIdentity {
    relative: PathBuf,
    size: u64,
    sha256: String,
}

struct StagedPayload {
    path: String,
    kind: &'static str,
    bytes: Vec<u8>,
}

/// Hashes emitted when the folder-size example template is materialized inside
/// an SDK-owned private snapshot. The template itself is never rewritten.
#[derive(Serialize)]
pub struct TemplateMaterializationReport {
    pub template_manifest_sha256: String,
    pub resolved_manifest_sha256: String,
    pub materialized: bool,
}

/// Validates the canonical P0 Rust consumer manifest and all declared payloads.
#[must_use]
pub fn validate(root: &Path) -> Report {
    match validate_inner(root) {
        Ok(mut diagnostics) => {
            redact_plugin_paths(&mut diagnostics, root);
            diagnostics.sort();
            diagnostics.dedup();
            Report {
                schema_version: 1,
                valid: diagnostics.is_empty(),
                diagnostics,
            }
        }
        Err(message) => Report {
            schema_version: 1,
            valid: false,
            diagnostics: vec![diagnostic(
                "SESDK-INPUT-001",
                "input",
                "plugin-project.json",
                redact_absolute_paths(&message),
            )],
        },
    }
}

/// Resolves the *only* author-template placeholders currently supported by the
/// 0→1 SDK: the folder-size visual-column example. Callers must pass a private
/// no-reparse snapshot, never the author's live project directory.
///
/// A static manifest is left untouched and reports identical input/output
/// hashes. Any template other than the exact folder-size template is rejected;
/// this intentionally is not a general templating language.
///
/// # Errors
///
/// Returns an error if the snapshot or its template/source inputs are unsafe,
/// the template is not the canonical folder-size one, or resolution would
/// leave a placeholder behind.
pub fn materialize_folder_size_template(
    root: &Path,
    bundle_id: &str,
    abi_schema: u32,
) -> Result<TemplateMaterializationReport, String> {
    if !valid_id(bundle_id) || abi_schema == 0 {
        return Err("SDK bundle ID or ABI schema is invalid".into());
    }
    let canonical_root = canonical_plugin_root(root)?;
    let manifest_path = canonical_root.join("plugin-project.json");
    let template = read_regular_utf8_file(
        &canonical_root,
        &manifest_path,
        MAX_MANIFEST_BYTES,
        "plugin-project.json",
    )?;
    const TOKENS: [&str; 4] = [
        "@SDK_BUNDLE_ID@",
        "@ABI_SCHEMA@",
        "@SOURCE_SIZE@",
        "@SOURCE_SHA256@",
    ];
    let template_manifest_sha256 = sha256_hex(template.as_bytes());
    if !TOKENS.iter().any(|token| template.contains(token)) {
        return Ok(TemplateMaterializationReport {
            template_manifest_sha256: template_manifest_sha256.clone(),
            resolved_manifest_sha256: template_manifest_sha256,
            materialized: false,
        });
    }
    let is_visual_column = template.contains("\"id\": \"rust-folder-size-visual-column\"");
    let is_size_map = template.contains("\"id\": \"rust-folder-size-map-view\"");
    if is_visual_column == is_size_map
        || TOKENS
            .iter()
            .any(|token| template.matches(token).count() != 1)
    {
        return Err(
            "only an exact supported folder-size example template may contain placeholders"
                .into(),
        );
    }
    let template_without_tokens = TOKENS
        .iter()
        .fold(template.clone(), |text, token| text.replace(token, ""));
    if !template_without_tokens.contains("\"value\": \"support@example.invalid\"")
        || template_without_tokens.matches('@').count() != 1
    {
        return Err("folder-size template contains an unsupported placeholder".into());
    }
    let source = read_regular_bytes(
        &canonical_root,
        &canonical_root.join("src/lib.rs"),
        MAX_PAYLOAD_BYTES,
    )?;
    let resolved = template
        .replace("@SDK_BUNDLE_ID@", bundle_id)
        .replace("@ABI_SCHEMA@", &abi_schema.to_string())
        .replace("@SOURCE_SIZE@", &source.len().to_string())
        .replace("@SOURCE_SHA256@", &sha256_hex(&source));
    if TOKENS.iter().any(|token| resolved.contains(token)) {
        return Err("folder-size template contains an unsupported placeholder".into());
    }
    let manifest: Manifest = serde_json::from_str(&resolved)
        .map_err(|error| format!("resolved folder-size manifest is invalid: {error}"))?;
    let declarations_are_exact = match manifest.package.id.as_str() {
        "rust-folder-size-visual-column" => {
            is_visual_column && is_exact_folder_size_declarations(&manifest)
        }
        "rust-folder-size-map-view" => is_size_map && is_exact_size_map_declarations(&manifest),
        _ => false,
    };
    if !declarations_are_exact
        || manifest.sdk.bundle_id != bundle_id
        || manifest.sdk.abi_schema != abi_schema
    {
        return Err(
            "resolved folder-size example declarations are not the approved 0→1 set".into(),
        );
    }
    ensure_regular_project_path(&canonical_root, &manifest_path)?;
    fs::write(&manifest_path, resolved.as_bytes())
        .map_err(|error| format!("could not write resolved private manifest: {error}"))?;
    let resolved_manifest_sha256 = sha256_hex(resolved.as_bytes());
    Ok(TemplateMaterializationReport {
        template_manifest_sha256,
        resolved_manifest_sha256,
        materialized: true,
    })
}

fn is_exact_folder_size_declarations(manifest: &Manifest) -> bool {
    let expected = [
        ("abi-root", "abi-root", &["abi"][..]),
        ("folder-size", "column", &["abi", "filesystem.read"][..]),
        ("folder-size-renderer", "renderer", &["abi"][..]),
    ];
    let expected_features = ["column", "recalculate", "settings"];
    manifest.features.len() == expected_features.len()
        && manifest
            .features
            .iter()
            .zip(expected_features)
            .all(|(actual, id)| {
                actual.id == id
                    && actual.capabilities
                        == if id == "column" {
                            &["abi", "filesystem.read"][..]
                        } else {
                            &["abi"][..]
                        }
            })
        && manifest.contributions.len() == expected.len()
        && manifest
            .contributions
            .iter()
            .zip(expected)
            .all(|(actual, (id, kind, capabilities))| {
                actual.id == id
                    && actual.kind == kind
                    && actual.feature_id == "column"
                    && actual.capabilities == capabilities
                    && actual.payload == "src/lib.rs"
            })
}

fn is_exact_size_map_declarations(manifest: &Manifest) -> bool {
    manifest.features.len() == 1
        && manifest.features[0].id == "view"
        && manifest.features[0].capabilities == ["abi", "filesystem.read"]
        && manifest.contributions.len() == 2
        && manifest.contributions[0].id == "abi-root"
        && manifest.contributions[0].kind == "abi-root"
        && manifest.contributions[0].feature_id == "view"
        && manifest.contributions[0].capabilities == ["abi"]
        && manifest.contributions[0].payload == "src/lib.rs"
        && manifest.contributions[1].id == "size-map"
        && manifest.contributions[1].kind == "view-mode"
        && manifest.contributions[1].feature_id == "view"
        && manifest.contributions[1].capabilities == ["abi"]
        && manifest.contributions[1].payload == "src/lib.rs"
}

/// Synthesizes the canonical host `manifest.json` for a P0 local-developer
/// `.sepack`. The archive deliberately contains only its runtime DLL; source
/// payloads are validation/build inputs and are never runtime package content.
///
/// The returned JSON is structurally compatible with `PackageManifestV1` and
/// is intentionally unsigned. A caller must place it in a package imported
/// through the host's local-developer provenance boundary.
///
/// # Errors
///
/// Returns an error when the project manifest, publisher email mapping, or
/// runtime DLL cannot be safely read and bound to the generated manifest.
pub fn synthesize_package_manifest(root: &Path, dll: &Path) -> Result<String, String> {
    if is_link_or_reparse(root)? {
        return Err("plugin root is a symlink or reparse point".into());
    }
    let canonical_root = root
        .canonicalize()
        .map_err(|error| format!("plugin root cannot be canonicalized: {error}"))?;
    if !canonical_root.is_dir() || is_link_or_reparse(&canonical_root)? {
        return Err("plugin root is not a regular directory".into());
    }
    let manifest_path = canonical_root.join("plugin-project.json");
    let source = read_regular_utf8_file(
        &canonical_root,
        &manifest_path,
        MAX_MANIFEST_BYTES,
        "plugin-project.json",
    )?;
    let manifest: Manifest = serde_json::from_str(&source).map_err(|error| {
        format!("plugin-project.json does not match the exact P0 schema: {error}")
    })?;
    validate_synthesis_manifest(&manifest)?;
    let dll_bytes = read_regular_bytes(&canonical_root, dll, MAX_PAYLOAD_BYTES)?;
    package_manifest_json(
        &manifest,
        &[StagedPayload {
            path: "plugin/plugin.dll".into(),
            kind: "rust_dll",
            bytes: dll_bytes,
        }],
    )
}

fn package_manifest_json(
    manifest: &Manifest,
    payloads: &[StagedPayload],
) -> Result<String, String> {
    let contacts = manifest
        .publisher
        .contacts
        .iter()
        .map(|contact| {
            if !matches!(contact.kind.as_str(), "support" | "security")
                || !canonical_email(&contact.value)
            {
                return Err(format!(
                    "publisher contact {:?} must be an unambiguous plain email for P0 package synthesis",
                    contact.value
                ));
            }
            Ok(json!({
                "kind": "email",
                "value": contact.value,
                "purposes": [contact.kind],
            }))
        })
        .collect::<Result<Vec<_>, _>>()?;
    let features = manifest
        .features
        .iter()
        .map(|feature| {
            json!({
                "id": feature.id,
                "capabilities": feature.capabilities,
                "dependencies": [],
            })
        })
        .collect::<Vec<_>>();
    let payloads = payloads
        .iter()
        .map(|payload| {
            json!({
                "path": payload.path,
                "size": payload.bytes.len(),
                "sha256": sha256_hex(&payload.bytes),
                "kind": payload.kind,
            })
        })
        .collect::<Vec<_>>();
    let package_manifest = json!({
        "manifest_version": 1,
        "package": { "id": manifest.package.id, "version": manifest.package.version },
        "publisher": {
            "id": manifest.publisher.id,
            "display_name": manifest.publisher.display_name,
            "contacts": contacts,
        },
        "sdk": {
            "bundle_id": manifest.sdk.bundle_id,
            "target": manifest.sdk.target,
            "abi_schema": manifest.sdk.abi_schema,
            "gpui": manifest.sdk.gpui,
            "ui_abi_fingerprint": manifest.sdk.ui_abi_fingerprint,
        },
        "rust": [{
            "id": manifest.rust.crate_name,
            "entrypoint": "plugin/plugin.dll",
            // This is a fixed mirror of ROOT_MODULE_CONTRACT_ID_V1. The
            // production loader compares it to root binary data before it
            // constructs a registrar; no author-provided source symbol is used.
            "root_contract_id": {
                "namespace": ROOT_MODULE_CONTRACT_NAMESPACE_V1,
                "value": ROOT_MODULE_CONTRACT_VALUE_V1,
            },
            "sdk_major": 1,
        }],
        "lua": [],
        "skins": [],
        "locales": [],
        "tools": [],
        "features": features,
        "dependencies": [],
        "payloads": payloads,
        "signature": { "kind": "unsigned" },
        "data_version": 1,
    });
    serde_json::to_string(&package_manifest)
        .map_err(|error| format!("could not serialize canonical package manifest: {error}"))
}

/// Stages one canonical local-developer package directory from a fully validated
/// plugin project. The newly-created output directory contains only package
/// runtime payloads and provenance notices, never private Rust source trees.
///
/// # Errors
///
/// Returns an error if project validation fails, an input/output path is unsafe,
/// private provenance cannot be copied exactly, or the private output directory
/// cannot be cleaned after a failed stage.
pub fn stage_package(root: &Path, dll: &Path, output: &Path) -> Result<(), String> {
    let report = validate(root);
    if !report.valid {
        let codes = report
            .diagnostics
            .iter()
            .map(|diagnostic| diagnostic.code.as_str())
            .collect::<Vec<_>>()
            .join(", ");
        return Err(format!(
            "plugin validation failed before package staging: {codes}"
        ));
    }
    let canonical_root = canonical_plugin_root(root)?;
    let manifest_source = read_regular_utf8_file(
        &canonical_root,
        &canonical_root.join("plugin-project.json"),
        MAX_MANIFEST_BYTES,
        "plugin-project.json",
    )?;
    let manifest: Manifest = serde_json::from_str(&manifest_source).map_err(|error| {
        format!("plugin-project.json does not match the exact P0 schema: {error}")
    })?;
    validate_synthesis_manifest(&manifest)?;
    let canonical_dll = dll
        .canonicalize()
        .map_err(|error| format!("plugin DLL cannot be canonicalized: {error}"))?;
    if !canonical_dll.starts_with(&canonical_root) {
        return Err("plugin DLL resolves outside the plugin root".into());
    }
    stage_validated_package(&canonical_root, &manifest, &canonical_dll, output)
}

fn stage_validated_package(
    root: &Path,
    manifest: &Manifest,
    dll: &Path,
    output: &Path,
) -> Result<(), String> {
    validate_synthesis_manifest(manifest)?;
    let mut private_diagnostics = Vec::new();
    validate_private_dependencies(
        root,
        &manifest.private_dependencies,
        &mut private_diagnostics,
    );
    if !private_diagnostics.is_empty() {
        return Err("private dependency provenance changed before package staging".into());
    }
    let mut payloads = vec![StagedPayload {
        path: "plugin/plugin.dll".into(),
        kind: "rust_dll",
        bytes: read_regular_bytes(root, dll, MAX_PAYLOAD_BYTES)?,
    }];
    let mut private_dependencies = manifest.private_dependencies.iter().collect::<Vec<_>>();
    private_dependencies.sort_by(|left, right| left.name.cmp(&right.name));
    let mut notice_dependencies = Vec::new();
    for dependency in private_dependencies {
        let mut licenses = Vec::new();
        for (license_path, hash) in &dependency.provenance.license_hashes {
            let package_path = format!(
                "licenses/private/{}-{}/{}",
                dependency.name, dependency.version, license_path
            );
            let bytes = read_regular_bytes(
                root,
                &root.join(&dependency.path).join(license_path),
                MAX_PAYLOAD_BYTES,
            )?;
            if sha256_hex(&bytes) != *hash {
                return Err(format!(
                    "private dependency license changed after validation: {}:{}",
                    dependency.name, license_path
                ));
            }
            payloads.push(StagedPayload {
                path: package_path.clone(),
                kind: "license",
                bytes,
            });
            licenses.push(json!({
                "source_path": license_path,
                "package_path": package_path,
                "sha256": hash,
            }));
        }
        notice_dependencies.push(json!({
            "name": dependency.name,
            "version": dependency.version,
            "vendor_path": dependency.path,
            "tree_sha256": dependency.tree_sha256,
            "source": dependency.provenance.source,
            "crate_sha256": dependency.provenance.crate_sha256,
            "license_expression": dependency.provenance.license_expression,
            "licenses": licenses,
        }));
    }
    if !notice_dependencies.is_empty() {
        payloads.push(StagedPayload {
            path: "notices/private-dependencies.json".into(),
            kind: "notice",
            bytes: serde_json::to_vec(&json!({
                "schema_version": 1,
                "private_dependencies": notice_dependencies,
            }))
            .map_err(|error| format!("could not serialize private dependency notice: {error}"))?,
        });
    }
    payloads.sort_by(|left, right| left.path.cmp(&right.path));
    if payloads
        .iter()
        .any(|payload| !safe_relative_path(&payload.path))
        || payloads
            .windows(2)
            .any(|pair| pair[0].path.eq_ignore_ascii_case(&pair[1].path))
    {
        return Err("canonical package payload paths are unsafe or collide".into());
    }
    let manifest_json = package_manifest_json(manifest, &payloads)?;
    validate_host_runtime_package_bounds(&payloads, &manifest_json)?;
    let output = create_private_stage_directory(output)?;
    let result = (|| {
        for payload in &payloads {
            write_new_stage_file(&output, &payload.path, &payload.bytes)?;
        }
        write_new_stage_file(&output, "manifest.json", manifest_json.as_bytes())
    })();
    if let Err(error) = result {
        return match remove_private_stage_directory(&output) {
            Ok(()) => Err(error),
            Err(cleanup) => Err(format!("{error}; private stage cleanup failed: {cleanup}")),
        };
    }
    Ok(())
}

/// Checks the exact limits the production PackageManifestV1 parser and
/// store-only ZIP importer apply. The ZIP calculation intentionally includes
/// local headers, central-directory headers, names, and the EOCD record.
fn validate_host_runtime_package_bounds(
    payloads: &[StagedPayload],
    manifest_json: &str,
) -> Result<(), String> {
    if payloads.len() > HOST_MAX_RUNTIME_PAYLOADS {
        return Err(format!(
            "runtime package has {} payloads; host accepts at most {HOST_MAX_RUNTIME_PAYLOADS}",
            payloads.len()
        ));
    }
    if manifest_json.len() > HOST_MAX_RUNTIME_MANIFEST_BYTES {
        return Err(format!(
            "runtime manifest is {} bytes; host accepts at most {HOST_MAX_RUNTIME_MANIFEST_BYTES}",
            manifest_json.len()
        ));
    }
    let mut archive_bytes = CANONICAL_ZIP_END_OF_CENTRAL_DIRECTORY_BYTES;
    for payload in payloads {
        archive_bytes =
            canonical_store_zip_entry_size(archive_bytes, &payload.path, payload.bytes.len())?;
    }
    archive_bytes =
        canonical_store_zip_entry_size(archive_bytes, "manifest.json", manifest_json.len())?;
    validate_host_canonical_zip_size(archive_bytes)
}

fn validate_host_canonical_zip_size(archive_bytes: u64) -> Result<(), String> {
    if archive_bytes > HOST_MAX_CANONICAL_ZIP_BYTES {
        return Err(format!(
            "canonical runtime ZIP is {archive_bytes} bytes; host accepts at most {HOST_MAX_CANONICAL_ZIP_BYTES}"
        ));
    }
    Ok(())
}

fn canonical_store_zip_entry_size(
    current: u64,
    path: &str,
    content_length: usize,
) -> Result<u64, String> {
    let path_length = path.len();
    if path_length > HOST_MAX_RUNTIME_PATH_BYTES {
        return Err(format!(
            "runtime package path exceeds the host {HOST_MAX_RUNTIME_PATH_BYTES}-byte limit: {path}"
        ));
    }
    let path_length =
        u64::try_from(path_length).map_err(|_| "runtime package path is oversized")?;
    let content_length =
        u64::try_from(content_length).map_err(|_| "runtime package payload is oversized")?;
    current
        .checked_add(CANONICAL_ZIP_LOCAL_HEADER_BYTES)
        .and_then(|size| size.checked_add(path_length))
        .and_then(|size| size.checked_add(content_length))
        .and_then(|size| size.checked_add(CANONICAL_ZIP_CENTRAL_HEADER_BYTES))
        .and_then(|size| size.checked_add(path_length))
        .ok_or_else(|| "canonical runtime ZIP size overflowed".into())
}

fn canonical_plugin_root(root: &Path) -> Result<PathBuf, String> {
    if is_link_or_reparse(root)? {
        return Err("plugin root is a symlink or reparse point".into());
    }
    let canonical = root
        .canonicalize()
        .map_err(|error| format!("plugin root cannot be canonicalized: {error}"))?;
    if !fs::metadata(&canonical).is_ok_and(|metadata| metadata.is_dir())
        || is_link_or_reparse(&canonical)?
    {
        return Err("plugin root is not a regular directory".into());
    }
    Ok(canonical)
}

fn create_private_stage_directory(output: &Path) -> Result<PathBuf, String> {
    if !output.is_absolute() || output.file_name().is_none() {
        return Err("package stage output must be an absolute directory leaf".into());
    }
    let parent = output
        .parent()
        .ok_or("package stage output has no parent directory")?;
    ensure_regular_directory_ancestors(parent)?;
    match fs::create_dir(output) {
        Ok(()) => {}
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
            return Err("package stage output directory must be private and newly created".into());
        }
        Err(error) => {
            return Err(format!(
                "could not create private package stage directory: {error}"
            ));
        }
    }
    if is_link_or_reparse(output)? || !fs::metadata(output).is_ok_and(|metadata| metadata.is_dir())
    {
        let _ = fs::remove_dir(output);
        return Err("package stage output directory is unsafe".into());
    }
    output
        .canonicalize()
        .map_err(|error| format!("package stage output cannot be canonicalized: {error}"))
}

fn ensure_regular_directory_ancestors(path: &Path) -> Result<(), String> {
    let mut ancestors = path.ancestors().collect::<Vec<_>>();
    ancestors.reverse();
    for ancestor in ancestors {
        if is_link_or_reparse(ancestor)?
            || !fs::metadata(ancestor).is_ok_and(|metadata| metadata.is_dir())
        {
            return Err(format!(
                "package stage ancestor is not a regular directory: {}",
                ancestor.display()
            ));
        }
    }
    Ok(())
}

fn write_new_stage_file(root: &Path, relative: &str, bytes: &[u8]) -> Result<(), String> {
    if !safe_relative_path(relative) {
        return Err("package stage path is unsafe".into());
    }
    let path = root.join(relative);
    let parent = path.parent().ok_or("package stage payload has no parent")?;
    let relative_parent = parent
        .strip_prefix(root)
        .map_err(|_| "package stage payload escapes its root")?;
    let mut current = root.to_owned();
    for component in relative_parent.components() {
        current.push(component);
        match fs::symlink_metadata(&current) {
            Ok(metadata) => {
                if metadata.file_type().is_symlink()
                    || !fs::metadata(&current).is_ok_and(|value| value.is_dir())
                    || is_link_or_reparse(&current)?
                {
                    return Err("package stage parent is unsafe".into());
                }
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                fs::create_dir(&current).map_err(|create| {
                    format!("could not create package stage directory: {create}")
                })?;
            }
            Err(error) => return Err(format!("package stage parent cannot be inspected: {error}")),
        }
    }
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&path)
        .map_err(|error| format!("could not create package stage file: {error}"))?;
    file.write_all(bytes)
        .map_err(|error| format!("could not write package stage file: {error}"))?;
    file.flush()
        .map_err(|error| format!("could not flush package stage file: {error}"))?;
    ensure_regular_project_path(root, &path)
}

fn remove_private_stage_directory(path: &Path) -> Result<(), String> {
    if is_link_or_reparse(path)? || !fs::metadata(path).is_ok_and(|metadata| metadata.is_dir()) {
        return Err("private stage root became unsafe".into());
    }
    for entry in
        fs::read_dir(path).map_err(|error| format!("cannot read private stage: {error}"))?
    {
        let entry = entry.map_err(|error| format!("cannot read private stage entry: {error}"))?;
        let child = entry.path();
        let metadata = fs::symlink_metadata(&child)
            .map_err(|error| format!("cannot inspect private stage entry: {error}"))?;
        if metadata.file_type().is_symlink() || is_link_or_reparse(&child)? {
            return Err("private stage contains a symlink or reparse point".into());
        }
        if fs::metadata(&child).is_ok_and(|value| value.is_dir()) {
            remove_private_stage_directory(&child)?;
        } else if fs::metadata(&child).is_ok_and(|value| value.is_file()) {
            fs::remove_file(&child)
                .map_err(|error| format!("cannot remove private stage file: {error}"))?;
        } else {
            return Err("private stage contains a non-regular entry".into());
        }
    }
    fs::remove_dir(path).map_err(|error| format!("cannot remove private stage directory: {error}"))
}

fn validate_synthesis_manifest(manifest: &Manifest) -> Result<(), String> {
    if manifest.schema_version != 1
        || !valid_id(&manifest.package.id)
        || !valid_version(&manifest.package.version)
        || !valid_id(&manifest.publisher.id)
        || manifest.publisher.display_name.trim().is_empty()
        || !valid_id(&manifest.rust.crate_name)
        || manifest.rust.entrypoint != "plugin.dll"
    {
        return Err(
            "project manifest cannot be represented as the canonical P0 package manifest".into(),
        );
    }
    if manifest.publisher.contacts.is_empty()
        || manifest
            .payloads
            .iter()
            .any(|payload| payload.kind != "rust-source")
        || manifest.features.iter().any(|feature| {
            !valid_id(&feature.id)
                || has_duplicates(&feature.capabilities)
                || feature
                    .capabilities
                    .iter()
                    .any(|capability| !valid_id(capability))
        })
        || has_duplicates(
            &manifest
                .features
                .iter()
                .map(|feature| feature.id.clone())
                .collect::<Vec<_>>(),
        )
    {
        return Err(
            "project manifest has unsupported runtime payloads, invalid publisher contacts, or invalid features"
                .into(),
        );
    }
    Ok(())
}

fn canonical_email(value: &str) -> bool {
    if !value.is_ascii()
        || value.trim() != value
        || value
            .bytes()
            .any(|byte| byte.is_ascii_control() || byte.is_ascii_whitespace())
        || value.matches('@').count() != 1
    {
        return false;
    }
    let Some((local, domain)) = value.split_once('@') else {
        return false;
    };
    (1..=64).contains(&local.len())
        && !local.starts_with('.')
        && !local.ends_with('.')
        && !local.contains("..")
        && local.bytes().all(|byte| {
            byte.is_ascii_alphanumeric()
                || matches!(
                    byte,
                    b'.' | b'!'
                        | b'#'
                        | b'$'
                        | b'%'
                        | b'&'
                        | b'\''
                        | b'*'
                        | b'+'
                        | b'-'
                        | b'/'
                        | b'='
                        | b'?'
                        | b'^'
                        | b'_'
                        | b'`'
                        | b'{'
                        | b'|'
                        | b'}'
                        | b'~'
                )
        })
        && canonical_email_domain(domain)
}

fn canonical_email_domain(value: &str) -> bool {
    (1..=253).contains(&value.len())
        && value.contains('.')
        && value.split('.').all(|label| {
            !label.is_empty()
                && label.len() <= 63
                && !label.starts_with('-')
                && !label.ends_with('-')
                && label
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
        })
}

/// Inspects a DLL's PE headers and exports without loading it into the validator process.
#[must_use]
pub fn inspect_dll(path: &Path) -> Report {
    let result = read_bounded_dll(path).and_then(|bytes| {
        inspect_pe_exports(&bytes, ABI_STABLE_ROOT_MODULE_LOADER_EXPORT)
            .map_err(std::io::Error::other)
    });
    match result {
        Ok(()) => Report {
            schema_version: 1,
            valid: true,
            diagnostics: Vec::new(),
        },
        Err(error) => Report {
            schema_version: 1,
            valid: false,
            diagnostics: vec![diagnostic(
                "SESDK-DLL-001",
                "binary",
                "plugin.dll",
                format!("DLL inspection failed: {error}"),
            )],
        },
    }
}

fn read_bounded_dll(path: &Path) -> std::io::Result<Vec<u8>> {
    let link_metadata = fs::symlink_metadata(path)?;
    if link_metadata.file_type().is_symlink()
        || is_link_or_reparse(path).map_err(std::io::Error::other)?
    {
        return Err(std::io::Error::other("DLL is a symlink or reparse point"));
    }
    let file = fs::File::open(path)?;
    let metadata = file.metadata()?;
    if !metadata.is_file() {
        return Err(std::io::Error::other("DLL is not a regular file"));
    }
    if metadata.len() > MAX_PAYLOAD_BYTES {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            format!("DLL exceeds the {MAX_PAYLOAD_BYTES}-byte limit"),
        ));
    }
    let mut bytes = Vec::with_capacity(usize::try_from(metadata.len()).unwrap_or(0));
    file.take(MAX_PAYLOAD_BYTES + 1).read_to_end(&mut bytes)?;
    if u64::try_from(bytes.len()).unwrap_or(u64::MAX) > MAX_PAYLOAD_BYTES {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            format!("DLL exceeds the {MAX_PAYLOAD_BYTES}-byte limit"),
        ));
    }
    Ok(bytes)
}

fn inspect_pe_exports(bytes: &[u8], required_root_export: &str) -> Result<(), String> {
    if bytes.get(..2) != Some(b"MZ") {
        return Err("not a DOS MZ executable".into());
    }
    let pe_offset = usize::try_from(read_u32(bytes, 0x3c)?).map_err(|_| "invalid PE offset")?;
    if bytes.get(pe_offset..pe_offset + 4) != Some(b"PE\0\0") {
        return Err("missing PE signature".into());
    }
    let coff = pe_offset + 4;
    if read_u16(bytes, coff)? != 0x8664 {
        return Err("PE machine is not x86_64".into());
    }
    let optional_size = usize::from(read_u16(bytes, coff + 16)?);
    let optional = coff + 20;
    if read_u16(bytes, optional)? != 0x20b {
        return Err("PE optional header is not PE32+".into());
    }
    let export_rva = read_u32(bytes, optional + 112)?;
    if export_rva == 0 {
        return Err("DLL has no export directory".into());
    }
    let sections = pe_sections(bytes, coff, optional_size)?;
    let export = rva_offset(export_rva, &sections)?;
    let number_of_names =
        usize::try_from(read_u32(bytes, export + 24)?).map_err(|_| "invalid export name count")?;
    if number_of_names > 16_384 {
        return Err("export name count exceeds inspection bound".into());
    }
    let names = rva_offset(read_u32(bytes, export + 32)?, &sections)?;
    for index in 0..number_of_names {
        let name_rva = read_u32(bytes, names + index * 4)?;
        let name = read_c_string(bytes, rva_offset(name_rva, &sections)?)?;
        if name == required_root_export {
            return Ok(());
        }
    }
    Err(format!(
        "required abi_stable root-module loader export {required_root_export:?} is absent"
    ))
}

#[derive(Clone, Copy)]
struct PeSection {
    virtual_address: u32,
    virtual_size: u32,
    raw_offset: u32,
    raw_size: u32,
}

fn pe_sections(bytes: &[u8], coff: usize, optional_size: usize) -> Result<Vec<PeSection>, String> {
    let count = usize::from(read_u16(bytes, coff + 2)?);
    if count == 0 || count > 96 {
        return Err("invalid PE section count".into());
    }
    let start = coff
        .checked_add(20)
        .and_then(|offset| offset.checked_add(optional_size))
        .ok_or("PE section table overflow")?;
    (0..count)
        .map(|index| {
            let offset = start
                .checked_add(index.checked_mul(40).ok_or("PE section table overflow")?)
                .ok_or("PE section table overflow")?;
            Ok(PeSection {
                virtual_size: read_u32(bytes, offset + 8)?,
                virtual_address: read_u32(bytes, offset + 12)?,
                raw_size: read_u32(bytes, offset + 16)?,
                raw_offset: read_u32(bytes, offset + 20)?,
            })
        })
        .collect()
}

fn rva_offset(rva: u32, sections: &[PeSection]) -> Result<usize, String> {
    let section = sections
        .iter()
        .find(|section| {
            let size = section.virtual_size.max(section.raw_size);
            rva >= section.virtual_address && rva - section.virtual_address < size
        })
        .ok_or("RVA is not contained in a PE section")?;
    usize::try_from(section.raw_offset + (rva - section.virtual_address))
        .map_err(|_| "RVA offset is invalid".into())
}

fn read_u16(bytes: &[u8], offset: usize) -> Result<u16, String> {
    let value = bytes.get(offset..offset + 2).ok_or("truncated PE header")?;
    Ok(u16::from_le_bytes([value[0], value[1]]))
}

fn read_u32(bytes: &[u8], offset: usize) -> Result<u32, String> {
    let value = bytes.get(offset..offset + 4).ok_or("truncated PE header")?;
    Ok(u32::from_le_bytes([value[0], value[1], value[2], value[3]]))
}

fn read_c_string(bytes: &[u8], offset: usize) -> Result<&str, String> {
    let rest = bytes
        .get(offset..)
        .ok_or("export string points outside DLL")?;
    let length = rest
        .iter()
        .take(1025)
        .position(|byte| *byte == 0)
        .ok_or("unterminated or oversized export name")?;
    std::str::from_utf8(&rest[..length]).map_err(|_| "export name is not ASCII/UTF-8".into())
}

fn validate_inner(root: &Path) -> Result<Vec<Diagnostic>, String> {
    if is_link_or_reparse(root)? {
        return Err("plugin root is a symlink or reparse point".into());
    }
    let canonical_root = root
        .canonicalize()
        .map_err(|error| format!("plugin root is unavailable: {error}"))?;
    if !canonical_root.is_dir() {
        return Err("plugin root is not a directory".into());
    }
    ensure_no_consumer_cargo_config(&canonical_root)?;
    let manifest_path = canonical_root.join("plugin-project.json");
    let source = read_regular_utf8_file(
        &canonical_root,
        &manifest_path,
        MAX_MANIFEST_BYTES,
        "plugin-project.json",
    )?;
    let manifest: Manifest = serde_json::from_str(&source).map_err(|error| {
        format!("plugin-project.json does not match the exact P0 schema: {error}")
    })?;
    let input_identities = capture_input_identities(&canonical_root, &manifest, &source)?;
    let expected = expected_sdk()?;
    let mut diagnostics = validate_manifest(&manifest, &canonical_root, &expected);
    if let Err(message) = verify_input_identities(&canonical_root, &input_identities) {
        diagnostics.push(diagnostic(
            "SESDK-TOCTOU-001",
            "input",
            "plugin-project.json",
            message,
        ));
    }
    if canonical_root
        != root
            .canonicalize()
            .map_err(|error| format!("plugin root changed during validation: {error}"))?
    {
        diagnostics.push(diagnostic(
            "SESDK-TOCTOU-002",
            "input",
            "plugin-project.json",
            "plugin root identity changed during validation",
        ));
    }
    Ok(diagnostics)
}

fn ensure_no_consumer_cargo_config(root: &Path) -> Result<(), String> {
    let cargo_directory = root.join(".cargo");
    if cargo_directory.exists() && is_link_or_reparse(&cargo_directory)? {
        return Err("consumer .cargo directory is a symlink or reparse point".into());
    }
    for relative in [".cargo/config", ".cargo/config.toml"] {
        let path = root.join(relative);
        if path.exists() {
            return Err(format!(
                "consumer Cargo configuration is forbidden: {relative}"
            ));
        }
    }
    Ok(())
}

fn capture_input_identities(
    root: &Path,
    manifest: &Manifest,
    manifest_source: &str,
) -> Result<Vec<InputIdentity>, String> {
    let mut identities = vec![InputIdentity {
        relative: PathBuf::from("plugin-project.json"),
        size: u64::try_from(manifest_source.len()).map_err(|_| "manifest is too large")?,
        sha256: sha256_hex(manifest_source.as_bytes()),
    }];
    for relative in [Path::new("Cargo.toml"), Path::new("Cargo.lock")]
        .into_iter()
        .chain(
            manifest
                .payloads
                .iter()
                .filter(|payload| safe_relative_path(&payload.path))
                .map(|payload| Path::new(&payload.path)),
        )
    {
        let path = root.join(relative);
        if !path.exists() {
            continue;
        }
        let maximum = if relative == Path::new("Cargo.toml") || relative == Path::new("Cargo.lock")
        {
            MAX_CARGO_FILE_BYTES
        } else {
            MAX_PAYLOAD_BYTES
        };
        let bytes = read_regular_bytes(root, &path, maximum)?;
        identities.push(InputIdentity {
            relative: relative.to_owned(),
            size: u64::try_from(bytes.len()).map_err(|_| "plugin input is too large")?,
            sha256: sha256_hex(&bytes),
        });
    }
    identities.sort_by(|left, right| left.relative.cmp(&right.relative));
    identities.dedup_by(|left, right| left.relative == right.relative);
    Ok(identities)
}

fn verify_input_identities(root: &Path, identities: &[InputIdentity]) -> Result<(), String> {
    for identity in identities {
        let bytes = read_regular_bytes(root, &root.join(&identity.relative), MAX_PAYLOAD_BYTES)?;
        if u64::try_from(bytes.len()).ok() != Some(identity.size)
            || sha256_hex(&bytes) != identity.sha256
        {
            return Err(format!(
                "validated input changed during validation: {}",
                identity.relative.display()
            ));
        }
    }
    Ok(())
}

fn expected_sdk() -> Result<ExpectedSdk, String> {
    let repository = Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(3)
        .ok_or("repository root unavailable")?;
    superexplorer_bundle_generator::verify_inventory()
        .map_err(|error| format!("SDK bundle inventory verification failed: {error}"))?;
    let lock: Value = read_json(&repository.join("sdk/sdk-lock.json"))?;
    let fingerprint: Value = read_json(&repository.join("sdk/ui-abi-fingerprint.json"))?;
    validate_protected_contract(
        lock.pointer("/protected_dependency_contract")
            .ok_or("sdk-lock protected dependency contract is missing")?,
    )?;
    let protected_graph = protected_graph(&lock)?;
    let gpui_packages = protected_gpui_packages(&protected_graph)?;
    Ok(ExpectedSdk {
        repository_root: repository.to_owned(),
        bundle_id: required_string(&lock, "/bundle_id")?,
        target: required_string(&lock, "/toolchain/target")?,
        abi_schema: lock
            .pointer("/build_policy/abi_schema_version")
            .and_then(Value::as_u64)
            .and_then(|value| u32::try_from(value).ok())
            .ok_or("sdk-lock ABI schema is missing")?,
        cargo_release: required_string(&lock, "/toolchain/cargo_release")?,
        cargo_commit_hash: required_string(&lock, "/toolchain/cargo_commit_hash")?,
        cargo_sha256: required_string(&lock, "/toolchain/cargo_sha256")?,
        rustc_release: required_string(&lock, "/toolchain/rustc_release")?,
        rustc_commit_hash: required_string(&lock, "/toolchain/rustc_commit_hash")?,
        rustc_sha256: required_string(&lock, "/toolchain/rustc_sha256")?,
        ui_abi_fingerprint: required_string(&fingerprint, "/fingerprint")?,
        gpui_repository: required_string(&lock, "/gpui/repository")?,
        gpui_revision: required_string(&lock, "/gpui/revision")?,
        gpui_packages,
        protected_graph,
        release_profile: release_profile_policy(&lock)?,
        // Product validation is local-only. Do not read CI configuration or
        // turn an external automation mapping into a package prerequisite.
        gates: BTreeMap::new(),
    })
}

fn release_profile_policy(lock: &Value) -> Result<ReleaseProfilePolicy, String> {
    let profile = lock
        .pointer("/build_policy/profile")
        .and_then(Value::as_object)
        .ok_or("sdk-lock release profile is missing")?;
    let codegen_units = profile
        .get("codegen_units")
        .and_then(Value::as_i64)
        .ok_or("sdk-lock release profile codegen_units is missing")?;
    Ok(ReleaseProfilePolicy {
        panic: profile
            .get("panic")
            .and_then(Value::as_str)
            .ok_or("sdk-lock release profile panic is missing")?
            .to_owned(),
        lto: profile
            .get("lto")
            .and_then(Value::as_str)
            .ok_or("sdk-lock release profile lto is missing")?
            .to_owned(),
        codegen_units,
        strip: profile.get("strip").cloned(),
        overflow_checks: profile.get("overflow_checks").and_then(Value::as_bool),
    })
}

fn validate_protected_contract(contract: &Value) -> Result<(), String> {
    if contract.get("schema_version").and_then(Value::as_u64) != Some(2)
        || contract.get("algorithm").and_then(Value::as_str) != Some("normalized-package-edges-v2")
        || !contract
            .get("edge_digest")
            .and_then(Value::as_str)
            .is_some_and(|digest| lower_hex(digest, 64))
    {
        return Err("sdk-lock protected dependency contract is invalid".into());
    }
    Ok(())
}

fn protected_graph(lock: &Value) -> Result<BTreeMap<String, ProtectedPackage>, String> {
    let packages: Vec<ProtectedPackage> = serde_json::from_value(
        lock.pointer("/protected_dependency_graph")
            .cloned()
            .ok_or("sdk-lock protected dependency graph is missing")?,
    )
    .map_err(|error| format!("sdk-lock protected dependency graph is invalid: {error}"))?;
    let mut graph = BTreeMap::new();
    for package in packages {
        if graph.insert(package.key.clone(), package).is_some() {
            return Err("sdk-lock protected dependency graph has a duplicate key".into());
        }
    }
    if graph.is_empty() {
        return Err("sdk-lock protected dependency graph is empty".into());
    }
    Ok(graph)
}

fn protected_gpui_packages(
    graph: &BTreeMap<String, ProtectedPackage>,
) -> Result<BTreeSet<String>, String> {
    let names = graph
        .values()
        .filter(|package| {
            package
                .source
                .as_deref()
                .is_some_and(|source| source.contains("gpui-ce-explorer"))
                || package
                    .path
                    .as_deref()
                    .is_some_and(|path| path.starts_with("vendor/gpui-ce/"))
        })
        .map(|package| package.name.clone())
        .collect::<BTreeSet<_>>();
    if names.is_empty() {
        return Err("sdk-lock has no protected GPUI packages".into());
    }
    Ok(names)
}

fn read_json(path: &Path) -> Result<Value, String> {
    let source =
        fs::read_to_string(path).map_err(|error| format!("{}: {error}", path.display()))?;
    serde_json::from_str(&source).map_err(|error| format!("{}: {error}", path.display()))
}

fn required_string(value: &Value, pointer: &str) -> Result<String, String> {
    value
        .pointer(pointer)
        .and_then(Value::as_str)
        .map(str::to_owned)
        .ok_or_else(|| format!("required SDK value {pointer} is missing"))
}

fn validate_manifest(manifest: &Manifest, root: &Path, expected: &ExpectedSdk) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();
    if manifest.schema_version != 1 {
        diagnostics.push(diagnostic(
            "SESDK-MANIFEST-001",
            "manifest",
            "schema_version",
            "unsupported schema version",
        ));
    }
    for (path, value) in [
        ("package.id", manifest.package.id.as_str()),
        ("publisher.id", manifest.publisher.id.as_str()),
        ("rust.crate_name", manifest.rust.crate_name.as_str()),
    ] {
        if !valid_id(value) {
            diagnostics.push(diagnostic(
                "SESDK-ID-001",
                "manifest",
                path,
                "ID must be normalized lowercase ASCII and at most 64 characters",
            ));
        }
    }
    if !valid_version(&manifest.package.version) {
        diagnostics.push(diagnostic(
            "SESDK-VERSION-001",
            "manifest",
            "package.version",
            "version must be numeric SemVer without metadata",
        ));
    }
    if manifest.publisher.display_name.trim().is_empty()
        || !manifest.publisher.contacts.iter().any(|contact| {
            matches!(contact.kind.as_str(), "support" | "security")
                && !contact.value.trim().is_empty()
        })
    {
        diagnostics.push(diagnostic(
            "SESDK-PUBLISHER-001",
            "manifest",
            "publisher.contacts",
            "publisher needs a nonempty support or security contact",
        ));
    }
    if manifest.sdk.bundle_id != expected.bundle_id
        || manifest.sdk.bundle_id.contains('@')
        || manifest.sdk.target != expected.target
        || manifest.sdk.abi_schema != expected.abi_schema
    {
        diagnostics.push(diagnostic(
            "SESDK-SDK-001",
            "compatibility",
            "sdk",
            "bundle, target, or ABI schema differs from the canonical SDK",
        ));
    }
    match (&manifest.sdk.gpui, &manifest.sdk.ui_abi_fingerprint) {
        (true, Some(value)) if value == &expected.ui_abi_fingerprint && lower_hex(value, 64) => {}
        (false, None) => {}
        _ => diagnostics.push(diagnostic(
            "SESDK-FINGERPRINT-001",
            "compatibility",
            "sdk.ui_abi_fingerprint",
            "GPUI usage and the exact UI ABI fingerprint must agree",
        )),
    }
    if manifest.rust.entrypoint != "plugin.dll" {
        diagnostics.push(diagnostic(
            "SESDK-PATH-001",
            "manifest",
            "rust.entrypoint",
            "P0 package entrypoint must be the canonical plugin.dll",
        ));
    }
    validate_private_dependencies(root, &manifest.private_dependencies, &mut diagnostics);
    validate_cargo_project(root, manifest, expected, &mut diagnostics);
    validate_built_dll(root, manifest, expected, &mut diagnostics);

    validate_payload_bounds(&manifest.payloads, &mut diagnostics);

    let mut features = BTreeMap::new();
    for (index, feature) in manifest.features.iter().enumerate() {
        let path = format!("features[{index}].id");
        if !valid_id(&feature.id) || features.insert(feature.id.as_str(), feature).is_some() {
            diagnostics.push(diagnostic(
                "SESDK-ID-002",
                "manifest",
                &path,
                "feature ID is invalid or duplicated",
            ));
        }
        if has_duplicates(&feature.capabilities)
            || feature.capabilities.iter().any(|value| !valid_id(value))
        {
            diagnostics.push(diagnostic(
                "SESDK-CAPABILITY-001",
                "manifest",
                &path,
                "feature capabilities are invalid or duplicated",
            ));
        }
    }

    let mut payloads = BTreeMap::new();
    for (index, payload) in manifest.payloads.iter().enumerate() {
        let path = format!("payloads[{index}].path");
        let folded = payload.path.to_ascii_lowercase();
        if !safe_relative_path(&payload.path) || payloads.insert(folded, payload).is_some() {
            diagnostics.push(diagnostic(
                "SESDK-PATH-002",
                "payload",
                &path,
                "payload path is unsafe or collides case-insensitively",
            ));
            continue;
        }
        if payload.kind != "rust-source" {
            diagnostics.push(diagnostic(
                "SESDK-PAYLOAD-001",
                "payload",
                &path,
                "P0 runtime packages accept build-time rust-source payloads only",
            ));
        }
        if payload.size > MAX_PAYLOAD_BYTES {
            diagnostics.push(diagnostic(
                "SESDK-BOUND-003",
                "payload",
                &path,
                format!(
                    "a payload may not exceed the {} byte limit",
                    MAX_PAYLOAD_BYTES
                ),
            ));
            continue;
        }
        validate_payload(root, payload, &path, &mut diagnostics);
    }

    let mut contributions = BTreeSet::new();
    for (index, contribution) in manifest.contributions.iter().enumerate() {
        let path = format!("contributions[{index}]");
        if !valid_id(&contribution.id) || !contributions.insert(&contribution.id) {
            diagnostics.push(diagnostic(
                "SESDK-ID-003",
                "manifest",
                &path,
                "contribution ID is invalid or duplicated",
            ));
        }
        let Some(feature) = features.get(contribution.feature_id.as_str()) else {
            diagnostics.push(diagnostic(
                "SESDK-FEATURE-001",
                "manifest",
                &path,
                "contribution references an unknown feature",
            ));
            continue;
        };
        let folder_size_kind = matches!(
            contribution.kind.as_str(),
            "abi-root" | "column" | "renderer" | "recalculate" | "settings"
        );
        let size_map_kind = matches!(contribution.kind.as_str(), "abi-root" | "view-mode");
        if !(matches!(contribution.kind.as_str(), "abi-root" | "gpui")
            || (manifest.package.id == "rust-folder-size-visual-column" && folder_size_kind)
            || (manifest.package.id == "rust-folder-size-map-view" && size_map_kind))
        {
            diagnostics.push(diagnostic(
                "SESDK-CONTRIBUTION-001",
                "manifest",
                &path,
                "contribution kind is unsupported for this 0→1 package",
            ));
        }
        if contribution.capabilities.is_empty()
            || contribution
                .capabilities
                .iter()
                .any(|capability| !feature.capabilities.contains(capability))
        {
            diagnostics.push(diagnostic(
                "SESDK-CAPABILITY-002",
                "manifest",
                &path,
                "contribution capability is not granted by its feature",
            ));
        }
        if !payloads.contains_key(&contribution.payload.to_ascii_lowercase()) {
            diagnostics.push(diagnostic(
                "SESDK-PAYLOAD-002",
                "manifest",
                &path,
                "contribution references an undeclared payload",
            ));
        }
    }
    if manifest.package.id == "rust-folder-size-visual-column"
        && !is_exact_folder_size_declarations(manifest)
    {
        diagnostics.push(diagnostic(
            "SESDK-CONTRIBUTION-003",
            "manifest",
            "contributions",
            "folder-size must declare its three features and exactly the ABI root, column, and renderer entries implemented by the registrar",
        ));
    }
    if manifest.package.id == "rust-folder-size-map-view"
        && !is_exact_size_map_declarations(manifest)
    {
        diagnostics.push(diagnostic(
            "SESDK-CONTRIBUTION-004",
            "manifest",
            "contributions",
            "size-map must declare its view feature and exactly the ABI root and view-mode entries implemented by the registrar",
        ));
    }
    if manifest
        .contributions
        .iter()
        .any(|value| value.kind == "gpui")
        != manifest.sdk.gpui
    {
        diagnostics.push(diagnostic(
            "SESDK-FINGERPRINT-002",
            "compatibility",
            "contributions",
            "GPUI contribution presence differs from sdk.gpui",
        ));
    }
    validate_verification(&manifest.verification, &expected.gates, &mut diagnostics);
    diagnostics
}

fn validate_private_dependencies(
    root: &Path,
    private_dependencies: &[PrivateDependency],
    diagnostics: &mut Vec<Diagnostic>,
) {
    let mut names = BTreeSet::new();
    let mut paths = BTreeSet::new();
    for (index, dependency) in private_dependencies.iter().enumerate() {
        let location = format!("private_dependencies[{index}]");
        let path_is_private_vendor = private_vendor_path_is_canonical(dependency);
        if !valid_id(&dependency.name)
            || !valid_version(&dependency.version)
            || !path_is_private_vendor
            || !lower_hex(&dependency.tree_sha256, 64)
            || dependency.provenance.source != CRATES_IO_REGISTRY_SOURCE
            || !lower_hex(&dependency.provenance.crate_sha256, 64)
            || dependency.provenance.license_expression.trim().is_empty()
            || dependency.provenance.license_hashes.is_empty()
            || dependency
                .provenance
                .license_hashes
                .keys()
                .any(|path| !safe_relative_path(path))
            || dependency
                .provenance
                .license_hashes
                .values()
                .any(|hash| !lower_hex(hash, 64))
            || !names.insert(dependency.name.as_str())
            || !paths.insert(dependency.path.as_str())
        {
            diagnostics.push(diagnostic(
                "SESDK-PRIVATE-001",
                "manifest",
                &location,
                "private dependency metadata must bind a unique vendor/private crate, tree hash, crates.io provenance, checksum, and license hashes",
            ));
            continue;
        }
        let tree = match private_dependency_tree_sha256(root, &dependency.path) {
            Ok(tree) => tree,
            Err(message) => {
                diagnostics.push(diagnostic(
                    "SESDK-PRIVATE-002",
                    "vendor",
                    &location,
                    message,
                ));
                continue;
            }
        };
        if tree != dependency.tree_sha256 {
            diagnostics.push(diagnostic(
                "SESDK-PRIVATE-003",
                "vendor",
                &location,
                "private dependency tree hash differs from the manifest binding",
            ));
        }
        for (license, expected_hash) in &dependency.provenance.license_hashes {
            let license_path = format!("{}/{}", dependency.path, license);
            let actual = read_regular_bytes(root, &root.join(&license_path), MAX_PAYLOAD_BYTES);
            if !matches!(actual, Ok(ref bytes) if sha256_hex(bytes) == *expected_hash) {
                diagnostics.push(diagnostic(
                    "SESDK-PRIVATE-004",
                    "vendor",
                    &location,
                    "private dependency license path is missing, unsafe, or differs from its provenance hash",
                ));
            }
        }
    }
}

fn private_vendor_path_is_canonical(dependency: &PrivateDependency) -> bool {
    let mut segments = dependency.path.split('/');
    let expected_leaf = format!("{}-{}", dependency.name, dependency.version);
    safe_relative_path(&dependency.path)
        && segments.next() == Some("vendor")
        && segments.next() == Some("private")
        && segments.next() == Some(expected_leaf.as_str())
        && segments.next().is_none()
}

fn private_dependency_tree_sha256(root: &Path, relative: &str) -> Result<String, String> {
    const MAX_PRIVATE_TREE_FILES: usize = 10_000;
    const MAX_PRIVATE_TREE_BYTES: u64 = 512 * 1024 * 1024;
    const MAX_PRIVATE_TREE_DEPTH: usize = 32;
    let tree_root = root.join(relative);
    if is_link_or_reparse(&tree_root)?
        || !fs::metadata(&tree_root).is_ok_and(|metadata| metadata.is_dir())
    {
        return Err("private dependency vendor directory is missing or unsafe".into());
    }
    let mut pending = vec![(tree_root, PathBuf::new(), 0_usize)];
    let mut files = Vec::new();
    let mut total = 0_u64;
    while let Some((directory, prefix, depth)) = pending.pop() {
        for entry in
            fs::read_dir(&directory).map_err(|_| "private dependency tree cannot be read")?
        {
            let entry = entry.map_err(|_| "private dependency tree cannot be read")?;
            let name = entry.file_name();
            let name = name
                .to_str()
                .ok_or("private dependency tree has a non-UTF-8 entry")?;
            if !safe_relative_path(name) {
                return Err("private dependency tree has an unsafe entry name".into());
            }
            let child_relative = prefix.join(name);
            let child = directory.join(name);
            if is_link_or_reparse(&child)? {
                return Err("private dependency tree has a symlink or reparse point".into());
            }
            let metadata =
                fs::metadata(&child).map_err(|_| "private dependency tree cannot be read")?;
            if metadata.is_dir() {
                if depth + 1 > MAX_PRIVATE_TREE_DEPTH {
                    return Err("private dependency tree exceeds its depth limit".into());
                }
                pending.push((child, child_relative, depth + 1));
            } else if metadata.is_file() {
                total = total
                    .checked_add(metadata.len())
                    .ok_or("private dependency tree is too large")?;
                if total > MAX_PRIVATE_TREE_BYTES || files.len() == MAX_PRIVATE_TREE_FILES {
                    return Err("private dependency tree exceeds its resource limits".into());
                }
                files.push(child_relative);
            } else {
                return Err("private dependency tree has a non-regular entry".into());
            }
        }
    }
    files.sort();
    let mut canonical = Vec::new();
    for file in files {
        let file_path = root.join(relative).join(&file);
        let bytes = read_regular_bytes(root, &file_path, MAX_PAYLOAD_BYTES)?;
        canonical.extend_from_slice(file.to_string_lossy().replace('\\', "/").as_bytes());
        canonical.push(0);
        canonical.extend_from_slice(&(bytes.len() as u64).to_le_bytes());
        canonical.extend_from_slice(sha256_hex(&bytes).as_bytes());
        canonical.push(0);
    }
    Ok(sha256_hex(&canonical))
}

fn validate_built_dll(
    root: &Path,
    _manifest: &Manifest,
    expected: &ExpectedSdk,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let dll = root
        .join("target")
        .join("superexplorer")
        .join(&expected.bundle_id)
        .join("build")
        .join("plugin.dll");
    match fs::symlink_metadata(&dll) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return,
        Err(error) => {
            diagnostics.push(diagnostic(
                "SESDK-DLL-001",
                "binary",
                "target/superexplorer",
                format!("built plugin DLL metadata cannot be read: {error}"),
            ));
            return;
        }
        Ok(_) => {}
    }
    if let Err(message) = ensure_regular_project_path(root, &dll) {
        diagnostics.push(diagnostic(
            "SESDK-DLL-001",
            "binary",
            "target/superexplorer",
            message,
        ));
        return;
    }
    let inspected = inspect_dll(&dll);
    diagnostics.extend(inspected.diagnostics);
}

fn validate_payload_bounds(payloads: &[Payload], diagnostics: &mut Vec<Diagnostic>) {
    if payloads.len() > MAX_PAYLOADS {
        diagnostics.push(diagnostic(
            "SESDK-BOUND-001",
            "payload",
            "payloads",
            format!("a plugin may declare at most {MAX_PAYLOADS} payloads"),
        ));
    }
    let total_size = payloads
        .iter()
        .try_fold(0_u64, |total, payload| total.checked_add(payload.size));
    if total_size.is_none_or(|total| total > MAX_TOTAL_PAYLOAD_BYTES) {
        diagnostics.push(diagnostic(
            "SESDK-BOUND-002",
            "payload",
            "payloads",
            format!(
                "declared payload bytes exceed the {} byte package limit",
                MAX_TOTAL_PAYLOAD_BYTES
            ),
        ));
    }
}

fn validate_cargo_project(
    root: &Path,
    plugin: &Manifest,
    expected: &ExpectedSdk,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let cargo_toml = root.join("Cargo.toml");
    let cargo_lock = root.join("Cargo.lock");
    let manifest = match read_project_toml(root, &cargo_toml) {
        Ok(value) => value,
        Err(message) => {
            diagnostics.push(diagnostic(
                "SESDK-CARGO-001",
                "cargo",
                "Cargo.toml",
                message,
            ));
            return;
        }
    };
    let lock = match read_project_toml(root, &cargo_lock) {
        Ok(value) => value,
        Err(message) => {
            diagnostics.push(diagnostic(
                "SESDK-CARGO-002",
                "cargo",
                "Cargo.lock",
                message,
            ));
            return;
        }
    };
    let packages = lock
        .get("package")
        .and_then(TomlValue::as_array)
        .and_then(|values| {
            values
                .iter()
                .map(TomlValue::as_table)
                .collect::<Option<Vec<_>>>()
        });
    let Some(packages) = packages else {
        diagnostics.push(diagnostic(
            "SESDK-CARGO-003",
            "cargo",
            "Cargo.lock",
            "Cargo.lock has no package records",
        ));
        return;
    };
    let lock_packages = cargo_lock_packages(&packages);
    let cargo_name = manifest
        .get("package")
        .and_then(TomlValue::as_table)
        .and_then(|table| table.get("name"))
        .and_then(TomlValue::as_str)
        .unwrap_or("");
    let lib = manifest.get("lib").and_then(TomlValue::as_table);
    let crate_types = lib
        .and_then(|table| table.get("crate-type"))
        .and_then(TomlValue::as_array);
    let lib_path = lib
        .and_then(|table| table.get("path"))
        .and_then(TomlValue::as_str)
        .unwrap_or("src/lib.rs");
    let expected_crate = plugin.rust.crate_name.replace('-', "_");
    if cargo_name.replace('-', "_") != expected_crate
        || !crate_types
            .is_some_and(|types| types.iter().any(|value| value.as_str() == Some("cdylib")))
        || !safe_relative_path(lib_path)
    {
        diagnostics.push(diagnostic(
            "SESDK-ENTRY-001",
            "cargo",
            "Cargo.toml",
            "manifest rust.crate_name or Cargo cdylib target is not canonical",
        ));
    }
    validate_cargo_policy(&manifest, expected, diagnostics);
    let mut dependencies = BTreeMap::new();
    collect_direct_dependencies(&manifest, "", &mut dependencies);
    let private_names = plugin
        .private_dependencies
        .iter()
        .map(|dependency| dependency.name.as_str())
        .collect::<BTreeSet<_>>();
    for (location, dependency) in dependencies {
        validate_direct_dependency(
            &location,
            &dependency,
            &packages,
            &private_names,
            expected,
            diagnostics,
        );
    }
    match cargo_metadata(root, expected) {
        Ok(metadata) => {
            validate_protected_metadata(
                &metadata,
                &lock_packages,
                plugin.sdk.gpui,
                expected,
                diagnostics,
            );
            validate_private_dependency_cargo_binding(
                root,
                &manifest,
                &lock_packages,
                &metadata,
                &plugin.private_dependencies,
                expected,
                diagnostics,
            );
        }
        Err(message) => diagnostics.push(diagnostic(
            "SESDK-METADATA-001",
            "cargo",
            "Cargo.toml",
            message,
        )),
    }
}

fn cargo_lock_packages(packages: &[&toml::map::Map<String, TomlValue>]) -> Vec<CargoLockPackage> {
    packages
        .iter()
        .filter_map(|package| {
            Some(CargoLockPackage {
                name: package.get("name")?.as_str()?.to_owned(),
                version: package.get("version")?.as_str()?.to_owned(),
                source: package
                    .get("source")
                    .and_then(TomlValue::as_str)
                    .map(str::to_owned),
                checksum: package
                    .get("checksum")
                    .and_then(TomlValue::as_str)
                    .map(str::to_owned),
            })
        })
        .collect()
}

/// Binds author-provided private crates to Cargo's patched resolution graph.
///
/// Cargo deliberately omits registry source and checksum for a patched path
/// package in `Cargo.lock`. The manifest and vendored `.cargo-checksum.json`
/// retain that provenance, while the exact patch, lock record, and metadata
/// path prove that Cargo resolved the declared vendor tree.
fn validate_private_dependency_cargo_binding(
    root: &Path,
    manifest: &TomlValue,
    lock_packages: &[CargoLockPackage],
    metadata: &CargoMetadata,
    private_dependencies: &[PrivateDependency],
    expected: &ExpectedSdk,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let private_by_name = private_dependencies
        .iter()
        .map(|dependency| (dependency.name.as_str(), dependency))
        .collect::<BTreeMap<_, _>>();
    let protected_names = expected
        .protected_graph
        .values()
        .map(|package| package.name.as_str())
        .collect::<BTreeSet<_>>();
    for (index, dependency) in private_dependencies.iter().enumerate() {
        let location = format!("private_dependencies[{index}]");
        if protected_names.contains(dependency.name.as_str()) {
            diagnostics.push(diagnostic(
                "SESDK-PRIVATE-010",
                "compatibility",
                &location,
                "private dependency may not shadow a protected SDK closure package",
            ));
        }
        validate_private_vendor_provenance(root, dependency, &location, diagnostics);
    }

    let patch_table = manifest.get("patch").and_then(TomlValue::as_table);
    if patch_table.is_some_and(|patch| patch.keys().any(|source| source != "crates-io")) {
        diagnostics.push(diagnostic(
            "SESDK-PRIVATE-005",
            "cargo",
            "patch",
            "private dependencies may only use the controlled [patch.crates-io] table",
        ));
    }
    let patches = patch_table
        .and_then(|patch| patch.get("crates-io"))
        .and_then(TomlValue::as_table);
    let mut patched_names = BTreeSet::new();
    for (name, patch) in patches.into_iter().flat_map(|table| table.iter()) {
        patched_names.insert(name.as_str());
        let Some(dependency) = private_by_name.get(name.as_str()) else {
            diagnostics.push(diagnostic(
                "SESDK-PRIVATE-005",
                "cargo",
                &format!("patch.crates-io.{name}"),
                "[patch.crates-io] contains an undeclared private dependency",
            ));
            continue;
        };
        let exact_path_patch = patch.as_table().is_some_and(|table| {
            table.len() == 1
                && table.get("path").and_then(TomlValue::as_str) == Some(dependency.path.as_str())
        });
        if !exact_path_patch {
            diagnostics.push(diagnostic(
                "SESDK-PRIVATE-005",
                "cargo",
                &format!("patch.crates-io.{name}"),
                "private dependency patch must contain only its exact manifest vendor/private path",
            ));
        }
    }
    for dependency in private_dependencies {
        if !patched_names.contains(dependency.name.as_str()) {
            diagnostics.push(diagnostic(
                "SESDK-PRIVATE-005",
                "cargo",
                &format!("patch.crates-io.{}", dependency.name),
                "every declared private dependency requires an exact [patch.crates-io] path binding",
            ));
        }
    }

    let mut direct_dependencies = BTreeMap::new();
    collect_direct_dependencies(manifest, "", &mut direct_dependencies);
    for dependency in private_dependencies {
        let expected_version = format!("={}", dependency.version);
        let is_exact_direct_registry_dependency = direct_dependencies.values().any(|direct| {
            direct.package == dependency.name
                && direct.version.as_deref() == Some(expected_version.as_str())
                && direct.git.is_none()
                && !direct.path
                && !direct.workspace
                && !direct.malformed
        });
        if !is_exact_direct_registry_dependency {
            diagnostics.push(diagnostic(
                "SESDK-PRIVATE-006",
                "cargo",
                &format!("dependencies.{}", dependency.name),
                "private dependency must be a direct exact-version registry dependency resolved through its patch",
            ));
        }
        let matching_locks = lock_packages
            .iter()
            .filter(|locked| locked.name == dependency.name && locked.version == dependency.version)
            .collect::<Vec<_>>();
        if matching_locks.len() != 1
            || matching_locks
                .first()
                .is_none_or(|locked| locked.source.is_some() || locked.checksum.is_some())
        {
            diagnostics.push(diagnostic(
                "SESDK-PRIVATE-007",
                "cargo",
                &format!("Cargo.lock.{}", dependency.name),
                "patched private dependency must have one exact source-less, checksum-less Cargo.lock record",
            ));
        }
    }

    validate_private_metadata_bijection(
        root,
        metadata,
        private_dependencies,
        &private_by_name,
        &protected_names,
        diagnostics,
    );
}

fn validate_private_vendor_provenance(
    root: &Path,
    dependency: &PrivateDependency,
    location: &str,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let cargo_toml = root.join(&dependency.path).join("Cargo.toml");
    let cargo = read_project_toml(root, &cargo_toml);
    let package = cargo
        .as_ref()
        .ok()
        .and_then(|value| value.get("package"))
        .and_then(TomlValue::as_table);
    if package.and_then(|package| package.get("name").and_then(TomlValue::as_str))
        != Some(dependency.name.as_str())
        || package.and_then(|package| package.get("version").and_then(TomlValue::as_str))
            != Some(dependency.version.as_str())
        || package.and_then(|package| package.get("license").and_then(TomlValue::as_str))
            != Some(dependency.provenance.license_expression.as_str())
    {
        diagnostics.push(diagnostic(
            "SESDK-PRIVATE-011",
            "vendor",
            location,
            "vendored Cargo.toml name, version, or license differs from private dependency provenance",
        ));
    }
    let checksum_path = root.join(&dependency.path).join(".cargo-checksum.json");
    let checksum = read_regular_utf8_file(
        root,
        &checksum_path,
        MAX_CARGO_FILE_BYTES,
        "private dependency checksum",
    )
    .ok()
    .and_then(|source| serde_json::from_str::<Value>(&source).ok());
    let checksum_files = checksum.as_ref().and_then(cargo_checksum_file_hashes);
    let inventory_matches = checksum_files.as_ref().is_some_and(|declared| {
        private_vendor_file_hashes(root, &dependency.path).is_ok_and(|actual| actual == *declared)
    });
    if checksum
        .as_ref()
        .and_then(|value| value.get("package"))
        .and_then(Value::as_str)
        != Some(dependency.provenance.crate_sha256.as_str())
        || !inventory_matches
    {
        diagnostics.push(diagnostic(
            "SESDK-PRIVATE-012",
            "vendor",
            location,
            "vendored .cargo-checksum.json does not exactly bind the crate checksum and non-checksum file inventory",
        ));
    }
}

fn cargo_checksum_file_hashes(value: &Value) -> Option<BTreeMap<String, String>> {
    let files = value.get("files")?.as_object()?;
    let mut normalized = BTreeSet::new();
    let mut hashes = BTreeMap::new();
    for (path, hash) in files {
        if path == ".cargo-checksum.json"
            || !safe_relative_path(path)
            || !lower_hex(hash.as_str()?, 64)
            || !normalized.insert(path.to_ascii_lowercase())
        {
            return None;
        }
        hashes.insert(path.clone(), hash.as_str()?.to_owned());
    }
    Some(hashes)
}

fn private_vendor_file_hashes(
    root: &Path,
    relative: &str,
) -> Result<BTreeMap<String, String>, String> {
    const MAX_PRIVATE_TREE_FILES: usize = 10_000;
    const MAX_PRIVATE_TREE_BYTES: u64 = 512 * 1024 * 1024;
    const MAX_PRIVATE_TREE_DEPTH: usize = 32;
    let tree_root = root.join(relative);
    if is_link_or_reparse(&tree_root)?
        || !fs::metadata(&tree_root).is_ok_and(|metadata| metadata.is_dir())
    {
        return Err("private dependency vendor directory is missing or unsafe".into());
    }
    let mut pending = vec![(tree_root, PathBuf::new(), 0_usize)];
    let mut total = 0_u64;
    let mut hashes = BTreeMap::new();
    let mut folded_paths = BTreeSet::new();
    while let Some((directory, prefix, depth)) = pending.pop() {
        for entry in
            fs::read_dir(&directory).map_err(|_| "private dependency tree cannot be read")?
        {
            let entry = entry.map_err(|_| "private dependency tree cannot be read")?;
            let name = entry.file_name();
            let name = name
                .to_str()
                .ok_or("private dependency tree has a non-UTF-8 entry")?;
            if !safe_relative_path(name) {
                return Err("private dependency tree has an unsafe entry name".into());
            }
            let child_relative = prefix.join(name);
            let child = directory.join(name);
            if is_link_or_reparse(&child)? {
                return Err("private dependency tree has a symlink or reparse point".into());
            }
            let metadata =
                fs::metadata(&child).map_err(|_| "private dependency tree cannot be read")?;
            if metadata.is_dir() {
                if depth + 1 > MAX_PRIVATE_TREE_DEPTH {
                    return Err("private dependency tree exceeds its depth limit".into());
                }
                pending.push((child, child_relative, depth + 1));
                continue;
            }
            if !metadata.is_file() {
                return Err("private dependency tree has a non-regular entry".into());
            }
            let normalized_path = child_relative.to_string_lossy().replace('\\', "/");
            if normalized_path == ".cargo-checksum.json" {
                continue;
            }
            total = total
                .checked_add(metadata.len())
                .ok_or("private dependency tree is too large")?;
            if total > MAX_PRIVATE_TREE_BYTES || hashes.len() == MAX_PRIVATE_TREE_FILES {
                return Err("private dependency tree exceeds its resource limits".into());
            }
            if !folded_paths.insert(normalized_path.to_ascii_lowercase()) {
                return Err("private dependency tree has a case-colliding file path".into());
            }
            hashes.insert(
                normalized_path,
                sha256_hex(&read_regular_bytes(root, &child, MAX_PAYLOAD_BYTES)?),
            );
        }
    }
    Ok(hashes)
}

fn validate_private_metadata_bijection(
    root: &Path,
    metadata: &CargoMetadata,
    private_dependencies: &[PrivateDependency],
    private_by_name: &BTreeMap<&str, &PrivateDependency>,
    protected_names: &BTreeSet<&str>,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let Some(resolve) = &metadata.resolve else {
        diagnostics.push(diagnostic(
            "SESDK-PRIVATE-008",
            "cargo",
            "cargo.metadata",
            "cargo metadata has no graph for private dependency binding",
        ));
        return;
    };
    let Some(root_id) = &resolve.root else {
        diagnostics.push(diagnostic(
            "SESDK-PRIVATE-008",
            "cargo",
            "cargo.metadata",
            "cargo metadata has no root for private dependency binding",
        ));
        return;
    };
    let nodes = resolve
        .nodes
        .iter()
        .map(|node| (node.id.as_str(), node))
        .collect::<BTreeMap<_, _>>();
    let packages = metadata
        .packages
        .iter()
        .map(|package| (package.id.as_str(), package))
        .collect::<BTreeMap<_, _>>();
    let reachable = reachable_node_ids(root_id, &nodes);
    let Some(root_node) = nodes.get(root_id.as_str()) else {
        diagnostics.push(diagnostic(
            "SESDK-PRIVATE-008",
            "cargo",
            "cargo.metadata",
            "cargo metadata root has no resolve node",
        ));
        return;
    };

    let mut declared_metadata_ids = BTreeSet::new();
    for dependency in private_dependencies {
        let matching = reachable
            .iter()
            .filter_map(|id| packages.get(id.as_str()).map(|package| (id, *package)))
            .filter(|(_, package)| {
                package.name == dependency.name
                    && package.version == dependency.version
                    && package.source.is_none()
                    && metadata_package_path(package, root).as_deref()
                        == Some(dependency.path.as_str())
            })
            .collect::<Vec<_>>();
        let root_has_exact_edge = matching.iter().any(|(id, _)| {
            root_node
                .deps
                .iter()
                .any(|edge| edge.name == dependency.name.replace('-', "_") && edge.pkg == **id)
        });
        if matching.len() != 1 || !root_has_exact_edge {
            diagnostics.push(diagnostic(
                "SESDK-PRIVATE-008",
                "cargo",
                &format!("cargo.metadata.{}", dependency.name),
                "declared private dependency is absent, ambiguous, or not directly reachable from the plugin root",
            ));
        } else if let Some((id, _)) = matching.first() {
            declared_metadata_ids.insert((*id).to_owned());
        }
    }

    for id in &reachable {
        let Some(package) = packages.get(id.as_str()) else {
            continue;
        };
        let relative = metadata_package_path(package, root);
        let is_private_path = relative
            .as_deref()
            .is_some_and(|path| path.starts_with("vendor/private/"));
        if protected_names.contains(package.name.as_str()) && is_private_path {
            diagnostics.push(diagnostic(
                "SESDK-PRIVATE-010",
                "compatibility",
                &format!("cargo.metadata.{}", package.name),
                "private vendor path shadows a protected SDK closure package",
            ));
        }
        if !is_private_path {
            continue;
        }
        let declared = private_by_name.get(package.name.as_str());
        let exact_declared_path = declared.is_some_and(|dependency| {
            package.version == dependency.version
                && package.source.is_none()
                && relative.as_deref() == Some(dependency.path.as_str())
                && declared_metadata_ids.contains(id)
        });
        if !exact_declared_path {
            diagnostics.push(diagnostic(
                "SESDK-PRIVATE-009",
                "cargo",
                &format!("cargo.metadata.{}", package.name),
                "reachable vendor/private crate is undeclared or differs from its manifest binding",
            ));
        }
    }
}

fn validate_cargo_policy(
    manifest: &TomlValue,
    expected: &ExpectedSdk,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let release = manifest
        .get("profile")
        .and_then(TomlValue::as_table)
        .and_then(|profiles| profiles.get("release"))
        .and_then(TomlValue::as_table);
    let Some(release) = release else {
        diagnostics.push(diagnostic(
            "SESDK-PROFILE-001",
            "cargo",
            "profile.release",
            "Cargo.toml must explicitly declare the canonical release profile",
        ));
        return;
    };
    check_profile_string(
        release,
        "panic",
        &expected.release_profile.panic,
        diagnostics,
    );
    check_profile_string(release, "lto", &expected.release_profile.lto, diagnostics);
    if release.get("codegen-units").and_then(TomlValue::as_integer)
        != Some(expected.release_profile.codegen_units)
    {
        diagnostics.push(diagnostic(
            "SESDK-PROFILE-001",
            "cargo",
            "profile.release.codegen-units",
            "release codegen-units differs from sdk-lock build policy",
        ));
    }
    check_profile_optional_json(
        release,
        "strip",
        expected.release_profile.strip.as_ref(),
        diagnostics,
    );
    if release.get("overflow-checks").and_then(TomlValue::as_bool)
        != expected.release_profile.overflow_checks
    {
        diagnostics.push(diagnostic(
            "SESDK-PROFILE-001",
            "cargo",
            "profile.release.overflow-checks",
            "release overflow-checks differs from sdk-lock build policy",
        ));
    }
    for (path, key) in cargo_override_keys(manifest, "") {
        diagnostics.push(diagnostic(
            "SESDK-CARGO-013",
            "cargo",
            &path,
            format!("Cargo manifest override {key:?} is forbidden by the SDK build policy"),
        ));
    }
}

fn check_profile_string(
    release: &toml::map::Map<String, TomlValue>,
    key: &str,
    expected: &str,
    diagnostics: &mut Vec<Diagnostic>,
) {
    if release.get(key).and_then(TomlValue::as_str) != Some(expected) {
        diagnostics.push(diagnostic(
            "SESDK-PROFILE-001",
            "cargo",
            &format!("profile.release.{key}"),
            format!("release {key} differs from sdk-lock build policy"),
        ));
    }
}

fn check_profile_optional_json(
    release: &toml::map::Map<String, TomlValue>,
    key: &str,
    expected: Option<&Value>,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let actual = release.get(key).and_then(toml_value_as_json);
    if actual.as_ref() != expected {
        diagnostics.push(diagnostic(
            "SESDK-PROFILE-001",
            "cargo",
            &format!("profile.release.{key}"),
            format!("release {key} differs from sdk-lock build policy"),
        ));
    }
}

fn toml_value_as_json(value: &TomlValue) -> Option<Value> {
    match value {
        TomlValue::String(value) => Some(Value::String(value.clone())),
        TomlValue::Boolean(value) => Some(Value::Bool(*value)),
        TomlValue::Integer(value) => Some(Value::Number((*value).into())),
        _ => None,
    }
}

fn cargo_override_keys(value: &TomlValue, prefix: &str) -> Vec<(String, String)> {
    let mut found = Vec::new();
    let Some(table) = value.as_table() else {
        return found;
    };
    for scope in ["target", "build", "profile"] {
        if let Some(value) = table.get(scope) {
            collect_cargo_override_keys(value, scope, prefix, &mut found);
        }
    }
    found
}

fn collect_cargo_override_keys(
    value: &TomlValue,
    key: &str,
    prefix: &str,
    found: &mut Vec<(String, String)>,
) {
    let path = if prefix.is_empty() {
        key.to_owned()
    } else {
        format!("{prefix}.{key}")
    };
    if matches!(
        key,
        "linker" | "rustflags" | "runner" | "rustc-wrapper" | "rustc_wrapper"
    ) {
        found.push((path.clone(), key.to_owned()));
    }
    if let Some(table) = value.as_table() {
        for (child_key, child_value) in table {
            collect_cargo_override_keys(child_value, child_key, &path, found);
        }
    }
}

fn cargo_metadata(root: &Path, expected: &ExpectedSdk) -> Result<CargoMetadata, String> {
    let rustc = trusted_rustc_path(expected)?;
    if let Some(variable) = forbidden_cargo_environment_variable(expected, &rustc) {
        return Err(format!(
            "fingerprint-affecting Cargo environment override is forbidden: {variable}"
        ));
    }
    let cargo = trusted_cargo_path(expected)?;
    let output = Command::new(cargo)
        .args([
            "metadata",
            "--locked",
            "--offline",
            "--format-version",
            "1",
            "--manifest-path",
        ])
        .arg(root.join("Cargo.toml"))
        .current_dir(&expected.repository_root)
        .env("CARGO_NET_OFFLINE", "true")
        .env("RUSTC", rustc)
        .env_remove("CARGO_TARGET_DIR")
        .env_remove("CARGO_MANIFEST_DIR")
        .output()
        .map_err(|error| format!("could not execute cargo metadata: {error}"))?;
    if !output.status.success() {
        return Err(format!(
            "cargo metadata --locked --offline failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    serde_json::from_slice(&output.stdout)
        .map_err(|error| format!("cargo metadata emitted invalid JSON: {error}"))
}

fn trusted_cargo_path(expected: &ExpectedSdk) -> Result<PathBuf, String> {
    let configured = std::env::var_os(TRUSTED_CARGO_PATH_ENV)
        .ok_or_else(|| format!("{TRUSTED_CARGO_PATH_ENV} is required"))?;
    let configured = PathBuf::from(configured);
    if !configured.is_absolute() || is_link_or_reparse(&configured)? {
        return Err("trusted Cargo path must be an absolute non-reparse executable".into());
    }
    let canonical = configured
        .canonicalize()
        .map_err(|error| format!("trusted Cargo cannot be resolved: {error}"))?;
    let metadata = fs::metadata(&canonical)
        .map_err(|error| format!("trusted Cargo metadata cannot be read: {error}"))?;
    if !metadata.is_file() {
        return Err("trusted Cargo path is not a regular file".into());
    }
    let expected_hash = std::env::var(TRUSTED_CARGO_SHA256_ENV)
        .map_err(|_| format!("{TRUSTED_CARGO_SHA256_ENV} is required"))?;
    if expected_hash != expected.cargo_sha256
        || !lower_hex(&expected_hash, 64)
        || sha256_hex(&fs::read(&canonical).map_err(|error| error.to_string())?) != expected_hash
    {
        return Err(
            "trusted Cargo executable hash differs from its explicit authority contract".into(),
        );
    }
    let output = Command::new(&canonical)
        .arg("-Vv")
        .env_remove("RUSTC")
        .env_remove("RUSTC_WRAPPER")
        .env_remove("RUSTC_WORKSPACE_WRAPPER")
        .env_remove("RUSTFLAGS")
        .env_remove("RUSTDOCFLAGS")
        .env_remove("CARGO_ENCODED_RUSTFLAGS")
        .env_remove("CARGO_BUILD_RUSTFLAGS")
        .output()
        .map_err(|error| format!("could not execute trusted Cargo: {error}"))?;
    if !output.status.success() {
        return Err("trusted Cargo -Vv failed".into());
    }
    let version = String::from_utf8(output.stdout)
        .map_err(|_| "trusted Cargo -Vv emitted non-UTF-8 output")?;
    if toolchain_field(&version, "release") != Some(expected.cargo_release.as_str())
        || toolchain_field(&version, "commit-hash") != Some(expected.cargo_commit_hash.as_str())
    {
        return Err("trusted Cargo version or commit hash differs from sdk-lock".into());
    }
    Ok(canonical)
}

fn trusted_rustc_path(expected: &ExpectedSdk) -> Result<PathBuf, String> {
    let configured = std::env::var_os(TRUSTED_RUSTC_PATH_ENV)
        .ok_or_else(|| format!("{TRUSTED_RUSTC_PATH_ENV} is required"))?;
    let configured = PathBuf::from(configured);
    if !configured.is_absolute() || is_link_or_reparse(&configured)? {
        return Err("trusted rustc path must be an absolute non-reparse executable".into());
    }
    let canonical = configured
        .canonicalize()
        .map_err(|error| format!("trusted rustc cannot be resolved: {error}"))?;
    let metadata = fs::metadata(&canonical)
        .map_err(|error| format!("trusted rustc metadata cannot be read: {error}"))?;
    if !metadata.is_file() {
        return Err("trusted rustc path is not a regular file".into());
    }
    let expected_hash = std::env::var(TRUSTED_RUSTC_SHA256_ENV)
        .map_err(|_| format!("{TRUSTED_RUSTC_SHA256_ENV} is required"))?;
    if expected_hash != expected.rustc_sha256
        || !lower_hex(&expected_hash, 64)
        || sha256_hex(&fs::read(&canonical).map_err(|error| error.to_string())?) != expected_hash
    {
        return Err(
            "trusted rustc executable hash differs from the sdk-lock authority contract".into(),
        );
    }
    let output = Command::new(&canonical)
        .arg("-Vv")
        .env_remove("RUSTC")
        .env_remove("RUSTC_WRAPPER")
        .env_remove("RUSTC_WORKSPACE_WRAPPER")
        .env_remove("RUSTFLAGS")
        .env_remove("RUSTDOCFLAGS")
        .output()
        .map_err(|error| format!("could not execute trusted rustc: {error}"))?;
    if !output.status.success() {
        return Err("trusted rustc -Vv failed".into());
    }
    let version = String::from_utf8(output.stdout)
        .map_err(|_| "trusted rustc -Vv emitted non-UTF-8 output")?;
    if toolchain_field(&version, "release") != Some(expected.rustc_release.as_str())
        || toolchain_field(&version, "commit-hash") != Some(expected.rustc_commit_hash.as_str())
        || toolchain_field(&version, "host") != Some(expected.target.as_str())
    {
        return Err("trusted rustc version, commit hash, or host differs from sdk-lock".into());
    }
    Ok(canonical)
}

fn toolchain_field<'a>(output: &'a str, name: &str) -> Option<&'a str> {
    output
        .lines()
        .find_map(|line| line.strip_prefix(&format!("{name}: ")))
        .filter(|value| !value.is_empty())
}

fn forbidden_cargo_environment_variable(
    expected: &ExpectedSdk,
    trusted_rustc: &Path,
) -> Option<String> {
    std::env::vars_os().find_map(|(name, value)| {
        if value.is_empty() {
            return None;
        }
        let name = name.to_string_lossy().into_owned();
        if name == "RUSTC"
            && PathBuf::from(&value).canonicalize().ok().as_deref() == Some(trusted_rustc)
            && std::env::var(TRUSTED_RUSTC_SHA256_ENV).ok().as_deref()
                == Some(expected.rustc_sha256.as_str())
        {
            return None;
        }
        is_forbidden_cargo_environment_name(&name).then_some(name)
    })
}

fn is_forbidden_cargo_environment_name(name: &str) -> bool {
    name == "RUSTC"
        || name == "RUSTC_BOOTSTRAP"
        || name == "RUSTC_WRAPPER"
        || name == "RUSTC_WORKSPACE_WRAPPER"
        || name == "RUSTFLAGS"
        || name == "RUSTDOCFLAGS"
        || name == "CARGO_ENCODED_RUSTFLAGS"
        || name == "CARGO_BUILD_RUSTFLAGS"
        || name == "CARGO_BUILD_RUSTC"
        || name == "CARGO_INCREMENTAL"
        || name == "CC"
        || name == "CXX"
        || name == "AR"
        || name == "LINKER"
        || name.starts_with("CARGO_PROFILE_")
        || (name.starts_with("CARGO_TARGET_")
            && (name.ends_with("_RUSTFLAGS")
                || name.ends_with("_LINKER")
                || name.ends_with("_RUNNER")))
        || name.ends_with("_CC")
        || name.ends_with("_CXX")
        || name.ends_with("_AR")
        || name.ends_with("_LINKER")
}

fn validate_protected_metadata(
    metadata: &CargoMetadata,
    lock_packages: &[CargoLockPackage],
    gpui_plugin: bool,
    expected: &ExpectedSdk,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let Some(resolve) = &metadata.resolve else {
        diagnostics.push(diagnostic(
            "SESDK-PROTECTED-001",
            "compatibility",
            "cargo.metadata",
            "cargo metadata has no resolved dependency graph",
        ));
        return;
    };
    let Some(root) = &resolve.root else {
        diagnostics.push(diagnostic(
            "SESDK-PROTECTED-001",
            "compatibility",
            "cargo.metadata",
            "cargo metadata has no root package",
        ));
        return;
    };
    let nodes = resolve
        .nodes
        .iter()
        .map(|node| (node.id.as_str(), node))
        .collect::<BTreeMap<_, _>>();
    let packages = metadata
        .packages
        .iter()
        .map(|package| (package.id.as_str(), package))
        .collect::<BTreeMap<_, _>>();
    let reachable = reachable_node_ids(root, &nodes);
    let protected_names = expected
        .protected_graph
        .values()
        .map(|package| package.name.as_str())
        .collect::<BTreeSet<_>>();
    let has_protected_input = reachable.iter().any(|id| {
        packages.get(id.as_str()).is_some_and(|package| {
            expected.gpui_packages.contains(&package.name) || package.name == "abi_stable"
        })
    });
    if !has_protected_input {
        return;
    }

    let mut canonical_by_metadata_id = BTreeMap::new();
    for id in &reachable {
        let Some(package) = packages.get(id.as_str()) else {
            diagnostics.push(diagnostic(
                "SESDK-PROTECTED-001",
                "compatibility",
                "cargo.metadata",
                format!("resolved package ID {id} has no package record"),
            ));
            continue;
        };
        if !protected_names.contains(package.name.as_str()) {
            continue;
        }
        let Some(canonical) = canonical_package_for_metadata(package, expected) else {
            diagnostics.push(diagnostic(
                "SESDK-PROTECTED-002",
                "compatibility",
                &format!("cargo.metadata.{}", package.name),
                "reachable protected package has a different source or version than sdk-lock",
            ));
            continue;
        };
        let checksum_matches = lock_packages.iter().any(|locked| {
            locked.name == package.name
                && locked.version == package.version
                && locked.source == package.source
                && locked.checksum == canonical.checksum
        });
        if !checksum_matches {
            diagnostics.push(diagnostic(
                "SESDK-PROTECTED-003",
                "compatibility",
                &format!("Cargo.lock.{}", package.name),
                "protected package checksum or lock source differs from sdk-lock",
            ));
        }
        canonical_by_metadata_id.insert(id.clone(), canonical.key.clone());
    }

    for id in &reachable {
        let Some(canonical_key) = canonical_by_metadata_id.get(id) else {
            continue;
        };
        let Some(node) = nodes.get(id.as_str()) else {
            continue;
        };
        let Some(canonical) = expected.protected_graph.get(canonical_key) else {
            continue;
        };
        let actual_features = string_set(&node.features);
        let approved_features = string_set(&canonical.features);
        let features_are_compatible = if gpui_plugin {
            actual_features == approved_features
        } else {
            actual_features.is_subset(&approved_features)
        };
        if !features_are_compatible {
            diagnostics.push(diagnostic(
                "SESDK-PROTECTED-004",
                "compatibility",
                &format!("cargo.metadata.{}.features", canonical.name),
                if gpui_plugin {
                    "GPUI plugin protected package features differ from sdk-lock"
                } else {
                    "protected package enables features not approved by sdk-lock"
                },
            ));
        }
        let actual_edges = node
            .deps
            .iter()
            .flat_map(|dependency| {
                let to = canonical_by_metadata_id
                    .get(&dependency.pkg)
                    .cloned()
                    .unwrap_or_else(|| format!("unapproved:{}", dependency.pkg));
                dependency.dep_kinds.iter().map(move |kind| {
                    normalized_edge(
                        &dependency.name,
                        &to,
                        kind.kind.as_deref().unwrap_or("normal"),
                        kind.target.as_deref(),
                    )
                })
            })
            .collect::<BTreeSet<_>>();
        let expected_edges = canonical
            .dependencies
            .iter()
            .flat_map(|dependency| {
                dependency.dep_kinds.iter().map(move |kind| {
                    normalized_edge(
                        &dependency.name,
                        &dependency.to,
                        &kind.kind,
                        kind.target.as_deref(),
                    )
                })
            })
            .collect::<BTreeSet<_>>();
        if actual_edges != expected_edges {
            diagnostics.push(diagnostic(
                "SESDK-PROTECTED-005",
                "compatibility",
                &format!("cargo.metadata.{}.dependencies", canonical.name),
                "protected dependency edges, kinds, or targets differ from sdk-lock",
            ));
        }
    }
}

fn reachable_node_ids(root: &str, nodes: &BTreeMap<&str, &CargoMetadataNode>) -> BTreeSet<String> {
    let mut remaining = vec![root.to_owned()];
    let mut reached = BTreeSet::new();
    while let Some(id) = remaining.pop() {
        if !reached.insert(id.clone()) {
            continue;
        }
        if let Some(node) = nodes.get(id.as_str()) {
            remaining.extend(node.deps.iter().map(|dependency| dependency.pkg.clone()));
        }
    }
    reached
}

fn canonical_package_for_metadata<'a>(
    package: &CargoMetadataPackage,
    expected: &'a ExpectedSdk,
) -> Option<&'a ProtectedPackage> {
    expected.protected_graph.values().find(|canonical| {
        canonical.name == package.name
            && canonical.version == package.version
            && match (&canonical.source, &canonical.path, &package.source) {
                (Some(source), None, Some(actual)) => source == actual,
                (None, Some(path), None) => {
                    metadata_package_path(package, &expected.repository_root).as_deref()
                        == Some(path.as_str())
                }
                _ => false,
            }
    })
}

fn metadata_package_path(package: &CargoMetadataPackage, repository_root: &Path) -> Option<String> {
    let package_root = normalized_metadata_path(Path::new(&package.manifest_path))
        .parent()?
        .to_owned();
    let repository_root = normalized_metadata_path(repository_root);
    package_root
        .strip_prefix(repository_root)
        .ok()
        .map(|path| path.to_string_lossy().replace('\\', "/"))
}

/// `std::fs::canonicalize` returns a Windows verbatim path (`\\?\`) while
/// Cargo's JSON metadata emits ordinary drive paths. Compare their ordinary
/// forms so a no-reparse consumer snapshot still binds its private crates.
fn normalized_metadata_path(path: &Path) -> PathBuf {
    let value = path.to_string_lossy();
    if let Some(path) = value.strip_prefix(r"\\?\UNC\") {
        return PathBuf::from(format!(r"\\{path}"));
    }
    if let Some(path) = value.strip_prefix(r"\\?\") {
        return PathBuf::from(path);
    }
    path.to_owned()
}

fn normalized_edge(name: &str, to: &str, kind: &str, target: Option<&str>) -> String {
    format!("{name}\u{1f}{to}\u{1f}{kind}\u{1f}{}", target.unwrap_or(""))
}

fn string_set(values: &[String]) -> BTreeSet<&str> {
    values.iter().map(String::as_str).collect()
}

fn read_project_toml(root: &Path, path: &Path) -> Result<TomlValue, String> {
    let source = read_regular_utf8_file(root, path, MAX_CARGO_FILE_BYTES, "Cargo project file")?;
    source.parse::<TomlValue>().map_err(|error| {
        format!(
            "{} is not valid TOML: {error}",
            path.file_name().unwrap().display()
        )
    })
}

fn read_regular_utf8_file(
    root: &Path,
    path: &Path,
    maximum_bytes: u64,
    label: &str,
) -> Result<String, String> {
    let bytes = read_regular_bytes(root, path, maximum_bytes)?;
    String::from_utf8(bytes).map_err(|error| format!("{label} is not UTF-8: {error}"))
}

fn read_regular_bytes(root: &Path, path: &Path, maximum_bytes: u64) -> Result<Vec<u8>, String> {
    ensure_regular_project_path(root, path)?;
    let canonical = path
        .canonicalize()
        .map_err(|error| format!("{} cannot be canonicalized: {error}", path.display()))?;
    let canonical_root = root
        .canonicalize()
        .map_err(|error| format!("plugin root cannot be canonicalized: {error}"))?;
    if !canonical.starts_with(canonical_root) {
        return Err(format!(
            "{} resolves outside the plugin root",
            path.display()
        ));
    }
    let metadata = fs::metadata(&canonical)
        .map_err(|error| format!("{} cannot be read: {error}", path.display()))?;
    if metadata.len() > maximum_bytes {
        return Err(format!(
            "{} exceeds the {} byte input limit",
            path.file_name().unwrap_or_default().to_string_lossy(),
            maximum_bytes
        ));
    }
    fs::read(&canonical).map_err(|error| format!("{} cannot be read: {error}", path.display()))
}

fn collect_direct_dependencies(
    value: &TomlValue,
    prefix: &str,
    output: &mut BTreeMap<String, DirectDependency>,
) {
    let Some(table) = value.as_table() else {
        return;
    };
    for section in ["dependencies", "dev-dependencies", "build-dependencies"] {
        if let Some(dependencies) = table.get(section).and_then(TomlValue::as_table) {
            for (alias, specification) in dependencies {
                let location = format!("{prefix}{section}.{alias}");
                output.insert(
                    location.clone(),
                    DirectDependency::from_toml(alias, specification, location),
                );
            }
        }
    }
    if let Some(targets) = table.get("target").and_then(TomlValue::as_table) {
        for (target, target_value) in targets {
            collect_direct_dependencies(target_value, &format!("target.{target}."), output);
        }
    }
}

#[derive(Debug)]
struct DirectDependency {
    package: String,
    version: Option<String>,
    git: Option<String>,
    rev: Option<String>,
    path: bool,
    workspace: bool,
    malformed: bool,
}

impl DirectDependency {
    fn from_toml(alias: &str, value: &TomlValue, _location: String) -> Self {
        match value {
            TomlValue::String(version) => Self {
                package: alias.to_owned(),
                version: Some(version.to_owned()),
                git: None,
                rev: None,
                path: false,
                workspace: false,
                malformed: false,
            },
            TomlValue::Table(table) => Self {
                package: table
                    .get("package")
                    .and_then(TomlValue::as_str)
                    .unwrap_or(alias)
                    .to_owned(),
                version: table
                    .get("version")
                    .and_then(TomlValue::as_str)
                    .map(str::to_owned),
                git: table
                    .get("git")
                    .and_then(TomlValue::as_str)
                    .map(str::to_owned),
                rev: table
                    .get("rev")
                    .and_then(TomlValue::as_str)
                    .map(str::to_owned),
                path: table.contains_key("path"),
                workspace: table
                    .get("workspace")
                    .and_then(TomlValue::as_bool)
                    .unwrap_or(false),
                malformed: table
                    .get("package")
                    .is_some_and(|value| value.as_str().is_none())
                    || table
                        .get("git")
                        .is_some_and(|value| value.as_str().is_none())
                    || table
                        .get("rev")
                        .is_some_and(|value| value.as_str().is_none()),
            },
            _ => Self {
                package: alias.to_owned(),
                version: None,
                git: None,
                rev: None,
                path: false,
                workspace: false,
                malformed: true,
            },
        }
    }
}

fn validate_direct_dependency(
    location: &str,
    dependency: &DirectDependency,
    packages: &[&toml::map::Map<String, TomlValue>],
    private_names: &BTreeSet<&str>,
    expected: &ExpectedSdk,
    diagnostics: &mut Vec<Diagnostic>,
) {
    if dependency.malformed {
        diagnostics.push(diagnostic(
            "SESDK-CARGO-004",
            "cargo",
            location,
            "dependency declaration has an unsupported type",
        ));
        return;
    }
    let public_sdk_contract = match dependency.package.as_str() {
        PUBLIC_SDK_CONTRACT_DEPENDENCY => Some(PUBLIC_SDK_CONTRACT_VERSION),
        PUBLIC_UI_SDK_CONTRACT_DEPENDENCY => Some(PUBLIC_UI_SDK_CONTRACT_VERSION),
        _ => None,
    };
    if is_private_workspace_crate(&dependency.package) && public_sdk_contract.is_none() {
        diagnostics.push(diagnostic(
            "SESDK-CARGO-005",
            "cargo",
            location,
            "direct dependency references a SuperExplorer private workspace crate",
        ));
    }
    if public_sdk_contract.is_some_and(|required_version| {
        dependency.path
            || dependency.workspace
            || dependency.git.is_some()
            || dependency.version.as_deref() != Some(required_version)
    }) {
        diagnostics.push(diagnostic(
            "SESDK-CARGO-014",
            "cargo",
            location,
            "public SDK contracts must use their exact registry-pinned 1.2.0 dependencies",
        ));
    }
    if (dependency.path || dependency.workspace)
        && !private_names.contains(dependency.package.as_str())
    {
        diagnostics.push(diagnostic(
            "SESDK-CARGO-006",
            "cargo",
            location,
            "path and workspace dependencies are not reproducible plugin dependencies",
        ));
    }
    if dependency.git.is_some() && !dependency.rev.as_deref().is_some_and(is_full_git_revision) {
        diagnostics.push(diagnostic(
            "SESDK-CARGO-007",
            "cargo",
            location,
            "git dependency must declare an exact 40-character revision",
        ));
    }
    if dependency.git.is_none() && dependency.version.as_deref().is_none_or(str::is_empty) {
        diagnostics.push(diagnostic(
            "SESDK-CARGO-008",
            "cargo",
            location,
            "registry dependency must declare a version",
        ));
    }
    let matches = packages
        .iter()
        .filter(|package| {
            package.get("name").and_then(TomlValue::as_str) == Some(&dependency.package)
        })
        .collect::<Vec<_>>();
    if matches.is_empty() {
        diagnostics.push(diagnostic(
            "SESDK-CARGO-009",
            "cargo",
            location,
            "direct dependency is not resolved in Cargo.lock",
        ));
        return;
    }
    if !private_names.contains(dependency.package.as_str())
        && matches.iter().all(|package| {
            package
                .get("source")
                .and_then(TomlValue::as_str)
                .is_none_or(str::is_empty)
        })
    {
        diagnostics.push(diagnostic(
            "SESDK-CARGO-010",
            "cargo",
            location,
            "direct dependency has no immutable Cargo.lock source",
        ));
    }
    if let (Some(git), Some(revision)) = (&dependency.git, &dependency.rev) {
        let expected_prefix = format!("git+{git}");
        let expected_suffix = format!("#{revision}");
        if matches.iter().all(|package| {
            package
                .get("source")
                .and_then(TomlValue::as_str)
                .is_none_or(|source| {
                    !source.starts_with(&expected_prefix) || !source.ends_with(&expected_suffix)
                })
        }) {
            diagnostics.push(diagnostic(
                "SESDK-CARGO-011",
                "cargo",
                location,
                "Cargo.lock git source does not resolve the declared immutable revision",
            ));
        }
    }
    if expected.gpui_packages.contains(&dependency.package) {
        let source_is_exact = dependency.git.as_deref() == Some(expected.gpui_repository.as_str())
            && dependency.rev.as_deref() == Some(expected.gpui_revision.as_str());
        let lock_is_exact = matches.iter().any(|package| {
            package
                .get("source")
                .and_then(TomlValue::as_str)
                .is_some_and(|source| {
                    source.starts_with(&format!("git+{}", expected.gpui_repository))
                        && source.ends_with(&format!("#{}", expected.gpui_revision))
                })
        });
        if !source_is_exact || !lock_is_exact {
            diagnostics.push(diagnostic(
                "SESDK-CARGO-012",
                "compatibility",
                location,
                "protected GPUI dependency differs from the approved SDK source revision",
            ));
        }
    }
}

fn is_private_workspace_crate(name: &str) -> bool {
    name.starts_with("explorer-") || name.starts_with("superexplorer-")
}

fn is_full_git_revision(value: &str) -> bool {
    value.len() == 40 && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

fn ensure_regular_project_path(root: &Path, path: &Path) -> Result<(), String> {
    let relative = path
        .strip_prefix(root)
        .map_err(|_| "project file escapes plugin root".to_owned())?;
    ensure_regular_relative_path(root, relative)
}

fn ensure_regular_relative_path(root: &Path, relative: &Path) -> Result<(), String> {
    let mut current = root.to_owned();
    let mut traversed = PathBuf::new();
    for component in relative.components() {
        current.push(component);
        traversed.push(component);
        if is_link_or_reparse(&current)? {
            return Err(format!(
                "{} is a symlink or reparse point",
                traversed.display()
            ));
        }
    }
    let metadata = fs::metadata(&current)
        .map_err(|error| format!("{} is unavailable: {error}", relative.display()))?;
    if !metadata.is_file() {
        return Err(format!("{} is not a regular file", relative.display()));
    }
    Ok(())
}

fn is_link_or_reparse(path: &Path) -> Result<bool, String> {
    let metadata = fs::symlink_metadata(path)
        .map_err(|error| format!("filesystem metadata cannot be read: {error}"))?;
    if metadata.file_type().is_symlink() {
        return Ok(true);
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x400;
        Ok(metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0)
    }
    #[cfg(not(windows))]
    Ok(false)
}

fn validate_payload(root: &Path, payload: &Payload, path: &str, diagnostics: &mut Vec<Diagnostic>) {
    if !lower_hex(&payload.sha256, 64) {
        diagnostics.push(diagnostic(
            "SESDK-HASH-001",
            "payload",
            path,
            "payload SHA-256 is not lowercase hexadecimal",
        ));
        return;
    }
    let candidate = root.join(PathBuf::from(&payload.path));
    if let Err(message) = ensure_regular_project_path(root, &candidate) {
        diagnostics.push(diagnostic("SESDK-PAYLOAD-003", "payload", path, message));
        return;
    }
    let Ok(canonical) = candidate.canonicalize() else {
        diagnostics.push(diagnostic(
            "SESDK-PAYLOAD-003",
            "payload",
            path,
            "declared payload is missing",
        ));
        return;
    };
    if !canonical.starts_with(root) {
        diagnostics.push(diagnostic(
            "SESDK-PATH-003",
            "payload",
            path,
            "payload resolves outside the plugin root",
        ));
        return;
    }
    let Ok(metadata) = fs::metadata(&canonical) else {
        diagnostics.push(diagnostic(
            "SESDK-PAYLOAD-004",
            "payload",
            path,
            "declared payload metadata cannot be read",
        ));
        return;
    };
    if metadata.len() > MAX_PAYLOAD_BYTES {
        diagnostics.push(diagnostic(
            "SESDK-BOUND-004",
            "payload",
            path,
            format!(
                "payload on disk exceeds the {} byte limit",
                MAX_PAYLOAD_BYTES
            ),
        ));
        return;
    }
    match fs::read(&canonical) {
        Ok(bytes) if bytes.len() as u64 == payload.size && sha256_hex(&bytes) == payload.sha256 => {
        }
        Ok(_) => diagnostics.push(diagnostic(
            "SESDK-HASH-002",
            "payload",
            path,
            "payload size or SHA-256 differs from the manifest",
        )),
        Err(error) => diagnostics.push(diagnostic(
            "SESDK-PAYLOAD-004",
            "payload",
            path,
            format!("payload cannot be read: {error}"),
        )),
    }
}

fn validate_verification(
    verification: &Verification,
    trusted: &BTreeMap<String, Evidence>,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let mut requirements = BTreeSet::new();
    for (index, requirement) in verification.requirements.iter().enumerate() {
        let path = format!("verification.requirements[{index}]");
        let evidence = &requirement.evidence;
        let trusted_evidence = trusted.get(&requirement.requirement_id);
        if !valid_requirement_id(&requirement.requirement_id)
            || !requirements.insert(&requirement.requirement_id)
            || trusted_evidence.is_none()
            || evidence.unit.is_empty()
            || evidence.integration.is_empty()
            || evidence.security.is_empty()
            || evidence.docs.is_empty()
            || [
                &evidence.unit,
                &evidence.integration,
                &evidence.uitest,
                &evidence.security,
                &evidence.docs,
            ]
            .into_iter()
            .flatten()
            .any(|id| !valid_id(id))
        {
            diagnostics.push(diagnostic(
                "SESDK-EVIDENCE-001",
                "verification",
                &path,
                "requirement ID or required evidence is missing, duplicated, or invalid",
            ));
        }
        if trusted_evidence.is_some_and(|expected| !same_evidence(&requirement.evidence, expected))
        {
            diagnostics.push(diagnostic(
                "SESDK-EVIDENCE-002",
                "verification",
                &path,
                "manifest evidence differs from the trusted gate mapping",
            ));
        }
    }
    for requirement_id in trusted.keys() {
        if !requirements.contains(requirement_id) {
            diagnostics.push(diagnostic(
                "SESDK-EVIDENCE-003",
                "verification",
                "verification.requirements",
                format!("required trusted gate is missing: {requirement_id}"),
            ));
        }
    }
}

fn same_evidence(left: &Evidence, right: &Evidence) -> bool {
    same_strings(&left.unit, &right.unit)
        && same_strings(&left.integration, &right.integration)
        && same_strings(&left.uitest, &right.uitest)
        && same_strings(&left.security, &right.security)
        && same_strings(&left.docs, &right.docs)
}

fn same_strings(left: &[String], right: &[String]) -> bool {
    left.iter().collect::<BTreeSet<_>>() == right.iter().collect::<BTreeSet<_>>()
        && left.len() == right.len()
}

fn diagnostic(code: &str, phase: &str, path: &str, message: impl Into<String>) -> Diagnostic {
    Diagnostic {
        code: code.into(),
        severity: "error".into(),
        phase: phase.into(),
        path: path.into(),
        message: message.into(),
    }
}

fn redact_plugin_paths(diagnostics: &mut [Diagnostic], root: &Path) {
    let mut roots = vec![root.to_string_lossy().into_owned()];
    if let Ok(canonical) = root.canonicalize() {
        roots.push(canonical.to_string_lossy().into_owned());
    }
    for diagnostic in diagnostics {
        for root in &roots {
            diagnostic.path = diagnostic.path.replace(root, "<plugin-root>");
            diagnostic.message = diagnostic.message.replace(root, "<plugin-root>");
        }
        diagnostic.path = redact_absolute_paths(&diagnostic.path);
        diagnostic.message = redact_absolute_paths(&diagnostic.message);
    }
}

fn redact_absolute_paths(value: &str) -> String {
    let characters = value.chars().collect::<Vec<_>>();
    let mut output = String::with_capacity(value.len());
    let mut cursor = 0_usize;
    while cursor < characters.len() {
        let is_drive_path = cursor + 2 < characters.len()
            && characters[cursor].is_ascii_alphabetic()
            && characters[cursor + 1] == ':'
            && matches!(characters[cursor + 2], '\\' | '/');
        let is_unc_path = cursor + 1 < characters.len()
            && characters[cursor] == '\\'
            && characters[cursor + 1] == '\\';
        if !is_drive_path && !is_unc_path {
            output.push(characters[cursor]);
            cursor += 1;
            continue;
        }
        output.push_str("<path>");
        let quote = cursor
            .checked_sub(1)
            .and_then(|index| characters.get(index))
            .copied()
            .filter(|character| matches!(character, '\'' | '"' | '`'));
        cursor += if is_drive_path { 3 } else { 2 };
        while cursor < characters.len()
            && !matches!(characters[cursor], '\n' | '\r')
            && quote.is_none_or(|delimiter| characters[cursor] != delimiter)
            && !(quote.is_none()
                && characters[cursor].is_whitespace()
                && characters
                    .get(cursor + 1)
                    .is_some_and(|next| !next.is_ascii_alphanumeric()))
        {
            cursor += 1;
        }
    }
    output
}

fn valid_id(value: &str) -> bool {
    (1..=64).contains(&value.len())
        && value.bytes().all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || matches!(byte, b'.' | b'_' | b'-')
        })
        && value
            .bytes()
            .next()
            .is_some_and(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit())
}

fn valid_version(value: &str) -> bool {
    let parts: Vec<_> = value.split('.').collect();
    parts.len() == 3
        && parts
            .iter()
            .all(|part| !part.is_empty() && part.bytes().all(|byte| byte.is_ascii_digit()))
}

fn valid_requirement_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 200
        && value.bytes().all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || matches!(byte, b'/' | b'-')
        })
        && !value.contains("//")
}

fn has_duplicates(values: &[String]) -> bool {
    let mut unique = BTreeSet::new();
    values.iter().any(|value| !unique.insert(value))
}

fn lower_hex(value: &str, length: usize) -> bool {
    value.len() == length
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn safe_relative_path(value: &str) -> bool {
    if value.is_empty()
        || !value.is_ascii()
        || value.starts_with('/')
        || value.starts_with('\\')
        || value.contains('\\')
        || value.contains(':')
        || value.bytes().any(|byte| byte.is_ascii_control())
    {
        return false;
    }
    value.split('/').all(|segment| {
        if segment.is_empty() || matches!(segment, "." | "..") || segment.ends_with([' ', '.']) {
            return false;
        }
        let stem = segment
            .split('.')
            .next()
            .unwrap_or_default()
            .to_ascii_uppercase();
        !matches!(
            stem.as_str(),
            "CON"
                | "PRN"
                | "AUX"
                | "NUL"
                | "COM1"
                | "COM2"
                | "COM3"
                | "COM4"
                | "COM5"
                | "COM6"
                | "COM7"
                | "COM8"
                | "COM9"
                | "LPT1"
                | "LPT2"
                | "LPT3"
                | "LPT4"
                | "LPT5"
                | "LPT6"
                | "LPT7"
                | "LPT8"
                | "LPT9"
        )
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        sync::atomic::{AtomicU64, Ordering},
        time::{SystemTime, UNIX_EPOCH},
    };

    static TEST_DIRECTORY_SEQUENCE: AtomicU64 = AtomicU64::new(0);

    struct TestDirectory(PathBuf);

    impl TestDirectory {
        fn new() -> Self {
            let sequence = TEST_DIRECTORY_SEQUENCE.fetch_add(1, Ordering::Relaxed);
            let stamp = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("system clock is before Unix epoch")
                .as_nanos();
            let path = std::env::temp_dir().join(format!(
                "superexplorer-plugin-tooling-{stamp}-{}-{sequence}",
                std::process::id()
            ));
            fs::create_dir_all(&path).expect("create isolated test directory");
            Self(path)
        }
    }

    impl Drop for TestDirectory {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    fn expected_for_test() -> ExpectedSdk {
        ExpectedSdk {
            repository_root: PathBuf::from("D:/sdk"),
            bundle_id: "sdk-test".into(),
            target: "x86_64-pc-windows-msvc".into(),
            abi_schema: 1,
            rustc_release: "1.97.1".into(),
            rustc_commit_hash: "a".into(),
            rustc_sha256: "c".repeat(64),
            cargo_release: "1.97.1".into(),
            cargo_commit_hash: "b".into(),
            cargo_sha256: "d".repeat(64),
            ui_abi_fingerprint: "a".repeat(64),
            gpui_repository: "https://github.com/damody/gpui-ce-explorer.git".into(),
            gpui_revision: "a".repeat(40),
            gpui_packages: BTreeSet::from(["gpui".into()]),
            protected_graph: BTreeMap::new(),
            release_profile: ReleaseProfilePolicy {
                panic: "unwind".into(),
                lto: "thin".into(),
                codegen_units: 1,
                strip: None,
                overflow_checks: None,
            },
            gates: BTreeMap::new(),
        }
    }

    fn manifest_for_test() -> Manifest {
        Manifest {
            schema_version: 1,
            package: Package {
                id: "plugin".into(),
                version: "0.1.0".into(),
            },
            publisher: Publisher {
                id: "publisher".into(),
                display_name: "Publisher".into(),
                contacts: vec![],
            },
            sdk: Sdk {
                bundle_id: "sdk-test".into(),
                target: "x86_64-pc-windows-msvc".into(),
                abi_schema: 1,
                gpui: false,
                ui_abi_fingerprint: None,
            },
            rust: RustPlugin {
                crate_name: "plugin".into(),
                entrypoint: "plugin.dll".into(),
            },
            features: vec![],
            contributions: vec![],
            payloads: vec![],
            private_dependencies: vec![],
            verification: Verification {
                requirements: vec![],
            },
        }
    }

    #[test]
    fn folder_size_template_materializes_only_inside_the_snapshot() {
        let temporary = TestDirectory::new();
        fs::create_dir_all(temporary.0.join("src")).expect("create source directory");
        fs::write(temporary.0.join("src/lib.rs"), b"folder-size source").expect("write source");
        let template = r#"{
  "schema_version": 1,
  "package": { "id": "rust-folder-size-visual-column", "version": "0.1.0" },
  "publisher": { "id": "example-publisher", "display_name": "Example", "contacts": [{ "kind": "support", "value": "support@example.invalid" }] },
  "sdk": { "bundle_id": "@SDK_BUNDLE_ID@", "target": "x86_64-pc-windows-msvc", "abi_schema": @ABI_SCHEMA@, "gpui": false, "ui_abi_fingerprint": null },
  "rust": { "crate_name": "rust-folder-size-visual-column", "entrypoint": "plugin.dll" },
  "features": [
    { "id": "column", "capabilities": ["abi", "filesystem.read"] },
    { "id": "recalculate", "capabilities": ["abi"] },
    { "id": "settings", "capabilities": ["abi"] }
  ],
  "contributions": [
    { "id": "abi-root", "feature_id": "column", "kind": "abi-root", "capabilities": ["abi"], "payload": "src/lib.rs" },
    { "id": "folder-size", "feature_id": "column", "kind": "column", "capabilities": ["abi", "filesystem.read"], "payload": "src/lib.rs" },
    { "id": "folder-size-renderer", "feature_id": "column", "kind": "renderer", "capabilities": ["abi"], "payload": "src/lib.rs" }
  ],
  "payloads": [{ "path": "src/lib.rs", "size": @SOURCE_SIZE@, "sha256": "@SOURCE_SHA256@", "kind": "rust-source" }],
  "private_dependencies": [],
  "verification": { "requirements": [] }
}"#;
        let manifest_path = temporary.0.join("plugin-project.json");
        fs::write(&manifest_path, template).expect("write template");

        let report = materialize_folder_size_template(&temporary.0, "sdk-test", 1)
            .expect("materialize template");
        let resolved = fs::read_to_string(&manifest_path).expect("read resolved template");
        assert!(report.materialized);
        assert_ne!(
            report.template_manifest_sha256,
            report.resolved_manifest_sha256
        );
        assert!(resolved.contains("\"bundle_id\": \"sdk-test\""));
        assert!(!resolved.contains("@SDK_BUNDLE_ID@"));
        assert!(!resolved.contains("@ABI_SCHEMA@"));
        assert!(!resolved.contains("@SOURCE_SIZE@"));
        assert!(!resolved.contains("@SOURCE_SHA256@"));
        assert!(serde_json::from_str::<Manifest>(&resolved).is_ok());
    }

    #[test]
    fn size_map_template_materializes_with_its_exact_manifest_declarations() {
        let temporary = TestDirectory::new();
        fs::create_dir_all(temporary.0.join("src")).expect("create source directory");
        fs::write(temporary.0.join("src/lib.rs"), b"size-map source").expect("write source");
        let template = r#"{
  "schema_version": 1,
  "package": { "id": "rust-folder-size-map-view", "version": "0.1.0" },
  "publisher": { "id": "example-publisher", "display_name": "Example", "contacts": [{ "kind": "support", "value": "support@example.invalid" }] },
  "sdk": { "bundle_id": "@SDK_BUNDLE_ID@", "target": "x86_64-pc-windows-msvc", "abi_schema": @ABI_SCHEMA@, "gpui": false, "ui_abi_fingerprint": null },
  "rust": { "crate_name": "rust-folder-size-map-view", "entrypoint": "plugin.dll" },
  "features": [{ "id": "view", "capabilities": ["abi", "filesystem.read"] }],
  "contributions": [
    { "id": "abi-root", "feature_id": "view", "kind": "abi-root", "capabilities": ["abi"], "payload": "src/lib.rs" },
    { "id": "size-map", "feature_id": "view", "kind": "view-mode", "capabilities": ["abi"], "payload": "src/lib.rs" }
  ],
  "payloads": [{ "path": "src/lib.rs", "size": @SOURCE_SIZE@, "sha256": "@SOURCE_SHA256@", "kind": "rust-source" }],
  "private_dependencies": [],
  "verification": { "requirements": [] }
}"#;
        let manifest_path = temporary.0.join("plugin-project.json");
        fs::write(&manifest_path, template).expect("write template");

        let report = materialize_folder_size_template(&temporary.0, "sdk-test", 1)
            .expect("materialize size-map template");
        let resolved = fs::read_to_string(&manifest_path).expect("read resolved template");
        let manifest = serde_json::from_str::<Manifest>(&resolved).expect("resolved manifest");

        assert!(report.materialized);
        assert_eq!(manifest.package.id, "rust-folder-size-map-view");
        assert!(is_exact_size_map_declarations(&manifest));
    }

    #[test]
    fn package_entrypoint_must_be_the_canonical_dll_name() {
        let directory = TestDirectory::new();
        let mut manifest = manifest_for_test();
        manifest.rust.entrypoint = "nested/plugin.dll".into();

        let diagnostics = validate_manifest(&manifest, &directory.0, &expected_for_test());

        assert!(has_code(&diagnostics, "SESDK-PATH-001"));
    }

    #[test]
    fn consumer_cargo_configuration_is_rejected() {
        let directory = TestDirectory::new();
        let cargo_directory = directory.0.join(".cargo");
        fs::create_dir(&cargo_directory).expect("create Cargo configuration directory");
        fs::write(cargo_directory.join("config.toml"), "[build]\n").expect("write Cargo config");

        assert!(ensure_no_consumer_cargo_config(&directory.0).is_err());
    }

    #[test]
    fn input_identity_detects_changed_content() {
        let directory = TestDirectory::new();
        let path = directory.0.join("payload.txt");
        fs::write(&path, "before").expect("write initial payload");
        let identity = InputIdentity {
            relative: PathBuf::from("payload.txt"),
            size: 6,
            sha256: sha256_hex(b"before"),
        };
        fs::write(path, "after!").expect("mutate payload");

        assert!(verify_input_identities(&directory.0, &[identity]).is_err());
    }

    #[test]
    fn cargo_environment_filter_rejects_toolchain_overrides() {
        assert!(is_forbidden_cargo_environment_name("RUSTC_WRAPPER"));
        assert!(is_forbidden_cargo_environment_name("CARGO_BUILD_RUSTC"));
        assert!(is_forbidden_cargo_environment_name(
            "CARGO_TARGET_X86_64_PC_WINDOWS_MSVC_LINKER"
        ));
        assert!(!is_forbidden_cargo_environment_name("CARGO_HOME"));
    }

    #[test]
    fn toolchain_field_reads_exact_key_value_pairs() {
        let output = "cargo 1.97.1\nrelease: 1.97.1\ncommit-hash: abc123\n";

        assert_eq!(toolchain_field(output, "release"), Some("1.97.1"));
        assert_eq!(toolchain_field(output, "commit-hash"), Some("abc123"));
        assert_eq!(toolchain_field(output, "host"), None);
    }

    #[test]
    fn diagnostic_path_redaction_preserves_unicode_text() {
        let message = "驗證失敗：D:\\temporary\\外掛\\plugin-project.json 不可讀";
        let redacted = redact_absolute_paths(message);

        assert_eq!(redacted, "驗證失敗：<path> 不可讀");
    }

    #[test]
    fn diagnostic_path_redaction_consumes_spaced_drive_unc_and_extended_paths() {
        assert_eq!(
            redact_absolute_paths("failed at D:\\Program Files\\Secret\\x"),
            "failed at <path>"
        );
        assert_eq!(
            redact_absolute_paths("failed at \\\\server\\share name\\Secret\\x"),
            "failed at <path>"
        );
        assert_eq!(
            redact_absolute_paths("failed at \\\\?\\C:\\Program Files\\Secret\\x"),
            "failed at <path>"
        );
        assert_eq!(
            redact_absolute_paths("failed at \"D:\\Program Files\\Secret\\x\" safely"),
            "failed at \"<path>\" safely"
        );
    }

    fn dependency_diagnostics(
        location: &str,
        dependency: DirectDependency,
        source: &str,
    ) -> Vec<Diagnostic> {
        let lock = source.parse::<TomlValue>().expect("valid test lock");
        let packages = lock
            .get("package")
            .and_then(TomlValue::as_array)
            .and_then(|values| {
                values
                    .iter()
                    .map(TomlValue::as_table)
                    .collect::<Option<Vec<_>>>()
            })
            .unwrap_or_default();
        let mut diagnostics = Vec::new();
        validate_direct_dependency(
            location,
            &dependency,
            &packages,
            &BTreeSet::new(),
            &expected_for_test(),
            &mut diagnostics,
        );
        diagnostics
    }

    fn expected_protected_for_test() -> ExpectedSdk {
        let mut expected = expected_for_test();
        expected.protected_graph = BTreeMap::from([
            (
                "gpui@0.2.2#vendor/gpui-ce/crates/gpui".into(),
                ProtectedPackage {
                    key: "gpui@0.2.2#vendor/gpui-ce/crates/gpui".into(),
                    name: "gpui".into(),
                    version: "0.2.2".into(),
                    source: None,
                    path: Some("vendor/gpui-ce/crates/gpui".into()),
                    checksum: None,
                    features: vec!["default".into()],
                    dependencies: vec![ProtectedDependency {
                        name: "abi_stable".into(),
                        to: "abi_stable@0.11.3#registry+https://github.com/rust-lang/crates.io-index"
                            .into(),
                        dep_kinds: vec![ProtectedDependencyKind {
                            kind: "normal".into(),
                            target: None,
                        }],
                    }],
                },
            ),
            (
                "abi_stable@0.11.3#registry+https://github.com/rust-lang/crates.io-index".into(),
                ProtectedPackage {
                    key: "abi_stable@0.11.3#registry+https://github.com/rust-lang/crates.io-index"
                        .into(),
                    name: "abi_stable".into(),
                    version: "0.11.3".into(),
                    source: Some("registry+https://github.com/rust-lang/crates.io-index".into()),
                    path: None,
                    checksum: Some("c".repeat(64)),
                    features: vec!["std".into()],
                    dependencies: vec![],
                },
            ),
        ]);
        expected
    }

    fn protected_metadata_for_test() -> CargoMetadata {
        serde_json::from_value(serde_json::json!({
            "packages": [
                {"id":"plugin 0.1.0 (path+file:///plugin)","name":"plugin","version":"0.1.0","source":null,"manifest_path":"D:/plugin/Cargo.toml"},
                {"id":"gpui 0.2.2 (path+file:///sdk/vendor/gpui-ce/crates/gpui)","name":"gpui","version":"0.2.2","source":null,"manifest_path":"D:/sdk/vendor/gpui-ce/crates/gpui/Cargo.toml"},
                {"id":"abi_stable 0.11.3 (registry+https://github.com/rust-lang/crates.io-index)","name":"abi_stable","version":"0.11.3","source":"registry+https://github.com/rust-lang/crates.io-index","manifest_path":"D:/cache/abi_stable/Cargo.toml"}
            ],
            "resolve": {"root":"plugin 0.1.0 (path+file:///plugin)","nodes":[
                {"id":"plugin 0.1.0 (path+file:///plugin)","features":[],"deps":[{"name":"gpui","pkg":"gpui 0.2.2 (path+file:///sdk/vendor/gpui-ce/crates/gpui)","dep_kinds":[{"kind":null,"target":null}]}]},
                {"id":"gpui 0.2.2 (path+file:///sdk/vendor/gpui-ce/crates/gpui)","features":["default"],"deps":[{"name":"abi_stable","pkg":"abi_stable 0.11.3 (registry+https://github.com/rust-lang/crates.io-index)","dep_kinds":[{"kind":null,"target":null}]}]},
                {"id":"abi_stable 0.11.3 (registry+https://github.com/rust-lang/crates.io-index)","features":["std"],"deps":[]}
            ]}
        }))
        .expect("valid injected metadata")
    }

    #[test]
    fn metadata_paths_normalize_windows_verbatim_snapshot_roots() {
        let package = CargoMetadataPackage {
            id: "private".into(),
            name: "private".into(),
            version: "1.0.0".into(),
            source: None,
            manifest_path: r"D:\plugin\vendor\private\crate\Cargo.toml".into(),
        };
        assert_eq!(
            metadata_package_path(&package, Path::new(r"\\?\D:\plugin")),
            Some("vendor/private/crate".into())
        );
    }

    fn protected_lock_for_test() -> Vec<CargoLockPackage> {
        vec![
            CargoLockPackage {
                name: "gpui".into(),
                version: "0.2.2".into(),
                source: None,
                checksum: None,
            },
            CargoLockPackage {
                name: "abi_stable".into(),
                version: "0.11.3".into(),
                source: Some("registry+https://github.com/rust-lang/crates.io-index".into()),
                checksum: Some("c".repeat(64)),
            },
        ]
    }

    fn protected_diagnostics(metadata: &CargoMetadata, gpui_plugin: bool) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        validate_protected_metadata(
            metadata,
            &protected_lock_for_test(),
            gpui_plugin,
            &expected_protected_for_test(),
            &mut diagnostics,
        );
        diagnostics
    }

    fn private_fixture_contract() -> (
        PathBuf,
        TomlValue,
        Vec<CargoLockPackage>,
        CargoMetadata,
        Vec<PrivateDependency>,
    ) {
        let sdk_root = Path::new(env!("CARGO_MANIFEST_DIR"))
            .ancestors()
            .nth(2)
            .expect("sdk root")
            .to_owned();
        let root = sdk_root.join("fixtures/private-dependency-contract");
        let vendor = "vendor/private/exif-lite-0.1.0";
        let private = PrivateDependency {
            name: "exif-lite".into(),
            version: "0.1.0".into(),
            path: vendor.into(),
            tree_sha256: private_dependency_tree_sha256(&root, vendor)
                .expect("fixture vendor tree"),
            provenance: PrivateDependencyProvenance {
                source: CRATES_IO_REGISTRY_SOURCE.into(),
                crate_sha256: "f".repeat(64),
                license_expression: "MIT OR Apache-2.0".into(),
                license_hashes: BTreeMap::from([
                    (
                        "LICENSE-MIT".into(),
                        sha256_hex(&fs::read(root.join(vendor).join("LICENSE-MIT")).unwrap()),
                    ),
                    (
                        "LICENSE-APACHE".into(),
                        sha256_hex(&fs::read(root.join(vendor).join("LICENSE-APACHE")).unwrap()),
                    ),
                ]),
            },
        };
        let cargo_toml =
            read_project_toml(&root, &root.join("Cargo.toml")).expect("fixture Cargo.toml");
        let lock = read_project_toml(&root, &root.join("Cargo.lock")).expect("fixture Cargo.lock");
        let package_tables = lock
            .get("package")
            .and_then(TomlValue::as_array)
            .unwrap()
            .iter()
            .map(TomlValue::as_table)
            .collect::<Option<Vec<_>>>()
            .unwrap();
        let output = Command::new("cargo")
            .args(["metadata", "--locked", "--offline", "--format-version", "1"])
            .current_dir(&root)
            .env("CARGO_NET_OFFLINE", "true")
            .output()
            .expect("run Cargo metadata for private fixture");
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let metadata = serde_json::from_slice(&output.stdout).expect("fixture Cargo metadata");
        (
            root,
            cargo_toml,
            cargo_lock_packages(&package_tables),
            metadata,
            vec![private],
        )
    }

    fn private_binding_diagnostics(
        root: &Path,
        cargo_toml: &TomlValue,
        lock: &[CargoLockPackage],
        metadata: &CargoMetadata,
        dependencies: &[PrivateDependency],
        expected: &ExpectedSdk,
    ) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        validate_private_dependencies(root, dependencies, &mut diagnostics);
        validate_private_dependency_cargo_binding(
            root,
            cargo_toml,
            lock,
            metadata,
            dependencies,
            expected,
            &mut diagnostics,
        );
        diagnostics
    }

    fn copied_private_fixture_vendor() -> (TestDirectory, PrivateDependency) {
        let (fixture_root, _, _, _, dependencies) = private_fixture_contract();
        let directory = TestDirectory::new();
        let relative = Path::new("vendor/private/exif-lite-0.1.0");
        let source_root = fixture_root.join(relative);
        let destination_root = directory.0.join(relative);
        for file in [
            ".cargo-checksum.json",
            "Cargo.toml",
            "LICENSE-APACHE",
            "LICENSE-MIT",
            "src/lib.rs",
        ] {
            let source = source_root.join(file);
            let destination = destination_root.join(file);
            fs::create_dir_all(destination.parent().unwrap())
                .expect("create copied fixture parent");
            fs::write(&destination, fs::read(source).expect("read fixture file"))
                .expect("copy fixture file");
        }
        let mut dependency = dependencies.into_iter().next().unwrap();
        dependency.tree_sha256 = private_dependency_tree_sha256(&directory.0, &dependency.path)
            .expect("copied fixture tree");
        (directory, dependency)
    }

    fn private_vendor_diagnostics(root: &Path, dependency: &PrivateDependency) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        validate_private_dependencies(root, std::slice::from_ref(dependency), &mut diagnostics);
        validate_private_vendor_provenance(
            root,
            dependency,
            "private_dependencies[0]",
            &mut diagnostics,
        );
        diagnostics
    }

    fn has_code(diagnostics: &[Diagnostic], code: &str) -> bool {
        diagnostics.iter().any(|diagnostic| diagnostic.code == code)
    }

    #[test]
    fn private_exif_fixture_binds_patch_lock_metadata_and_provenance() {
        let (root, cargo_toml, lock, metadata, dependencies) = private_fixture_contract();
        let diagnostics = private_binding_diagnostics(
            &root,
            &cargo_toml,
            &lock,
            &metadata,
            &dependencies,
            &expected_for_test(),
        );
        assert!(diagnostics.is_empty(), "{diagnostics:#?}");
    }

    #[test]
    fn private_exif_fixture_mutations_fail_closed() {
        let (root, cargo_toml, lock, metadata, dependencies) = private_fixture_contract();

        let mut nested_path = dependencies.clone();
        nested_path[0].path = "vendor/private/exif-lite/0.1.0".into();
        assert!(has_code(
            &private_binding_diagnostics(
                &root,
                &cargo_toml,
                &lock,
                &metadata,
                &nested_path,
                &expected_for_test(),
            ),
            "SESDK-PRIVATE-001"
        ));

        let mut wrong_leaf = dependencies.clone();
        wrong_leaf[0].path = "vendor/private/not-exif-lite-0.1.0".into();
        assert!(has_code(
            &private_binding_diagnostics(
                &root,
                &cargo_toml,
                &lock,
                &metadata,
                &wrong_leaf,
                &expected_for_test(),
            ),
            "SESDK-PRIVATE-001"
        ));

        let mut wrong_patch = cargo_toml.clone();
        wrong_patch["patch"]["crates-io"]["exif-lite"]["path"] =
            TomlValue::String("vendor/private/other".into());
        assert!(has_code(
            &private_binding_diagnostics(
                &root,
                &wrong_patch,
                &lock,
                &metadata,
                &dependencies,
                &expected_for_test(),
            ),
            "SESDK-PRIVATE-005"
        ));

        let mut wrong_source = dependencies.clone();
        wrong_source[0].provenance.source = "registry+https://example.invalid/index".into();
        assert!(has_code(
            &private_binding_diagnostics(
                &root,
                &cargo_toml,
                &lock,
                &metadata,
                &wrong_source,
                &expected_for_test(),
            ),
            "SESDK-PRIVATE-001"
        ));

        let mut wrong_version = dependencies.clone();
        wrong_version[0].version = "0.1.1".into();
        assert!(has_code(
            &private_binding_diagnostics(
                &root,
                &cargo_toml,
                &lock,
                &metadata,
                &wrong_version,
                &expected_for_test(),
            ),
            "SESDK-PRIVATE-006"
        ));

        let mut wrong_checksum = dependencies.clone();
        wrong_checksum[0].provenance.crate_sha256 = "a".repeat(64);
        assert!(has_code(
            &private_binding_diagnostics(
                &root,
                &cargo_toml,
                &lock,
                &metadata,
                &wrong_checksum,
                &expected_for_test(),
            ),
            "SESDK-PRIVATE-012"
        ));

        let mut wrong_license = dependencies.clone();
        wrong_license[0]
            .provenance
            .license_hashes
            .insert("LICENSE-MIT".into(), "a".repeat(64));
        assert!(has_code(
            &private_binding_diagnostics(
                &root,
                &cargo_toml,
                &lock,
                &metadata,
                &wrong_license,
                &expected_for_test(),
            ),
            "SESDK-PRIVATE-004"
        ));

        let mut unreachable = metadata.clone();
        let root_id = unreachable.resolve.as_ref().unwrap().root.clone().unwrap();
        unreachable
            .resolve
            .as_mut()
            .unwrap()
            .nodes
            .iter_mut()
            .find(|node| node.id == root_id)
            .unwrap()
            .deps
            .clear();
        assert!(has_code(
            &private_binding_diagnostics(
                &root,
                &cargo_toml,
                &lock,
                &unreachable,
                &dependencies,
                &expected_for_test(),
            ),
            "SESDK-PRIVATE-008"
        ));

        let mut extra = metadata.clone();
        let extra_id = "extra-private 0.1.0 (path+file:///private)".to_owned();
        extra.packages.push(CargoMetadataPackage {
            id: extra_id.clone(),
            name: "extra-private".into(),
            version: "0.1.0".into(),
            source: None,
            manifest_path: root
                .join("vendor/private/extra-private-0.1.0/Cargo.toml")
                .to_string_lossy()
                .into_owned(),
        });
        extra
            .resolve
            .as_mut()
            .unwrap()
            .nodes
            .push(CargoMetadataNode {
                id: extra_id.clone(),
                deps: vec![],
                features: vec![],
            });
        extra
            .resolve
            .as_mut()
            .unwrap()
            .nodes
            .iter_mut()
            .find(|node| node.id == root_id)
            .unwrap()
            .deps
            .push(CargoMetadataDependency {
                name: "extra-private".into(),
                pkg: extra_id,
                dep_kinds: vec![CargoMetadataDependencyKind {
                    kind: None,
                    target: None,
                }],
            });
        assert!(has_code(
            &private_binding_diagnostics(
                &root,
                &cargo_toml,
                &lock,
                &extra,
                &dependencies,
                &expected_for_test(),
            ),
            "SESDK-PRIVATE-009"
        ));

        let mut shadow = dependencies.clone();
        shadow[0].name = "abi_stable".into();
        assert!(has_code(
            &private_binding_diagnostics(
                &root,
                &cargo_toml,
                &lock,
                &metadata,
                &shadow,
                &expected_protected_for_test(),
            ),
            "SESDK-PRIVATE-010"
        ));
    }

    #[test]
    fn private_vendor_checksum_inventory_rejects_tampered_source_license_and_extra_files() {
        for (path, replacement) in [
            ("src/lib.rs", b"tampered parser source".as_slice()),
            ("LICENSE-MIT", b"tampered license text".as_slice()),
            ("README.md", b"undeclared extra file".as_slice()),
        ] {
            let (directory, dependency) = copied_private_fixture_vendor();
            let target = directory.0.join(&dependency.path).join(path);
            if let Some(parent) = target.parent() {
                fs::create_dir_all(parent).expect("create mutation parent");
            }
            fs::write(target, replacement).expect("mutate copied fixture");
            assert!(
                has_code(
                    &private_vendor_diagnostics(&directory.0, &dependency),
                    "SESDK-PRIVATE-012"
                ),
                "mutation {path} must invalidate the vendored file inventory"
            );
        }

        let hash = "a".repeat(64);
        let collision = serde_json::json!({
            "files": { "src/lib.rs": hash, "SRC/lib.rs": "b".repeat(64) },
            "package": "f".repeat(64),
        });
        assert!(cargo_checksum_file_hashes(&collision).is_none());
    }

    #[test]
    fn stage_package_emits_only_runtime_payloads_without_private_dependencies() {
        let directory = TestDirectory::new();
        let dll = directory.0.join("plugin.dll");
        fs::write(&dll, b"runtime dll").expect("write test dll");
        let output = directory.0.join("stage");
        let mut manifest = manifest_for_test();
        manifest.publisher.contacts.push(Contact {
            kind: "support".into(),
            value: "support@example.invalid".into(),
        });

        stage_validated_package(&directory.0, &manifest, &dll, &output)
            .expect("stage package without private dependencies");

        let manifest: Value = serde_json::from_slice(
            &fs::read(output.join("manifest.json")).expect("read staged manifest"),
        )
        .expect("parse staged manifest");
        let payloads = manifest["payloads"].as_array().unwrap();
        assert_eq!(payloads.len(), 1);
        assert_eq!(payloads[0]["path"], "plugin/plugin.dll");
        let rust = manifest["rust"].as_array().expect("runtime Rust entries");
        assert_eq!(rust.len(), 1);
        assert_eq!(
            rust[0]["root_contract_id"],
            json!({
                "namespace": ROOT_MODULE_CONTRACT_NAMESPACE_V1,
                "value": ROOT_MODULE_CONTRACT_VALUE_V1,
            })
        );
        assert!(rust[0].get("root_module").is_none());
        assert!(!output.join("notices/private-dependencies.json").exists());
        assert_eq!(
            fs::read(output.join("plugin/plugin.dll")).unwrap(),
            b"runtime dll"
        );
    }

    #[test]
    fn runtime_staging_matches_host_payload_and_path_bounds() {
        let payload = |index| StagedPayload {
            path: format!("payloads/{index:03}.bin"),
            kind: "notice",
            bytes: vec![],
        };
        let accepted = (0..HOST_MAX_RUNTIME_PAYLOADS)
            .map(payload)
            .collect::<Vec<_>>();
        assert!(validate_host_runtime_package_bounds(&accepted, "{}").is_ok());

        let rejected = (0..=HOST_MAX_RUNTIME_PAYLOADS)
            .map(payload)
            .collect::<Vec<_>>();
        assert!(validate_host_runtime_package_bounds(&rejected, "{}").is_err());

        let long_path = StagedPayload {
            path: "p".repeat(HOST_MAX_RUNTIME_PATH_BYTES + 1),
            kind: "notice",
            bytes: vec![],
        };
        assert!(validate_host_runtime_package_bounds(&[long_path], "{}").is_err());
    }

    #[test]
    fn runtime_staging_matches_host_manifest_and_exact_zip_bounds() {
        let payload = StagedPayload {
            path: "plugin/plugin.dll".into(),
            kind: "rust_dll",
            bytes: vec![],
        };
        assert!(
            validate_host_runtime_package_bounds(
                &[payload],
                &" ".repeat(HOST_MAX_RUNTIME_MANIFEST_BYTES),
            )
            .is_ok()
        );
        assert!(
            validate_host_runtime_package_bounds(
                &[],
                &" ".repeat(HOST_MAX_RUNTIME_MANIFEST_BYTES + 1),
            )
            .is_err()
        );

        let empty_archive = CANONICAL_ZIP_END_OF_CENTRAL_DIRECTORY_BYTES;
        assert_eq!(
            canonical_store_zip_entry_size(empty_archive, "a", 0).unwrap(),
            CANONICAL_ZIP_END_OF_CENTRAL_DIRECTORY_BYTES
                + CANONICAL_ZIP_LOCAL_HEADER_BYTES
                + CANONICAL_ZIP_CENTRAL_HEADER_BYTES
                + 2
        );
        let content = usize::try_from(
            HOST_MAX_CANONICAL_ZIP_BYTES
                - CANONICAL_ZIP_END_OF_CENTRAL_DIRECTORY_BYTES
                - CANONICAL_ZIP_LOCAL_HEADER_BYTES
                - CANONICAL_ZIP_CENTRAL_HEADER_BYTES
                - 2,
        )
        .unwrap();
        assert_eq!(
            canonical_store_zip_entry_size(empty_archive, "a", content).unwrap(),
            HOST_MAX_CANONICAL_ZIP_BYTES
        );
        assert!(validate_host_canonical_zip_size(HOST_MAX_CANONICAL_ZIP_BYTES).is_ok());
        assert!(validate_host_canonical_zip_size(HOST_MAX_CANONICAL_ZIP_BYTES + 1).is_err());
    }

    #[test]
    fn stage_package_carries_private_licenses_and_canonical_provenance_notice() {
        let (directory, dependency) = copied_private_fixture_vendor();
        let dll = directory.0.join("plugin.dll");
        fs::write(&dll, b"runtime dll").expect("write test dll");
        let output = directory.0.join("stage-private");
        let mut manifest = manifest_for_test();
        manifest.publisher.contacts.push(Contact {
            kind: "support".into(),
            value: "support@example.invalid".into(),
        });
        manifest.private_dependencies = vec![dependency.clone()];

        stage_validated_package(&directory.0, &manifest, &dll, &output)
            .expect("stage package with private dependency");

        let staged_manifest: Value = serde_json::from_slice(
            &fs::read(output.join("manifest.json")).expect("read staged manifest"),
        )
        .expect("parse staged manifest");
        let payloads = staged_manifest["payloads"].as_array().unwrap();
        assert!(payloads.iter().any(|payload| {
            payload["path"] == "licenses/private/exif-lite-0.1.0/LICENSE-MIT"
                && payload["kind"] == "license"
        }));
        assert!(payloads.iter().any(|payload| {
            payload["path"] == "notices/private-dependencies.json" && payload["kind"] == "notice"
        }));
        assert_eq!(
            fs::read(output.join("licenses/private/exif-lite-0.1.0/LICENSE-MIT")).unwrap(),
            fs::read(directory.0.join(&dependency.path).join("LICENSE-MIT")).unwrap()
        );
        let notice: Value = serde_json::from_slice(
            &fs::read(output.join("notices/private-dependencies.json")).unwrap(),
        )
        .unwrap();
        assert_eq!(notice["schema_version"], 1);
        assert_eq!(notice["private_dependencies"][0]["name"], "exif-lite");
        assert_eq!(
            notice["private_dependencies"][0]["crate_sha256"],
            dependency.provenance.crate_sha256
        );
    }

    #[test]
    fn stage_package_rejects_private_license_and_provenance_mutations_before_output_creation() {
        let (directory, dependency) = copied_private_fixture_vendor();
        let dll = directory.0.join("plugin.dll");
        fs::write(&dll, b"runtime dll").expect("write test dll");
        let mut manifest = manifest_for_test();
        manifest.publisher.contacts.push(Contact {
            kind: "support".into(),
            value: "support@example.invalid".into(),
        });
        manifest.private_dependencies = vec![dependency.clone()];

        fs::write(
            directory.0.join(&dependency.path).join("LICENSE-MIT"),
            b"changed license",
        )
        .expect("mutate private license");
        let output = directory.0.join("stage-tampered-license");
        assert!(stage_validated_package(&directory.0, &manifest, &dll, &output).is_err());
        assert!(!output.exists());

        let (directory, mut dependency) = copied_private_fixture_vendor();
        let dll = directory.0.join("plugin.dll");
        fs::write(&dll, b"runtime dll").expect("write test dll");
        dependency.provenance.source = "registry+https://example.invalid/index".into();
        let mut manifest = manifest_for_test();
        manifest.publisher.contacts.push(Contact {
            kind: "support".into(),
            value: "support@example.invalid".into(),
        });
        manifest.private_dependencies = vec![dependency];
        let output = directory.0.join("stage-tampered-provenance");
        assert!(stage_validated_package(&directory.0, &manifest, &dll, &output).is_err());
        assert!(!output.exists());
    }

    #[test]
    fn stage_package_requires_a_new_private_output_directory() {
        let directory = TestDirectory::new();
        let dll = directory.0.join("plugin.dll");
        fs::write(&dll, b"runtime dll").expect("write test dll");
        let output = directory.0.join("existing-stage");
        fs::create_dir(&output).expect("create non-private output directory");
        let mut manifest = manifest_for_test();
        manifest.publisher.contacts.push(Contact {
            kind: "support".into(),
            value: "support@example.invalid".into(),
        });

        assert!(stage_validated_package(&directory.0, &manifest, &dll, &output).is_err());
        assert!(output.is_dir());
        assert!(fs::read_dir(output).unwrap().next().is_none());
    }

    #[test]
    fn unsafe_windows_paths_are_rejected() {
        for path in [
            "../x",
            "a/../x",
            "C:x",
            "//server/x",
            r"\\?\C:\x",
            "a\\b",
            "a/NUL.txt",
            "a/x. ",
            "é.txt",
        ] {
            assert!(!safe_relative_path(path), "accepted {path}");
        }
        assert!(safe_relative_path("payload/plugin.dll"));
    }

    #[test]
    fn ids_and_hashes_are_strict() {
        assert!(valid_id("publisher.plugin-1"));
        assert!(!valid_id("Publisher"));
        assert!(!valid_id("_leading"));
        assert!(lower_hex(&"a".repeat(64), 64));
        assert!(!lower_hex(&"A".repeat(64), 64));
    }

    #[test]
    fn unknown_manifest_fields_are_rejected() {
        let result = serde_json::from_str::<Manifest>(r#"{"schema_version":1,"unexpected":true}"#);
        assert!(result.is_err());
    }

    #[test]
    fn cargo_files_are_required_and_regular() {
        let directory = TestDirectory::new();
        let mut diagnostics = Vec::new();
        validate_cargo_project(
            &directory.0,
            &manifest_for_test(),
            &expected_for_test(),
            &mut diagnostics,
        );
        assert!(has_code(&diagnostics, "SESDK-CARGO-001"));

        fs::write(
            directory.0.join("Cargo.toml"),
            "[package]\nname='plugin'\nversion='0.1.0'\n",
        )
        .expect("write test manifest");
        diagnostics.clear();
        validate_cargo_project(
            &directory.0,
            &manifest_for_test(),
            &expected_for_test(),
            &mut diagnostics,
        );
        assert!(
            has_code(&diagnostics, "SESDK-CARGO-002"),
            "{diagnostics:#?}"
        );
    }

    #[test]
    fn private_unlocked_and_unresolved_dependencies_fail_closed() {
        let diagnostics = dependency_diagnostics(
            "dependencies.private",
            DirectDependency {
                package: "explorer-ui".into(),
                version: Some("1".into()),
                git: None,
                rev: None,
                path: false,
                workspace: false,
                malformed: false,
            },
            "[[package]]\nname='explorer-ui'\nversion='1.0.0'\nsource='registry+https://github.com/rust-lang/crates.io-index'\n",
        );
        assert!(has_code(&diagnostics, "SESDK-CARGO-005"));

        let diagnostics = dependency_diagnostics(
            "dependencies.private_git",
            DirectDependency {
                package: "private-git".into(),
                version: None,
                git: Some("https://example.invalid/private.git".into()),
                rev: Some("main".into()),
                path: false,
                workspace: false,
                malformed: false,
            },
            "[[package]]\nname='private-git'\nversion='1.0.0'\nsource='git+https://example.invalid/private.git#bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb'\n",
        );
        assert!(has_code(&diagnostics, "SESDK-CARGO-007"));
        assert!(has_code(&diagnostics, "SESDK-CARGO-011"));
    }

    #[test]
    fn public_sdk_contract_dependency_is_the_only_allowed_explorer_crate() {
        let lock = "[[package]]\nname='explorer-extension-api'\nversion='1.2.0'\nsource='registry+https://github.com/rust-lang/crates.io-index'\n";
        let valid = dependency_diagnostics(
            "dependencies.explorer-extension-api",
            DirectDependency {
                package: PUBLIC_SDK_CONTRACT_DEPENDENCY.into(),
                version: Some(PUBLIC_SDK_CONTRACT_VERSION.into()),
                git: None,
                rev: None,
                path: false,
                workspace: false,
                malformed: false,
            },
            lock,
        );
        assert!(valid.is_empty(), "{valid:#?}");

        let path_dependency = dependency_diagnostics(
            "dependencies.explorer-extension-api",
            DirectDependency {
                package: PUBLIC_SDK_CONTRACT_DEPENDENCY.into(),
                version: Some(PUBLIC_SDK_CONTRACT_VERSION.into()),
                git: None,
                rev: None,
                path: true,
                workspace: false,
                malformed: false,
            },
            lock,
        );
        assert!(has_code(&path_dependency, "SESDK-CARGO-014"));
        assert!(has_code(&path_dependency, "SESDK-CARGO-006"));

        let wrong_version = dependency_diagnostics(
            "dependencies.explorer-extension-api",
            DirectDependency {
                package: PUBLIC_SDK_CONTRACT_DEPENDENCY.into(),
                version: Some("=1.2.1".into()),
                git: None,
                rev: None,
                path: false,
                workspace: false,
                malformed: false,
            },
            lock,
        );
        assert!(has_code(&wrong_version, "SESDK-CARGO-014"));

        let private_explorer = dependency_diagnostics(
            "dependencies.explorer-ui",
            DirectDependency {
                package: "explorer-ui".into(),
                version: Some("=1.2.0".into()),
                git: None,
                rev: None,
                path: false,
                workspace: false,
                malformed: false,
            },
            "[[package]]\nname='explorer-ui'\nversion='1.2.0'\nsource='registry+https://github.com/rust-lang/crates.io-index'\n",
        );
        assert!(has_code(&private_explorer, "SESDK-CARGO-005"));
    }

    #[test]
    fn protected_gpui_source_drift_is_rejected() {
        let diagnostics = dependency_diagnostics(
            "dependencies.gpui",
            DirectDependency {
                package: "gpui".into(),
                version: None,
                git: Some("https://github.com/example/fork.git".into()),
                rev: Some("b".repeat(40)),
                path: false,
                workspace: false,
                malformed: false,
            },
            "[[package]]\nname='gpui'\nversion='0.2.2'\nsource='git+https://github.com/example/fork.git#bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb'\n",
        );
        assert!(has_code(&diagnostics, "SESDK-CARGO-012"));
    }

    #[test]
    fn protected_metadata_accepts_the_exact_canonical_closure() {
        assert!(protected_diagnostics(&protected_metadata_for_test(), true).is_empty());
    }

    #[test]
    fn protected_features_allow_non_gpui_subsets_but_require_gpui_exactness() {
        let mut subset = protected_metadata_for_test();
        subset.resolve.as_mut().unwrap().nodes[1].features.clear();
        subset.resolve.as_mut().unwrap().nodes[2].features.clear();
        assert!(protected_diagnostics(&subset, false).is_empty());
        assert!(has_code(
            &protected_diagnostics(&subset, true),
            "SESDK-PROTECTED-004"
        ));

        let mut extra = protected_metadata_for_test();
        extra.resolve.as_mut().unwrap().nodes[2]
            .features
            .push("unapproved".into());
        assert!(has_code(
            &protected_diagnostics(&extra, false),
            "SESDK-PROTECTED-004"
        ));
        assert!(has_code(
            &protected_diagnostics(&extra, true),
            "SESDK-PROTECTED-004"
        ));
    }

    #[test]
    fn protected_metadata_rejects_a_transitive_second_gpui() {
        let mut metadata = protected_metadata_for_test();
        let evil_id = "gpui 0.2.2 (git+https://example.invalid/gpui#bbbb)".to_owned();
        metadata.packages.push(CargoMetadataPackage {
            id: evil_id.clone(),
            name: "gpui".into(),
            version: "0.2.2".into(),
            source: Some("git+https://example.invalid/gpui#bbbb".into()),
            manifest_path: "D:/cache/evil-gpui/Cargo.toml".into(),
        });
        metadata.resolve.as_mut().unwrap().nodes[0]
            .deps
            .push(CargoMetadataDependency {
                name: "second_gpui".into(),
                pkg: evil_id,
                dep_kinds: vec![CargoMetadataDependencyKind {
                    kind: None,
                    target: None,
                }],
            });
        assert!(has_code(
            &protected_diagnostics(&metadata, false),
            "SESDK-PROTECTED-002"
        ));
    }

    #[test]
    fn protected_metadata_rejects_abi_stable_version_features_and_source_drift() {
        let abi_id = "abi_stable 0.11.3 (registry+https://github.com/rust-lang/crates.io-index)";
        let mut version = protected_metadata_for_test();
        version
            .packages
            .iter_mut()
            .find(|package| package.id == abi_id)
            .unwrap()
            .version = "0.11.4".into();
        assert!(has_code(
            &protected_diagnostics(&version, false),
            "SESDK-PROTECTED-002"
        ));

        let mut source = protected_metadata_for_test();
        source
            .packages
            .iter_mut()
            .find(|package| package.id == abi_id)
            .unwrap()
            .source = Some("registry+https://example.invalid/index".into());
        assert!(has_code(
            &protected_diagnostics(&source, false),
            "SESDK-PROTECTED-002"
        ));

        let mut features = protected_metadata_for_test();
        features.resolve.as_mut().unwrap().nodes[2]
            .features
            .push("extra".into());
        assert!(has_code(
            &protected_diagnostics(&features, false),
            "SESDK-PROTECTED-004"
        ));
    }

    #[test]
    fn protected_metadata_rejects_dependency_kind_and_target_drift() {
        let mut metadata = protected_metadata_for_test();
        let kind = &mut metadata.resolve.as_mut().unwrap().nodes[1].deps[0].dep_kinds[0];
        kind.kind = Some("build".into());
        kind.target = Some("cfg(target_os = \"windows\")".into());
        assert!(has_code(
            &protected_diagnostics(&metadata, false),
            "SESDK-PROTECTED-005"
        ));
    }

    fn write_u16(bytes: &mut [u8], offset: usize, value: u16) {
        bytes[offset..offset + 2].copy_from_slice(&value.to_le_bytes());
    }

    fn write_u32(bytes: &mut [u8], offset: usize, value: u32) {
        bytes[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
    }

    fn pe_with_export(export: &str) -> Vec<u8> {
        let mut bytes = vec![0_u8; 0x400];
        bytes[..2].copy_from_slice(b"MZ");
        write_u32(&mut bytes, 0x3c, 0x80);
        bytes[0x80..0x84].copy_from_slice(b"PE\0\0");
        let coff = 0x84;
        write_u16(&mut bytes, coff, 0x8664);
        write_u16(&mut bytes, coff + 2, 1);
        write_u16(&mut bytes, coff + 16, 0xf0);
        let optional = coff + 20;
        write_u16(&mut bytes, optional, 0x20b);
        write_u32(&mut bytes, optional + 112, 0x1000);
        let section = optional + 0xf0;
        write_u32(&mut bytes, section + 8, 0x200);
        write_u32(&mut bytes, section + 12, 0x1000);
        write_u32(&mut bytes, section + 16, 0x200);
        write_u32(&mut bytes, section + 20, 0x200);
        let directory = 0x200;
        write_u32(&mut bytes, directory + 24, 1);
        write_u32(&mut bytes, directory + 32, 0x1030);
        write_u32(&mut bytes, 0x230, 0x1040);
        let export_bytes = export.as_bytes();
        bytes[0x240..0x240 + export_bytes.len()].copy_from_slice(export_bytes);
        bytes[0x240 + export_bytes.len()] = 0;
        bytes
    }

    #[test]
    fn dll_inspection_checks_the_fixed_abi_stable_loader_export_without_loading() {
        assert!(
            inspect_pe_exports(
                &pe_with_export(ABI_STABLE_ROOT_MODULE_LOADER_EXPORT),
                ABI_STABLE_ROOT_MODULE_LOADER_EXPORT
            )
            .is_ok()
        );
        assert!(
            inspect_pe_exports(
                &pe_with_export("plugin_root"),
                ABI_STABLE_ROOT_MODULE_LOADER_EXPORT
            )
            .is_err()
        );
        assert!(inspect_pe_exports(b"not a dll", ABI_STABLE_ROOT_MODULE_LOADER_EXPORT).is_err());

        let directory = TestDirectory::new();
        let oversized = directory.0.join("oversized.dll");
        let file = fs::File::create(&oversized).expect("create sparse oversized DLL");
        file.set_len(MAX_PAYLOAD_BYTES + 1)
            .expect("extend sparse oversized DLL");
        let report = inspect_dll(&oversized);
        assert!(!report.valid);
        assert!(report.diagnostics[0].message.contains("byte limit"));
    }

    #[test]
    fn canonical_built_dll_is_optional_before_build_and_checked_after_build() {
        let directory = TestDirectory::new();
        let manifest = manifest_for_test();
        let expected = expected_for_test();
        let mut diagnostics = Vec::new();
        validate_built_dll(&directory.0, &manifest, &expected, &mut diagnostics);
        assert!(diagnostics.is_empty());

        let dll = directory
            .0
            .join("target/superexplorer/sdk-test/build/plugin.dll");
        fs::create_dir_all(dll.parent().unwrap()).expect("create canonical DLL directory");
        fs::write(&dll, pe_with_export(ABI_STABLE_ROOT_MODULE_LOADER_EXPORT))
            .expect("write valid PE test DLL");
        validate_built_dll(&directory.0, &manifest, &expected, &mut diagnostics);
        assert!(diagnostics.is_empty());

        fs::write(&dll, pe_with_export("plugin_root")).expect("replace invalid PE test DLL");
        validate_built_dll(&directory.0, &manifest, &expected, &mut diagnostics);
        assert!(has_code(&diagnostics, "SESDK-DLL-001"));
    }

    #[test]
    fn cargo_diagnostics_are_structured_and_deterministic() {
        let dependency = DirectDependency {
            package: "unlocked".into(),
            version: None,
            git: None,
            rev: None,
            path: true,
            workspace: true,
            malformed: false,
        };
        let first = dependency_diagnostics("dependencies.unlocked", dependency, "");
        let second = dependency_diagnostics(
            "dependencies.unlocked",
            DirectDependency {
                package: "unlocked".into(),
                version: None,
                git: None,
                rev: None,
                path: true,
                workspace: true,
                malformed: false,
            },
            "",
        );
        let serialize = |diagnostics: &[Diagnostic]| serde_json::to_string(diagnostics).unwrap();
        assert_eq!(serialize(&first), serialize(&second));
        assert!(first.iter().all(|diagnostic| {
            diagnostic.severity == "error"
                && !diagnostic.code.is_empty()
                && !diagnostic.phase.is_empty()
                && !diagnostic.path.is_empty()
                && !diagnostic.message.is_empty()
        }));
    }

    #[test]
    fn release_profile_must_match_the_full_canonical_policy() {
        let valid = r#"
            [profile.release]
            panic = "unwind"
            lto = "thin"
            codegen-units = 1
        "#
        .parse::<TomlValue>()
        .unwrap();
        let mut diagnostics = Vec::new();
        validate_cargo_policy(&valid, &expected_for_test(), &mut diagnostics);
        assert!(diagnostics.is_empty());

        let invalid = r#"
            [profile.release]
            panic = "abort"
            lto = false
            codegen-units = 16
            strip = "symbols"
            overflow-checks = true
        "#
        .parse::<TomlValue>()
        .unwrap();
        validate_cargo_policy(&invalid, &expected_for_test(), &mut diagnostics);
        assert!(has_code(&diagnostics, "SESDK-PROFILE-001"));
    }

    #[test]
    fn target_build_and_profile_toolchain_overrides_are_rejected() {
        let manifest = r#"
            [profile.release]
            panic = "unwind"
            lto = "thin"
            codegen-units = 1

            [target.x86_64-pc-windows-msvc]
            linker = "evil-link.exe"
            rustflags = ["-Ctarget-feature=+crt-static"]
            runner = "evil-runner.exe"

            [build]
            rustc-wrapper = "evil-wrapper.exe"
        "#
        .parse::<TomlValue>()
        .unwrap();
        let mut diagnostics = Vec::new();
        validate_cargo_policy(&manifest, &expected_for_test(), &mut diagnostics);
        let paths = diagnostics
            .iter()
            .filter(|diagnostic| diagnostic.code == "SESDK-CARGO-013")
            .map(|diagnostic| diagnostic.path.as_str())
            .collect::<BTreeSet<_>>();
        assert_eq!(
            paths,
            BTreeSet::from([
                "build.rustc-wrapper",
                "target.x86_64-pc-windows-msvc.linker",
                "target.x86_64-pc-windows-msvc.runner",
                "target.x86_64-pc-windows-msvc.rustflags",
            ])
        );
    }

    #[test]
    fn payload_count_and_total_size_limits_fail_before_reading_files() {
        let payload = |size| Payload {
            path: "payload.bin".into(),
            size,
            sha256: "a".repeat(64),
            kind: "rust-source".into(),
        };
        let mut diagnostics = Vec::new();
        let payloads = (0..=MAX_PAYLOADS).map(|_| payload(1)).collect::<Vec<_>>();
        validate_payload_bounds(&payloads, &mut diagnostics);
        assert!(has_code(&diagnostics, "SESDK-BOUND-001"));

        diagnostics.clear();
        validate_payload_bounds(
            &[payload(MAX_TOTAL_PAYLOAD_BYTES), payload(1)],
            &mut diagnostics,
        );
        assert!(has_code(&diagnostics, "SESDK-BOUND-002"));
    }

    #[test]
    fn symlink_payload_components_are_rejected_when_supported() {
        let directory = TestDirectory::new();
        let target = directory.0.join("target");
        fs::create_dir_all(&target).expect("create target directory");
        fs::write(target.join("payload.bin"), b"payload").expect("write target payload");
        let link = directory.0.join("linked");
        #[cfg(windows)]
        {
            // Some Windows CI identities lack SeCreateSymbolicLinkPrivilege. This runtime
            // branch is covered wherever symlink creation is available.
            if let Ok(()) = std::os::windows::fs::symlink_dir(&target, &link) {
                let error =
                    ensure_regular_relative_path(&directory.0, Path::new("linked/payload.bin"))
                        .expect_err("symlink component must be rejected");
                assert!(error.contains("symlink or reparse point"));
            }
        }
        #[cfg(not(windows))]
        let _ = link;
    }
}
