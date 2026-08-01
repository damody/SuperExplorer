#![allow(
    non_camel_case_types,
    reason = "abi_stable convention generates the RootModule reference suffix"
)]

//! Private P0-0 fixture contract for proving the `abi_stable` loading boundary.
//!
//! This is intentionally not the public plugin SDK API. Task 2 owns that API.

use abi_stable::{
    StableAbi,
    library::RootModule,
    package_version_strings,
    sabi_types::VersionStrings,
    std_types::{RResult, RString},
};

/// Typed terminal from the fixture registrar. No Rust panic crosses this boundary.
pub type RegistrarResult = RResult<u32, RString>;

/// Minimal prefix root module with one registrar callback.
///
/// `layout-mismatch` is deliberately a test-only incompatible layout used by
/// the host fixture to prove `abi_stable` rejects the DLL before invocation.
#[repr(C)]
#[derive(StableAbi)]
#[sabi(kind(Prefix(prefix_ref = AbiFixtureRoot_Ref)))]
#[sabi(missing_field(panic))]
pub struct AbiFixtureRoot {
    #[cfg(not(feature = "layout-mismatch"))]
    pub abi_schema: u32,
    #[cfg(feature = "layout-mismatch")]
    pub abi_schema: u64,
    #[sabi(last_prefix_field)]
    pub registrar: extern "C" fn(bool) -> RegistrarResult,
}

impl RootModule for AbiFixtureRoot_Ref {
    abi_stable::declare_root_module_statics! {AbiFixtureRoot_Ref}

    const BASE_NAME: &'static str = "abi_root_fixture_plugin";
    const NAME: &'static str = "abi_root_fixture_plugin";
    const VERSION_STRINGS: VersionStrings = package_version_strings!();
}

#[cfg(test)]
mod tests {
    use super::*;

    extern "C" fn test_registrar(_: bool) -> RegistrarResult {
        RResult::ROk(0)
    }

    #[test]
    fn root_is_a_prefix_type_with_a_registrar() {
        let _ = AbiFixtureRoot {
            abi_schema: 1,
            registrar: test_registrar,
        };
    }
}
