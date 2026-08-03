//! Standalone P0 consumer using the public Rust-first extension author API.
//!
//! The fixture intentionally has no GPUI contribution. It proves that a clean,
//! offline consumer can export the SDK root and implement an ordinary Rust
//! registrar trait without declaring its own FFI callbacks or root layout.

use std::{env, fs, path::PathBuf};

use abi_stable::{
    export_root_module,
    prefix_type::PrefixTypeTrait,
    std_types::{ROption, RResult, RString, RVec},
};
use explorer_extension_api::{
    ABI_SCHEMA_V1, AbiErrorCodeV1, AbiErrorV1, EXTENSION_ID_NAMESPACE_V1,
    ExtensionRegistrarImplementationV1, ExtensionRootModuleV1, ExtensionRootModuleV1_Ref,
    PluginMetadataV1, ROOT_MODULE_CONTRACT_ID_V1, RegisteredContributionKindV1,
    RegisteredContributionV1, RegistrarOutputResultV1,
    RegistrarOutputV1, RegistrarRequestV1, RegistrationOutcomeV1, SDK_MAJOR_VERSION_V1, StableIdV1,
};

const MARKER_ENVIRONMENT_VARIABLE: &str = "P0_CONSUMER_REGISTRAR_MARKER";
const PLUGIN_ID: StableIdV1 = StableIdV1::new(EXTENSION_ID_NAMESPACE_V1, 1_001);
const PRIMARY_INTERFACE_ID: StableIdV1 = StableIdV1::new(EXTENSION_ID_NAMESPACE_V1, 1_002);

struct P0ConsumerRegistrar;

impl ExtensionRegistrarImplementationV1 for P0ConsumerRegistrar {
    fn create() -> Self {
        Self
    }

    fn register(&self, request: RegistrarRequestV1) -> RegistrarOutputResultV1 {
        if request.abi_schema != ABI_SCHEMA_V1 {
            return RResult::RErr(AbiErrorV1::new(
                AbiErrorCodeV1::SCHEMA_MISMATCH,
                ROOT_MODULE_CONTRACT_ID_V1,
                request.abi_schema.into_raw(),
            ));
        }
        if request.root_contract_id != ROOT_MODULE_CONTRACT_ID_V1 {
            return RResult::RErr(AbiErrorV1::new(
                AbiErrorCodeV1::UNSUPPORTED_ID,
                request.root_contract_id,
                0,
            ));
        }
        if request.sdk_major != SDK_MAJOR_VERSION_V1 {
            return RResult::RErr(AbiErrorV1::new(
                AbiErrorCodeV1::SDK_MAJOR_MISMATCH,
                ROOT_MODULE_CONTRACT_ID_V1,
                u32::from(request.sdk_major),
            ));
        }

        // Exercise the exact direct registry dependency patched to the private
        // vendor tree. This proves the source snapshot keeps its private,
        // provenance-bound dependency available to Cargo offline.
        let _ = exif_lite::parser_name();
        if let Err(error) = mark_callback_invocation() {
            return RResult::RErr(AbiErrorV1::new(
                AbiErrorCodeV1::REGISTRATION_REJECTED,
                ROOT_MODULE_CONTRACT_ID_V1,
                error.len() as u32,
            ));
        }

        RResult::ROk(RegistrarOutputV1 {
            outcome: RegistrationOutcomeV1::accepted(1),
            // NativeExtensionLifecycleV1 admits only non-empty output whose
            // accepted count matches the contribution batch exactly. This is
            // deliberately bound to the fixture manifest's `main` feature
            // and its declared `abi` capability.
            contributions: RVec::from(vec![RegisteredContributionV1 {
                feature_id: RString::from("main"),
                contribution_id: RString::from("abi-root"),
                kind: RegisteredContributionKindV1::COLUMN,
                required_capabilities: RVec::from(vec![RString::from("abi")]),
                interface_id: PRIMARY_INTERFACE_ID,
                expected_sort: ROption::RNone,
                opaque_contract: ROption::RNone,
                renderer_contribution_id: ROption::RNone,
                provider: ROption::RNone,
            }]),
        })
    }
}

