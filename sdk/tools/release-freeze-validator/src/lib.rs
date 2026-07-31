use serde::Deserialize;
use serde_json::Value;
use superexplorer_ui_abi_fingerprint::production_fingerprint_from_lock;

#[derive(Clone, Deserialize)]
pub struct Metadata {
    pub schema_version: u32,
    pub release_frozen: bool,
    pub protected_tag: Option<Tag>,
    pub source: FrozenSource,
    pub rc_id: String,
    pub bundle_id: String,
    pub release_input_fingerprint: String,
    pub signature_reference: String,
    pub provenance_reference: String,
}

#[derive(Clone, Deserialize)]
pub struct Tag {
    pub name: String,
    pub object_revision: String,
    pub tree: String,
    pub protection_record: String,
}

#[derive(Clone, Deserialize)]
pub struct FrozenSource {
    pub revision: String,
    pub tree: String,
}

/// Validates release-freeze metadata against canonical generated artifacts.
///
/// # Errors
/// Returns a fail-closed diagnostic for missing protection evidence or any identity drift.
pub fn validate(
    metadata: &Metadata,
    lock: &Value,
    manifest: &Value,
    fingerprint: &Value,
) -> Result<(), String> {
    let tag = metadata
        .protected_tag
        .as_ref()
        .ok_or("missing protected tag")?;
    if metadata.schema_version != 1
        || !metadata.release_frozen
        || !tag.name.starts_with("gpui-sdk-v")
        || tag.protection_record.is_empty()
        || metadata.rc_id.is_empty()
        || metadata.signature_reference.is_empty()
        || metadata.provenance_reference.is_empty()
    {
        return Err("release protection metadata is incomplete".into());
    }
    for object_id in [
        tag.object_revision.as_str(),
        tag.tree.as_str(),
        metadata.source.revision.as_str(),
        metadata.source.tree.as_str(),
    ] {
        if !is_lower_hex(object_id, 40) {
            return Err("release source identity is not a full Git object ID".into());
        }
    }
    if tag.object_revision != metadata.source.revision || tag.tree != metadata.source.tree {
        return Err("protected tag differs from frozen source".into());
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
    if !is_lower_hex(artifact_fingerprint, 64)
        || artifact_fingerprint != computed.fingerprint
        || artifact_fingerprint != metadata.release_input_fingerprint
    {
        return Err("release input fingerprint mismatch".into());
    }
    Ok(())
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

    fn fixture() -> (Metadata, Value, Value, Value) {
        let lock = lock(REV, TREE, "sdk-bundle");
        let computed = production_fingerprint_from_lock(&lock).unwrap();
        let metadata = Metadata {
            schema_version: 1,
            release_frozen: true,
            protected_tag: Some(Tag {
                name: "gpui-sdk-v1".into(),
                object_revision: REV.into(),
                tree: TREE.into(),
                protection_record: "protected-branch-policy/1".into(),
            }),
            source: FrozenSource {
                revision: REV.into(),
                tree: TREE.into(),
            },
            rc_id: "rc-1".into(),
            bundle_id: "sdk-bundle".into(),
            release_input_fingerprint: computed.fingerprint.clone(),
            signature_reference: "sig/1".into(),
            provenance_reference: "prov/1".into(),
        };
        let manifest = serde_json::json!({"bundle_id":"sdk-bundle"});
        let fingerprint =
            serde_json::json!({"bundle_id":"sdk-bundle","fingerprint":computed.fingerprint});
        (metadata, lock, manifest, fingerprint)
    }

    #[test]
    fn frozen_release_validates() {
        let (metadata, lock, manifest, fingerprint) = fixture();
        assert!(validate(&metadata, &lock, &manifest, &fingerprint).is_ok());
    }

    #[test]
    fn protection_and_bundle_drift_reject() {
        let (metadata, lock, manifest, fingerprint) = fixture();
        let mut missing = metadata.clone();
        missing.protected_tag = None;
        assert!(validate(&missing, &lock, &manifest, &fingerprint).is_err());
        let mut unfrozen = metadata.clone();
        unfrozen.release_frozen = false;
        assert!(validate(&unfrozen, &lock, &manifest, &fingerprint).is_err());
        for index in 0..3 {
            let mut values = [lock.clone(), manifest.clone(), fingerprint.clone()];
            values[index]["bundle_id"] = Value::String("other".into());
            assert!(validate(&metadata, &values[0], &values[1], &values[2]).is_err());
        }
    }

    #[test]
    fn changed_revision_requires_new_rc_fingerprint_and_bundle() {
        let (mut metadata, mut lock, manifest, fingerprint) = fixture();
        let new_revision = "3333333333333333333333333333333333333333";
        metadata.source.revision = new_revision.into();
        metadata.protected_tag.as_mut().unwrap().object_revision = new_revision.into();
        lock["gpui"]["revision"] = Value::String(new_revision.into());
        assert!(validate(&metadata, &lock, &manifest, &fingerprint).is_err());
    }

    #[test]
    fn remote_main_is_not_a_release_input() {
        let (metadata, mut lock, manifest, fingerprint) = fixture();
        lock["remote_main"] = Value::String("moved".into());
        assert!(validate(&metadata, &lock, &manifest, &fingerprint).is_ok());
    }
}
