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
#![allow(
    non_camel_case_types,
    reason = "abi_stable convention generates the RootModule reference suffix"
)]
#![allow(
    non_local_definitions,
    reason = "abi_stable 0.11.3 sabi_trait expansion emits nested interface impls"
)]
#![allow(
    unused_qualifications,
    reason = "abi_stable trait-supertype expansion is intentionally explicit"
)]
#![allow(
    clippy::used_underscore_binding,
    reason = "abi_stable generated sabi_trait forwarding names are implementation detail"
)]
#![allow(
    clippy::expl_impl_clone_on_copy,
    reason = "abi_stable generates Clone and Copy for prefix reference types"
)]
#![allow(
    unsafe_code,
    reason = "abi_stable generates checked prefix accessors and trait-object trampolines"
)]
#![allow(
    clippy::cast_ptr_alignment,
    reason = "abi_stable's generated prefix accessor checks field accessibility first"
)]
//! Public, non-UI ABI contract for `SuperExplorer` extensions.
//!
//! # ABI rules
//!
//! A Rust plugin exports exactly one [`ExtensionRootModuleV1`] through
//! [`abi_stable::export_root_module`]. The root carries only fixed-width values and
//! a checked SDK-owned registrar factory; it neither owns nor accepts a GPUI
//! entity, a private `SuperExplorer` type, a native handle, a closure, a future, an
//! ordinary Rust trait object, or a `std` collection. Plugin authors implement
//! ordinary Rust traits, while SDK-owned adapters erase them into `#[sabi_trait]`
//! objects. Cross-DLL owned values use `abi_stable` types such as
//! [`abi_stable::std_types::RResult`].
//!
//! Version 1.x freezes the complete root-module shape because `abi_stable 0.11.3`
//! root reflection rejects a newer host layout with additional fields when loading
//! an older DLL. Evolution therefore uses the fixed descriptor/capability data
//! contract and approved non-exhaustive values; structural ABI changes require a
//! new SDK major. Existing fields and numeric meanings never change during 1.x.
//! New error/outcome codes are represented by transparent numeric newtypes so an
//! older host can preserve and report an unknown value without guessing its meaning.

use std::{
    any::Any,
    mem,
    panic::{AssertUnwindSafe, catch_unwind},
    sync::atomic::{AtomicU8, Ordering},
};

use abi_stable::{
    StableAbi,
    library::RootModule,
    package_version_strings, sabi_trait,
    sabi_types::VersionStrings,
    std_types::{RBox, ROption, RResult, RString, RVec},
};

mod jobs;

pub use jobs::{
    AbiInputStreamServicesV1, AbiJobHostServicesV1, IncrementalResultBatchV1,
    IncrementalResultEntryV1, IncrementalResultSinkV1, InputStreamCapabilityV1,
    InputStreamLengthOutcomeV1, InputStreamReadOutcomeV1, InputStreamReadRequestV1,
    InputStreamSeekOriginV1, InputStreamSeekOutcomeV1, InputStreamSeekRequestV1,
    InputStreamStatusV1, InputStreamV1, ItemHandleV1, JobContextV1, JobControlStateV1, JobHandleV1,
    JobHostServicesV1, JobProgressSinkV1, JobProgressStatusV1, JobProgressUpdateV1,
    JobProviderImplementationV1, JobProviderObjectV1, JobTerminalV1, LocationHandleV1,
    MAX_INCREMENTAL_RESULT_BYTES_V1, MAX_INCREMENTAL_RESULT_ITEMS_V1,
    MAX_INPUT_STREAM_READ_BYTES_V1, MAX_PLUGIN_VALUE_BYTES_V1, PluginItemOutcomeV1,
    PluginItemResultTransportErrorV1, PluginItemResultV1, PluginValueKindV1,
    PluginValueTransportErrorV1, PluginValueV1, SinkCapabilityV1, SinkSubmitOutcomeV1,
    SinkSubmitStatusV1, StableSortValueKindV1, StableSortValueTransportErrorV1, StableSortValueV1,
};

/// The `SE` namespace revision one (`0x5345` is ASCII `SE`).
///
/// The high 16 bits name the authority and the low 16 bits are its semantic
/// revision. The authority and revision are validated independently, which makes
/// an identifier from a future namespace unambiguously incompatible with v1.
pub const EXTENSION_ID_NAMESPACE_V1: IdNamespaceV1 = IdNamespaceV1(0x5345_0001);

/// The ABI layout and semantic-contract schema used by this root module.
pub const ABI_SCHEMA_V1: AbiSchemaIdV1 = AbiSchemaIdV1(0x5345_0001);

/// The required semantic identifier for the single v1 root module.
pub const ROOT_MODULE_CONTRACT_ID_V1: StableIdV1 = StableIdV1 {
    namespace: EXTENSION_ID_NAMESPACE_V1,
    value: 1,
};

/// The only supported SDK major for this root-module contract.
pub const SDK_MAJOR_VERSION_V1: u16 = 1;
/// Required descriptor-batch semantic revision for the fixed SDK V1 root.
pub const DESCRIPTOR_CONTRACT_REVISION_V1: u32 = 1;

