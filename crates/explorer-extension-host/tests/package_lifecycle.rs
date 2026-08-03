use std::{fmt::Write as _, fs, path::PathBuf};

use base64::{Engine as _, engine::general_purpose::STANDARD};
use explorer_extension_host::{
    BuiltInPackageSourceV1, PackageResolverV1, PackageSourceV1, PackageValidationErrorV1,
    PackageValidationResultV1, PackageValidatorV1, SealedPackageStoreV1,
    TrustedPublisherKeyStoreV1, TrustedPublisherKeyV1,
};
use ring::{
    rand::SystemRandom,
    signature::{Ed25519KeyPair, KeyPair},
};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use tempfile::TempDir;

const PACKAGE_ID: &str = "example.lifecycle";
const TARGET: &str = "x86_64-pc-windows-msvc";

struct Fixture {
    source_root: TempDir,
    _sealed_root: TempDir,
    package_root: PathBuf,
    payload_path: PathBuf,
    manifest_path: PathBuf,
    validator: PackageValidatorV1,
}

impl Fixture {
    fn source(&self) -> BuiltInPackageSourceV1 {
        BuiltInPackageSourceV1::new(self.source_root.path().to_path_buf())
    }
}

fn fixture(dependencies: Vec<(&str, &str, bool)>) -> Fixture {
    let source_root = tempfile::tempdir().expect("source temporary directory");
    let sealed_root = tempfile::tempdir().expect("sealed temporary directory");
    let package_root = source_root.path().join("built-in-package");
    let payload_path = package_root.join("data/payload.bin");
    let manifest_path = package_root.join("manifest.json");
    let payload = b"validated package payload";
    fs::create_dir_all(payload_path.parent().expect("payload parent")).expect("payload directory");
    fs::write(&payload_path, payload).expect("payload bytes");

    let key_pair = signing_key();
    let mut payload_sha256 = String::with_capacity(64);
    for byte in Sha256::digest(payload) {
        write!(&mut payload_sha256, "{byte:02x}").expect("hex digest write");
    }
    let mut manifest = manifest_value(&payload_sha256, dependencies);
    let signing_manifest =
        explorer_extension_host::PackageManifestV1::parse_json(&manifest.to_string())
            .expect("signing manifest structure");
    let signature = STANDARD.encode(
        key_pair
            .sign(
                &signing_manifest
                    .canonical_ed25519_signing_bytes()
                    .expect("canonical signing bytes"),
            )
            .as_ref(),
    );
    manifest["signature"]["signature"] = Value::String(signature);
    fs::write(&manifest_path, manifest.to_string()).expect("signed manifest");

    let trusted_key = TrustedPublisherKeyV1::new(
        "example.signing".to_owned(),
        "example.publisher".to_owned(),
        key_pair.public_key().as_ref(),
    )
    .expect("trusted signing key");
    let trusted_keys = TrustedPublisherKeyStoreV1::new([trusted_key]).expect("trust store");
    let sealed_store = SealedPackageStoreV1::new(sealed_root.path()).expect("sealed store");

    Fixture {
        source_root,
        _sealed_root: sealed_root,
        package_root,
        payload_path,
        manifest_path,
        validator: PackageValidatorV1::new(trusted_keys, sealed_store),
    }
}

fn signing_key() -> Ed25519KeyPair {
    let rng = SystemRandom::new();
    let pkcs8 = Ed25519KeyPair::generate_pkcs8(&rng).expect("PKCS#8 key");
    Ed25519KeyPair::from_pkcs8(pkcs8.as_ref()).expect("Ed25519 key pair")
}

fn manifest_value(payload_sha256: &str, dependencies: Vec<(&str, &str, bool)>) -> Value {
    json!({
        "manifest_version": 1,
        "package": { "id": PACKAGE_ID, "version": "1.0.0" },
        "publisher": {
            "id": "example.publisher",
            "display_name": "Example Publisher",
            "contacts": [{
                "kind": "email",
                "value": "support@example.invalid",
                "purposes": ["support", "security"]
            }]
        },
        "sdk": {
            "bundle_id": "dev.20260802",
            "target": TARGET,
            "abi_schema": 1,
            "gpui": false,
            "ui_abi_fingerprint": null
        },
        "rust": [],
        "lua": [],
        "skins": [],
        "locales": [],
        "tools": [],
        "features": [],
        "dependencies": dependencies.into_iter().map(|(package_id, version_requirement, optional)| json!({
            "package_id": package_id,
            "version_requirement": version_requirement,
            "optional": optional
        })).collect::<Vec<_>>(),
        "payloads": [{
            "path": "data/payload.bin",
            "size": 25,
            "sha256": payload_sha256,
            "kind": "data"
        }],
        "signature": {
            "kind": "ed25519",
            "key_id": "example.signing",
            "signature": ""
        },
        "data_version": 1
    })
}

