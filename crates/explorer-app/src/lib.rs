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
pub mod bookmark_store;
pub mod branding;
mod brokered_service;
mod folder_size_service;
mod mft_focus;
#[cfg(windows)]
mod mft_journal;
mod mft_migration;
#[cfg(windows)]
mod mft_persistence;
#[cfg(windows)]
mod mft_query;
mod mft_runtime;
#[cfg(windows)]
mod mft_size_map;
#[cfg(windows)]
mod mft_sqlite;
mod pointer_capture;
mod remote_service;
pub mod session_lifecycle;
pub mod session_store;
pub mod startup;
pub mod system_theme;
pub mod visual_fixture;
pub mod windows_prerequisites;
