#![cfg_attr(
    not(test),
    deny(
        clippy::unwrap_used,
        clippy::expect_used,
        clippy::panic,
        clippy::todo,
        clippy::unimplemented
    )
)]
//! Process-resident extension-host composition seam.
//!
//! This crate validates the data-only v1 ABI contract before dispatching its
//! registrar callback. DLL discovery, root-module loading, manifests, feature
//! contributions, and lifecycle policy remain later tasks.

use explorer_extension_api::{
    ABI_SCHEMA_V1, AbiErrorCodeV1, AbiErrorV1, ExtensionRootModuleV1_Ref, IdNamespaceV1,
    PluginMetadataV1, ROOT_MODULE_CONTRACT_ID_V1, RegistrationOutcomeV1, RegistrationStatusV1,
    SDK_MAJOR_VERSION_V1, StableIdV1, registrar_request_v1,
};

/// Inert process-lifetime owner installed by the application composition root.
///
/// Starting and stopping are idempotent because process shutdown can be requested
/// explicitly and again from a drop path. A stopped host cannot be restarted: native
/// plugin loading is a startup-only lifecycle in the platform design.
#[derive(Debug, Default)]
pub struct ExtensionHost {
    state: LifecycleState,
}

#[derive(Debug, Default, Eq, PartialEq)]
enum LifecycleState {
    #[default]
    New,
    Running,
    Stopped,
}

impl ExtensionHost {
    /// Creates the inert host seam in its unstarted state.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            state: LifecycleState::New,
        }
    }

    /// Starts the host once during application startup.
    pub fn start(&mut self) {
        if self.state == LifecycleState::New {
            self.state = LifecycleState::Running;
        }
    }

    /// Stops the host once during application shutdown.
    pub fn shutdown(&mut self) {
        if self.state == LifecycleState::Running {
            self.state = LifecycleState::Stopped;
        }
    }

    /// Returns whether the process-lifetime host is currently active.
    #[must_use]
    pub fn is_running(&self) -> bool {
        self.state == LifecycleState::Running
    }

    /// Validates a root module's required, non-callback data.
    ///
    /// The dynamic loader added in task 3 validates the `abi_stable` layout before
    /// it creates this reference. This method completes semantic validation before
    /// calling a plugin-provided registrar.
    ///
    /// # Errors
    ///
    /// Returns [`HostRegistrationErrorV1::Incompatible`] when a required schema,
    /// SDK major, root contract, plugin ID, or interface ID is malformed or
    /// unsupported.
    pub fn validate_root(
        &self,
        root: ExtensionRootModuleV1_Ref,
    ) -> Result<PluginMetadataV1, HostRegistrationErrorV1> {
        if root.abi_schema() != ABI_SCHEMA_V1 {
            return Err(HostRegistrationErrorV1::Incompatible(AbiErrorV1::new(
                AbiErrorCodeV1::SCHEMA_MISMATCH,
                ROOT_MODULE_CONTRACT_ID_V1,
                root.abi_schema().into_raw(),
            )));
        }

        validate_required_id(root.root_contract_id(), ROOT_MODULE_CONTRACT_ID_V1)?;

        if root.sdk_major() != SDK_MAJOR_VERSION_V1 {
            return Err(HostRegistrationErrorV1::Incompatible(AbiErrorV1::new(
                AbiErrorCodeV1::SDK_MAJOR_MISMATCH,
                ROOT_MODULE_CONTRACT_ID_V1,
                u32::from(root.sdk_major()),
            )));
        }

        let metadata = root.metadata();
        validate_id_in_extension_namespace(metadata.plugin_id)?;
        validate_id_in_extension_namespace(metadata.primary_interface_id)?;

        Ok(metadata)
    }

    /// Validates the root and invokes its registrar once.
    ///
    /// A plugin callback returns typed ABI errors. `abi_stable` only supports the
    /// non-unwinding C function ABI here, so plugin callbacks must catch their own
    /// Rust panics and return [`AbiErrorCodeV1::CALLBACK_PANICKED`]. The host maps
    /// that explicit terminal to [`HostRegistrationErrorV1::Panicked`].
    ///
    /// # Errors
    ///
    /// Returns validation failures before invoking the registrar, and translates a
    /// plugin's typed error or translated panic terminal after invocation.
    pub fn register_root(
        &self,
        root: ExtensionRootModuleV1_Ref,
    ) -> Result<RegistrationOutcomeV1, HostRegistrationErrorV1> {
        self.validate_root(root)?;

        let registrar = root.registrar();
        // The accessor returns None for an older 1.x prefix. This tail is never
        // required for registration and is intentionally not invoked here.
        let _optional_contract_query = registrar.describe_contract();

        registrar
            .register()
            .invoke(registrar_request_v1())
            .into_result()
            .map_err(|error| {
                if error.code == AbiErrorCodeV1::CALLBACK_PANICKED {
                    HostRegistrationErrorV1::Panicked(error)
                } else {
                    HostRegistrationErrorV1::Plugin(error)
                }
            })
            .and_then(validate_registration_outcome)
    }
}

