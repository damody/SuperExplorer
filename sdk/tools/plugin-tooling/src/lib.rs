use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Path, PathBuf},
};
use superexplorer_ui_abi_fingerprint::sha256_hex;
use toml::Value as TomlValue;

const MAX_PAYLOADS: usize = 128;
const MAX_PAYLOAD_BYTES: u64 = 64 * 1024 * 1024;
const MAX_TOTAL_PAYLOAD_BYTES: u64 = 512 * 1024 * 1024;

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
    bundle_id: String,
    target: String,
    abi_schema: u32,
    ui_abi_fingerprint: String,
    gpui_repository: String,
    gpui_revision: String,
    gpui_packages: BTreeSet<String>,
    gates: BTreeMap<String, Evidence>,
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
    Ok(ExpectedSdk {
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
        gpui_packages: protected_gpui_packages(&lock)?,
        gates: gate_map,
    })
}

fn protected_gpui_packages(lock: &Value) -> Result<BTreeSet<String>, String> {
    let repository = required_string(lock, "/gpui/repository")?;
    let packages = lock
        .pointer("/protected_dependency_graph")
        .and_then(Value::as_array)
        .ok_or("sdk-lock protected dependency graph is missing")?;
    let names = packages
        .iter()
        .filter(|package| {
            package
                .get("source")
                .and_then(Value::as_str)
                .is_some_and(|source| source.contains(&repository))
        })
        .filter_map(|package| package.get("name").and_then(Value::as_str))
        .map(str::to_owned)
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
}
