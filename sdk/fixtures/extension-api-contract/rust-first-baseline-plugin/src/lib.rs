use abi_stable::{
    export_root_module,
    prefix_type::PrefixTypeTrait,
    std_types::{ROption, RResult, RVec},
};
use explorer_extension_api::*;
use std::{env, fs, path::PathBuf};

fn marker() -> Option<PathBuf> {
    env::var_os("EXTENSION_API_CONTRACT_MARKER").map(PathBuf::from)
}
struct BaselineRegistrar;
impl ExtensionRegistrarImplementationV1 for BaselineRegistrar {
    fn create() -> Self {
        Self
    }
    fn register(&self, _: RegistrarRequestV1) -> RegistrarOutputResultV1 {
        if env::var("EXTENSION_API_CONTRACT_MODE").ok().as_deref() == Some("panic") {
            panic!("baseline panic");
        }
        if let Some(path) = marker() {
            fs::write(path, b"rust-first baseline registrar invoked")
                .map_err(|e| {
                    AbiErrorV1::new(
                        AbiErrorCodeV1::CALLBACK_PANICKED,
                        ROOT_MODULE_CONTRACT_ID_V1,
                        e.raw_os_error().unwrap_or_default() as u32,
                    )
                })
                .ok();
        }
        RResult::ROk(RegistrarOutputV1 {
            outcome: RegistrationOutcomeV1::accepted(0),
            contributions: RVec::new(),
        })
    }
}
#[export_root_module]
pub fn get_library() -> ExtensionRootModuleV1_Ref {
    ExtensionRootModuleV1::new::<BaselineRegistrar>(
        PluginMetadataV1 {
            plugin_id: StableIdV1::new(EXTENSION_ID_NAMESPACE_V1, 100),
            primary_interface_id: StableIdV1::new(EXTENSION_ID_NAMESPACE_V1, 101),
        },
        ROption::RNone,
    )
    .leak_into_prefix()
}
