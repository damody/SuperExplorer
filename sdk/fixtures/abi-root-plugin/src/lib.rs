//! Fixture plugin that translates registrar panics into an FFI-safe terminal.

use std::{
    env, fs,
    panic::{AssertUnwindSafe, catch_unwind},
    path::PathBuf,
};

use abi_root_fixture_contract::{AbiFixtureRoot, AbiFixtureRoot_Ref, RegistrarResult};
use abi_stable::{
    export_root_module,
    prefix_type::PrefixTypeTrait,
    std_types::{RResult, RString},
};

const MARKER_ENVIRONMENT_VARIABLE: &str = "ABI_ROOT_FIXTURE_MARKER";

fn marker_path() -> Option<PathBuf> {
    env::var_os(MARKER_ENVIRONMENT_VARIABLE).map(PathBuf::from)
}

fn mark_registrar_invocation() -> Result<(), RString> {
    let Some(path) = marker_path() else {
        return Ok(());
    };
    fs::write(path, b"registrar invoked")
        .map_err(|error| RString::from(format!("could not write fixture marker: {error}")))
}

extern "C" fn registrar(should_panic: bool) -> RegistrarResult {
    if let Err(error) = mark_registrar_invocation() {
        return RResult::RErr(error);
    }

    let terminal = catch_unwind(AssertUnwindSafe(|| {
        if should_panic {
            panic!("fixture registrar panic");
        }
        7_u32
    }));
    match terminal {
        Ok(value) => RResult::ROk(value),
        Err(_) => RResult::RErr(RString::from("plugin callback panicked")),
    }
}

#[export_root_module]
pub fn get_library() -> AbiFixtureRoot_Ref {
    AbiFixtureRoot {
        abi_schema: 1,
        registrar,
    }
    .leak_into_prefix()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registrar_translates_panics_to_typed_errors() {
        assert_eq!(registrar(false).into_result(), Ok(7));
        assert_eq!(
            registrar(true).into_result(),
            Err(RString::from("plugin callback panicked"))
        );
    }
}
