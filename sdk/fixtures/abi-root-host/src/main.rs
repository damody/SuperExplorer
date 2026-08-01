//! Host-side verifier for the P0-0 root-module fixture.

use std::{
    env,
    path::{Path, PathBuf},
};

use abi_root_fixture_contract::AbiFixtureRoot_Ref;
use abi_stable::{library::RootModule, std_types::RResult};

fn compatible_load(path: &Path) -> Result<(), String> {
    let root = AbiFixtureRoot_Ref::load_from_file(path)
        .map_err(|error| format!("compatible root rejected: {error}"))?;

    match (root.registrar())(false) {
        RResult::ROk(7) => {}
        RResult::ROk(value) => return Err(format!("unexpected registrar value: {value}")),
        RResult::RErr(error) => return Err(format!("registrar failed: {error}")),
    }
    match (root.registrar())(true) {
        RResult::RErr(error) if error.as_str() == "plugin callback panicked" => Ok(()),
        RResult::RErr(error) => Err(format!("unexpected panic terminal: {error}")),
        RResult::ROk(value) => Err(format!("panic escaped as success: {value}")),
    }
}

fn layout_mismatch_is_rejected_before_callback(path: &Path, marker: &Path) -> Result<(), String> {
    if AbiFixtureRoot_Ref::load_from_file(path).is_ok() {
        return Err("layout-mismatch plugin unexpectedly loaded".to_owned());
    }

    if !marker.exists() {
        Ok(())
    } else {
        Err(format!(
            "layout mismatch invoked registrar and wrote marker: {}",
            marker.display()
        ))
    }
}

fn run(mode: &str, path: &Path, marker: Option<&Path>) -> Result<(), String> {
    match mode {
        "compatible" => compatible_load(path),
        "mismatch" => layout_mismatch_is_rejected_before_callback(
            path,
            marker.ok_or_else(|| "mismatch mode requires ABI_ROOT_FIXTURE_MARKER".to_owned())?,
        ),
        _ => Err("mode must be compatible or mismatch".to_owned()),
    }
}

fn main() -> Result<(), String> {
    let mut arguments = env::args_os();
    let _program = arguments.next();
    let mode = arguments.next().ok_or_else(|| {
        "usage: abi-root-fixture-host <compatible|mismatch> <plugin-path>".to_owned()
    })?;
    let path = arguments
        .next()
        .map(std::path::PathBuf::from)
        .ok_or_else(|| "missing plugin path".to_owned())?;
    if arguments.next().is_some() {
        return Err("too many arguments".to_owned());
    }

    let marker = env::var_os("ABI_ROOT_FIXTURE_MARKER").map(PathBuf::from);
    run(mode.to_string_lossy().as_ref(), &path, marker.as_deref())
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::run;

    #[test]
    fn command_requires_a_mode_and_plugin_path() {
        assert_eq!(
            run("other", Path::new("not-loaded.dll"), None),
            Err("mode must be compatible or mismatch".to_owned())
        );
    }
}
