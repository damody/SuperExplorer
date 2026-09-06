//! Provider-owned remote filesystem primitives.
//!
//! This crate intentionally keeps remote paths out of Win32 Shell APIs. The application
//! composes it into the Explorer command loop in a later integration step.

pub mod adb;
pub mod adb_tools;
pub mod ftp;
mod ftp_protocol;
pub mod gdrive;
mod gdrive_protocol;
mod provider;
pub mod sftp;
mod transfer;

pub use adb::{AdbClient, AdbDevice, AdbDeviceState, AdbDirectoryEntry, AdbProvider};
pub use adb_tools::{
    AdbCandidateRejection, AdbDeviceSnapshot, AdbInstallOutcome, AdbToolInstaller,
    AdbToolProvenance, AdbToolResolver, ResolvedAdbTool,
};
pub use explorer_model::RemoteProviderCapabilities;
pub use ftp::FtpProvider;
pub use gdrive::GdriveProvider;
pub use provider::{
    RemoteEntry, RemoteEntryKind, RemoteMetadata, RemoteProvider, RemoteProviderRegistry,
};
pub use sftp::SftpProvider;
pub use transfer::{
    TransferEngine, TransferItemOutcome, TransferMode, TransferResult, TransferStage,
    sanitize_transfer_diagnostic,
};
