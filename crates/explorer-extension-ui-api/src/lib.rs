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
//! Public GPUI-facing extension API boundary.
//!
//! This crate depends on [`explorer_extension_api`] and will receive the public UI
//! contribution contracts in later tasks. It must not depend on the private
//! `explorer-ui` implementation or on the extension host.
