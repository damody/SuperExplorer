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
//! Explorer process composition and startup lifecycle.
#![allow(
    clippy::must_use_candidate,
    reason = "application composition accessors do not own resources that require consumption"
)]

pub mod application;
pub mod branding;
mod brokered_service;
#[cfg(windows)]
mod mft_size_map;
mod pointer_capture;
pub mod session_lifecycle;
pub mod session_store;
pub mod startup;
pub mod system_theme;
pub mod visual_fixture;
pub mod windows_prerequisites;
