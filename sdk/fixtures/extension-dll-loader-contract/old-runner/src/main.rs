use std::{env, path::Path, process::ExitCode};

use abi_stable::{library::RootModule, std_types::RResult};
use explorer_extension_api::{
    ABI_SCHEMA_V1, ExtensionRootModuleV1_Ref, ROOT_MODULE_CONTRACT_ID_V1, RegistrarRequestV1,
    SDK_MAJOR_VERSION_V1,
};

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("old host compatibility gate: {error}");
            ExitCode::FAILURE
        }
    }
}

fn run() -> Result<(), String> {
    let mut arguments = env::args_os().skip(1);
    let plugin = arguments.next().ok_or("missing plugin path")?;
    let marker = arguments.next().ok_or("missing callback marker path")?;
    if arguments.next().is_some() {
        return Err("usage: old-runner <plugin> <marker>".to_owned());
    }
    let plugin = Path::new(&plugin);
    let marker = Path::new(&marker);
    let root = ExtensionRootModuleV1_Ref::load_from_file(plugin)
        .map_err(|error| format!("old v1 host rejected new DLL: {error}"))?;
    let result = root.registrar().register().invoke(RegistrarRequestV1 {
        abi_schema: ABI_SCHEMA_V1,
        root_contract_id: ROOT_MODULE_CONTRACT_ID_V1,
        sdk_major: SDK_MAJOR_VERSION_V1,
        reserved: 0,
    });
    if !matches!(result, RResult::ROk(_)) {
        return Err(format!(
            "old v1 host registrar rejected new DLL: {result:?}"
        ));
    }
    if !marker.exists() {
        return Err("old v1 host did not reach the required registrar callback".to_owned());
    }
    Ok(())
}
