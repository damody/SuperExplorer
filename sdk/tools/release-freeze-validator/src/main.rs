use std::{env, fs, path::Path};

use release_freeze_validator::{Metadata, validate};
use serde_json::Value;

fn main() {
    let mut arguments = std::env::args().skip(1);
    if arguments.next().as_deref() != Some("verify") || arguments.next().is_some() {
        eprintln!("usage: release-freeze-validator verify");
        std::process::exit(2);
    }
    if let Err(error) = verify() {
        eprintln!("release freeze validation failed: {error}");
        std::process::exit(1);
    }
}

fn verify() -> Result<(), String> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(3)
        .ok_or("repository root unavailable")?;
    let metadata: Metadata = read(root, "sdk/snapshot/release-freeze.json")?;
    let lock: Value = read(root, "sdk/sdk-lock.json")?;
    let manifest: Value = read(root, "sdk/bundle-manifest.json")?;
    let fingerprint: Value = read(root, "sdk/ui-abi-fingerprint.json")?;
    validate(&metadata, &lock, &manifest, &fingerprint)
}

fn read<T: serde::de::DeserializeOwned>(root: &Path, relative: &str) -> Result<T, String> {
    let source =
        fs::read_to_string(root.join(relative)).map_err(|error| format!("{relative}: {error}"))?;
    serde_json::from_str(&source).map_err(|error| format!("{relative}: {error}"))
}