/// A stable semantic-ID namespace.
///
/// Its high 16 bits identify the assigning authority; its low 16 bits are that
/// authority's namespace revision. A namespace value of zero is never valid.
#[repr(transparent)]
#[derive(Clone, Copy, Debug, Eq, PartialEq, StableAbi)]
pub struct IdNamespaceV1(u32);

impl IdNamespaceV1 {
    /// Builds a namespace from an assigning authority and namespace revision.
    #[must_use]
    pub const fn new(authority: u16, revision: u16) -> Self {
        Self(((authority as u32) << 16) | (revision as u32))
    }

    /// Returns the assigning authority half of this namespace.
    #[must_use]
    pub const fn authority(self) -> u16 {
        (self.0 >> 16) as u16
    }

    /// Returns the namespace-semantic revision half of this namespace.
    #[must_use]
    pub const fn revision(self) -> u16 {
        let bytes = self.0.to_le_bytes();
        u16::from_le_bytes([bytes[0], bytes[1]])
    }

    /// Returns the wire representation.
    #[must_use]
    pub const fn into_raw(self) -> u32 {
        self.0
    }

    /// Whether this is a well-formed, non-zero namespace.
    #[must_use]
    pub const fn is_valid(self) -> bool {
        self.authority() != 0 && self.revision() != 0
    }
}

/// A stable numeric ID scoped to an [`IdNamespaceV1`].
///
/// `value` is allocated by the namespace authority and zero is reserved. Numeric
/// values are never re-used for a different semantic meaning within a namespace
/// revision.
#[repr(C)]
#[derive(Clone, Copy, Debug, Eq, PartialEq, StableAbi)]
pub struct StableIdV1 {
    /// The assigning authority and semantic namespace revision.
    pub namespace: IdNamespaceV1,
    /// The authority-assigned, non-zero identifier.
    pub value: u64,
}

impl StableIdV1 {
    /// Constructs a stable ID. Callers should use [`Self::is_valid`] before using
    /// untrusted ABI data.
    #[must_use]
    pub const fn new(namespace: IdNamespaceV1, value: u64) -> Self {
        Self { namespace, value }
    }

    /// Validates the namespace encoding and reserved value.
    #[must_use]
    pub const fn is_valid(self) -> bool {
        self.namespace.is_valid() && self.value != 0
    }

    /// Validates this ID against its expected namespace.
    #[must_use]
    pub const fn is_in_namespace(self, namespace: IdNamespaceV1) -> bool {
        self.is_valid() && self.namespace.0 == namespace.0
    }
}

/// A fixed-width ABI schema identifier.
///
/// The representation follows the same authority/revision split as
/// [`IdNamespaceV1`]. It is deliberately distinct from contribution IDs so a
/// schema revision cannot be used where a semantic ID is required.
#[repr(transparent)]
#[derive(Clone, Copy, Debug, Eq, PartialEq, StableAbi)]
pub struct AbiSchemaIdV1(u32);

impl AbiSchemaIdV1 {
    /// Builds a schema identifier from its assigning authority and semantic
    /// revision.
    #[must_use]
    pub const fn new(authority: u16, revision: u16) -> Self {
        Self(((authority as u32) << 16) | (revision as u32))
    }

    /// Returns the schema's assigning authority.
    #[must_use]
    pub const fn authority(self) -> u16 {
        (self.0 >> 16) as u16
    }

    /// Returns the schema's semantic revision.
    #[must_use]
    pub const fn revision(self) -> u16 {
        let bytes = self.0.to_le_bytes();
        u16::from_le_bytes([bytes[0], bytes[1]])
    }

    /// Returns the wire representation.
    #[must_use]
    pub const fn into_raw(self) -> u32 {
        self.0
    }

    /// Whether this is a well-formed schema identifier.
    #[must_use]
    pub const fn is_valid(self) -> bool {
        self.authority() != 0 && self.revision() != 0
    }
}

/// A SHA-256 UI ABI fingerprint reported by a GPUI-capable extension DLL.
///
/// The host binds these fixed-width bytes to both the sealed manifest and its
/// approved SDK artifact before a plugin callback may run.
#[repr(transparent)]
#[derive(Clone, Copy, Debug, Eq, PartialEq, StableAbi)]
pub struct UiAbiFingerprintV1([u8; 32]);

impl UiAbiFingerprintV1 {
    /// Creates a fingerprint from fixed-width SHA-256 bytes.
    #[must_use]
    pub const fn new(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }

    /// Returns the fixed-width SHA-256 bytes.
    #[must_use]
    pub const fn bytes(self) -> [u8; 32] {
        self.0
    }

    /// Parses exactly 64 lowercase hexadecimal SHA-256 characters.
    #[must_use]
    pub fn from_lower_hex(value: &str) -> Option<Self> {
        if value.len() != 64 {
            return None;
        }
        let mut bytes = [0_u8; 32];
        for (index, pair) in value.as_bytes().chunks_exact(2).enumerate() {
            bytes[index] = hex_nibble(pair[0])? << 4 | hex_nibble(pair[1])?;
        }
        Some(Self(bytes))
    }
}

const fn hex_nibble(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        _ => None,
    }
}

