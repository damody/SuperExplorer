use base64::{Engine, engine::general_purpose::STANDARD};
use explorer_extension_host::{
    PackageManifestV1, PackageValidationBudgetV1, PackageValidationCancellationV1,
    PackageValidationRequestV1, PackageValidatorV1, SealedPackageStoreV1,
    TrustedPublisherKeyStoreV1, TrustedPublisherKeyV1,
};
use ring::signature::Ed25519KeyPair;
use serde_json::Value;
use std::{
    env, fs,
    path::PathBuf,
    time::{Duration, Instant},
};

const PUBLIC_B64: &str = "6kpsY+KcUgq+9VB7Ey7F+ZVHdq6+vnuSQh7qaRRG0iw=";

fn main() {
    let root = PathBuf::from(env::var_os("PACKAGE_VALIDATION_FIXTURE_ROOT").expect("fixture root"));
    let source = root.join("source");
    let sealed = root.join("sealed");
    fs::create_dir_all(source.join("data")).expect("source dirs");
    fs::create_dir_all(&sealed).expect("sealed dir");
    fs::write(source.join("data/payload.bin"), b"verified payload").expect("payload");
    let valid: Value = serde_json::from_str(
        &fs::read_to_string(root.join("manifests/valid-signed.json")).expect("manifest"),
    )
    .expect("json");
    let public = STANDARD.decode(PUBLIC_B64).expect("public key");
    let key = TrustedPublisherKeyV1::new(
        "example.signing".into(),
        "example.publisher".into(),
        &public,
    )
    .expect("key");
    let store = SealedPackageStoreV1::new(&sealed).expect("sealed store");
    let validator = PackageValidatorV1::new(
        TrustedPublisherKeyStoreV1::new([key]).expect("store"),
        store.clone(),
    );
    write_manifest(&source, &valid);
    let request = PackageValidationRequestV1::new(source.clone());
    let result = validator.validate(&request).expect("valid signed package");
    let _guard = result.activation_guard().expect("activation guard");
    let mut changed = valid.clone();
    changed["data_version"] = Value::from(99);
    write_manifest(&source, &changed);
    result
        .activation_guard()
        .expect("sealed generation remains valid after source mutation");

    let mut unsigned = valid.clone();
    unsigned["signature"] = serde_json::json!({"kind":"unsigned"});
    for (name, path, expected) in [
        ("absolute", "C:/escape.bin", "unsafe"),
        ("drive", "D:/escape.bin", "unsafe"),
        ("unc", "//server/share.bin", "unsafe"),
        ("dotdot", "../escape.bin", "unsafe"),
    ] {
        let mut v = unsigned.clone();
        v["payloads"][0]["path"] = Value::String(path.into());
        let v = sign(v);
        expect(&validator, &source, &v, expected, name);
    }
    let mut collision = unsigned.clone();
    let p = collision["payloads"][0].clone();
    collision["payloads"].as_array_mut().unwrap().push(p);
    expect(&validator, &source, &collision, "duplicate", "collision");
    fs::remove_file(source.join("data/payload.bin")).expect("remove");
    let signed_unsigned = sign(unsigned.clone());
    expect(
        &validator,
        &source,
        &signed_unsigned,
        "could not access package path",
        "missing",
    );
    fs::write(source.join("data/payload.bin"), b"tampered bytes").expect("tamper");
    expect(&validator, &source, &signed_unsigned, "size is", "size");
    fs::write(source.join("data/payload.bin"), b"verified payload").expect("restore");
    let mut hash = unsigned.clone();
    hash["payloads"][0]["sha256"] =
        Value::String("bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb".into());
    let hash = sign(hash);
    expect(&validator, &source, &hash, "SHA-256 digest", "hash");
    let mut target = unsigned.clone();
    target["sdk"]["target"] = Value::String("aarch64-pc-windows-msvc".into());
    let target = sign(target);
    expect(
        &validator,
        &source,
        &target,
        "does not match host target",
        "target",
    );
    expect(
        &validator,
        &source,
        &unsigned,
        "signature is required",
        "unsigned-default",
    );
    let mut bad_sig = valid.clone();
    bad_sig["signature"]["signature"] = Value::String("AAAA".into());
    expect(
        &validator,
        &source,
        &bad_sig,
        "signature verification failed",
        "bad-signature",
    );
    let mut unknown = valid.clone();
    unknown["signature"]["key_id"] = Value::String("unknown.signing".into());
    expect(&validator, &source, &unknown, "not trusted", "untrusted");
    let wrong_key =
        TrustedPublisherKeyV1::new("example.signing".into(), "other.publisher".into(), &public)
            .expect("wrong key");
    let wrong_validator = PackageValidatorV1::new(
        TrustedPublisherKeyStoreV1::new([wrong_key]).expect("store"),
        store,
    );
    expect(
        &wrong_validator,
        &source,
        &valid,
        "does not match manifest publisher ID",
        "publisher-mismatch",
    );
    let extra = valid.clone();
    write_manifest(&source, &extra);
    fs::write(source.join("extra.txt"), b"extra").expect("extra");
    expect(&validator, &source, &extra, "undeclared file", "extra");
    fs::remove_file(source.join("extra.txt")).expect("cleanup");
    let deadline = PackageValidationRequestV1::new(source.clone()).with_budget(
        PackageValidationBudgetV1::with_deadline(Instant::now() - Duration::from_secs(1)),
    );
    assert!(
        validator
            .validate(&deadline)
            .unwrap_err()
            .to_string()
            .contains("deadline")
    );
    let token = PackageValidationCancellationV1::new();
    token.cancel();
    let cancelled = PackageValidationRequestV1::new(source)
        .with_budget(PackageValidationBudgetV1::default().with_cancellation(token));
    assert!(
        validator
            .validate(&cancelled)
            .unwrap_err()
            .to_string()
            .contains("cancelled")
    );
    println!("package validation v1 contract: PASS (18 cases)");
}

fn write_manifest(root: &PathBuf, value: &Value) {
    fs::write(root.join("manifest.json"), value.to_string()).expect("manifest write");
}

fn sign(mut value: Value) -> Value {
    value["signature"] = serde_json::json!({
        "kind": "ed25519",
        "key_id": "example.signing",
        "signature": ""
    });
    let manifest = PackageManifestV1::parse_json(&value.to_string()).expect("signable manifest");
    let key_pair =
        Ed25519KeyPair::from_seed_unchecked(&[7_u8; 32]).expect("fixed fixture signing seed");
    let signature = STANDARD.encode(
        key_pair
            .sign(
                &manifest
                    .canonical_ed25519_signing_bytes()
                    .expect("canonical signing bytes"),
            )
            .as_ref(),
    );
    value["signature"]["signature"] = Value::String(signature);
    value
}

fn expect(v: &PackageValidatorV1, source: &PathBuf, value: &Value, text: &str, name: &str) {
    write_manifest(source, value);
    let error = v
        .validate(&PackageValidationRequestV1::new(source.clone()))
        .unwrap_err()
        .to_string();
    assert!(
        error
            .to_ascii_lowercase()
            .contains(&text.to_ascii_lowercase()),
        "{name}: {error}"
    );
}
