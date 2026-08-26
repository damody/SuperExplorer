//! Provider-owned remote filesystem primitives.
//!
//! This crate intentionally keeps remote paths out of Win32 Shell APIs. The application
//! composes it into the Explorer command loop in a later integration step.

pub mod adb;
mod provider;
pub mod sftp;
mod transfer;

pub use adb::{AdbClient, AdbDevice, AdbDeviceState, AdbDirectoryEntry, AdbProvider};
pub use provider::{RemoteEntry, RemoteEntryKind, RemoteProvider, RemoteProviderRegistry};
pub use sftp::SftpProvider;
pub use transfer::{TransferEngine, TransferItemOutcome, TransferMode, TransferResult};