/// Typed ABI error code, intentionally non-exhaustive by numeric convention.
///
/// Codes not known to an older host remain valid data and must be surfaced as an
/// unknown code instead of being mapped to a different error meaning.
#[repr(transparent)]
#[derive(Clone, Copy, Debug, Eq, PartialEq, StableAbi)]
pub struct AbiErrorCodeV1(u32);

impl AbiErrorCodeV1 {
    /// The ABI schema does not match the host requirement.
    pub const SCHEMA_MISMATCH: Self = Self(1);
    /// A stable ID has a malformed namespace or reserved value.
    pub const INVALID_ID: Self = Self(2);
    /// A stable ID is valid but has an unsupported semantic meaning.
    pub const UNSUPPORTED_ID: Self = Self(3);
    /// The plugin rejected the host's registrar request.
    pub const REGISTRATION_REJECTED: Self = Self(4);
    /// The callback ended by unwinding instead of returning a typed result.
    pub const CALLBACK_PANICKED: Self = Self(5);
    /// The plugin was built for a different SDK major.
    pub const SDK_MAJOR_MISMATCH: Self = Self(6);
    /// A registrar returned the explicit non-success status.
    pub const REGISTRATION_OUTCOME_REJECTED: Self = Self(7);
    /// A registrar returned the reserved malformed status zero.
    pub const MALFORMED_REGISTRATION_OUTCOME: Self = Self(8);
    /// A registrar returned a future status unknown to this host.
    pub const UNKNOWN_REGISTRATION_OUTCOME: Self = Self(9);

    /// Returns the wire representation.
    #[must_use]
    pub const fn into_raw(self) -> u32 {
        self.0
    }
}

/// A data-only ABI error. `detail` is code-specific and never contains a pointer
/// or host-native handle.
#[repr(C)]
#[derive(Clone, Copy, Debug, Eq, PartialEq, StableAbi)]
pub struct AbiErrorV1 {
    /// The stable category of failure.
    pub code: AbiErrorCodeV1,
    /// The ID associated with the failure, or the root contract ID when none is
    /// more specific.
    pub subject: StableIdV1,
    /// Code-specific fixed-width diagnostic detail.
    pub detail: u32,
}

impl AbiErrorV1 {
    /// Constructs a typed ABI error.
    #[must_use]
    pub const fn new(code: AbiErrorCodeV1, subject: StableIdV1, detail: u32) -> Self {
        Self {
            code,
            subject,
            detail,
        }
    }
}

/// Result returned by an ABI callback.
pub type AbiResultV1<T> = RResult<T, AbiErrorV1>;

/// Disposes a payload caught at an SDK panic boundary without allowing its
/// destructor to begin another unwind through the boundary.
///
/// A plugin controls the concrete `Any` type supplied to `panic_any`; dropping
/// it can itself panic and would otherwise start a second unwind while an ABI
/// trampoline is translating the first one. Drop the original payload within a
/// second containment boundary so ordinary payload resources are reclaimed. If
/// that destructor itself panics, the replacement opaque payload is the one
/// intentionally quarantined; the host's per-plugin fault policy bounds such
/// terminal faults. No quarantined payload carries a host capability or callback
/// that can be invoked after the boundary returns.
fn dispose_caught_panic_payload_v1(payload: Box<dyn Any + Send>) {
    if let Err(hostile_drop_payload) = catch_unwind(AssertUnwindSafe(|| drop(payload))) {
        mem::forget(hostile_drop_payload);
    }
}

/// Immutable plugin metadata checked as root data before the registrar callback.
#[repr(C)]
#[derive(Clone, Copy, Debug, Eq, PartialEq, StableAbi)]
pub struct PluginMetadataV1 {
    /// Stable plugin identity. Manifest validation owns the human-readable package
    /// identity; only this numeric SDK identity crosses the ABI record.
    pub plugin_id: StableIdV1,
    /// Stable primary interface identity for diagnostics and registration.
    pub primary_interface_id: StableIdV1,
}

/// Data the host supplies to a validated registrar invocation.
#[repr(C)]
#[derive(Clone, Copy, Debug, Eq, PartialEq, StableAbi)]
pub struct RegistrarRequestV1 {
    /// The ABI schema that the host already validated on the root.
    pub abi_schema: AbiSchemaIdV1,
    /// The semantic identity that the host already validated on the root.
    pub root_contract_id: StableIdV1,
    /// The SDK major selected by the host.
    pub sdk_major: u16,
    /// Explicit padding keeps the C layout stable without relying on Rust's tail
    /// padding rules.
    pub reserved: u16,
}

/// Non-exhaustive fixed-width registration status.
///
/// Only [`Self::ACCEPTED`] represents a successful registrar outcome. Future
/// values remain data that older hosts can reject without treating as success.
#[repr(transparent)]
#[derive(Clone, Copy, Debug, Eq, PartialEq, StableAbi)]
pub struct RegistrationStatusV1(u32);

impl RegistrationStatusV1 {
    /// The sole successful v1 status.
    pub const ACCEPTED: Self = Self(1);
    /// The plugin explicitly declined registration.
    pub const REJECTED: Self = Self(2);

    /// Constructs a status from its stable wire representation.
    #[must_use]
    pub const fn from_raw(raw: u32) -> Self {
        Self(raw)
    }