fn marker_path() -> Option<PathBuf> {
    env::var_os(MARKER_ENVIRONMENT_VARIABLE).map(PathBuf::from)
}

fn mark_callback_invocation() -> Result<(), RString> {
    let Some(path) = marker_path() else {
        return Ok(());
    };
    fs::write(&path, b"p0 consumer registrar invoked").map_err(|error| {
        RString::from(format!(
            "could not write P0 consumer registrar marker {}: {error}",
            path.display()
        ))
    })
}

/// The sole ABI root module. `abi_stable` exports its fixed loader symbol;
/// semantic identity is data in [`ExtensionRootModuleV1`], never an
/// author-configurable manifest string.
#[export_root_module]
pub fn plugin_root() -> ExtensionRootModuleV1_Ref {
    ExtensionRootModuleV1::new::<P0ConsumerRegistrar>(
        PluginMetadataV1 {
            plugin_id: PLUGIN_ID,
            primary_interface_id: PRIMARY_INTERFACE_ID,
        },
        ROption::RNone,
    )
    .leak_into_prefix()
}

#[cfg(test)]
mod tests {
    use explorer_extension_api::{AbiSchemaIdV1, IdNamespaceV1, registrar_request_v1};

    use super::*;

    #[test]
    fn root_is_the_fixed_public_v1_contract() {
        let root = plugin_root();

        assert_eq!(root.abi_schema(), ABI_SCHEMA_V1);
        assert_eq!(root.root_contract_id(), ROOT_MODULE_CONTRACT_ID_V1);
        assert_eq!(root.sdk_major(), SDK_MAJOR_VERSION_V1);
        assert_eq!(root.metadata().plugin_id, PLUGIN_ID);
        assert_eq!(root.ui_abi_fingerprint_sha256(), ROption::RNone);
    }

    #[test]
    fn mismatched_root_contract_is_rejected_before_marker_write() {
        let root = plugin_root();
        let registrar = root.create_registrar().create().into_result().unwrap();
        let request = RegistrarRequestV1 {
            root_contract_id: StableIdV1::new(IdNamespaceV1::new(0x1234, 1), 1),
            ..registrar_request_v1()
        };

        assert!(matches!(
            registrar.register(request).into_result(),
            Err(AbiErrorV1 {
                code: AbiErrorCodeV1::UNSUPPORTED_ID,
                ..
            })
        ));
    }

    #[test]
    fn matching_public_contract_calls_the_registrar() {
        let root = plugin_root();
        let registrar = root.create_registrar().create().into_result().unwrap();
        let result = registrar
            .register(registrar_request_v1())
            .into_result()
            .unwrap();

        assert_eq!(result.outcome, RegistrationOutcomeV1::accepted(1));
        assert_eq!(result.contributions.len(), 1);
        let contribution = &result.contributions[0];
        assert_eq!(contribution.feature_id, "main");
        assert_eq!(contribution.contribution_id, "abi-root");
        assert_eq!(contribution.kind, RegisteredContributionKindV1::COLUMN);
        assert_eq!(contribution.required_capabilities.as_slice(), ["abi"]);
    }

    #[test]
    fn schema_mismatch_is_typed() {
        let root = plugin_root();
        let registrar = root.create_registrar().create().into_result().unwrap();
        let request = RegistrarRequestV1 {
            abi_schema: AbiSchemaIdV1::new(0x5345, 2),
            ..registrar_request_v1()
        };

        assert!(matches!(
            registrar.register(request).into_result(),
            Err(AbiErrorV1 {
                code: AbiErrorCodeV1::SCHEMA_MISMATCH,
                ..
            })
        ));
    }
}
