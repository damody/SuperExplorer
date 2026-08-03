//! Dependency lock anchor for the rust-tokei code-lines example.

/// Keep the package buildable while the example consumer is assembled.
pub fn pinned_tokei_version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}