    /// Returns the stable wire representation.
    #[must_use]
    pub const fn into_raw(self) -> u32 {
        self.0
    }
}

/// Typed registrar outcome whose status must be validated by the host.
#[repr(C)]
#[derive(Clone, Copy, Debug, Eq, PartialEq, StableAbi)]
pub struct RegistrationOutcomeV1 {
    /// Non-exhaustive registration terminal status.
    pub status: RegistrationStatusV1,
    /// Number of interfaces accepted during this call.
    pub registered_interface_count: u32,
}

/// Fixed contribution category declared by an extension registrar.
#[repr(transparent)]
#[derive(Clone, Copy, Debug, Eq, PartialEq, StableAbi)]
pub struct RegisteredContributionKindV1(u32);

impl RegisteredContributionKindV1 {
    pub const COLUMN: Self = Self(1);
    pub const GPUI_RENDERER: Self = Self(2);
    pub const COMMAND: Self = Self(3);
    pub const FORM: Self = Self(4);
    pub const OPERATION_PLAN: Self = Self(5);
    pub const VIEW_MODE: Self = Self(6);
    pub const RESOURCE: Self = Self(7);

    #[must_use]
    pub const fn into_raw(self) -> u32 {
        self.0
    }
}

/// Sealed opaque schema declaration supplied by a registrar.
#[repr(C)]
#[derive(Clone, Copy, Debug, Eq, PartialEq, StableAbi)]
pub struct OpaquePayloadContractV1 {
    pub schema: StableIdV1,
    pub schema_version: u32,
}

/// Complete ABI descriptor for one contribution and optional job provider.
/// Strings are descriptive only; the host revalidates them against the sealed
/// package manifest before retaining this object.
#[repr(C)]
#[derive(StableAbi)]
pub struct RegisteredContributionV1 {
    pub feature_id: RString,
    pub contribution_id: RString,
    pub kind: RegisteredContributionKindV1,
    pub required_capabilities: RVec<RString>,
    pub interface_id: StableIdV1,
    pub expected_sort: ROption<StableSortValueKindV1>,
    pub opaque_contract: ROption<OpaquePayloadContractV1>,
    pub renderer_contribution_id: ROption<RString>,
    pub provider: ROption<JobProviderObjectV1>,
}

/// Complete stateful registrar result; registration status cannot claim success
/// without the actual descriptors and provider objects.
#[repr(C)]
#[derive(StableAbi)]
pub struct RegistrarOutputV1 {
    pub outcome: RegistrationOutcomeV1,
    pub contributions: RVec<RegisteredContributionV1>,
}

pub type RegistrarOutputResultV1 = AbiResultV1<RegistrarOutputV1>;

/// Private ABI vtable. Plugin authors implement
/// [`ExtensionRegistrarImplementationV1`], never this generated trait or its
/// `_TO` object type.
#[sabi_trait]
#[doc(hidden)]
pub trait AbiRegistrarObjectV1: Send + Sync {
    #[sabi(last_prefix_field)]
    fn register(&self, request: RegistrarRequestV1) -> RegistrarOutputResultV1;
}

/// Opaque resident registrar object. It is created only by the SDK factory and
/// retained by the host; its ABI vtable is deliberately not public.
#[repr(transparent)]
#[derive(StableAbi)]
pub struct RegistrarObjectV1(AbiRegistrarObjectV1_TO<'static, RBox<()>>);

/// SDK-facing construction trait. Plugin authors implement ordinary Rust
/// traits; only the SDK-owned factory trampoline crosses the C ABI. A registrar
/// adapter permanently fault-latches after its first panic and never re-enters
/// the implementation from that object again.
pub trait ExtensionRegistrarImplementationV1: Send + Sync {
    fn create() -> Self
    where
        Self: Sized;
    fn register(&self, request: RegistrarRequestV1) -> RegistrarOutputResultV1;
}

const REGISTRAR_IDLE_V1: u8 = 0;
const REGISTRAR_RUNNING_V1: u8 = 1;
const REGISTRAR_FAULTED_V1: u8 = 2;

struct RegistrarAdapterV1<T> {
    registrar: Option<T>,
    invocation_state: AtomicU8,
}

impl<T: ExtensionRegistrarImplementationV1> AbiRegistrarObjectV1 for RegistrarAdapterV1<T> {
    fn register(&self, request: RegistrarRequestV1) -> RegistrarOutputResultV1 {
        if self
            .invocation_state
            .compare_exchange(
                REGISTRAR_IDLE_V1,
                REGISTRAR_RUNNING_V1,
                Ordering::AcqRel,
                Ordering::Acquire,
            )
            .is_err()
        {
            return RResult::RErr(AbiErrorV1::new(
                AbiErrorCodeV1::CALLBACK_PANICKED,
                ROOT_MODULE_CONTRACT_ID_V1,
                0,
            ));
        }
        let Some(registrar) = self.registrar.as_ref() else {
            self.invocation_state
                .store(REGISTRAR_FAULTED_V1, Ordering::Release);
            return RResult::RErr(AbiErrorV1::new(
                AbiErrorCodeV1::CALLBACK_PANICKED,
                ROOT_MODULE_CONTRACT_ID_V1,
                0,
            ));
        };
        match catch_unwind(AssertUnwindSafe(|| registrar.register(request))) {
            Ok(result) => {
                self.invocation_state
                    .store(REGISTRAR_IDLE_V1, Ordering::Release);
                result
            }
            Err(payload) => {
                self.invocation_state
                    .store(REGISTRAR_FAULTED_V1, Ordering::Release);
                dispose_caught_panic_payload_v1(payload);
                RResult::RErr(AbiErrorV1::new(
                    AbiErrorCodeV1::CALLBACK_PANICKED,
                    ROOT_MODULE_CONTRACT_ID_V1,
                    0,
                ))
            }
        }
    }
}
impl<T> Drop for RegistrarAdapterV1<T> {
    fn drop(&mut self) {
        if let Some(registrar) = self.registrar.take()
            && let Err(payload) = catch_unwind(AssertUnwindSafe(|| drop(registrar)))
        {
            dispose_caught_panic_payload_v1(payload);
        }
    }
}

impl RegistrarObjectV1 {
    #[doc(hidden)]
    pub fn register(&self, request: RegistrarRequestV1) -> RegistrarOutputResultV1 {
        self.0.register(request)
    }
}

pub type RegistrarFactoryResultV1 = AbiResultV1<RegistrarObjectV1>;

#[repr(transparent)]
#[derive(Clone, Copy, StableAbi)]
pub struct RegistrarFactoryV1(extern "C" fn() -> RegistrarFactoryResultV1);

impl RegistrarFactoryV1 {
    #[must_use]
    pub fn new<T: ExtensionRegistrarImplementationV1 + 'static>() -> Self {
        Self(registrar_factory_trampoline::<T>)
    }

    #[must_use]
    pub fn create(self) -> RegistrarFactoryResultV1 {
        (self.0)()
    }
}

