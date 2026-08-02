use std::{env, fs, path::PathBuf, process};

use explorer_extension_host::{PackageManifestV1, PayloadKindV1};

fn main() {
    let root = PathBuf::from(env::var_os("PACKAGE_MANIFEST_FIXTURE_ROOT").expect("fixture root"));
    let fixtures = root.join("manifests");
    let cases = [
        ("valid-multi-content.json", true, ""),
        ("valid-minimal-empty-arrays.json", true, ""),
        ("valid-contacts-all-kinds.json", true, ""),
        (
            "malformed-json.json",
            false,
            "invalid package manifest JSON",
        ),
        (
            "unsupported-version.json",
            false,
            "unsupported package manifest version",
        ),
        ("unknown-field.json", false, "invalid package manifest JSON"),
        ("duplicate-feature-id.json", false, "duplicate identifier"),
        ("bad-id.json", false, "invalid normalized identifier"),
        ("bad-sha256.json", false, "invalid SHA-256 format"),
        (
            "invalid-gpui-true-null.json",
            false,
            "sdk.gpui is true but sdk.ui_abi_fingerprint is missing",
        ),
        (
            "invalid-gpui-true-bad-hash.json",
            false,
            "invalid SHA-256 format",
        ),
        (
            "invalid-gpui-false-hash.json",
            false,
            "sdk.gpui is false but sdk.ui_abi_fingerprint is non-null",
        ),
        (
            "invalid-contact-malformed-kind.json",
            false,
            "invalid package manifest JSON",
        ),
        (
            "invalid-contact-unsupported-purpose.json",
            false,
            "invalid package manifest JSON",
        ),
    ];
    for (name, expected_ok, expected_error) in cases {
        let path = fixtures.join(name);
        let mut source = fs::read_to_string(&path).unwrap_or_else(|e| panic!("{name}: {e}"));
        if !name.starts_with("invalid-contact-") {
            source = source.replace("\"contacts\":[]", "\"contacts\":[{\"kind\":\"email\",\"value\":\"support@example.invalid\",\"purposes\":[\"support\"]}]");
        }
        let result = PackageManifestV1::parse_json(&source);
        match (expected_ok, result) {
            (true, Ok(manifest)) => {
                assert_eq!(manifest.manifest_version, 1, "{name}");
                if name == "valid-multi-content.json" {
                    assert_eq!(manifest.payloads.len(), 8);
                    let kinds = manifest
                        .payloads
                        .iter()
                        .map(|payload| payload.kind)
                        .collect::<Vec<_>>();
                    for expected in [
                        PayloadKindV1::RustDll,
                        PayloadKindV1::LuaScript,
                        PayloadKindV1::SkinAsset,
                        PayloadKindV1::Locale,
                        PayloadKindV1::Tool,
                        PayloadKindV1::License,
                        PayloadKindV1::Notice,
                        PayloadKindV1::Data,
                    ] {
                        assert!(
                            kinds.contains(&expected),
                            "missing payload kind: {expected:?}"
                        );
                    }
                }
            }
            (true, Err(error)) => panic!("{name} unexpectedly rejected: {error}"),
            (false, Ok(_)) => panic!("{name} unexpectedly accepted"),
            (false, Err(error)) => {
                let text = error.to_string();
                assert!(text.contains(expected_error), "{name}: {text}");
            }
        }
    }
    for name in ["valid-multi-content.json", "valid-contacts-all-kinds.json"] {
        let source = fs::read_to_string(fixtures.join(name)).expect("valid contact fixture");
        let manifest = PackageManifestV1::parse_json(&source).expect("valid manifest");
        manifest
            .validate_publisher_contact_policy()
            .expect("contact policy");
    }
    let policy_cases = [
        (
            "invalid-contact-missing-contact.json",
            "at least one public contact",
        ),
        (
            "invalid-contact-missing-purpose.json",
            "must declare at least one purpose",
        ),
        (
            "invalid-contact-community-only.json",
            "at least one support or security purpose",
        ),
        (
            "invalid-contact-duplicate-purpose.json",
            "duplicate publisher contact purpose",
        ),
        (
            "invalid-contact-duplicate.json",
            "duplicate publisher contact",
        ),
        (
            "invalid-contact-conflicting-kinds.json",
            "conflicting publisher contact kinds",
        ),
        (
            "invalid-contact-email-value.json",
            "invalid publisher contact value",
        ),
        (
            "invalid-contact-website-value.json",
            "invalid publisher contact value",
        ),
        (
            "invalid-contact-support-forum-value.json",
            "invalid publisher contact value",
        ),
        (
            "invalid-contact-github-value.json",
            "invalid publisher contact value",
        ),
        (
            "invalid-contact-discord-server-value.json",
            "invalid publisher contact value",
        ),
        (
            "invalid-contact-discord-user-value.json",
            "invalid publisher contact value",
        ),
        (
            "invalid-contact-qq-value.json",
            "invalid publisher contact value",
        ),
        ("invalid-contact-other-value.json", "must not be empty"),
        (
            "invalid-publisher-display-name.json",
            "publisher.display_name must not be empty",
        ),
        (
            "invalid-contact-canonical-duplicate.json",
            "duplicate publisher contact",
        ),
    ];
    let policy_case_count = policy_cases.len();
    for (name, expected_error) in policy_cases {
        let source = fs::read_to_string(fixtures.join(name)).expect("policy fixture");
        let error = PackageManifestV1::parse_json(&source)
            .expect_err("invalid contact policy must be rejected by parse_json");
        assert!(
            error.to_string().contains(expected_error),
            "{name}: {error}"
        );
    }
    let total_cases = cases.len() + policy_case_count;
    println!("package manifest v1 contract: PASS ({} cases)", total_cases);
    process::exit(0);
}
