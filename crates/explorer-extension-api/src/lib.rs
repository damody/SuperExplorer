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
    clippy::expl_impl_clone_on_copy,
    reason = "abi_stable generates Clone and Copy for prefix reference types"
)]
#![allow(
    unsafe_code,
    reason = "abi_stable generates guarded optional-prefix accessors for SDK 1.x tails"
)]
#![allow(
    clippy::cast_ptr_alignment,
    reason = "abi_stable's generated optional-prefix accessor checks field accessibility first"
)]
//! Public, non-UI ABI contract for `SuperExplorer` extensions.
//!
//! # ABI rules
//!
//! A Rust plugin exports exactly one [`ExtensionRootModuleV1`] through
//! [`abi_stable::export_root_module`]. The root carries only fixed-width values and
//! an [`ExtensionRegistrarV1`] prefix type; it neither owns nor accepts a GPUI
//! entity, a private `SuperExplorer` type, a native handle, a closure, a future, an
//! ordinary Rust trait object, or a `std` collection. Cross-DLL owned values use
//! `abi_stable` types such as [`abi_stable::std_types::RResult`].
//!
//! Version 1.x is append-only. New root or registrar functions may be appended
//! after the `last_prefix_field`; hosts must treat those tail accessors as optional.
//! Existing fields and the meanings of their numeric IDs never change during 1.x.
//! New error/outcome codes are represented by transparent numeric newtypes so an
//! older host can preserve and report an unknown value without guessing its meaning.

use std::panic::{AssertUnwindSafe, catch_unwind};

