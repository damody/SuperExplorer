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
fn main() {
    if let Err(error) = explorer_uitest::run_from_env() {
        eprintln!("explorer-uitest: {error:#}");
        std::process::exit(2);
    }
}
