//! Process-isolated pre-callback ABI verifier for legacy and Rust-first fixtures.

use std::{env, path::Path};

use abi_stable::library::RootModule;
use explorer_extension_api::{ExtensionRootModuleV1_Ref, registrar_request_v1};
use explorer_extension_host::ExtensionHost;

fn run(mode: &str, plugin: &Path, marker: &Path) -> Result<(), String> {
    if mode == "rust-first-baseline" {
        let root = ExtensionRootModuleV1_Ref::load_from_file(plugin)
            .map_err(|error| format!("current host rejected Rust-first baseline: {error}"))?;
        ExtensionHost::new()
            .validate_root(root)
            .map_err(|error| format!("Rust-first baseline root rejected: {error:?}"))?;
        if root.descriptor_contract_revision() != 1 {
            return Err("Rust-first baseline reports the wrong descriptor contract revision".into());
        }
        let registrar = root
            .create_registrar()
            .create()
            .into_result()
            .map_err(|error| format!("baseline registrar factory failed: {error:?}"))?;
        let output = registrar
            .register(registrar_request_v1())
            .into_result()
            .map_err(|error| format!("baseline registrar failed: {error:?}"))?;
        if !output.outcome.is_accepted() || !marker.exists() {
            return Err("baseline registrar did not execute successfully".into());
        }
        return Ok(());
    }

    if !matches!(
        mode,
        "compatible"
            | "schema-mismatch"
            | "root-contract-mismatch"
            | "sdk-major-mismatch"
            | "panic"
            | "raw-panic"
    ) {
        return Err("unknown legacy raw fixture mode".to_owned());
    }
    match ExtensionRootModuleV1_Ref::load_from_file(plugin) {
        Err(_) if !marker.exists() => Ok(()),
        Err(error) => Err(format!("legacy layout rejection ran foreign code: {error}")),
        Ok(_) => Err("legacy raw root was not layout-rejected before callback".to_owned()),
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
