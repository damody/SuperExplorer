use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Path, PathBuf},
    process::Command,
};
use superexplorer_ui_abi_fingerprint::sha256_hex;
use toml::Value as TomlValue;

const MAX_PAYLOADS: usize = 128;
const MAX_PAYLOAD_BYTES: u64 = 64 * 1024 * 1024;
const MAX_TOTAL_PAYLOAD_BYTES: u64 = 512 * 1024 * 1024;
const ABI_STABLE_ROOT_MODULE_LOADER_EXPORT: &str = "_1as_0lib_1header_0root_bmodule_bloader";

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
    root_module: String,
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

#[derive(Clone, Serialize, Eq, Ord, PartialEq, PartialOrd)]
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

#[derive(Deserialize)]
struct CargoMetadata {
    packages: Vec<CargoMetadataPackage>,
    resolve: Option<CargoMetadataResolve>,
}

#[derive(Deserialize)]
struct CargoMetadataPackage {
    id: String,
    name: String,
    version: String,
    source: Option<String>,
    manifest_path: String,
}

#[derive(Deserialize)]
struct CargoMetadataResolve {
    root: Option<String>,
    nodes: Vec<CargoMetadataNode>,
}

#[derive(Deserialize)]
struct CargoMetadataNode {
    id: String,
    #[serde(default)]
    deps: Vec<CargoMetadataDependency>,
    #[serde(default)]
    features: Vec<String>,
}

#[derive(Deserialize)]
struct CargoMetadataDependency {
    name: String,
    pkg: String,
    #[serde(default)]
    dep_kinds: Vec<CargoMetadataDependencyKind>,
}

#[derive(Deserialize)]
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

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct TrustedGates {
    schema_version: u32,
    requirements: Vec<RequirementEvidence>,
}

/// Validates the canonical P0 Rust consumer manifest and all declared payloads.
#[must_use]
pub fn validate(root: &Path) -> Report {
    match validate_inner(root) {
        Ok(mut diagnostics) => {
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
                message,
            )],
        },
    }
}

/// Inspects a DLL's PE headers and exports without loading it into the validator process.
#[must_use]
pub fn inspect_dll(path: &Path) -> Report {
    let result = fs::read(path).and_then(|bytes| {
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
                &path.to_string_lossy(),
                format!("DLL inspection failed: {error}"),
            )],
        },
    }
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
    let manifest_path = canonical_root.join("plugin-project.json");
    let source = fs::read_to_string(&manifest_path)
        .map_err(|error| format!("could not read plugin-project.json: {error}"))?;
    let manifest: Manifest = serde_json::from_str(&source).map_err(|error| {
        format!("plugin-project.json does not match the exact P0 schema: {error}")
    })?;
    let expected = expected_sdk()?;
    Ok(validate_manifest(&manifest, &canonical_root, &expected))
}

