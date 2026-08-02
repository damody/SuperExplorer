//! Process-isolated pre-callback ABI verifier for an old SDK 1.0 plugin.

use std::{env, path::Path};

use abi_stable::library::RootModule;
use explorer_extension_api::ExtensionRootModuleV1_Ref;
use explorer_extension_host::{ExtensionHost, HostRegistrationErrorV1};

fn run(mode: &str, plugin: &Path, marker: &Path) -> Result<(), String> {
    let root = ExtensionRootModuleV1_Ref::load_from_file(plugin)
        .map_err(|error| format!("current host rejected old plugin layout: {error}"))?;
    if root.registrar().describe_contract().is_some() {
        return Err("old v1 plugin unexpectedly exposes the optional registrar tail".to_owned());
    }
    if root.registrar().ui_abi_fingerprint_sha256().is_some() {
        return Err(
            "old v1 plugin unexpectedly exposes the optional UI fingerprint tail".to_owned(),
        );
    }
    // Registrar dispatch is intentionally private to the task 3.5 guarded
    // executor. This fixture preserves the old-v1 layout and required-root-data
    // compatibility contract without reopening a public raw-root callback path.
    let result = ExtensionHost::new().validate_root(root);
    match mode {
        "compatible" | "panic" | "raw-panic" => {
            if result.is_err() {
                return Err(format!("old v1 plugin failed pre-callback validation: {result:?}"));
            }
            if marker.exists() {
                Err("pre-callback ABI validation unexpectedly invoked a registrar".to_owned())
            } else {
                Ok(())
            }
        }
        "schema-mismatch" | "root-contract-mismatch" | "sdk-major-mismatch" => match result {
            Err(HostRegistrationErrorV1::Incompatible(_)) if !marker.exists() => Ok(()),
            other => Err(format!("{mode} did not reject before callback: {other:?}")),
        },
        _ => Err("mode must be compatible, schema-mismatch, root-contract-mismatch, sdk-major-mismatch, panic, or raw-panic".to_owned()),
    }
}

fn main() -> Result<(), String> {
    let mut arguments = env::args_os();
    let _program = arguments.next();
    let mode = arguments.next().ok_or("missing mode")?;
    let plugin = arguments.next().ok_or("missing plugin path")?;
    let marker = arguments.next().ok_or("missing marker path")?;
    if arguments.next().is_some() {
        return Err("too many arguments".to_owned());
    }
    run(
        &mode.to_string_lossy(),
        Path::new(&plugin),
        Path::new(&marker),
    )
}
