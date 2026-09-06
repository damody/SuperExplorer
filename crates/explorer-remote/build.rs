use std::{env, fs, path::PathBuf};

fn main() {
    let manifest = PathBuf::from(env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR"));
    let source = manifest.join("google-oauth.local.json");
    let bundled =
        PathBuf::from(env::var("OUT_DIR").expect("OUT_DIR")).join("google-oauth.bundled.json");
    if source.exists() {
        fs::copy(&source, &bundled).expect("copy google-oauth.local.json into the build output");
    } else {
        fs::write(&bundled, "{}").expect("write empty bundled Google OAuth JSON");
        println!(
            "cargo:warning=google-oauth.local.json is missing; Google Drive builds without a bundled OAuth client"
        );
    }
    println!("cargo:rerun-if-changed=google-oauth.local.json");
    let include_path = bundled.to_string_lossy().replace('\\', "/");
    println!("cargo:rustc-env=SUPEREXPLORER_GDRIVE_OAUTH_JSON={include_path}");
}
