//! Release-freeze evidence validation.
//!
//! A release freeze is deliberately not inferred from a development snapshot.
//! Production validation needs a locally resolvable annotated tag plus externally
//! verified, hash-addressed protection, signature, and provenance evidence.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    fs,
    path::{Component, Path, PathBuf},
    process::Command,
};
use superexplorer_ui_abi_fingerprint::{production_fingerprint_from_lock, sha256_hex};

const SCHEMA_VERSION: u32 = 2;
const SDK_LOCK_PATH: &str = "sdk/sdk-lock.json";
const BUNDLE_MANIFEST_PATH: &str = "sdk/bundle-manifest.json";
const UI_FINGERPRINT_PATH: &str = "sdk/ui-abi-fingerprint.json";
const LEDGER_PATH: &str = "sdk/snapshot/release-ledger.json";
const GPUI_GATE_MANIFEST_PATH: &str = "sdk/ci/gpui-update-gates.json";
const AUTHORIZED_GPUI_REPOSITORY: &str = "https://github.com/damody/gpui-ce-explorer.git";

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Metadata {
    pub schema_version: u32,
    pub release_frozen: bool,
    pub evidence_mode: EvidenceMode,
    pub protected_tag: ProtectedTag,
    pub source: FrozenSource,
    pub rc_id: String,
    pub bundle_id: String,
    pub release_input_digest: String,
    pub artifacts: ReleaseArtifacts,
    pub protection: ProtectionEvidence,
    pub signature: SignatureEvidence,
    pub provenance: ProvenanceEvidence,
    pub prior_release_ledger: ArtifactReference,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum EvidenceMode {
    Production,
    Fixture,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ProtectedTag {
    pub name: String,
    /// The annotated tag object, not its peeled commit.
    pub tag_object: String,
    pub object_revision: String,
    pub tree: String,
    pub repository: String,
    pub signer_primary_fingerprint: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct FrozenSource {
    pub revision: String,
    pub tree: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ReleaseArtifacts {
    pub sdk_lock: ArtifactReference,
    pub bundle_manifest: ArtifactReference,
    pub ui_abi_fingerprint: ArtifactReference,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ArtifactReference {
    pub path: String,
    pub sha256: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ProtectionEvidence {
    pub provider: String,
    pub policy_id: String,
    pub record: ArtifactReference,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct SignatureEvidence {
    pub verification: String,
    pub signer: String,
    pub primary_fingerprint: String,
    pub artifact: ArtifactReference,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ProvenanceEvidence {
    pub builder: String,
    pub predicate_type: String,
    pub artifact: ArtifactReference,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ReleaseLedger {
    schema_version: u32,
    releases: Vec<ReleaseLedgerEntry>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ReleaseLedgerEntry {
    rc_id: String,
    bundle_id: String,
    source: FrozenSource,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
struct GateManifest {
    schema_version: u32,
    required_gate_count: usize,
    gates: Vec<GateDefinition>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
#[serde(deny_unknown_fields)]
struct GateDefinition {
    id: String,
    kind: String,
    path: String,
    required: bool,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct GateAttestation {
    schema_version: u32,
    gate_manifest_sha256: String,
    candidate_plan_digest: String,
    workflow_run_id: String,
    nonce: String,
    results: Vec<GateResult>,
    attestation_sha256: String,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
#[serde(deny_unknown_fields)]
struct GateResult {
    id: String,
    exit_code: i64,
}

/// Validates the parsed release records and canonical SDK artifacts.
///
/// This deliberately does not trust a source named `main`: only the recorded
/// tag/commit/tree and generated release inputs are part of the digest.
///
/// # Errors
/// Returns a fail-closed diagnostic for any identity, bundle, proof, or ledger
/// mismatch. Byte-level reference checks and Git object checks are performed by
/// [`validate_at_root`].
pub(crate) fn validate(
    metadata: &Metadata,
    lock: &Value,
    manifest: &Value,
    fingerprint: &Value,
    ledger: &Value,
    protection_proof: &Value,
    gate_manifest: &GateManifest,
    gate_manifest_sha256: &str,
    expected_mode: EvidenceMode,
) -> Result<(), String> {
    if metadata.schema_version != SCHEMA_VERSION
        || !metadata.release_frozen
        || metadata.evidence_mode != expected_mode
        || metadata.rc_id.is_empty()
        || metadata.bundle_id.is_empty()
        || metadata.protected_tag.name.is_empty()
    {
        return Err("release-freeze metadata is incomplete or has the wrong mode".into());
    }
    if !metadata.protected_tag.name.starts_with("gpui-sdk-v") {
        return Err("protected tag name does not use the GPUI SDK namespace".into());
    }
    if metadata.protected_tag.repository != AUTHORIZED_GPUI_REPOSITORY {
        return Err("protected tag repository is not the authorized GPUI origin".into());
    }
    if !is_upper_hex(&metadata.protected_tag.signer_primary_fingerprint, 40)
        || !is_upper_hex(&metadata.signature.primary_fingerprint, 40)
        || metadata.protected_tag.signer_primary_fingerprint
            != metadata.signature.primary_fingerprint
    {
        return Err("tag and bundle signatures must bind the same primary GPG fingerprint".into());
    }
    for object_id in [
        metadata.protected_tag.tag_object.as_str(),
        metadata.protected_tag.object_revision.as_str(),
        metadata.protected_tag.tree.as_str(),
        metadata.source.revision.as_str(),
        metadata.source.tree.as_str(),
    ] {
        if !is_lower_hex(object_id, 40) {
            return Err("release source identity is not a lowercase full Git object ID".into());
        }
    }
    if metadata.protected_tag.object_revision != metadata.source.revision
        || metadata.protected_tag.tree != metadata.source.tree
    {
        return Err("protected tag differs from frozen source".into());
    }
    check_artifact_path(&metadata.artifacts.sdk_lock, SDK_LOCK_PATH)?;
    check_artifact_path(&metadata.artifacts.bundle_manifest, BUNDLE_MANIFEST_PATH)?;
    check_artifact_path(&metadata.artifacts.ui_abi_fingerprint, UI_FINGERPRINT_PATH)?;
    check_artifact_path(&metadata.prior_release_ledger, LEDGER_PATH)?;
    for reference in [
        &metadata.protection.record,
        &metadata.signature.artifact,
        &metadata.provenance.artifact,
    ] {
        check_safe_reference(reference)?;
    }
    if metadata.protection.provider.is_empty()
        || metadata.protection.policy_id.is_empty()
        || metadata.signature.signer.is_empty()
        || metadata.provenance.builder.is_empty()
        || metadata.provenance.predicate_type.is_empty()
    {
        return Err("structured release evidence is incomplete".into());
    }
    validate_protection_proof(protection_proof, metadata, expected_mode)?;
    match expected_mode {
        EvidenceMode::Production if metadata.signature.verification != "detached_gpg" => {
            return Err("production release requires detached GPG evidence".into());
        }
        EvidenceMode::Fixture if metadata.signature.verification != "fixture_unsigned" => {
            return Err("fixture release must explicitly use fixture_unsigned evidence".into());
        }
        _ => {}
    }
    if lock.pointer("/gpui/revision").and_then(Value::as_str)
        != Some(metadata.source.revision.as_str())
        || lock.pointer("/gpui/tree").and_then(Value::as_str) != Some(metadata.source.tree.as_str())
    {
        return Err("SDK lock differs from frozen source".into());
    }
    if lock.pointer("/gpui/approved_snapshot/release_frozen") != Some(&Value::Bool(true))
        || lock
            .pointer("/gpui/approved_snapshot/source/revision")
            .and_then(Value::as_str)
            != Some(metadata.source.revision.as_str())
        || lock
            .pointer("/gpui/approved_snapshot/source/tree")
            .and_then(Value::as_str)
            != Some(metadata.source.tree.as_str())
    {
        return Err("SDK lock does not embed the frozen approved snapshot".into());
    }
    validate_embedded_development_gates(lock, metadata, gate_manifest, gate_manifest_sha256)?;
    for value in [lock, manifest, fingerprint] {
        if value.get("bundle_id").and_then(Value::as_str) != Some(metadata.bundle_id.as_str()) {
            return Err("bundle ID mismatch".into());
        }
    }
    let computed = production_fingerprint_from_lock(lock)?;
    let artifact_fingerprint = fingerprint
        .get("fingerprint")
        .and_then(Value::as_str)
        .ok_or("fingerprint artifact is incomplete")?;
    if !is_lower_hex(artifact_fingerprint, 64) || artifact_fingerprint != computed.fingerprint {
        return Err("UI ABI fingerprint artifact differs from the SDK lock".into());
    }
    let digest = release_input_digest(metadata)?;
    if metadata.release_input_digest != digest {
        return Err("release input digest mismatch".into());
    }
    let ledger: ReleaseLedger = serde_json::from_value(ledger.clone())
        .map_err(|error| format!("invalid immutable prior-release ledger: {error}"))?;
    if ledger.schema_version != 1 {
        return Err("unsupported immutable prior-release ledger schema".into());
    }
    for previous in ledger.releases {
        if previous.rc_id == metadata.rc_id
            && (previous.source.revision != metadata.source.revision
                || previous.source.tree != metadata.source.tree
                || previous.bundle_id != metadata.bundle_id)
        {
            return Err("RC ID was already used for a different immutable release input".into());
        }
        if previous.bundle_id == metadata.bundle_id
            && (previous.source.revision != metadata.source.revision
                || previous.source.tree != metadata.source.tree)
        {
            return Err("bundle ID was already used for a different frozen source".into());
        }
    }
    Ok(())
}

fn validate_protection_proof(
    proof: &Value,
    metadata: &Metadata,
    expected_mode: EvidenceMode,
) -> Result<(), String> {
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct ProtectionProof {
        schema_version: u32,
        provider: String,
        policy_id: String,
        repository: String,
        tag_name: String,
        tag_object: String,
        object_revision: String,
        tree: String,
    }

    let proof: ProtectionProof = serde_json::from_value(proof.clone())
        .map_err(|error| format!("invalid protected-tag proof: {error}"))?;
    if proof.schema_version != 1
        || proof.provider != metadata.protection.provider
        || proof.policy_id != metadata.protection.policy_id
        || proof.tag_name != metadata.protected_tag.name
        || proof.tag_object != metadata.protected_tag.tag_object
        || proof.object_revision != metadata.protected_tag.object_revision
        || proof.tree != metadata.protected_tag.tree
    {
        return Err("protected-tag proof does not bind the recorded tag identity".into());
    }
    if expected_mode == EvidenceMode::Production
        && proof.repository != metadata.protected_tag.repository
    {
        return Err("protected-tag proof repository differs from the authorized tag origin".into());
    }
    Ok(())
}

fn validate_embedded_development_gates(
    lock: &Value,
    metadata: &Metadata,
    gate_manifest: &GateManifest,
    gate_manifest_sha256: &str,
) -> Result<(), String> {
    if gate_manifest.schema_version != 1
        || gate_manifest.required_gate_count != gate_manifest.gates.len()
        || gate_manifest.gates.iter().any(|gate| {
            !gate.required || gate.id.is_empty() || gate.kind.is_empty() || gate.path.is_empty()
        })
    {
        return Err("canonical GPUI gate manifest is incomplete".into());
    }
    let snapshot = lock
        .pointer("/gpui/approved_snapshot")
        .ok_or("SDK lock has no embedded approved snapshot")?;
    if snapshot
        .pointer("/approval/channel")
        .and_then(Value::as_str)
        != Some("development")
        || snapshot.pointer("/approval/state").and_then(Value::as_str) != Some("approved")
        || snapshot
            .pointer("/candidate_plan_digest")
            .and_then(Value::as_str)
            .is_none()
    {
        return Err("frozen SDK lock is not derived from an approved development snapshot".into());
    }
    let gates: GateAttestation = serde_json::from_value(
        snapshot
            .pointer("/approval/gates")
            .cloned()
            .ok_or("approved development snapshot has no full gate attestation")?,
    )
    .map_err(|error| format!("invalid approved development gate attestation: {error}"))?;
    if gates.schema_version != 1
        || !is_lower_hex(&gates.gate_manifest_sha256, 64)
        || gates.gate_manifest_sha256 != gate_manifest_sha256
        || !is_lower_hex(&gates.candidate_plan_digest, 64)
        || gates.candidate_plan_digest
            != snapshot
                .pointer("/candidate_plan_digest")
                .and_then(Value::as_str)
                .unwrap_or_default()
        || gates.workflow_run_id.is_empty()
        || gates.nonce.is_empty()
        || gates.attestation_sha256 != gate_attestation_digest(&gates)?
    {
        return Err(
            "approved development gate attestation is not canonical or digest-bound".into(),
        );
    }
    if snapshot
        .pointer("/approval/proof/candidate_plan_digest")
        .and_then(Value::as_str)
        != Some(gates.candidate_plan_digest.as_str())
        || snapshot
            .pointer("/approval/proof/workflow_run_id")
            .and_then(Value::as_str)
            != Some(gates.workflow_run_id.as_str())
        || snapshot
            .pointer("/approval/proof/nonce")
            .and_then(Value::as_str)
            != Some(gates.nonce.as_str())
    {
        return Err("approved development proof differs from its full gate attestation".into());
    }
    let expected_results = gate_manifest
        .gates
        .iter()
        .map(|gate| GateResult {
            id: gate.id.clone(),
            exit_code: 0,
        })
        .collect::<Vec<_>>();
    if gates.results != expected_results {
        return Err(
            "approved development gate attestation does not prove every required gate".into(),
        );
    }
    if snapshot.pointer("/source/revision").and_then(Value::as_str)
        != Some(metadata.source.revision.as_str())
        || snapshot.pointer("/source/tree").and_then(Value::as_str)
            != Some(metadata.source.tree.as_str())
    {
        return Err(
            "approved development snapshot source differs from frozen release source".into(),
        );
    }
    Ok(())
}

fn gate_attestation_digest(attestation: &GateAttestation) -> Result<String, String> {
    #[derive(Serialize)]
    struct GateAttestationBody<'a> {
        schema_version: u32,
        gate_manifest_sha256: &'a str,
        candidate_plan_digest: &'a str,
        workflow_run_id: &'a str,
        nonce: &'a str,
        results: &'a [GateResult],
    }
    serde_json::to_vec(&GateAttestationBody {
        schema_version: attestation.schema_version,
        gate_manifest_sha256: &attestation.gate_manifest_sha256,
        candidate_plan_digest: &attestation.candidate_plan_digest,
        workflow_run_id: &attestation.workflow_run_id,
        nonce: &attestation.nonce,
        results: &attestation.results,
    })
    .map(|bytes| sha256_hex(&bytes))
    .map_err(|error| error.to_string())
}

/// Validates release files and the local annotated GPUI tag.
///
/// Production mode intentionally cannot consume `fixture_unsigned` metadata.
/// The fixture mode is only exposed through the separate test CLI command.
///
/// # Errors
/// Returns an error if a referenced file escapes the root, has a different hash,
/// parses differently, or the tag does not resolve to the frozen commit and tree.
pub fn validate_at_root(root: &Path, expected_mode: EvidenceMode) -> Result<(), String> {
    validate_at_paths(
        root,
        &root.join("sdk/snapshot/release-freeze.json"),
        None,
        None,
        expected_mode,
    )
}

/// Validates staged production metadata before it is published.
///
/// `metadata_path` and `ledger_path` may be transaction staging files, while all
/// referenced SDK artifacts and the GPUI tag remain rooted at `root`. This keeps
/// the production command fixed to the repository while allowing a release
/// transaction to prove its final ledger snapshot before either file is visible.
///
/// # Errors
/// Returns a fail-closed diagnostic for a bad staged file, artifact, or tag.
pub fn validate_at_paths(
    root: &Path,
    metadata_path: &Path,
    ledger_path: Option<&Path>,
    evidence_directory: Option<&Path>,
    expected_mode: EvidenceMode,
) -> Result<(), String> {
    let metadata: Metadata = read_json_path(metadata_path)?;
    let lock: Value = read_reference(root, &metadata.artifacts.sdk_lock)?;
    let manifest: Value = read_reference(root, &metadata.artifacts.bundle_manifest)?;
    let fingerprint: Value = read_reference(root, &metadata.artifacts.ui_abi_fingerprint)?;
    let ledger: Value = match ledger_path {
        Some(path) => read_reference_path(path, &metadata.prior_release_ledger)?,
        None => read_reference(root, &metadata.prior_release_ledger)?,
    };
    let protection_proof: Value =
        read_evidence_json(root, evidence_directory, &metadata.protection.record)?;
    for reference in [&metadata.signature.artifact, &metadata.provenance.artifact] {
        read_evidence_bytes(root, evidence_directory, reference)?;
    }
    let gate_manifest_path = root.join(GPUI_GATE_MANIFEST_PATH);
    let gate_manifest_bytes = fs::read(&gate_manifest_path)
        .map_err(|error| format!("{}: {error}", gate_manifest_path.display()))?;
    let gate_manifest: GateManifest = serde_json::from_slice(&gate_manifest_bytes)
        .map_err(|error| format!("{}: {error}", gate_manifest_path.display()))?;
    let gate_manifest_sha256 = sha256_hex(&gate_manifest_bytes);
    validate(
        &metadata,
        &lock,
        &manifest,
        &fingerprint,
        &ledger,
        &protection_proof,
        &gate_manifest,
        &gate_manifest_sha256,
        expected_mode,
    )?;
    let tag_repository = match expected_mode {
        EvidenceMode::Production => root.join("vendor/gpui-ce"),
        EvidenceMode::Fixture => root.to_path_buf(),
    };
    verify_annotated_tag(&tag_repository, &metadata.protected_tag)
}

fn read_evidence_json(
    root: &Path,
    evidence_directory: Option<&Path>,
    reference: &ArtifactReference,
) -> Result<Value, String> {
    let bytes = read_evidence_bytes(root, evidence_directory, reference)?;
    serde_json::from_slice(&bytes).map_err(|error| format!("{}: {error}", reference.path))
}

fn read_evidence_bytes(
    root: &Path,
    evidence_directory: Option<&Path>,
    reference: &ArtifactReference,
) -> Result<Vec<u8>, String> {
    match evidence_directory {
        Some(directory) => {
            let file_name = Path::new(&reference.path)
                .file_name()
                .ok_or("staged evidence path is invalid")?;
            let staged_path = directory.join(file_name);
            check_safe_reference(reference)?;
            let bytes = fs::read(&staged_path)
                .map_err(|error| format!("{}: {error}", staged_path.display()))?;
            if sha256_hex(&bytes) != reference.sha256 {
                return Err(format!(
                    "referenced artifact hash mismatch: {}",
                    reference.path
                ));
            }
            Ok(bytes)
        }
        None => read_reference_bytes(root, reference),
    }
}

/// Returns the digest which binds every immutable release input except itself.
///
/// # Errors
/// Returns an error only if canonical JSON serialization fails.
pub fn release_input_digest(metadata: &Metadata) -> Result<String, String> {
    #[derive(Serialize)]
    struct ReleaseInput<'a> {
        schema_version: u32,
        rc_id: &'a str,
        bundle_id: &'a str,
        protected_tag: &'a ProtectedTag,
        source: &'a FrozenSource,
        artifacts: &'a ReleaseArtifacts,
        protection: &'a ProtectionEvidence,
        signature: &'a SignatureEvidence,
        provenance: &'a ProvenanceEvidence,
        prior_release_ledger: &'a ArtifactReference,
    }
    let value = ReleaseInput {
        schema_version: metadata.schema_version,
        rc_id: &metadata.rc_id,
        bundle_id: &metadata.bundle_id,
        protected_tag: &metadata.protected_tag,
        source: &metadata.source,
        artifacts: &metadata.artifacts,
        protection: &metadata.protection,
        signature: &metadata.signature,
        provenance: &metadata.provenance,
        prior_release_ledger: &metadata.prior_release_ledger,
    };
    serde_json::to_vec(&value)
        .map(|bytes| sha256_hex(&bytes))
        .map_err(|error| error.to_string())
}

fn check_artifact_path(reference: &ArtifactReference, expected: &str) -> Result<(), String> {
    if reference.path != expected {
        return Err(format!("release artifact must be {expected}"));
    }
    check_safe_reference(reference)
}

fn check_safe_reference(reference: &ArtifactReference) -> Result<(), String> {
    if !is_lower_hex(&reference.sha256, 64) {
        return Err("release evidence hash is not lowercase SHA-256".into());
    }
    let path = Path::new(&reference.path);
    if path.as_os_str().is_empty()
        || path.is_absolute()
        || path.components().any(|component| {
            matches!(
                component,
                Component::ParentDir | Component::RootDir | Component::Prefix(_)
            )
        })
    {
        return Err("release evidence path escapes its root".into());
    }
    Ok(())
}

fn read_json_path<T: serde::de::DeserializeOwned>(path: &Path) -> Result<T, String> {
    let source =
        fs::read_to_string(path).map_err(|error| format!("{}: {error}", path.display()))?;
    serde_json::from_str(&source).map_err(|error| format!("{}: {error}", path.display()))
}

fn read_reference<T: serde::de::DeserializeOwned>(
    root: &Path,
    reference: &ArtifactReference,
) -> Result<T, String> {
    let bytes = read_reference_bytes(root, reference)?;
    serde_json::from_slice(&bytes).map_err(|error| format!("{}: {error}", reference.path))
}

fn read_reference_path<T: serde::de::DeserializeOwned>(
    path: &Path,
    reference: &ArtifactReference,
) -> Result<T, String> {
    check_safe_reference(reference)?;
    let bytes = fs::read(path).map_err(|error| format!("{}: {error}", path.display()))?;
    if sha256_hex(&bytes) != reference.sha256 {
        return Err(format!(
            "referenced artifact hash mismatch: {}",
            reference.path
        ));
    }
    serde_json::from_slice(&bytes).map_err(|error| format!("{}: {error}", reference.path))
}

fn read_reference_bytes(root: &Path, reference: &ArtifactReference) -> Result<Vec<u8>, String> {
    check_safe_reference(reference)?;
    let path = rooted_path(root, &reference.path)?;
    let bytes = fs::read(&path).map_err(|error| format!("{}: {error}", path.display()))?;
    if sha256_hex(&bytes) != reference.sha256 {
        return Err(format!(
            "referenced artifact hash mismatch: {}",
            reference.path
        ));
    }
    Ok(bytes)
}

fn rooted_path(root: &Path, relative: &str) -> Result<PathBuf, String> {
    let path = Path::new(relative);
    if path.is_absolute()
        || path.components().any(|component| {
            matches!(
                component,
                Component::ParentDir | Component::RootDir | Component::Prefix(_)
            )
        })
    {
        return Err("path escapes validation root".into());
    }
    Ok(root.join(path))
}

fn verify_annotated_tag(root: &Path, tag: &ProtectedTag) -> Result<(), String> {
    let object = git(
        root,
        ["rev-parse", "--verify", &format!("refs/tags/{}", tag.name)],
    )?;
    if object != tag.tag_object {
        return Err("local tag object differs from recorded protected tag object".into());
    }
    if git(root, ["cat-file", "-t", &object])? != "tag" {
        return Err("release tag is not an annotated tag object".into());
    }
    let revision = git(
        root,
        ["rev-parse", "--verify", &format!("{}^{{}}", tag.name)],
    )?;
    if revision != tag.object_revision {
        return Err("annotated tag does not peel to the frozen revision".into());
    }
    if git(root, ["show", "-s", "--format=%T", &revision])? != tag.tree {
        return Err("annotated tag commit does not have the frozen tree".into());
    }
    Ok(())
}

fn git<const N: usize>(root: &Path, arguments: [&str; N]) -> Result<String, String> {
    let output = Command::new("git")
        .arg("-C")
        .arg(root)
        .args(arguments)
        .output()
        .map_err(|error| format!("unable to execute git: {error}"))?;
    if !output.status.success() {
        return Err(format!(
            "git release-tag verification failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    String::from_utf8(output.stdout)
        .map_err(|error| error.to_string())
        .map(|value| value.trim().to_owned())
}

fn is_lower_hex(value: &str, length: usize) -> bool {
    value.len() == length
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn is_upper_hex(value: &str, length: usize) -> bool {
    value.len() == length
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'A'..=b'F').contains(&byte))
}

#[cfg(test)]
mod tests {
    use super::*;

    const REV: &str = "1111111111111111111111111111111111111111";
    const TREE: &str = "2222222222222222222222222222222222222222";
    const TAG: &str = "3333333333333333333333333333333333333333";
    const HASH: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
    const PRIMARY_FINGERPRINT: &str = "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA";

    fn gate_manifest() -> GateManifest {
        GateManifest {
            schema_version: 1,
            required_gate_count: 2,
            gates: vec![
                GateDefinition {
                    id: "toolchain".into(),
                    kind: "powershell".into(),
                    path: "sdk/tests/toolchain-contract.ps1".into(),
                    required: true,
                },
                GateDefinition {
                    id: "offline-host-plugin".into(),
                    kind: "powershell".into(),
                    path: "sdk/tests/offline-host-plugin-contract.ps1".into(),
                    required: true,
                },
            ],
        }
    }

    fn gate_manifest_hash(gates: &GateManifest) -> String {
        sha256_hex(&serde_json::to_vec(gates).unwrap())
    }

    fn gate_attestation(gates: &GateManifest) -> GateAttestation {
        let mut attestation = GateAttestation {
            schema_version: 1,
            gate_manifest_sha256: gate_manifest_hash(gates),
            candidate_plan_digest: HASH.into(),
            workflow_run_id: "fixture-run".into(),
            nonce: "fixture-nonce".into(),
            results: gates
                .gates
                .iter()
                .map(|gate| GateResult {
                    id: gate.id.clone(),
                    exit_code: 0,
                })
                .collect(),
            attestation_sha256: String::new(),
        };
        attestation.attestation_sha256 = gate_attestation_digest(&attestation).unwrap();
        attestation
    }

    fn reference(path: &str) -> ArtifactReference {
        ArtifactReference {
            path: path.into(),
            sha256: HASH.into(),
        }
    }

    fn lock(revision: &str, tree: &str, bundle: &str) -> Value {
        let gates = gate_manifest();
        let attestation = gate_attestation(&gates);
        serde_json::json!({
            "bundle_id": bundle,
            "toolchain": {"rustc_release":"1.97.1","rustc_commit_hash":"a","cargo_release":"1.97.1","cargo_commit_hash":"b","target":"x86_64-pc-windows-msvc"},
            "gpui": {
                "revision":revision,
                "tree":tree,
                "approved_snapshot":{
                    "release_frozen":true,
                    "source":{"revision":revision,"tree":tree},
                    "candidate_plan_digest":HASH,
                    "approval":{
                        "channel":"development",
                        "state":"approved",
                        "proof":{
                            "candidate_plan_digest":HASH,
                            "workflow_run_id":"fixture-run",
                            "nonce":"fixture-nonce"
                        },
                        "gates":attestation
                    },
                    "production":{"features":[]}
                }
            },
            "protected_dependency_graph": [], "protected_dependency_contract": {"schema_version":2,"edge_digest":"x"},
            "sdk_public_source_hashes": [], "release_profiles": {},
            "build_policy": {"profile":{"panic":"unwind","lto":"thin","codegen_units":1},"allocator":{},"crt":{},"rustflags":[],"abi_schema_version":1}
        })
    }

    fn fixture() -> (Metadata, Value, Value, Value, Value) {
        let lock = lock(REV, TREE, "sdk-bundle");
        let fingerprint = serde_json::json!({
            "bundle_id":"sdk-bundle",
            "fingerprint":production_fingerprint_from_lock(&lock).unwrap().fingerprint
        });
        let mut metadata = Metadata {
            schema_version: SCHEMA_VERSION,
            release_frozen: true,
            evidence_mode: EvidenceMode::Fixture,
            protected_tag: ProtectedTag {
                name: "gpui-sdk-v1".into(),
                tag_object: TAG.into(),
                object_revision: REV.into(),
                tree: TREE.into(),
                repository: AUTHORIZED_GPUI_REPOSITORY.into(),
                signer_primary_fingerprint: PRIMARY_FINGERPRINT.into(),
            },
            source: FrozenSource {
                revision: REV.into(),
                tree: TREE.into(),
            },
            rc_id: "rc-1".into(),
            bundle_id: "sdk-bundle".into(),
            release_input_digest: String::new(),
            artifacts: ReleaseArtifacts {
                sdk_lock: reference(SDK_LOCK_PATH),
                bundle_manifest: reference(BUNDLE_MANIFEST_PATH),
                ui_abi_fingerprint: reference(UI_FINGERPRINT_PATH),
            },
            protection: ProtectionEvidence {
                provider: "fixture".into(),
                policy_id: "fixture-policy".into(),
                record: reference("evidence/protection.json"),
            },
            signature: SignatureEvidence {
                verification: "fixture_unsigned".into(),
                signer: "fixture".into(),
                primary_fingerprint: PRIMARY_FINGERPRINT.into(),
                artifact: reference("evidence/signature.json"),
            },
            provenance: ProvenanceEvidence {
                builder: "fixture".into(),
                predicate_type: "fixture".into(),
                artifact: reference("evidence/provenance.json"),
            },
            prior_release_ledger: reference(LEDGER_PATH),
        };
        metadata.release_input_digest = release_input_digest(&metadata).unwrap();
        let manifest = serde_json::json!({"bundle_id":"sdk-bundle"});
        let ledger = serde_json::json!({"schema_version":1,"releases":[]});
        (metadata, lock, manifest, fingerprint, ledger)
    }

    fn protection_proof(metadata: &Metadata) -> Value {
        serde_json::json!({
            "schema_version":1,
            "provider":metadata.protection.provider,
            "policy_id":metadata.protection.policy_id,
            "repository":metadata.protected_tag.repository,
            "tag_name":metadata.protected_tag.name,
            "tag_object":metadata.protected_tag.tag_object,
            "object_revision":metadata.protected_tag.object_revision,
            "tree":metadata.protected_tag.tree,
        })
    }

    fn validate_fixture(
        metadata: &Metadata,
        lock: &Value,
        manifest: &Value,
        fingerprint: &Value,
        ledger: &Value,
        expected_mode: EvidenceMode,
    ) -> Result<(), String> {
        let gates = gate_manifest();
        validate(
            metadata,
            lock,
            manifest,
            fingerprint,
            ledger,
            &protection_proof(metadata),
            &gates,
            &gate_manifest_hash(&gates),
            expected_mode,
        )
    }

    #[test]
    fn fixture_release_validates_and_is_not_production() {
        let (metadata, lock, manifest, fingerprint, ledger) = fixture();
        assert!(
            validate_fixture(
                &metadata,
                &lock,
                &manifest,
                &fingerprint,
                &ledger,
                EvidenceMode::Fixture
            )
            .is_ok()
        );
        assert!(
            validate_fixture(
                &metadata,
                &lock,
                &manifest,
                &fingerprint,
                &ledger,
                EvidenceMode::Production
            )
            .is_err()
        );
    }

    #[test]
    fn unknown_fields_are_rejected() {
        let (metadata, ..) = fixture();
        let mut value = serde_json::to_value(metadata).unwrap();
        value["untrusted"] = Value::Bool(true);
        assert!(serde_json::from_value::<Metadata>(value).is_err());
    }

    #[test]
    fn digest_binds_tag_bundle_and_evidence() {
        let (metadata, lock, manifest, fingerprint, ledger) = fixture();
        for mutate in [
            |value: &mut Metadata| value.protected_tag.name.push_str("-drift"),
            |value: &mut Metadata| value.bundle_id.push_str("-drift"),
            |value: &mut Metadata| value.protection.policy_id.push_str("-drift"),
        ] {
            let mut changed = metadata.clone();
            mutate(&mut changed);
            assert!(
                validate_fixture(
                    &changed,
                    &lock,
                    &manifest,
                    &fingerprint,
                    &ledger,
                    EvidenceMode::Fixture
                )
                .is_err()
            );
        }
    }

    #[test]
    fn digest_binds_signer_and_protected_tag_proof_identities() {
        let (metadata, ..) = fixture();
        for mutate in [
            |value: &mut Metadata| {
                value.protected_tag.signer_primary_fingerprint =
                    "BBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBB".into()
            },
            |value: &mut Metadata| {
                value.signature.primary_fingerprint =
                    "BBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBB".into()
            },
            |value: &mut Metadata| {
                value.protection.record.sha256 =
                    "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb".into()
            },
        ] {
            let mut changed = metadata.clone();
            mutate(&mut changed);
            assert_ne!(
                release_input_digest(&changed).unwrap(),
                metadata.release_input_digest
            );
        }
    }

    #[test]
    fn protected_tag_proof_and_full_gate_attestation_are_fail_closed() {
        let (metadata, mut lock, manifest, mut fingerprint, ledger) = fixture();
        let gates = gate_manifest();
        let manifest_hash = gate_manifest_hash(&gates);
        let mut proof = protection_proof(&metadata);
        proof["tag_name"] = Value::String("gpui-sdk-vwrong".into());
        assert!(
            validate(
                &metadata,
                &lock,
                &manifest,
                &fingerprint,
                &ledger,
                &proof,
                &gates,
                &manifest_hash,
                EvidenceMode::Fixture
            )
            .is_err()
        );

        let attestation_value = lock
            .pointer_mut("/gpui/approved_snapshot/approval/gates")
            .unwrap();
        let mut attestation: GateAttestation =
            serde_json::from_value(attestation_value.clone()).unwrap();
        attestation.results[0].exit_code = 1;
        attestation.attestation_sha256 = gate_attestation_digest(&attestation).unwrap();
        *attestation_value = serde_json::to_value(attestation).unwrap();
        fingerprint["fingerprint"] =
            Value::String(production_fingerprint_from_lock(&lock).unwrap().fingerprint);
        assert!(
            validate_fixture(
                &metadata,
                &lock,
                &manifest,
                &fingerprint,
                &ledger,
                EvidenceMode::Fixture
            )
            .is_err()
        );
    }

    #[test]
    fn prior_ledger_prevents_rc_reuse_after_regeneration() {
        let (metadata, lock, manifest, fingerprint, mut ledger) = fixture();
        ledger["releases"] = serde_json::json!([{
            "rc_id":"rc-1", "bundle_id":"fresh-bundle",
            "source":{"revision":"4444444444444444444444444444444444444444","tree":TREE}
        }]);
        assert!(
            validate_fixture(
                &metadata,
                &lock,
                &manifest,
                &fingerprint,
                &ledger,
                EvidenceMode::Fixture
            )
            .is_err()
        );
    }

    #[test]
    fn embedded_snapshot_must_be_marked_frozen_and_match_the_tagged_source() {
        let (metadata, mut lock, manifest, mut fingerprint, ledger) = fixture();
        lock["gpui"]["approved_snapshot"]["release_frozen"] = Value::Bool(false);
        fingerprint["fingerprint"] =
            Value::String(production_fingerprint_from_lock(&lock).unwrap().fingerprint);
        assert!(
            validate_fixture(
                &metadata,
                &lock,
                &manifest,
                &fingerprint,
                &ledger,
                EvidenceMode::Fixture
            )
            .is_err()
        );

        lock["gpui"]["approved_snapshot"]["release_frozen"] = Value::Bool(true);
        lock["gpui"]["approved_snapshot"]["source"]["revision"] =
            Value::String("4444444444444444444444444444444444444444".into());
        fingerprint["fingerprint"] =
            Value::String(production_fingerprint_from_lock(&lock).unwrap().fingerprint);
        assert!(
            validate_fixture(
                &metadata,
                &lock,
                &manifest,
                &fingerprint,
                &ledger,
                EvidenceMode::Fixture
            )
            .is_err()
        );
    }

    #[test]
    fn changed_frozen_source_and_bundle_require_a_new_rc() {
        let (mut metadata, _lock, _manifest, _fingerprint, mut ledger) = fixture();
        ledger["releases"] = serde_json::json!([{
            "rc_id":"rc-1", "bundle_id":"sdk-bundle",
            "source":{"revision":REV,"tree":TREE}
        }]);
        let changed_revision = "4444444444444444444444444444444444444444";
        let changed_tree = "5555555555555555555555555555555555555555";
        let new_bundle_lock = lock(changed_revision, changed_tree, "new-bundle");
        let manifest = serde_json::json!({"bundle_id":"new-bundle"});
        let fingerprint = serde_json::json!({
            "bundle_id":"new-bundle",
            "fingerprint":production_fingerprint_from_lock(&new_bundle_lock).unwrap().fingerprint
        });
        metadata.bundle_id = "new-bundle".into();
        metadata.source = FrozenSource {
            revision: changed_revision.into(),
            tree: changed_tree.into(),
        };
        metadata.protected_tag.object_revision = changed_revision.into();
        metadata.protected_tag.tree = changed_tree.into();
        metadata.release_input_digest = release_input_digest(&metadata).unwrap();
        assert!(
            validate_fixture(
                &metadata,
                &new_bundle_lock,
                &manifest,
                &fingerprint,
                &ledger,
                EvidenceMode::Fixture
            )
            .is_err()
        );

        metadata.rc_id = "rc-2".into();
        metadata.bundle_id = "sdk-bundle".into();
        metadata.release_input_digest = release_input_digest(&metadata).unwrap();
        let reused_lock = lock(changed_revision, changed_tree, "sdk-bundle");
        let reused_manifest = serde_json::json!({"bundle_id":"sdk-bundle"});
        let reused_fingerprint = serde_json::json!({
            "bundle_id":"sdk-bundle",
            "fingerprint":production_fingerprint_from_lock(&reused_lock).unwrap().fingerprint
        });
        assert!(
            validate_fixture(
                &metadata,
                &reused_lock,
                &reused_manifest,
                &reused_fingerprint,
                &ledger,
                EvidenceMode::Fixture
            )
            .is_err()
        );

        metadata.bundle_id = "new-bundle".into();
        metadata.release_input_digest = release_input_digest(&metadata).unwrap();
        assert!(
            validate_fixture(
                &metadata,
                &new_bundle_lock,
                &manifest,
                &fingerprint,
                &ledger,
                EvidenceMode::Fixture
            )
            .is_ok()
        );
    }
}