/// Typed failure from host-side ABI validation or registrar dispatch.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum HostRegistrationErrorV1 {
    /// The root's required ABI data is not compatible with this host.
    Incompatible(AbiErrorV1),
    /// The plugin returned a typed registration error.
    Plugin(AbiErrorV1),
    /// The registrar reported a translated panic terminal.
    Panicked(AbiErrorV1),
}

fn validate_required_id(
    actual: StableIdV1,
    expected: StableIdV1,
) -> Result<(), HostRegistrationErrorV1> {
    if !actual.is_valid() {
        return Err(HostRegistrationErrorV1::Incompatible(AbiErrorV1::new(
            AbiErrorCodeV1::INVALID_ID,
            actual,
            0,
        )));
    }
    if actual != expected {
        return Err(HostRegistrationErrorV1::Incompatible(AbiErrorV1::new(
            AbiErrorCodeV1::UNSUPPORTED_ID,
            actual,
            0,
        )));
    }
    Ok(())
}

fn validate_id_in_extension_namespace(id: StableIdV1) -> Result<(), HostRegistrationErrorV1> {
    if id.is_in_namespace(extension_id_namespace_v1()) {
        return Ok(());
    }

    let code = if id.is_valid() {
        AbiErrorCodeV1::UNSUPPORTED_ID
    } else {
        AbiErrorCodeV1::INVALID_ID
    };
    Err(HostRegistrationErrorV1::Incompatible(AbiErrorV1::new(
        code, id, 0,
    )))
}

fn validate_registration_outcome(
    outcome: RegistrationOutcomeV1,
) -> Result<RegistrationOutcomeV1, HostRegistrationErrorV1> {
    let raw_status = outcome.status.into_raw();
    if raw_status == RegistrationStatusV1::ACCEPTED.into_raw() {
        return Ok(outcome);
    }

    let code = if raw_status == RegistrationStatusV1::REJECTED.into_raw() {
        AbiErrorCodeV1::REGISTRATION_OUTCOME_REJECTED
    } else if raw_status == 0 {
        AbiErrorCodeV1::MALFORMED_REGISTRATION_OUTCOME
    } else {
        AbiErrorCodeV1::UNKNOWN_REGISTRATION_OUTCOME
    };
    Err(HostRegistrationErrorV1::Plugin(AbiErrorV1::new(
        code,
        ROOT_MODULE_CONTRACT_ID_V1,
        raw_status,
    )))
}