fn expected_sdk() -> Result<ExpectedSdk, String> {
    let repository = Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(3)
        .ok_or("repository root unavailable")?;
    let lock: Value = read_json(&repository.join("sdk/sdk-lock.json"))?;
    let fingerprint: Value = read_json(&repository.join("sdk/ui-abi-fingerprint.json"))?;
    let gates: TrustedGates = serde_json::from_str(
        &fs::read_to_string(repository.join("sdk/ci/plugin-gates.json"))
            .map_err(|error| format!("plugin-gates.json: {error}"))?,
    )
    .map_err(|error| format!("plugin-gates.json: {error}"))?;
    if gates.schema_version != 1 {
        return Err("plugin-gates schema is unsupported".into());
    }
    let mut gate_map = BTreeMap::new();
    for requirement in gates.requirements {
        if gate_map
            .insert(requirement.requirement_id, requirement.evidence)
            .is_some()
        {
            return Err("plugin-gates contains a duplicate requirement".into());
        }
    }
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
        ui_abi_fingerprint: required_string(&fingerprint, "/fingerprint")?,
        gpui_repository: required_string(&lock, "/gpui/repository")?,
        gpui_revision: required_string(&lock, "/gpui/revision")?,
        gpui_packages,
        protected_graph,
        release_profile: release_profile_policy(&lock)?,
        gates: gate_map,
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
        ("rust.root_module", manifest.rust.root_module.as_str()),
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
    if !safe_relative_path(&manifest.rust.entrypoint) {
        diagnostics.push(diagnostic(
            "SESDK-PATH-001",
            "manifest",
            "rust.entrypoint",
            "entrypoint path is unsafe",
        ));
    }
    validate_cargo_project(root, manifest.sdk.gpui, expected, &mut diagnostics);
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
        if !matches!(
            payload.kind.as_str(),
            "rust-source" | "license" | "notice" | "locale" | "dll"
        ) {
            diagnostics.push(diagnostic(
                "SESDK-PAYLOAD-001",
                "payload",
                &path,
                "P0 payload kind is unsupported",
            ));
        }
        if payload.size > MAX_PAYLOAD_BYTES {
            diagnostics.push(diagnostic(
                "SESDK-PAYLOAD-BOUND-003",
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
        if !matches!(contribution.kind.as_str(), "abi-root" | "gpui") {
            diagnostics.push(diagnostic(
                "SESDK-CONTRIBUTION-001",
                "manifest",
                &path,
                "P0 contribution kind is unsupported",
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
            "SESDK-PAYLOAD-BOUND-001",
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
            "SESDK-PAYLOAD-BOUND-002",
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
    gpui_plugin: bool,
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
    validate_cargo_policy(&manifest, expected, diagnostics);
    let mut dependencies = BTreeMap::new();
    collect_direct_dependencies(&manifest, "", &mut dependencies);
    for (location, dependency) in dependencies {
        validate_direct_dependency(&location, &dependency, &packages, expected, diagnostics);
    }
    match cargo_metadata(root) {
        Ok(metadata) => validate_protected_metadata(
            &metadata,
            &lock_packages,
            gpui_plugin,
            expected,
            diagnostics,
        ),
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

fn cargo_metadata(root: &Path) -> Result<CargoMetadata, String> {
    let output = Command::new("cargo")
        .args(["metadata", "--locked", "--offline", "--format-version", "1"])
        .current_dir(root)
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
    Path::new(&package.manifest_path)
        .parent()?
        .strip_prefix(repository_root)
        .ok()
        .map(|path| path.to_string_lossy().replace('\\', "/"))
}

fn normalized_edge(name: &str, to: &str, kind: &str, target: Option<&str>) -> String {
    format!("{name}\u{1f}{to}\u{1f}{kind}\u{1f}{}", target.unwrap_or(""))
}

fn string_set(values: &[String]) -> BTreeSet<&str> {
    values.iter().map(String::as_str).collect()
}

fn read_project_toml(root: &Path, path: &Path) -> Result<TomlValue, String> {
    if !path.starts_with(root) {
        return Err("project file escapes plugin root".into());
    }
    ensure_regular_project_path(root, path)?;
    let source = fs::read_to_string(path).map_err(|error| {
        format!(
            "{} cannot be read: {error}",
            path.file_name().unwrap().display()
        )
    })?;
    source.parse::<TomlValue>().map_err(|error| {
        format!(
            "{} is not valid TOML: {error}",
            path.file_name().unwrap().display()
        )
    })
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
    if is_private_workspace_crate(&dependency.package) {
        diagnostics.push(diagnostic(
            "SESDK-CARGO-005",
            "cargo",
            location,
            "direct dependency references a SuperExplorer private workspace crate",
        ));
    }
    if dependency.path || dependency.workspace {
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
    if matches.iter().all(|package| {
        package
            .get("source")
            .and_then(TomlValue::as_str)
            .is_none_or(str::is_empty)
    }) {
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
    for component in relative.components() {
        current.push(component);
        if is_link_or_reparse(&current)? {
            return Err(format!(
                "{} is a symlink or reparse point",
                current.display()
            ));
        }
    }
    let metadata = fs::metadata(&current)
        .map_err(|error| format!("{} is unavailable: {error}", current.display()))?;
    if !metadata.is_file() {
        return Err(format!("{} is not a regular file", current.display()));
    }
    Ok(())
}

fn is_link_or_reparse(path: &Path) -> Result<bool, String> {
    let metadata = fs::symlink_metadata(path)
        .map_err(|error| format!("{} metadata cannot be read: {error}", path.display()))?;
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
            "SESDK-PAYLOAD-BOUND-004",
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
                root_module: "plugin_root".into(),
            },
            features: vec![],
            contributions: vec![],
            payloads: vec![],
            verification: Verification {
                requirements: vec![],
            },
        }
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

    fn has_code(diagnostics: &[Diagnostic], code: &str) -> bool {
        diagnostics.iter().any(|diagnostic| diagnostic.code == code)
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
        validate_cargo_project(&directory.0, false, &expected_for_test(), &mut diagnostics);
        assert!(has_code(&diagnostics, "SESDK-CARGO-001"));

        fs::write(
            directory.0.join("Cargo.toml"),
            "[package]\nname='plugin'\nversion='0.1.0'\n",
        )
        .expect("write test manifest");
        diagnostics.clear();
        validate_cargo_project(&directory.0, false, &expected_for_test(), &mut diagnostics);
        assert!(has_code(&diagnostics, "SESDK-CARGO-002"));
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
    }

    #[test]
    fn canonical_built_dll_is_optional_before_build_and_checked_after_build() {
        let directory = TestDirectory::new();
        let mut manifest = manifest_for_test();
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

        manifest.rust.root_module = "logical_plugin_root".into();
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
        assert!(has_code(&diagnostics, "SESDK-PAYLOAD-BOUND-001"));

        diagnostics.clear();
        validate_payload_bounds(
            &[payload(MAX_TOTAL_PAYLOAD_BYTES), payload(1)],
            &mut diagnostics,
        );
        assert!(has_code(&diagnostics, "SESDK-PAYLOAD-BOUND-002"));
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
