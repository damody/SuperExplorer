//! A real SDK 1.0 plugin with no `describe_contract` registrar tail.

use std::{env, fs, path::PathBuf};

use abi_stable::{export_root_module, prefix_type::PrefixTypeTrait, std_types::RResult};
use explorer_extension_api::{
    ABI_SCHEMA_V1, AbiSchemaIdV1, ExtensionRegistrarV1, ExtensionRootModuleV1,
    ExtensionRootModuleV1_Ref, PluginMetadataV1, ROOT_MODULE_CONTRACT_ID_V1, RegistrarCallbackV1,
    RegistrarImplementationV1, RegistrarRequestV1, RegistrarResultV1, RegistrationOutcomeV1,
    SDK_MAJOR_VERSION_V1, StableIdV1,
};

const MODE_ENV: &str = "EXTENSION_API_CONTRACT_MODE";
const MARKER_ENV: &str = "EXTENSION_API_CONTRACT_MARKER";

fn mode() -> String {
    env::var(MODE_ENV).unwrap_or_else(|_| "compatible".to_owned())
}

fn marker_path() -> Option<PathBuf> {
    env::var_os(MARKER_ENV).map(PathBuf::from)
}

struct FixtureRegistrar;

impl RegistrarImplementationV1 for FixtureRegistrar {
    fn register(_: RegistrarRequestV1) -> RegistrarResultV1 {
        if mode() == "panic" {
            if let Some(marker) = marker_path() {
                let _ = fs::write(marker, b"old-v1 registrar entered before translated panic");
            }
            panic!("old v1 fixture registrar panic");
        }
        if let Some(marker) = marker_path()
            && let Err(error) = fs::write(marker, b"old-v1 registrar invoked")
        {
            return RResult::RErr(explorer_extension_api::AbiErrorV1::new(
                explorer_extension_api::AbiErrorCodeV1::CALLBACK_PANICKED,
                ROOT_MODULE_CONTRACT_ID_V1,
                error.raw_os_error().unwrap_or_default() as u32,
            ));
        }
        RResult::ROk(RegistrationOutcomeV1::accepted(1))
    }
}

extern "C" fn raw_panics(_: RegistrarRequestV1) -> RegistrarResultV1 {
    panic!("deliberately fabricated raw registrar panic");
}

fn registrar_callback() -> RegistrarCallbackV1 {
    if mode() == "raw-panic" {
        // Deliberately bypass the safe SDK constructor to prove that unsafe
        // fabrication fails closed by terminating this isolated process.
        unsafe {
            std::mem::transmute::<
                extern "C" fn(RegistrarRequestV1) -> RegistrarResultV1,
                RegistrarCallbackV1,
            >(raw_panics)
        }
    } else {
        RegistrarCallbackV1::new::<FixtureRegistrar>()
    }
}

fn root_values() -> (AbiSchemaIdV1, StableIdV1, u16) {
    match mode().as_str() {
        "schema-mismatch" => (
            AbiSchemaIdV1::new(0x5345, 2),
            ROOT_MODULE_CONTRACT_ID_V1,
            SDK_MAJOR_VERSION_V1,
        ),
        "root-contract-mismatch" => (
            ABI_SCHEMA_V1,
            StableIdV1::new(explorer_extension_api::EXTENSION_ID_NAMESPACE_V1, 99),
            SDK_MAJOR_VERSION_V1,
        ),
        "sdk-major-mismatch" => (ABI_SCHEMA_V1, ROOT_MODULE_CONTRACT_ID_V1, 2),
        _ => (
            ABI_SCHEMA_V1,
            ROOT_MODULE_CONTRACT_ID_V1,
            SDK_MAJOR_VERSION_V1,
        ),
    }
}

#[export_root_module]
pub fn get_library() -> ExtensionRootModuleV1_Ref {
    let (abi_schema, root_contract_id, sdk_major) = root_values();
    let registrar = ExtensionRegistrarV1 {
        register: registrar_callback(),
    }
    .leak_into_prefix();
    ExtensionRootModuleV1 {
        abi_schema,
        root_contract_id,
        sdk_major,
        reserved: 0,
        metadata: PluginMetadataV1 {
            plugin_id: StableIdV1::new(explorer_extension_api::EXTENSION_ID_NAMESPACE_V1, 100),
            primary_interface_id: StableIdV1::new(
                explorer_extension_api::EXTENSION_ID_NAMESPACE_V1,
                101,
            ),
        },
        registrar,
    }
    .leak_into_prefix()
}
