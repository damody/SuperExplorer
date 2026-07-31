//! Fixed-root UI ABI fingerprinting and pre-callback compatibility checks.
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{env, fs, path::Path};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UiAbiFingerprint {
    pub bundle_id: String,
    pub fingerprint: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompatibilityDiagnostic {
    pub host_bundle_id: String,
    pub plugin_bundle_id: String,
    pub host_fingerprint: String,
    pub plugin_fingerprint: String,
}

/// Rejects a plugin before any callback when its bundle or UI ABI fingerprint differs.
///
/// # Errors
/// Returns a typed diagnostic containing both bundle IDs when comparison fails.
pub fn compare_before_callback(
    host: &UiAbiFingerprint,
    plugin: &UiAbiFingerprint,
) -> Result<(), CompatibilityDiagnostic> {
    if host.fingerprint == plugin.fingerprint {
        Ok(())
    } else {
        Err(CompatibilityDiagnostic {
            host_bundle_id: host.bundle_id.clone(),
            plugin_bundle_id: plugin.bundle_id.clone(),
            host_fingerprint: host.fingerprint.clone(),
            plugin_fingerprint: plugin.fingerprint.clone(),
        })
    }
}

/// Produces the deterministic fingerprint for exactly the UI ABI inputs. `host_build_id` is intentionally absent.
///
/// # Errors
/// Returns an error if any required UI ABI input is absent or cannot be canonicalized.
pub fn fingerprint(bundle_id: String, inputs: &Value) -> Result<UiAbiFingerprint, String> {
    validate_inputs(inputs)?;
    let mut projected = inputs.clone();
    projected
        .as_object_mut()
        .ok_or("UI ABI inputs must be a JSON object")?
        .remove("host_build_id");
    Ok(UiAbiFingerprint {
        bundle_id,
        fingerprint: sha256_hex(&serde_json::to_vec(&projected).map_err(|e| e.to_string())?),
    })
}

/// Derives the production fingerprint from canonical SDK lock data.
///
/// # Errors
/// Returns an error when any required lock or policy input is absent.
pub fn production_fingerprint_from_lock(lock: &Value) -> Result<UiAbiFingerprint, String> {
    let bundle_id = required_string(lock, "bundle_id")?;
    fingerprint(bundle_id, &production_inputs(lock)?)
}

/// Serializes a stable, newline-terminated fingerprint artifact.
///
/// # Errors
/// Returns an error if artifact serialization fails.
pub fn artifact_bytes(fingerprint: &UiAbiFingerprint) -> Result<Vec<u8>, String> {
    let mut bytes = serde_json::to_vec_pretty(fingerprint).map_err(|error| error.to_string())?;
    bytes.push(b'\n');
    Ok(bytes)
}

fn validate_inputs(inputs: &Value) -> Result<(), String> {
    for key in [
        "toolchain",
        "target",
        "gpui",
        "protected_dependency_graph",
        "protected_dependency_contract",
        "sdk_public_source_hashes",
        "features",
        "release_profile",
        "panic",
        "allocator",
        "crt",
        "lto",
        "codegen_units",
        "rustflags",
        "abi_schema_version",
    ] {
        if inputs.get(key).is_none_or(Value::is_null) {
            return Err(format!("missing UI ABI input: {key}"));
        }
    }
    Ok(())
}

/// Production commands are fixed to the repository root and never accept injected lock data.
///
/// # Errors
/// Returns an error for an invalid command, stale/missing canonical inputs, or serialization failure.
pub fn run(arguments: impl IntoIterator<Item = String>) -> Result<(), String> {
    let args: Vec<_> = arguments.into_iter().collect();
    if !matches!(args.as_slice(), [command] if command == "generate" || command == "verify") {
        return Err("usage: superexplorer-ui-abi-fingerprint generate".to_owned());
    }
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(3)
        .ok_or("repository root unavailable")?;
    let lock: Value = serde_json::from_str(
        &fs::read_to_string(root.join("sdk/sdk-lock.json")).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;
    let artifact = production_fingerprint_from_lock(&lock)?;
    let bytes = artifact_bytes(&artifact)?;
    let path = root.join("sdk/ui-abi-fingerprint.json");
    if args[0] == "verify" {
        if fs::read(&path).map_err(|e| e.to_string())? == bytes {
            Ok(())
        } else {
            Err("UI ABI fingerprint artifact is stale".to_owned())
        }
    } else {
        fs::write(path, bytes).map_err(|e| e.to_string())
    }
}

fn required_string(value: &Value, key: &str) -> Result<String, String> {
    value
        .get(key)
        .and_then(Value::as_str)
        .filter(|item| !item.is_empty())
        .map(str::to_owned)
        .ok_or_else(|| format!("missing required {key}"))
}

fn production_inputs(lock: &Value) -> Result<Value, String> {
    let policy = lock
        .get("build_policy")
        .ok_or("sdk lock has no build policy")?;
    Ok(serde_json::json!({
        "toolchain": lock.get("toolchain"), "target": "x86_64-pc-windows-msvc", "gpui": lock.get("gpui"),
        "protected_dependency_graph": lock.get("protected_dependency_graph"), "protected_dependency_contract": lock.get("protected_dependency_contract"), "sdk_public_source_hashes": lock.get("sdk_public_source_hashes"),
        "features": lock.pointer("/gpui/approved_snapshot/production/features"), "release_profile": lock.get("release_profiles"),
        "panic": policy.pointer("/profile/panic"), "allocator": policy.get("allocator"), "crt": policy.get("crt"),
        "lto": policy.pointer("/profile/lto"), "codegen_units": policy.pointer("/profile/codegen_units"),
        "rustflags": policy.get("rustflags"), "abi_schema_version": policy.get("abi_schema_version"),
    }))
}

#[allow(
    clippy::format_collect,
    clippy::many_single_char_names,
    clippy::semicolon_if_nothing_returned,
    clippy::unreadable_literal,
    unused_parens
)]
/// Returns the lowercase SHA-256 digest of the supplied bytes.
#[must_use]
pub fn sha256_hex(input: &[u8]) -> String {
    const K: [u32; 64] = [
        0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4,
        0xab1c5ed5, 0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe,
        0x9bdc06a7, 0xc19bf174, 0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc, 0x2de92c6f,
        0x4a7484aa, 0x5cb0a9dc, 0x76f988da, 0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7,
        0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967, 0x27b70a85, 0x2e1b2138, 0x4d2c6dfc,
        0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85, 0xa2bfe8a1, 0xa81a664b,
        0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070, 0x19a4c116,
        0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3,
        0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7,
        0xc67178f2,
    ];
    let mut data = input.to_vec();
    let bits = (data.len() as u64) * 8;
    data.push(0x80);
    while data.len() % 64 != 56 {
        data.push(0)
    }
    data.extend_from_slice(&bits.to_be_bytes());
    let mut h: [u32; 8] = [
        0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a, 0x510e527f, 0x9b05688c, 0x1f83d9ab,
        0x5be0cd19,
    ];
    for block in data.chunks_exact(64) {
        let mut w = [0u32; 64];
        for i in 0..16 {
            w[i] = u32::from_be_bytes(block[i * 4..i * 4 + 4].try_into().unwrap())
        }
        for i in 16..64 {
            w[i] = w[i - 16]
                .wrapping_add(
                    (w[i - 15].rotate_right(7) ^ w[i - 15].rotate_right(18) ^ (w[i - 15] >> 3)),
                )
                .wrapping_add(w[i - 7])
                .wrapping_add(
                    w[i - 2].rotate_right(17) ^ w[i - 2].rotate_right(19) ^ (w[i - 2] >> 10),
                );
        }
        let (mut a, mut b, mut c, mut d, mut e, mut f, mut g, mut hh) =
            (h[0], h[1], h[2], h[3], h[4], h[5], h[6], h[7]);
        for i in 0..64 {
            let t1 = hh
                .wrapping_add(e.rotate_right(6) ^ e.rotate_right(11) ^ e.rotate_right(25))
                .wrapping_add((e & f) ^ (!e & g))
                .wrapping_add(K[i])
                .wrapping_add(w[i]);
            let t2 = (a.rotate_right(2) ^ a.rotate_right(13) ^ a.rotate_right(22))
                .wrapping_add((a & b) ^ (a & c) ^ (b & c));
            hh = g;
            g = f;
            f = e;
            e = d.wrapping_add(t1);
            d = c;
            c = b;
            b = a;
            a = t1.wrapping_add(t2);
        }
        for (x, y) in h.iter_mut().zip([a, b, c, d, e, f, g, hh]) {
            *x = (*x).wrapping_add(y)
        }
    }
    h.iter().map(|v| format!("{v:08x}")).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn sha256_nist_vectors() {
        assert_eq!(
            sha256_hex(b""),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
        assert_eq!(
            sha256_hex(b"abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
        assert_eq!(
            sha256_hex(&vec![b'a'; 1000]),
            "41edece42d63e8d9bf515a9ba6932e1c20cbc9f5a5d134645adb5db1b9737ea3"
        );
    }
    fn inputs() -> Value {
        serde_json::json!({"toolchain":{"rustc":"a","cargo":"b"},"target":"x86_64-pc-windows-msvc","gpui":{"revision":"r","tree":"t"},"protected_dependency_graph":["p"],"protected_dependency_contract":{"schema_version":2,"edge_digest":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"},"sdk_public_source_hashes":["s"],"features":["f"],"release_profile":{"name":"release"},"panic":"unwind","allocator":{"policy":"default"},"crt":{"policy":"dynamic"},"lto":"thin","codegen_units":1,"rustflags":[],"abi_schema_version":1})
    }
    #[test]
    fn every_single_factor_changes_and_rejects() {
        let base = inputs();
        let host = fingerprint("bundle-a".into(), &base).unwrap();
        for key in [
            "toolchain",
            "target",
            "gpui",
            "protected_dependency_graph",
            "protected_dependency_contract",
            "sdk_public_source_hashes",
            "features",
            "release_profile",
            "panic",
            "allocator",
            "crt",
            "lto",
            "codegen_units",
            "rustflags",
            "abi_schema_version",
        ] {
            let mut changed = base.clone();
            changed[key] = serde_json::json!({"changed":key});
            let plugin = fingerprint("bundle-b".into(), &changed).unwrap();
            assert_ne!(host.fingerprint, plugin.fingerprint);
            let d = compare_before_callback(&host, &plugin).unwrap_err();
            assert_eq!(d.host_bundle_id, "bundle-a");
            assert_eq!(d.plugin_bundle_id, "bundle-b");
        }
    }
    #[test]
    fn unrelated_host_build_id_is_not_an_input() {
        let mut first = inputs();
        first["host_build_id"] = serde_json::json!("host-a");
        let mut second = inputs();
        second["host_build_id"] = serde_json::json!("host-b");
        let a = fingerprint("bundle-a".into(), &first).unwrap();
        let b = fingerprint("bundle-b".into(), &second).unwrap();
        assert_eq!(a.fingerprint, b.fingerprint);
        assert!(compare_before_callback(&a, &b).is_ok());
    }

    #[test]
    fn dependency_kind_or_target_change_is_incompatible() {
        let mut normal = inputs();
        normal["protected_dependency_graph"] = serde_json::json!([{
            "key":"example@1", "dependencies":[{"name":"dep","to":"dep@1","dep_kinds":[{"kind":"normal","target":null}]}]
        }]);
        let mut targeted = normal.clone();
        targeted["protected_dependency_graph"][0]["dependencies"][0]["dep_kinds"][0]["target"] =
            serde_json::json!("cfg(windows)");
        let host = fingerprint("host-bundle".into(), &normal).unwrap();
        let plugin = fingerprint("plugin-bundle".into(), &targeted).unwrap();
        assert_ne!(host.fingerprint, plugin.fingerprint);
        assert!(compare_before_callback(&host, &plugin).is_err());
    }
}
