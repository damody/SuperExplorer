use std::{env, fs, process::ExitCode};

use explorer_extension_host::PackageManifestV1;

fn main() -> ExitCode {
    let Some(path) = env::args_os().nth(1) else {
        eprintln!("usage: package-manifest-example-validator <manifest.json>");
        return ExitCode::FAILURE;
    };
    let source = match fs::read_to_string(path) {
        Ok(source) => source,
        Err(error) => {
            eprintln!("unable to read example manifest: {error}");
            return ExitCode::FAILURE;
        }
    };
    match PackageManifestV1::parse_json(&source) {
        Ok(_) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("example manifest is not a valid PackageManifestV1: {error}");
            ExitCode::FAILURE
        }
    }
}