const fn extension_id_namespace_v1() -> IdNamespaceV1 {
    explorer_extension_api::EXTENSION_ID_NAMESPACE_V1
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicBool, Ordering};

    use abi_stable::{prefix_type::PrefixTypeTrait, std_types::RResult};
    use explorer_extension_api::{
        ABI_SCHEMA_V1, AbiErrorCodeV1, AbiErrorV1, ExtensionRegistrarV1, ExtensionRootModuleV1,
        PluginMetadataV1, ROOT_MODULE_CONTRACT_ID_V1, RegistrarCallbackV1,
        RegistrarImplementationV1, RegistrarRequestV1, RegistrarResultV1, RegistrationOutcomeV1,
        RegistrationStatusV1, SDK_MAJOR_VERSION_V1, StableIdV1,
    };

    use super::{
        ExtensionHost, ExtensionRootModuleV1_Ref, HostRegistrationErrorV1, LifecycleState,
    };

    const PLUGIN_ID: StableIdV1 = StableIdV1::new(super::extension_id_namespace_v1(), 100);
    const INTERFACE_ID: StableIdV1 = StableIdV1::new(super::extension_id_namespace_v1(), 101);

    fn root<T: RegistrarImplementationV1>(
        abi_schema: explorer_extension_api::AbiSchemaIdV1,
        root_contract_id: StableIdV1,
        sdk_major: u16,
        metadata: PluginMetadataV1,
    ) -> ExtensionRootModuleV1_Ref {
        let registrar = ExtensionRegistrarV1 {
            register: RegistrarCallbackV1::new::<T>(),
            describe_contract,
        }
        .leak_into_prefix();
        ExtensionRootModuleV1 {
            abi_schema,
            root_contract_id,
            sdk_major,
            reserved: 0,
            metadata,
            registrar,
        }
        .leak_into_prefix()
    }

    fn valid_metadata() -> PluginMetadataV1 {
        PluginMetadataV1 {
            plugin_id: PLUGIN_ID,
            primary_interface_id: INTERFACE_ID,
        }
    }

    struct Succeeds;

    impl RegistrarImplementationV1 for Succeeds {
        fn register(_: RegistrarRequestV1) -> RegistrarResultV1 {
            RResult::ROk(RegistrationOutcomeV1::accepted(2))
        }
    }

    extern "C" fn describe_contract() -> StableIdV1 {
        ROOT_MODULE_CONTRACT_ID_V1
    }

    struct ReturnsError;

    impl RegistrarImplementationV1 for ReturnsError {
        fn register(_: RegistrarRequestV1) -> RegistrarResultV1 {
            RResult::RErr(AbiErrorV1::new(
                AbiErrorCodeV1::REGISTRATION_REJECTED,
                INTERFACE_ID,
                7,
            ))
        }
    }

    struct Panics;

    impl RegistrarImplementationV1 for Panics {
        fn register(_: RegistrarRequestV1) -> RegistrarResultV1 {
            panic!("synthetic registrar panic");
        }
    }

    struct RejectedOutcome;

    impl RegistrarImplementationV1 for RejectedOutcome {
        fn register(_: RegistrarRequestV1) -> RegistrarResultV1 {
            RResult::ROk(RegistrationOutcomeV1 {
                status: RegistrationStatusV1::REJECTED,
                registered_interface_count: 0,
            })
        }
    }

    struct MalformedOutcome;

    impl RegistrarImplementationV1 for MalformedOutcome {
        fn register(_: RegistrarRequestV1) -> RegistrarResultV1 {
            RResult::ROk(RegistrationOutcomeV1 {
                status: RegistrationStatusV1::from_raw(0),
                registered_interface_count: 0,
            })
        }
    }

    struct UnknownOutcome;

    impl RegistrarImplementationV1 for UnknownOutcome {
        fn register(_: RegistrarRequestV1) -> RegistrarResultV1 {
            RResult::ROk(RegistrationOutcomeV1 {
                status: RegistrationStatusV1::from_raw(99),
                registered_interface_count: 0,
            })
        }
    }

    static SCHEMA_CALLBACK_CALLED: AtomicBool = AtomicBool::new(false);

    struct MarksSchemaCallback;

    impl RegistrarImplementationV1 for MarksSchemaCallback {
        fn register(_: RegistrarRequestV1) -> RegistrarResultV1 {
            SCHEMA_CALLBACK_CALLED.store(true, Ordering::SeqCst);
            RResult::ROk(RegistrationOutcomeV1::accepted(0))
        }
    }

    static SDK_CALLBACK_CALLED: AtomicBool = AtomicBool::new(false);

    struct MarksSdkCallback;

    impl RegistrarImplementationV1 for MarksSdkCallback {
        fn register(_: RegistrarRequestV1) -> RegistrarResultV1 {
            SDK_CALLBACK_CALLED.store(true, Ordering::SeqCst);
            RResult::ROk(RegistrationOutcomeV1::accepted(0))
        }
    }

    #[test]
    fn host_start_and_shutdown_transition_exactly_once() {
        let mut host = ExtensionHost::new();

        host.start();
        host.start();
        assert_eq!(host.state, LifecycleState::Running);

        host.shutdown();
        host.shutdown();
        host.start();
        assert_eq!(host.state, LifecycleState::Stopped);
        assert!(!host.is_running());
    }

    #[test]
    fn incompatible_schema_is_rejected_before_registrar_callback() {
        SCHEMA_CALLBACK_CALLED.store(false, Ordering::SeqCst);
        let host = ExtensionHost::new();
        let invalid_schema = explorer_extension_api::AbiSchemaIdV1::new(0x5345, 2);
        let result = host.register_root(root::<MarksSchemaCallback>(
            invalid_schema,
            ROOT_MODULE_CONTRACT_ID_V1,
            SDK_MAJOR_VERSION_V1,
            valid_metadata(),
        ));

        assert!(matches!(
            result,
            Err(HostRegistrationErrorV1::Incompatible(AbiErrorV1 {
                code: AbiErrorCodeV1::SCHEMA_MISMATCH,
                ..
            }))
        ));
        assert!(!SCHEMA_CALLBACK_CALLED.load(Ordering::SeqCst));
    }

    #[test]
    fn incompatible_semantic_id_is_rejected_before_registrar_callback() {
        let host = ExtensionHost::new();
        let result = host.register_root(root::<Succeeds>(
            ABI_SCHEMA_V1,
            StableIdV1::new(super::extension_id_namespace_v1(), 99),
            SDK_MAJOR_VERSION_V1,
            valid_metadata(),
        ));

        assert!(matches!(
            result,
            Err(HostRegistrationErrorV1::Incompatible(AbiErrorV1 {
                code: AbiErrorCodeV1::UNSUPPORTED_ID,
                ..
            }))
        ));
    }

    #[test]
    fn incompatible_sdk_major_is_rejected_before_registrar_callback() {
        SDK_CALLBACK_CALLED.store(false, Ordering::SeqCst);
        let host = ExtensionHost::new();
        let result = host.register_root(root::<MarksSdkCallback>(
            ABI_SCHEMA_V1,
            ROOT_MODULE_CONTRACT_ID_V1,
            SDK_MAJOR_VERSION_V1 + 1,
            valid_metadata(),
        ));

        assert!(matches!(
            result,
            Err(HostRegistrationErrorV1::Incompatible(AbiErrorV1 {
                code: AbiErrorCodeV1::SDK_MAJOR_MISMATCH,
                ..
            }))
        ));
        assert!(!SDK_CALLBACK_CALLED.load(Ordering::SeqCst));
    }

    #[test]
    fn registrar_typed_error_and_panic_are_translated_at_boundary() {
        let host = ExtensionHost::new();
        let typed_error = host.register_root(root::<ReturnsError>(
            ABI_SCHEMA_V1,
            ROOT_MODULE_CONTRACT_ID_V1,
            SDK_MAJOR_VERSION_V1,
            valid_metadata(),
        ));
        let panic_error = host.register_root(root::<Panics>(
            ABI_SCHEMA_V1,
            ROOT_MODULE_CONTRACT_ID_V1,
            SDK_MAJOR_VERSION_V1,
            valid_metadata(),
        ));

        assert!(matches!(
            typed_error,
            Err(HostRegistrationErrorV1::Plugin(AbiErrorV1 {
                code: AbiErrorCodeV1::REGISTRATION_REJECTED,
                ..
            }))
        ));
        assert!(matches!(
            panic_error,
            Err(HostRegistrationErrorV1::Panicked(AbiErrorV1 {
                code: AbiErrorCodeV1::CALLBACK_PANICKED,
                ..
            }))
        ));
    }

    #[test]
    fn rejected_malformed_and_unknown_outcomes_are_not_successes() {
        let host = ExtensionHost::new();
        let rejected = host.register_root(root::<RejectedOutcome>(
            ABI_SCHEMA_V1,
            ROOT_MODULE_CONTRACT_ID_V1,
            SDK_MAJOR_VERSION_V1,
            valid_metadata(),
        ));
        let malformed = host.register_root(root::<MalformedOutcome>(
            ABI_SCHEMA_V1,
            ROOT_MODULE_CONTRACT_ID_V1,
            SDK_MAJOR_VERSION_V1,
            valid_metadata(),
        ));
        let unknown = host.register_root(root::<UnknownOutcome>(
            ABI_SCHEMA_V1,
            ROOT_MODULE_CONTRACT_ID_V1,
            SDK_MAJOR_VERSION_V1,
            valid_metadata(),
        ));

        assert!(matches!(
            rejected,
            Err(HostRegistrationErrorV1::Plugin(AbiErrorV1 {
                code: AbiErrorCodeV1::REGISTRATION_OUTCOME_REJECTED,
                detail: 2,
                ..
            }))
        ));
        assert!(matches!(
            malformed,
            Err(HostRegistrationErrorV1::Plugin(AbiErrorV1 {
                code: AbiErrorCodeV1::MALFORMED_REGISTRATION_OUTCOME,
                detail: 0,
                ..
            }))
        ));
        assert!(matches!(
            unknown,
            Err(HostRegistrationErrorV1::Plugin(AbiErrorV1 {
                code: AbiErrorCodeV1::UNKNOWN_REGISTRATION_OUTCOME,
                detail: 99,
                ..
            }))
        ));
    }

    #[test]
    fn valid_root_dispatches_only_after_all_data_validation_passes() {
        let host = ExtensionHost::new();
        let outcome = host.register_root(root::<Succeeds>(
            ABI_SCHEMA_V1,
            ROOT_MODULE_CONTRACT_ID_V1,
            SDK_MAJOR_VERSION_V1,
            valid_metadata(),
        ));

        assert_eq!(outcome, Ok(RegistrationOutcomeV1::accepted(2)));
    }
}
