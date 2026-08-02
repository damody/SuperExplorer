use std::{env, fs, path::PathBuf};

fn main() {
    println!("cargo:rerun-if-env-changed=SUPEREXPLORER_UI_ABI_FINGERPRINT");
    let fingerprint = env::var("SUPEREXPLORER_UI_ABI_FINGERPRINT")
        .expect("SUPEREXPLORER_UI_ABI_FINGERPRINT must come from canonical SDK artifact");
    assert!(fingerprint.len() == 64 && fingerprint.bytes().all(|byte| byte.is_ascii_hexdigit()));
    let bytes = fingerprint
        .as_bytes()
        .chunks_exact(2)
        .map(|pair| u8::from_str_radix(std::str::from_utf8(pair).unwrap(), 16).unwrap())
        .collect::<Vec<_>>();
    let output = PathBuf::from(env::var_os("OUT_DIR").unwrap()).join("fingerprint.rs");
    fs::write(
        output,
        format!("pub const CANONICAL_FINGERPRINT: [u8; 32] = {:?};", bytes),
    )
    .unwrap();
}
