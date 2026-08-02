#![allow(
    non_camel_case_types,
    reason = "abi_stable convention generates the RootModule reference suffix"
)]
#![allow(
    unsafe_code,
    reason = "the fixture models the old prefix layout, including generated accessors"
)]

//! Copied SDK 1.0 ABI surface for the old-plugin compatibility fixture.
//!
//! Its package, type, and root-module identities intentionally match the current
//! `explorer-extension-api`. The only difference is that 1.0 ends the registrar
//! prefix at `register`; it predates the optional 1.x `describe_contract` tail.

use std::panic::{AssertUnwindSafe, catch_unwind};

use abi_stable::{
    StableAbi, library::RootModule, package_version_strings, sabi_types::VersionStrings,
    std_types::RResult,
};

pub const EXTENSION_ID_NAMESPACE_V1: IdNamespaceV1 = IdNamespaceV1(0x5345_0001);
pub const ABI_SCHEMA_V1: AbiSchemaIdV1 = AbiSchemaIdV1(0x5345_0001);
pub const ROOT_MODULE_CONTRACT_ID_V1: StableIdV1 = StableIdV1 {
    namespace: EXTENSION_ID_NAMESPACE_V1,
    value: 1,
};
pub const SDK_MAJOR_VERSION_V1: u16 = 1;

#[repr(transparent)]
#[derive(Clone, Copy, Debug, Eq, PartialEq, StableAbi)]
pub struct IdNamespaceV1(u32);

impl IdNamespaceV1 {
    #[must_use]
    pub const fn new(authority: u16, revision: u16) -> Self {
        Self(((authority as u32) << 16) | (revision as u32))
    }

    #[must_use]
    pub const fn is_valid(self) -> bool {
        (self.0 >> 16) != 0 && (self.0 as u16) != 0
    }
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Eq, PartialEq, StableAbi)]
pub struct StableIdV1 {
    pub namespace: IdNamespaceV1,
    pub value: u64,
}

impl StableIdV1 {
    #[must_use]
    pub const fn new(namespace: IdNamespaceV1, value: u64) -> Self {
        Self { namespace, value }
    }
}

#[repr(transparent)]
#[derive(Clone, Copy, Debug, Eq, PartialEq, StableAbi)]
pub struct AbiSchemaIdV1(u32);

impl AbiSchemaIdV1 {
    #[must_use]
    pub const fn new(authority: u16, revision: u16) -> Self {
        Self(((authority as u32) << 16) | (revision as u32))
    }
}

#[repr(transparent)]
#[derive(Clone, Copy, Debug, Eq, PartialEq, StableAbi)]
pub struct AbiErrorCodeV1(u32);

impl AbiErrorCodeV1 {
    pub const CALLBACK_PANICKED: Self = Self(5);
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Eq, PartialEq, StableAbi)]
pub struct AbiErrorV1 {
    pub code: AbiErrorCodeV1,
    pub subject: StableIdV1,
    pub detail: u32,
}

impl AbiErrorV1 {
    #[must_use]
    pub const fn new(code: AbiErrorCodeV1, subject: StableIdV1, detail: u32) -> Self {
        Self {
            code,
            subject,
            detail,
        }
    }
}

pub type AbiResultV1<T> = RResult<T, AbiErrorV1>;

#[repr(C)]
#[derive(Clone, Copy, Debug, Eq, PartialEq, StableAbi)]
pub struct PluginMetadataV1 {
    pub plugin_id: StableIdV1,
    pub primary_interface_id: StableIdV1,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Eq, PartialEq, StableAbi)]
pub struct RegistrarRequestV1 {
    pub abi_schema: AbiSchemaIdV1,
    pub root_contract_id: StableIdV1,
    pub sdk_major: u16,
    pub reserved: u16,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Eq, PartialEq, StableAbi)]
pub struct RegistrationOutcomeV1 {
    pub status: RegistrationStatusV1,
    pub registered_interface_count: u32,
}

#[repr(transparent)]
#[derive(Clone, Copy, Debug, Eq, PartialEq, StableAbi)]
pub struct RegistrationStatusV1(u32);

impl RegistrationStatusV1 {
    pub const ACCEPTED: Self = Self(1);
    pub const REJECTED: Self = Self(2);

    #[must_use]
    pub const fn from_raw(value: u32) -> Self {
        Self(value)
    }

    #[must_use]
    pub const fn into_raw(self) -> u32 {
        self.0
    }
}

impl RegistrationOutcomeV1 {
    #[must_use]
    pub const fn accepted(registered_interface_count: u32) -> Self {
        Self {
            status: RegistrationStatusV1::ACCEPTED,
            registered_interface_count,
        }
    }
}

pub type RegistrarResultV1 = AbiResultV1<RegistrationOutcomeV1>;

pub trait RegistrarImplementationV1 {
    fn register(request: RegistrarRequestV1) -> RegistrarResultV1;
}

#[repr(transparent)]
#[derive(Clone, Copy, StableAbi)]
pub struct RegistrarCallbackV1(extern "C" fn(RegistrarRequestV1) -> RegistrarResultV1);

impl RegistrarCallbackV1 {
    #[must_use]
    pub fn new<T: RegistrarImplementationV1>() -> Self {
        Self(registrar_trampoline::<T>)
    }

    pub fn invoke(self, request: RegistrarRequestV1) -> RegistrarResultV1 {
        (self.0)(request)
    }
}

extern "C" fn registrar_trampoline<T: RegistrarImplementationV1>(
    request: RegistrarRequestV1,
) -> RegistrarResultV1 {
    translate_registrar_panic(|| T::register(request))
}

/// The same panic wrapper supplied by the SDK; it is called inside the old
/// plugin callback and prevents a Rust unwind from crossing the C ABI.
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

#[repr(C)]
#[derive(StableAbi)]
#[sabi(kind(Prefix(prefix_ref = ExtensionRegistrarV1_Ref)))]
#[sabi(missing_field(panic))]
pub struct ExtensionRegistrarV1 {
    #[sabi(last_prefix_field)]
    pub register: RegistrarCallbackV1,
}

#[repr(C)]
#[derive(StableAbi)]
#[sabi(kind(Prefix(prefix_ref = ExtensionRootModuleV1_Ref)))]
#[sabi(missing_field(panic))]
pub struct ExtensionRootModuleV1 {
    pub abi_schema: AbiSchemaIdV1,
    pub root_contract_id: StableIdV1,
    pub sdk_major: u16,
    pub reserved: u16,
    pub metadata: PluginMetadataV1,
    #[sabi(last_prefix_field)]
    pub registrar: ExtensionRegistrarV1_Ref,
}

impl RootModule for ExtensionRootModuleV1_Ref {
    abi_stable::declare_root_module_statics! {ExtensionRootModuleV1_Ref}

    const BASE_NAME: &'static str = "superexplorer_extension_v1";
    const NAME: &'static str = "superexplorer_extension_v1";
    const VERSION_STRINGS: VersionStrings = package_version_strings!();
}