#[abi_stable::sabi_extern_fn]
extern "C" fn registrar_factory_trampoline<T: ExtensionRegistrarImplementationV1 + 'static>()
-> RegistrarFactoryResultV1 {
    match catch_unwind(AssertUnwindSafe(|| {
        RegistrarObjectV1(AbiRegistrarObjectV1_TO::from_value(
            RegistrarAdapterV1 {
                registrar: Some(T::create()),
                invocation_state: AtomicU8::new(REGISTRAR_IDLE_V1),
            },
            sabi_trait::TD_Opaque,
        ))
    })) {
        Ok(registrar) => RResult::ROk(registrar),
        Err(payload) => {
            dispose_caught_panic_payload_v1(payload);
            RResult::RErr(AbiErrorV1::new(
                AbiErrorCodeV1::CALLBACK_PANICKED,
                ROOT_MODULE_CONTRACT_ID_V1,
                0,
            ))
        }
    }
}

impl RegistrationOutcomeV1 {
    /// Constructs a successful registration outcome.
    #[must_use]
    pub const fn accepted(registered_interface_count: u32) -> Self {
        Self {
            status: RegistrationStatusV1::ACCEPTED,
            registered_interface_count,
        }
    }

    /// Whether the plugin explicitly accepted the request.
    #[must_use]
    pub const fn is_accepted(self) -> bool {
        self.status.into_raw() == RegistrationStatusV1::ACCEPTED.into_raw()
    }
}

/// The single `abi_stable` root module exported by a Rust extension DLL.
///
/// A plugin exposes this through one `#[abi_stable::export_root_module]` function.
/// The loader validates the `RootModule` layout; the host then validates these data
/// fields before it constructs and invokes [`RegistrarObjectV1`].
#[repr(C)]
#[derive(StableAbi)]
#[sabi(kind(Prefix(prefix_ref = ExtensionRootModuleV1_Ref)))]
#[sabi(missing_field(panic))]
pub struct ExtensionRootModuleV1 {
    /// Required ABI schema identity.
    pub abi_schema: AbiSchemaIdV1,
    /// Required root semantic identity.
    pub root_contract_id: StableIdV1,
    /// Required SDK major compatibility declaration.
    pub sdk_major: u16,
    /// Explicit C-layout padding for [`Self::sdk_major`].
    pub reserved: u16,
    /// Required data-only plugin identity metadata.
    pub metadata: PluginMetadataV1,
    /// Optional binary UI ABI fingerprint checked before any registrar object is
    /// constructed or invoked.
    pub ui_abi_fingerprint_sha256: ROption<UiAbiFingerprintV1>,
    /// SDK-owned factory for the plugin's ordinary Rust registrar implementation.
    pub create_registrar: RegistrarFactoryV1,
    /// Required revision for the descriptor batch contract.
    #[sabi(last_prefix_field)]
    pub descriptor_contract_revision: u32,
}

impl ExtensionRootModuleV1 {
    /// Builds a complete root module from an ordinary Rust registrar implementation.
    #[must_use]
    pub fn new<T>(
        metadata: PluginMetadataV1,
        ui_abi_fingerprint_sha256: ROption<UiAbiFingerprintV1>,
    ) -> Self
    where
        T: ExtensionRegistrarImplementationV1 + 'static,
    {
        Self {
            abi_schema: ABI_SCHEMA_V1,
            root_contract_id: ROOT_MODULE_CONTRACT_ID_V1,
            sdk_major: SDK_MAJOR_VERSION_V1,
            reserved: 0,
            metadata,
            ui_abi_fingerprint_sha256,
            create_registrar: RegistrarFactoryV1::new::<T>(),
            descriptor_contract_revision: DESCRIPTOR_CONTRACT_REVISION_V1,
        }
    }
}

