use std::process::ExitCode;
fn main() -> ExitCode {
    match superexplorer_ui_abi_fingerprint::run(std::env::args().skip(1)) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("ui ABI fingerprint: {error}");
            ExitCode::FAILURE
        }
    }
}
