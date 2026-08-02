//! Process-isolated host verifier for an old SDK 1.0 plugin.

use std::{env, path::Path};

use abi_stable::library::RootModule;
use explorer_extension_api::{AbiErrorCodeV1, ExtensionRootModuleV1_Ref};
use explorer_extension_host::{ExtensionHost, HostRegistrationErrorV1};

fn run(mode: &str, plugin: &Path, marker: &Path) -> Result<(), String> {
    let root = ExtensionRootModuleV1_Ref::load_from_file(plugin)
        .map_err(|error| format!("current host rejected old plugin layout: {error}"))?;
    if root.registrar().describe_contract().is_some() {
        return Err("old v1 plugin unexpectedly exposes the optional registrar tail".to_owned());
    }
    let mut host = ExtensionHost::new();
    host.start();
    let result = host.register_root(root);
    host.shutdown();
    match mode {
        "compatible" => {
            if result.is_err() {
                return Err(format!("compatible old v1 plugin failed: {result:?}"));
            }
            if marker.exists() {
                Ok(())
            } else {
                Err("compatible plugin did not invoke registrar marker".to_owned())
            }
        }
        "panic" => match result {
            Err(HostRegistrationErrorV1::Panicked(error))
                if error.code == AbiErrorCodeV1::CALLBACK_PANICKED && marker.exists() => Ok(()),
            other => Err(format!("panic was not translated to typed Panicked: {other:?}")),
        },
        "raw-panic" => {
            // The unsafe fixture is expected to abort before control returns from
            // `register_root`.  Leave an explicit sentinel if it ever does return
            // so the process runner cannot confuse this clean error path with the
            // expected abnormal FFI-boundary termination.
            std::fs::write(marker, b"raw callback returned")
                .map_err(|error| format!("failed to write raw-return sentinel: {error}"))?;
            Err(format!(
                "unsafe raw panic unexpectedly returned from registrar: {result:?}"
            ))
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