impl RootModule for ExtensionRootModuleV1_Ref {
    abi_stable::declare_root_module_statics! {ExtensionRootModuleV1_Ref}

    const BASE_NAME: &'static str = "superexplorer_extension_v1";
    const NAME: &'static str = "superexplorer_extension_v1";
    const VERSION_STRINGS: VersionStrings = package_version_strings!();
}

/// Builds the request that a v1 host passes after its root-data validation.
#[must_use]
pub const fn registrar_request_v1() -> RegistrarRequestV1 {
    RegistrarRequestV1 {
        abi_schema: ABI_SCHEMA_V1,
        root_contract_id: ROOT_MODULE_CONTRACT_ID_V1,
        sdk_major: SDK_MAJOR_VERSION_V1,
        reserved: 0,
    }
}

#[cfg(test)]
mod tests {
    use std::{
        panic::panic_any,
        process::Command,
        sync::atomic::{AtomicUsize, Ordering},
    };

    use abi_stable::{library::RootModule, prefix_type::PrefixTypeTrait};

    use super::*;

    #[test]
    fn namespace_encodes_authority_and_semantic_revision() {
        let namespace = IdNamespaceV1::new(0x1234, 9);

        assert_eq!(namespace.authority(), 0x1234);
        assert_eq!(namespace.revision(), 9);
        assert_eq!(namespace.into_raw(), 0x1234_0009);
        assert!(namespace.is_valid());
        assert!(!IdNamespaceV1::new(0, 1).is_valid());
        assert!(!IdNamespaceV1::new(1, 0).is_valid());
    }

    #[test]
    fn stable_ids_reject_reserved_value_and_wrong_namespace() {
        let valid = StableIdV1::new(EXTENSION_ID_NAMESPACE_V1, 42);
        let reserved = StableIdV1::new(EXTENSION_ID_NAMESPACE_V1, 0);
        let foreign = StableIdV1::new(IdNamespaceV1::new(0x1234, 1), 42);

        assert!(valid.is_in_namespace(EXTENSION_ID_NAMESPACE_V1));
        assert!(!reserved.is_valid());
        assert!(!foreign.is_in_namespace(EXTENSION_ID_NAMESPACE_V1));
    }

    #[test]
    fn ui_abi_fingerprint_requires_canonical_lower_hex_sha256() {
        let canonical = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";
        assert_eq!(
            UiAbiFingerprintV1::from_lower_hex(canonical),
            Some(UiAbiFingerprintV1::new([
                0x01, 0x23, 0x45, 0x67, 0x89, 0xab, 0xcd, 0xef, 0x01, 0x23, 0x45, 0x67, 0x89, 0xab,
                0xcd, 0xef, 0x01, 0x23, 0x45, 0x67, 0x89, 0xab, 0xcd, 0xef, 0x01, 0x23, 0x45, 0x67,
                0x89, 0xab, 0xcd, 0xef,
            ]))
        );
        assert_eq!(
            UiAbiFingerprintV1::from_lower_hex(&canonical.to_ascii_uppercase()),
            None
        );
        assert_eq!(UiAbiFingerprintV1::from_lower_hex(&canonical[..63]), None);
        assert_eq!(
            UiAbiFingerprintV1::from_lower_hex(&format!("{canonical}0")),
            None
        );
        assert_eq!(
            UiAbiFingerprintV1::from_lower_hex(&format!("g{}", &canonical[1..])),
            None
        );
    }

    #[test]
    fn root_contract_and_request_are_v1_values() {
        let request = registrar_request_v1();

        assert!(ABI_SCHEMA_V1.is_valid());
        assert_eq!(request.abi_schema, ABI_SCHEMA_V1);
        assert_eq!(request.root_contract_id, ROOT_MODULE_CONTRACT_ID_V1);
        assert_eq!(request.sdk_major, SDK_MAJOR_VERSION_V1);
    }

    #[test]
    fn root_module_version_matches_the_first_fixed_v1_baseline() {
        assert_eq!(
            <ExtensionRootModuleV1_Ref as RootModule>::VERSION_STRINGS
                .version
                .as_str(),
            "1.2.0"
        );
    }

    #[test]
    fn root_prefix_owns_the_checked_registrar_factory() {
        struct Accepts;

        impl ExtensionRegistrarImplementationV1 for Accepts {
            fn create() -> Self {
                Self
            }
            fn register(&self, _: RegistrarRequestV1) -> RegistrarOutputResultV1 {
                RResult::ROk(RegistrarOutputV1 {
                    outcome: RegistrationOutcomeV1::accepted(0),
                    contributions: RVec::new(),
                })
            }
        }

        let root = ExtensionRootModuleV1::new::<Accepts>(
            PluginMetadataV1 {
                plugin_id: StableIdV1::new(EXTENSION_ID_NAMESPACE_V1, 10),
                primary_interface_id: StableIdV1::new(EXTENSION_ID_NAMESPACE_V1, 11),
            },
            ROption::RNone,
        )
        .leak_into_prefix();

        assert_eq!(root.abi_schema(), ABI_SCHEMA_V1);
        assert_eq!(root.root_contract_id(), ROOT_MODULE_CONTRACT_ID_V1);
        assert_eq!(root.sdk_major(), SDK_MAJOR_VERSION_V1);
        assert_eq!(root.ui_abi_fingerprint_sha256(), ROption::RNone);
        assert_eq!(
            root.descriptor_contract_revision(),
            DESCRIPTOR_CONTRACT_REVISION_V1
        );
        let registrar = root.create_registrar().create().into_result().unwrap();
        assert_eq!(
            registrar
                .register(registrar_request_v1())
                .into_result()
                .unwrap()
                .outcome,
            RegistrationOutcomeV1::accepted(0)
        );
    }

