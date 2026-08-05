#![cfg_attr(feature = "foreign-root", allow(dead_code, non_camel_case_types))]

#[cfg(not(feature = "foreign-root"))]
use abi_stable::std_types::{ROption, RResult, RString, RVec};
#[cfg(feature = "foreign-root")]
use abi_stable::{StableAbi, library::RootModule, sabi_types::VersionStrings};
use abi_stable::{export_root_module, prefix_type::PrefixTypeTrait};
#[cfg(feature = "foreign-root")]
use explorer_extension_api::ExtensionRootModuleV1_Ref;
#[cfg(not(feature = "foreign-root"))]
use explorer_extension_api::{
    ABI_SCHEMA_V1, ExtensionRegistrarImplementationV1, ExtensionRootModuleV1,
    ExtensionRootModuleV1_Ref, PluginMetadataV1, ROOT_MODULE_CONTRACT_ID_V1,
    RegisteredContributionKindV1, RegisteredContributionV1,
    RegistrarOutputResultV1, RegistrarOutputV1, RegistrarRequestV1, RegistrationOutcomeV1,
    SDK_MAJOR_VERSION_V1, StableIdV1, UiAbiFingerprintV1,
};

include!(concat!(env!("OUT_DIR"), "/fingerprint.rs"));

#[cfg(not(feature = "foreign-root"))]
struct Registrar;
#[cfg(not(feature = "foreign-root"))]
impl ExtensionRegistrarImplementationV1 for Registrar {
    fn create() -> Self {
        Self
    }

    fn register(&self, _: RegistrarRequestV1) -> RegistrarOutputResultV1 {
        mark_callback("register");
        if std::env::var_os("EXTENSION_DLL_LOADER_CONTRACT_RAW_ABORT").is_some() {
            // Fixture-only raw termination: this intentionally bypasses the
            // guarded return path so the next helper process must recover the
            // durable native-call marker.
            std::process::abort();
        }
        if let Some(delay) = std::env::var_os("EXTENSION_DLL_LOADER_CONTRACT_SLOW_MS")
            .and_then(|value| value.to_string_lossy().parse::<u64>().ok())
        {
            std::thread::sleep(std::time::Duration::from_millis(delay));
        }
        RResult::ROk(RegistrarOutputV1 {
            outcome: RegistrationOutcomeV1::accepted(1),
            contributions: RVec::from(vec![RegisteredContributionV1 {
                feature_id: RString::from("fixture"),
                contribution_id: RString::from(if cfg!(feature = "alternate") {
                    "alternate-column"
                } else {
                    "loader-column"
                }),
                kind: RegisteredContributionKindV1::COLUMN,
                required_capabilities: RVec::new(),
                interface_id: StableIdV1::new(
                    explorer_extension_api::EXTENSION_ID_NAMESPACE_V1,
                    if cfg!(feature = "alternate") {
                        203
                    } else {
                        201
                    },
                ),
                expected_sort: ROption::RNone,
                opaque_contract: ROption::RNone,
                renderer_contribution_id: ROption::RNone,
                provider: ROption::RNone,
                visual_column: ROption::RNone,
                size_map_view: ROption::RNone,
                virtual_folder_provider: ROption::RNone,
                batch_column_provider: ROption::RNone,
            }]),
        })
    }
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
    ExtensionRootModuleV1::new::<Registrar>(
        PluginMetadataV1 {
            plugin_id: StableIdV1::new(explorer_extension_api::EXTENSION_ID_NAMESPACE_V1, plugin),
            primary_interface_id: StableIdV1::new(
                explorer_extension_api::EXTENSION_ID_NAMESPACE_V1,
                interface,
            ),
        },
        fingerprint,
    )
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
