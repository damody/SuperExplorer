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
    release_input_digest: String,
    source: FrozenSource,
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
pub fn validate(
    metadata: &Metadata,
    lock: &Value,
    manifest: &Value,
    fingerprint: &Value,
    ledger: &Value,
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
                || previous.bundle_id != metadata.bundle_id
                || previous.release_input_digest != metadata.release_input_digest)
        {
            return Err("RC ID was already used for a different immutable release input".into());
        }
    }
    Ok(())
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
    let metadata: Metadata = read_json(root, "sdk/snapshot/release-freeze.json")?;
    let lock: Value = read_reference(root, &metadata.artifacts.sdk_lock)?;
    let manifest: Value = read_reference(root, &metadata.artifacts.bundle_manifest)?;
    let fingerprint: Value = read_reference(root, &metadata.artifacts.ui_abi_fingerprint)?;
    let ledger: Value = read_reference(root, &metadata.prior_release_ledger)?;
    for reference in [
        &metadata.protection.record,
        &metadata.signature.artifact,
        &metadata.provenance.artifact,
    ] {
        read_reference_bytes(root, reference)?;
    }
    validate(
        &metadata,
        &lock,
        &manifest,
        &fingerprint,
        &ledger,
        expected_mode,
    )?;
    let tag_repository = match expected_mode {
        EvidenceMode::Production => root.join("vendor/gpui-ce"),
        EvidenceMode::Fixture => root.to_path_buf(),
    };
    verify_annotated_tag(&tag_repository, &metadata.protected_tag)
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

fn read_json<T: serde::de::DeserializeOwned>(root: &Path, relative: &str) -> Result<T, String> {
    let path = rooted_path(root, relative)?;
    let source =
        fs::read_to_string(&path).map_err(|error| format!("{}: {error}", path.display()))?;
    serde_json::from_str(&source).map_err(|error| format!("{}: {error}", path.display()))
}

fn read_reference<T: serde::de::DeserializeOwned>(
    root: &Path,
    reference: &ArtifactReference,
) -> Result<T, String> {
    let bytes = read_reference_bytes(root, reference)?;
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

#[cfg(test)]
mod tests {
    use super::*;

    const REV: &str = "1111111111111111111111111111111111111111";
    const TREE: &str = "2222222222222222222222222222222222222222";
    const TAG: &str = "3333333333333333333333333333333333333333";
    const HASH: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";

    fn reference(path: &str) -> ArtifactReference {
        ArtifactReference {
            path: path.into(),
            sha256: HASH.into(),
        }
    }

    fn lock(revision: &str, tree: &str, bundle: &str) -> Value {
        serde_json::json!({
            "bundle_id": bundle,
            "toolchain": {"rustc_release":"1.97.1","rustc_commit_hash":"a","cargo_release":"1.97.1","cargo_commit_hash":"b","target":"x86_64-pc-windows-msvc"},
            "gpui": {"revision":revision,"tree":tree,"approved_snapshot":{"production":{"features":[]}}},
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

    #[test]
    fn fixture_release_validates_and_is_not_production() {
        let (metadata, lock, manifest, fingerprint, ledger) = fixture();
        assert!(
            validate(
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
            validate(
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
                validate(
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
    fn prior_ledger_prevents_rc_reuse_after_regeneration() {
        let (metadata, lock, manifest, fingerprint, mut ledger) = fixture();
        ledger["releases"] = serde_json::json!([{
            "rc_id":"rc-1", "bundle_id":"fresh-bundle", "release_input_digest": HASH,
            "source":{"revision":"4444444444444444444444444444444444444444","tree":TREE}
        }]);
        assert!(
            validate(
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
}
