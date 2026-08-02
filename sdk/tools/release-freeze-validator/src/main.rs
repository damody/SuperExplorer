use std::{env, fs, path::PathBuf};

use release_freeze_validator::{
    EvidenceMode, Metadata, release_input_digest, validate_at_paths, validate_at_root,
};
use serde_json::Value;
use superexplorer_ui_abi_fingerprint::production_fingerprint_from_lock;

fn main() {
    if let Err(error) = run(env::args().skip(1).collect()) {
        eprintln!("release freeze validation failed: {error}");
        std::process::exit(1);
    }
}

fn run(arguments: Vec<String>) -> Result<(), String> {
    match arguments.as_slice() {
        [command] if command == "verify" => validate_at_root(&repository_root()?, EvidenceMode::Production),
        [command, flag, root] if command == "verify-fixture" && flag == "--root" => {
            validate_at_root(&PathBuf::from(root), EvidenceMode::Fixture)
        }
        [command, metadata_flag, metadata, ledger_flag, ledger, evidence_flag, evidence]
            if command == "verify-staged"
                && metadata_flag == "--metadata"
                && ledger_flag == "--ledger"
                && evidence_flag == "--evidence-dir" =>
        {
            validate_at_paths(
                &repository_root()?,
                &PathBuf::from(metadata),
                Some(&PathBuf::from(ledger)),
                Some(&PathBuf::from(evidence)),
                EvidenceMode::Production,
            )
        }
        [command, flag, path] if command == "digest" && flag == "--metadata" => {
            let source = fs::read_to_string(path).map_err(|error| format!("{path}: {error}"))?;
            let metadata: Metadata = serde_json::from_str(&source).map_err(|error| error.to_string())?;
            println!("{}", release_input_digest(&metadata)?);
            Ok(())
        }
        [command, flag, path] if command == "ui-fingerprint" && flag == "--lock" => {
            let source = fs::read_to_string(path).map_err(|error| format!("{path}: {error}"))?;
            let lock: Value = serde_json::from_str(&source).map_err(|error| error.to_string())?;
            println!("{}", production_fingerprint_from_lock(&lock)?.fingerprint);
            Ok(())
        }
        _ => Err("usage: release-freeze-validator verify | verify-fixture --root <root> | verify-staged --metadata <path> --ledger <path> --evidence-dir <path> | digest --metadata <path> | ui-fingerprint --lock <path>".into()),
    }
}

fn repository_root() -> Result<PathBuf, String> {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(3)
        .map(PathBuf::from)
        .ok_or_else(|| "repository root unavailable".into())
}