    #[test]
    fn registrar_panic_is_a_typed_terminal_before_crossing_c_abi() {
        struct Panics;

        impl ExtensionRegistrarImplementationV1 for Panics {
            fn create() -> Self {
                Self
            }
            fn register(&self, _: RegistrarRequestV1) -> RegistrarOutputResultV1 {
                panic!("synthetic registrar panic");
            }
        }

        let registrar = RegistrarFactoryV1::new::<Panics>()
            .create()
            .into_result()
            .unwrap();
        let result = registrar.register(registrar_request_v1());

        assert!(matches!(
            result.into_result(),
            Err(AbiErrorV1 {
                code: AbiErrorCodeV1::CALLBACK_PANICKED,
                subject: ROOT_MODULE_CONTRACT_ID_V1,
                detail: 0,
            })
        ));
    }

    static HOSTILE_PAYLOAD_DROPS: AtomicUsize = AtomicUsize::new(0);
    static NORMAL_IMPLEMENTATION_DROPS: AtomicUsize = AtomicUsize::new(0);
    static PANICKING_IMPLEMENTATION_DROPS: AtomicUsize = AtomicUsize::new(0);
    static REGISTRAR_CALLBACKS: AtomicUsize = AtomicUsize::new(0);
    static PROVIDER_CALLBACKS: AtomicUsize = AtomicUsize::new(0);

    struct HostilePanicPayloadV1;
    impl Drop for HostilePanicPayloadV1 {
        fn drop(&mut self) {
            HOSTILE_PAYLOAD_DROPS.fetch_add(1, Ordering::SeqCst);
            panic!("hostile panic payload drop");
        }
    }

    struct FactoryPanicsV1;
    impl ExtensionRegistrarImplementationV1 for FactoryPanicsV1 {
        fn create() -> Self {
            panic_any(HostilePanicPayloadV1);
        }

        fn register(&self, _: RegistrarRequestV1) -> RegistrarOutputResultV1 {
            RResult::RErr(AbiErrorV1::new(
                AbiErrorCodeV1::CALLBACK_PANICKED,
                ROOT_MODULE_CONTRACT_ID_V1,
                0,
            ))
        }
    }

    struct RegistrarPanicsV1;
    impl ExtensionRegistrarImplementationV1 for RegistrarPanicsV1 {
        fn create() -> Self {
            Self
        }

        fn register(&self, _: RegistrarRequestV1) -> RegistrarOutputResultV1 {
            REGISTRAR_CALLBACKS.fetch_add(1, Ordering::SeqCst);
            panic_any(HostilePanicPayloadV1);
        }
    }

    struct DropPanicsV1;
    impl ExtensionRegistrarImplementationV1 for DropPanicsV1 {
        fn create() -> Self {
            Self
        }

        fn register(&self, _: RegistrarRequestV1) -> RegistrarOutputResultV1 {
            RResult::RErr(AbiErrorV1::new(
                AbiErrorCodeV1::CALLBACK_PANICKED,
                ROOT_MODULE_CONTRACT_ID_V1,
                0,
            ))
        }
    }
    impl Drop for DropPanicsV1 {
        fn drop(&mut self) {
            PANICKING_IMPLEMENTATION_DROPS.fetch_add(1, Ordering::SeqCst);
            panic!("extension implementation drop");
        }
    }

    struct NormalDropV1;
    impl ExtensionRegistrarImplementationV1 for NormalDropV1 {
        fn create() -> Self {
            Self
        }

        fn register(&self, _: RegistrarRequestV1) -> RegistrarOutputResultV1 {
            RResult::RErr(AbiErrorV1::new(
                AbiErrorCodeV1::CALLBACK_PANICKED,
                ROOT_MODULE_CONTRACT_ID_V1,
                0,
            ))
        }
    }
    impl Drop for NormalDropV1 {
        fn drop(&mut self) {
            NORMAL_IMPLEMENTATION_DROPS.fetch_add(1, Ordering::SeqCst);
        }
    }

    struct ProviderPanicsV1;
    impl JobProviderImplementationV1 for ProviderPanicsV1 {
        fn run(&self, _: JobContextV1) -> JobTerminalV1 {
            PROVIDER_CALLBACKS.fetch_add(1, Ordering::SeqCst);
            panic_any(HostilePanicPayloadV1);
        }
    }

    #[derive(Clone)]
    struct ClosedHostServicesV1;

