#![cfg_attr(feature = "foreign-root", allow(dead_code, non_camel_case_types))]

#[cfg(not(feature = "foreign-root"))]
use abi_stable::std_types::{ROption, RResult};
use abi_stable::{export_root_module, prefix_type::PrefixTypeTrait};
#[cfg(feature = "foreign-root")]
use abi_stable::{library::RootModule, sabi_types::VersionStrings, StableAbi};
#[cfg(feature = "foreign-root")]
use explorer_extension_api::ExtensionRootModuleV1_Ref;
#[cfg(not(feature = "foreign-root"))]
use explorer_extension_api::{
    ExtensionRegistrarV1, ExtensionRootModuleV1, ExtensionRootModuleV1_Ref, PluginMetadataV1,
    RegistrarCallbackV1, RegistrarImplementationV1, RegistrarRequestV1, RegistrarResultV1,
    RegistrationOutcomeV1, StableIdV1, UiAbiFingerprintV1, ABI_SCHEMA_V1,
    ROOT_MODULE_CONTRACT_ID_V1, SDK_MAJOR_VERSION_V1,
};

include!(concat!(env!("OUT_DIR"), "/fingerprint.rs"));

#[cfg(not(feature = "foreign-root"))]
struct Registrar;
#[cfg(not(feature = "foreign-root"))]
impl RegistrarImplementationV1 for Registrar {
    fn register(_: RegistrarRequestV1) -> RegistrarResultV1 {
        mark_callback("register");
        RResult::ROk(RegistrationOutcomeV1::accepted(0))
    }
}
#[cfg(not(feature = "foreign-root"))]
extern "C" fn describe_contract() -> StableIdV1 {
    mark_callback("describe_contract");
    ROOT_MODULE_CONTRACT_ID_V1
}

#[cfg(not(feature = "foreign-root"))]
fn mark_callback(callback: &str) {
    if let Some(marker) = std::env::var_os("EXTENSION_DLL_LOADER_CONTRACT_MARKER") {
        let entrypoint = if cfg!(feature = "alternate") {
            "alternate"
        } else {
            "primary"
        };
        let line = format!("{callback}:{entrypoint}\n");
        let _ = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(marker)
            .and_then(|mut file| std::io::Write::write_all(&mut file, line.as_bytes()));
    }
}

#[export_root_module]
#[cfg(not(feature = "foreign-root"))]
pub fn get_library() -> ExtensionRootModuleV1_Ref {
    let plugin = if cfg!(feature = "alternate") {
        202
    } else {
        200
    };
    let interface = if cfg!(feature = "alternate") {
        203
    } else {
        201
    };
    let fingerprint = if cfg!(feature = "gpui") {
        let bytes = if cfg!(feature = "wrong-fingerprint") {
            [7; 32]
        } else {
            CANONICAL_FINGERPRINT
        };
        ROption::RSome(UiAbiFingerprintV1::new(bytes))
    } else {
        ROption::RNone
    };
    ExtensionRootModuleV1 {
        abi_schema: ABI_SCHEMA_V1,
        root_contract_id: ROOT_MODULE_CONTRACT_ID_V1,
        sdk_major: SDK_MAJOR_VERSION_V1,
        reserved: 0,
        metadata: PluginMetadataV1 {
            plugin_id: StableIdV1::new(explorer_extension_api::EXTENSION_ID_NAMESPACE_V1, plugin),
            primary_interface_id: StableIdV1::new(
                explorer_extension_api::EXTENSION_ID_NAMESPACE_V1,
                interface,
            ),
        },
        registrar: ExtensionRegistrarV1 {
            register: RegistrarCallbackV1::new::<Registrar>(),
            describe_contract,
            ui_abi_fingerprint_sha256: fingerprint,
        }
        .leak_into_prefix(),
    }
    .leak_into_prefix()
}

/// Deliberately incompatible root layout for the batch-atomicity rejection
/// scenario. It exports the expected root-module symbol and name, so the
/// production loader reaches `abi_stable`'s layout check rather than rejecting
/// a non-DLL or a missing export.
#[cfg(feature = "foreign-root")]
#[repr(C)]
#[derive(StableAbi)]
#[sabi(kind(Prefix(prefix_ref = ForeignRootModuleV1_Ref)))]
#[sabi(missing_field(panic))]
pub struct ForeignRootModuleV1 {
    incompatible_layout: u64,
    #[sabi(last_prefix_field)]
    marker: u64,
}

#[cfg(feature = "foreign-root")]
impl RootModule for ForeignRootModuleV1_Ref {
    abi_stable::declare_root_module_statics! {ForeignRootModuleV1_Ref}

    const BASE_NAME: &'static str = "superexplorer_extension_v1";
    const NAME: &'static str = "superexplorer_extension_v1";
    const VERSION_STRINGS: VersionStrings =
        <ExtensionRootModuleV1_Ref as RootModule>::VERSION_STRINGS;
}

#[cfg(feature = "foreign-root")]
#[export_root_module]
pub fn get_library() -> ForeignRootModuleV1_Ref {
    ForeignRootModuleV1 {
        incompatible_layout: 0,
        marker: 0,
    }
    .leak_into_prefix()
}
