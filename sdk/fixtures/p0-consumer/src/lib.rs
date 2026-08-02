#![allow(
    non_camel_case_types,
    reason = "abi_stable convention generates the RootModule reference suffix"
)]

//! Standalone P0 consumer root module.
//!
//! This fixture proves the SDK loading boundary only.  It deliberately does
//! not construct or render a GPUI element: the public GPUI contribution API is
//! introduced by Task 2.  The immutable fingerprint callback lets a host make
//! its pre-callback compatibility decision now without pretending a renderer
//! exists.

use std::{
    env, fs,
    panic::{catch_unwind, AssertUnwindSafe},
    path::PathBuf,
};

use abi_stable::{
    export_root_module,
    library::RootModule,
    package_version_strings,
    prefix_type::PrefixTypeTrait,
    sabi_types::VersionStrings,
    std_types::{RResult, RStr, RString},
    StableAbi,
};

/// The P0 root layout version.  Task 2 replaces this fixture-only layout with
/// the versioned public extension API.
pub const ABI_SCHEMA_VERSION: u32 = 1;

/// A snapshot-bound immutable compatibility value, copied from the canonical
/// SDK artifact when this template was produced.
pub const UI_ABI_FINGERPRINT: &str =
    "92a05fdb333b30307a6ee3ec0da73f6fa2a44f92c6ac7735d24d662fcc089f59";

const MARKER_ENVIRONMENT_VARIABLE: &str = "P0_CONSUMER_REGISTRAR_MARKER";

pub type RegistrarResult = RResult<u32, RString>;

/// Minimal, prefix-compatible root.  Both compatibility fields are exposed
/// before the callback; the registrar itself repeats validation defensively
/// before it can write its invocation marker.
#[repr(C)]
#[derive(StableAbi)]
#[sabi(kind(Prefix(prefix_ref = P0ConsumerRoot_Ref)))]
#[sabi(missing_field(panic))]
pub struct P0ConsumerRoot {
    pub abi_schema: u32,
    pub ui_abi_fingerprint: extern "C" fn() -> RString,
    #[sabi(last_prefix_field)]
    pub registrar: extern "C" fn(u32, RStr<'static>) -> RegistrarResult,
}

impl RootModule for P0ConsumerRoot_Ref {
    abi_stable::declare_root_module_statics! {P0ConsumerRoot_Ref}

    const BASE_NAME: &'static str = "p0_consumer";
    const NAME: &'static str = "p0_consumer";
    const VERSION_STRINGS: VersionStrings = package_version_strings!();
}

extern "C" fn immutable_ui_abi_fingerprint() -> RString {
    RString::from(UI_ABI_FINGERPRINT)
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

fn registrar_inner(expected_schema: u32, expected_fingerprint: RStr<'static>) -> RegistrarResult {
    if expected_schema != ABI_SCHEMA_VERSION {
        return RResult::RErr(RString::from("ABI schema mismatch"));
    }
    if expected_fingerprint.as_str() != UI_ABI_FINGERPRINT {
        return RResult::RErr(RString::from("UI ABI fingerprint mismatch"));
    }
    // This must remain after all compatibility checks.  Host tests use it to
    // prove a mismatched fingerprint never crosses the callback boundary.
    match mark_callback_invocation() {
        Ok(()) => RResult::ROk(7),
        Err(error) => RResult::RErr(error),
    }
}

extern "C" fn registrar(
    expected_schema: u32,
    expected_fingerprint: RStr<'static>,
) -> RegistrarResult {
    match catch_unwind(AssertUnwindSafe(|| {
        registrar_inner(expected_schema, expected_fingerprint)
    })) {
        Ok(terminal) => terminal,
        Err(_) => RResult::RErr(RString::from("P0 consumer registrar panicked")),
    }
}

/// `plugin_root` is the manifest-declared `abi_stable` root-module export.
#[export_root_module]
pub fn plugin_root() -> P0ConsumerRoot_Ref {
    P0ConsumerRoot {
        abi_schema: ABI_SCHEMA_VERSION,
        ui_abi_fingerprint: immutable_ui_abi_fingerprint,
        registrar,
    }
    .leak_into_prefix()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn root_declares_immutable_pre_callback_compatibility_data() {
        let root = plugin_root();
        assert_eq!(root.abi_schema(), ABI_SCHEMA_VERSION);
        assert_eq!((root.ui_abi_fingerprint())().as_str(), UI_ABI_FINGERPRINT);
    }

    #[test]
    fn mismatched_fingerprint_is_rejected_before_marker_write() {
        assert_eq!(
            registrar(
                ABI_SCHEMA_VERSION,
                RStr::from_str("not-the-sdk-fingerprint")
            )
            .into_result(),
            Err(RString::from("UI ABI fingerprint mismatch"))
        );
    }

    #[test]
    fn matching_compatibility_data_calls_the_registrar() {
        assert_eq!(
            registrar(ABI_SCHEMA_VERSION, RStr::from_str(UI_ABI_FINGERPRINT)).into_result(),
            Ok(7)
        );
    }
}