    impl AbiJobHostServicesV1 for ClosedHostServicesV1 {
        fn poll_control(&self) -> JobControlStateV1 {
            JobControlStateV1::ACTIVE
        }

        fn submit_results(&self, batch: IncrementalResultBatchV1) -> SinkSubmitOutcomeV1 {
            SinkSubmitOutcomeV1 {
                status: SinkSubmitStatusV1::CLOSED,
                remaining_batch_credits: 0,
                remaining_item_credits: 0,
                remaining_byte_credits: 0,
                rejected_batch: ROption::RSome(batch),
            }
        }

        fn submit_progress(&self, _: JobProgressUpdateV1) -> JobProgressStatusV1 {
            JobProgressStatusV1::CLOSED
        }
    }

    fn hostile_panic_context_v1() -> JobContextV1 {
        let job = JobHandleV1::from_host([1; 16], 1);
        let capability = SinkCapabilityV1::from_host([2; 16]);
        let services = JobHostServicesV1::from_host(ClosedHostServicesV1);
        JobContextV1 {
            job,
            item: ROption::RNone,
            location: LocationHandleV1::from_host([3; 16], 1),
            feature_epoch: 1,
            job_generation: 1,
            item_generation: 0,
            location_generation: 1,
            source_generation: 1,
            input: ROption::RNone,
            sink: services.result_sink(job, capability),
            progress: services.progress_sink(job, capability),
        }
    }

    fn run_hostile_panic_child_v1() {
        HOSTILE_PAYLOAD_DROPS.store(0, Ordering::SeqCst);
        NORMAL_IMPLEMENTATION_DROPS.store(0, Ordering::SeqCst);
        PANICKING_IMPLEMENTATION_DROPS.store(0, Ordering::SeqCst);
        REGISTRAR_CALLBACKS.store(0, Ordering::SeqCst);
        PROVIDER_CALLBACKS.store(0, Ordering::SeqCst);

        assert!(matches!(
            RegistrarFactoryV1::new::<FactoryPanicsV1>()
                .create()
                .into_result(),
            Err(AbiErrorV1 {
                code: AbiErrorCodeV1::CALLBACK_PANICKED,
                ..
            })
        ));

        let registrar = RegistrarFactoryV1::new::<RegistrarPanicsV1>()
            .create()
            .into_result()
            .unwrap();
        assert!(matches!(
            registrar.register(registrar_request_v1()).into_result(),
            Err(AbiErrorV1 {
                code: AbiErrorCodeV1::CALLBACK_PANICKED,
                ..
            })
        ));
        assert!(matches!(
            registrar.register(registrar_request_v1()).into_result(),
            Err(AbiErrorV1 {
                code: AbiErrorCodeV1::CALLBACK_PANICKED,
                ..
            })
        ));
        assert_eq!(REGISTRAR_CALLBACKS.load(Ordering::SeqCst), 1);
        drop(registrar);

        let provider = JobProviderObjectV1::new(ProviderPanicsV1);
        assert_eq!(
            provider.invoke(hostile_panic_context_v1()),
            JobTerminalV1::PANICKED
        );
        assert_eq!(
            provider.invoke(hostile_panic_context_v1()),
            JobTerminalV1::PANICKED
        );
        assert_eq!(PROVIDER_CALLBACKS.load(Ordering::SeqCst), 1);
        drop(provider);

        let drop_only = RegistrarFactoryV1::new::<DropPanicsV1>()
            .create()
            .into_result()
            .unwrap();
        drop(drop_only);

        let normal_drop = RegistrarFactoryV1::new::<NormalDropV1>()
            .create()
            .into_result()
            .unwrap();
        drop(normal_drop);

        assert_eq!(HOSTILE_PAYLOAD_DROPS.load(Ordering::SeqCst), 3);
        assert_eq!(PANICKING_IMPLEMENTATION_DROPS.load(Ordering::SeqCst), 1);
        assert_eq!(NORMAL_IMPLEMENTATION_DROPS.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn hostile_panic_payloads_are_quarantined_in_a_subprocess() {
        const CHILD_ENV: &str = "SUPEREXPLORER_API_HOSTILE_PANIC_CHILD";
        if std::env::var_os(CHILD_ENV).is_some() {
            run_hostile_panic_child_v1();
            return;
        }

        let output = Command::new(std::env::current_exe().unwrap())
            .arg("--exact")
            .arg("tests::hostile_panic_payloads_are_quarantined_in_a_subprocess")
            .arg("--nocapture")
            .env(CHILD_ENV, "1")
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "hostile panic child failed:\nstdout:\n{}\nstderr:\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
    }

    #[test]
    fn registration_status_numeric_constants_are_frozen() {
        assert_eq!(RegistrationStatusV1::ACCEPTED.into_raw(), 1);
        assert_eq!(RegistrationStatusV1::REJECTED.into_raw(), 2);
        assert_eq!(AbiErrorCodeV1::REGISTRATION_OUTCOME_REJECTED.into_raw(), 7);
        assert_eq!(AbiErrorCodeV1::MALFORMED_REGISTRATION_OUTCOME.into_raw(), 8);
        assert_eq!(AbiErrorCodeV1::UNKNOWN_REGISTRATION_OUTCOME.into_raw(), 9);
    }
}
