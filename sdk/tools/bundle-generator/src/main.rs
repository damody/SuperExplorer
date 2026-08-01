use std::process::ExitCode;

fn main() -> ExitCode {
    match superexplorer_bundle_generator::run(std::env::args().skip(1)) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("bundle generator: {error}");
            ExitCode::FAILURE
        }
    }
}