use abi_stable::{
    StableAbi, library::RootModule, package_version_strings, sabi_types::VersionStrings,
    std_types::RResult,
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

/// Immutable plugin metadata checked as root data before the registrar callback.
#[repr(C)]
#[derive(Clone, Copy, Debug, Eq, PartialEq, StableAbi)]
pub struct PluginMetadataV1 {
    /// Stable package identity. Manifest validation supplies its human-readable
    /// representation in a later task; only this numeric identity crosses the ABI.
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

/// Result returned by the required v1 registrar function.
pub type RegistrarResultV1 = AbiResultV1<RegistrationOutcomeV1>;

/// Data-only registrar implementation selected at compile time by an extension.
///
/// This trait never crosses the ABI boundary and must not be used as a trait
/// object. The SDK turns its static [`Self::register`] method into the ABI callback
/// through [`RegistrarCallbackV1::new`].
pub trait RegistrarImplementationV1 {
    /// Produces the typed terminal registrar result.
    fn register(request: RegistrarRequestV1) -> RegistrarResultV1;
}

/// SDK-owned, panic-containing registrar callback.
///
/// The wrapped ABI function pointer is private. Safe extension code can construct
/// this type only through [`Self::new`], which routes the implementation through
/// the SDK trampoline and [`translate_registrar_panic`]. Deliberately fabricating
/// this transparent representation through unsafe code is outside this safe API.
#[repr(transparent)]
#[derive(Clone, Copy, StableAbi)]
pub struct RegistrarCallbackV1(extern "C" fn(RegistrarRequestV1) -> RegistrarResultV1);

impl RegistrarCallbackV1 {
    /// Creates a panic-containing ABI callback for `T`.
    #[must_use]
    pub fn new<T: RegistrarImplementationV1>() -> Self {
        Self(registrar_trampoline::<T>)
    }

    /// Invokes the SDK-owned callback.
    #[must_use]
    pub fn invoke(self, request: RegistrarRequestV1) -> RegistrarResultV1 {
        (self.0)(request)
    }
}

extern "C" fn registrar_trampoline<T: RegistrarImplementationV1>(
    request: RegistrarRequestV1,
) -> RegistrarResultV1 {
    translate_registrar_panic(|| T::register(request))
}

/// Converts a registrar implementation panic into the typed ABI terminal.
///
/// This is called by the SDK-owned [`RegistrarCallbackV1`] trampoline. The closure
/// is an in-process implementation detail and never crosses the ABI; this helper
/// exists because allowing a Rust unwind to leave an `extern "C"` callback would
/// abort rather than produce a recoverable plugin error. Safe extension code cannot
/// construct a raw registrar function pointer; intentionally fabricating the
/// transparent callback representation through unsafe code is outside this API's
/// safety guarantee.
pub fn translate_registrar_panic(
    callback: impl FnOnce() -> RegistrarResultV1,
) -> RegistrarResultV1 {
    match catch_unwind(AssertUnwindSafe(callback)) {
        Ok(result) => result,
        Err(_) => RResult::RErr(AbiErrorV1::new(
            AbiErrorCodeV1::CALLBACK_PANICKED,
            ROOT_MODULE_CONTRACT_ID_V1,
            0,
        )),
    }
}

/// The prefix-type registrar for the v1 root module.
///
/// During SDK 1.x, new functions are appended after `register` and are accessed
/// by hosts as `Option<extern "C" fn(...) -> ...>`. Existing plugins omit
/// those fields safely; they must never be re-ordered or given new meanings.
#[repr(C)]
#[derive(StableAbi)]
#[sabi(kind(Prefix(prefix_ref = ExtensionRegistrarV1_Ref)))]
#[sabi(missing_field(panic))]
pub struct ExtensionRegistrarV1 {
    /// Registers interfaces after the host validates every required root datum.
    /// Safe plugins construct this value with [`RegistrarCallbackV1::new`].
    #[sabi(last_prefix_field)]
    pub register: RegistrarCallbackV1,
    /// Optionally reports the root contract understood by this registrar.
    ///
    /// This first tail field models the SDK 1.x append-only rule. A host sees
    /// `None` when it receives an older registrar prefix and must continue using
    /// the required [`Self::register`] function without treating its absence as an
    /// incompatibility.
    #[sabi(missing_field(option))]
    pub describe_contract: extern "C" fn() -> StableIdV1,
}

/// The single `abi_stable` root module exported by a Rust extension DLL.
///
/// A plugin exposes this through one `#[abi_stable::export_root_module]` function.
/// The loader validates the `RootModule` layout; the host then validates these data
/// fields before it invokes [`ExtensionRegistrarV1::register`].
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
    /// Required prefix registrar. SDK 1.x only appends optional root fields after
    /// this field; it never changes the preceding layout or semantics.
    #[sabi(last_prefix_field)]
    pub registrar: ExtensionRegistrarV1_Ref,
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
    fn root_contract_and_request_are_v1_values() {
        let request = registrar_request_v1();

        assert!(ABI_SCHEMA_V1.is_valid());
        assert_eq!(request.abi_schema, ABI_SCHEMA_V1);
        assert_eq!(request.root_contract_id, ROOT_MODULE_CONTRACT_ID_V1);
        assert_eq!(request.sdk_major, SDK_MAJOR_VERSION_V1);
    }

    #[test]
    fn root_module_version_matches_the_v1_optional_tail_release() {
        assert_eq!(
            <ExtensionRootModuleV1_Ref as RootModule>::VERSION_STRINGS
                .version
                .as_str(),
            "1.1.0"
        );
    }

    #[test]
    fn root_and_registrar_are_prefix_types() {
        struct Accepts;

        impl RegistrarImplementationV1 for Accepts {
            fn register(_: RegistrarRequestV1) -> RegistrarResultV1 {
                RResult::ROk(RegistrationOutcomeV1::accepted(0))
            }
        }

        extern "C" fn describe_contract() -> StableIdV1 {
            ROOT_MODULE_CONTRACT_ID_V1
        }

        let registrar = ExtensionRegistrarV1 {
            register: RegistrarCallbackV1::new::<Accepts>(),
            describe_contract,
        }
        .leak_into_prefix();
        let root = ExtensionRootModuleV1 {
            abi_schema: ABI_SCHEMA_V1,
            root_contract_id: ROOT_MODULE_CONTRACT_ID_V1,
            sdk_major: SDK_MAJOR_VERSION_V1,
            reserved: 0,
            metadata: PluginMetadataV1 {
                plugin_id: StableIdV1::new(EXTENSION_ID_NAMESPACE_V1, 10),
                primary_interface_id: StableIdV1::new(EXTENSION_ID_NAMESPACE_V1, 11),
            },
            registrar,
        }
        .leak_into_prefix();

        assert_eq!(root.abi_schema(), ABI_SCHEMA_V1);
        assert_eq!(root.root_contract_id(), ROOT_MODULE_CONTRACT_ID_V1);
        assert_eq!(root.sdk_major(), SDK_MAJOR_VERSION_V1);
        assert_eq!(
            root.registrar().describe_contract().map(|query| query()),
            Some(ROOT_MODULE_CONTRACT_ID_V1)
        );
        assert_eq!(
            root.registrar()
                .register()
                .invoke(registrar_request_v1())
                .into_result(),
            Ok(RegistrationOutcomeV1::accepted(0))
        );
    }

    #[test]
    fn registrar_panic_is_a_typed_terminal_before_crossing_c_abi() {
        struct Panics;

        impl RegistrarImplementationV1 for Panics {
            fn register(_: RegistrarRequestV1) -> RegistrarResultV1 {
                panic!("synthetic registrar panic");
            }
        }

        let result = RegistrarCallbackV1::new::<Panics>().invoke(registrar_request_v1());

        assert_eq!(
            result.into_result(),
            Err(AbiErrorV1::new(
                AbiErrorCodeV1::CALLBACK_PANICKED,
                ROOT_MODULE_CONTRACT_ID_V1,
                0
            ))
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