fn validate(fixture: &Fixture) -> Vec<PackageValidationResultV1> {
    fixture
        .source()
        .discover()
        .expect("built-in source discovery")
        .iter()
        .map(|candidate| {
            candidate
                .validate(&fixture.validator)
                .expect("signed built-in validation")
        })
        .collect()
}

#[test]
fn built_in_signed_package_is_sealed_resolved_and_activation_guarded() {
    let fixture = fixture(Vec::new());
    let validated = validate(&fixture);

    let resolution = PackageResolverV1::resolve(&validated);

    assert_eq!(resolution.resolved_packages().len(), 1);
    assert!(resolution.blocked_packages().is_empty());
    assert_eq!(
        resolution.resolved_packages()[0].manifest().package.id,
        PACKAGE_ID
    );
    let _activation_guard = resolution.resolved_packages()[0]
        .validation_result()
        .activation_guard()
        .expect("sealed generation activation guard");
    assert!(fixture.package_root.is_dir());
}

#[test]
fn tampered_payload_or_signature_never_produces_a_validated_activation_candidate() {
    let payload_fixture = fixture(Vec::new());
    fs::write(&payload_fixture.payload_path, b"tampered package payload")
        .expect("tampered payload bytes");
    let payload_candidate = payload_fixture
        .source()
        .discover()
        .expect("payload source discovery")
        .pop()
        .expect("payload package candidate");
    assert!(
        payload_candidate
            .validate(&payload_fixture.validator)
            .is_err()
    );

    let signature_fixture = fixture(Vec::new());
    let mut manifest: Value = serde_json::from_str(
        &fs::read_to_string(&signature_fixture.manifest_path).expect("signed manifest bytes"),
    )
    .expect("signed manifest JSON");
    manifest["signature"]["signature"] = Value::String("AAAA".to_owned());
    fs::write(&signature_fixture.manifest_path, manifest.to_string()).expect("tampered signature");
    let signature_candidate = signature_fixture
        .source()
        .discover()
        .expect("signature source discovery")
        .pop()
        .expect("signature package candidate");
    assert!(
        signature_candidate
            .validate(&signature_fixture.validator)
            .is_err()
    );

    let unknown_key_fixture = fixture(Vec::new());
    let mut manifest: Value = serde_json::from_str(
        &fs::read_to_string(&unknown_key_fixture.manifest_path).expect("signed manifest bytes"),
    )
    .expect("signed manifest JSON");
    manifest["signature"]["key_id"] = Value::String("unknown.signing".to_owned());
    fs::write(&unknown_key_fixture.manifest_path, manifest.to_string())
        .expect("unknown signing-key manifest");
    let unknown_key_candidate = unknown_key_fixture
        .source()
        .discover()
        .expect("unknown-key source discovery")
        .pop()
        .expect("unknown-key package candidate");
    assert!(matches!(
        unknown_key_candidate.validate(&unknown_key_fixture.validator),
        Err(PackageValidationErrorV1::UnknownSigningKey { .. })
    ));

    let empty_resolution = PackageResolverV1::resolve(&[]);
    assert!(empty_resolution.resolved_packages().is_empty());
    assert!(empty_resolution.blocked_packages().is_empty());
}

#[test]
fn unsatisfied_dependency_is_excluded_from_the_only_activation_eligible_set() {
    let fixture = fixture(vec![("example.missing", "^1.0.0", false)]);
    let validated = validate(&fixture);

    let resolution = PackageResolverV1::resolve(&validated);

    assert!(resolution.resolved_packages().is_empty());
    assert_eq!(resolution.blocked_packages().len(), 1);
    assert_eq!(resolution.blocked_packages()[0].package_id(), PACKAGE_ID);
    assert!(resolution.diagnostics().iter().any(|diagnostic| {
        diagnostic.dependency_package_id.as_deref() == Some("example.missing")
    }));
    assert!(
        resolution
            .resolved_packages()
            .iter()
            .all(|package| package.manifest().package.id != PACKAGE_ID)
    );
}
