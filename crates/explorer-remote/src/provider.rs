//! Platform-neutral remote provider boundary.

use std::{collections::HashMap, path::Path, sync::Arc};

use anyhow::{Result, bail};
use explorer_model::{CancellationToken, LocationDescriptor, VirtualLocationDescriptor};

pub(crate) fn validate_remote_location(
    location: &VirtualLocationDescriptor,
    provider_id: &str,
    allow_root: bool,
) -> Result<()> {
    if location.provider_id != provider_id
        || location.container_identity == [0; 16]
        || location.container_generation == 0
        || location
            .public_authority
            .as_deref()
            .is_none_or(str::is_empty)
    {
        bail!("remote location authority is invalid");
    }
    if !allow_root && location.components.is_empty() {
        bail!("remote filesystem root cannot be mutated");
    }
    if location.components.iter().any(|component| {
        component.is_empty()
            || matches!(component.as_str(), "." | "..")
            || component.contains(['/', '\\', '\0', '\r', '\n'])
    }) {
        bail!("remote path contains an invalid component");
    }
    Ok(())
}

pub const MAX_TRANSFER_DEPTH: usize = 64;
pub const MAX_TRANSFER_NODES: usize = 100_000;
pub const MAX_TRANSFER_FILE_BYTES: u64 = 32 * 1024 * 1024 * 1024;
pub const MAX_OPERATION_STAGING_BYTES: u64 = 64 * 1024 * 1024 * 1024;
pub const MAX_PROCESS_STAGING_BYTES: u64 = 128 * 1024 * 1024 * 1024;
pub const MINIMUM_FREE_SPACE_RESERVE_BYTES: u64 = 2 * 1024 * 1024 * 1024;

pub(crate) const fn transfer_tree_within_limits(depth: usize, nodes: usize) -> bool {
    depth <= MAX_TRANSFER_DEPTH && nodes <= MAX_TRANSFER_NODES
}

pub(crate) const fn transfer_bytes_within_limits(file_bytes: u64, operation_bytes: u64) -> bool {
    file_bytes <= MAX_TRANSFER_FILE_BYTES && operation_bytes <= MAX_OPERATION_STAGING_BYTES
}

pub(crate) fn validate_windows_component(component: &str) -> Result<()> {
    if component.is_empty()
        || matches!(component, "." | "..")
        || component.ends_with(['.', ' '])
        || component.contains(['/', '\\', ':', '\0', '\r', '\n'])
    {
        bail!("remote name is not a safe Windows path component");
    }
    let stem = component
        .split_once('.')
        .map_or(component, |(stem, _)| stem)
        .trim_end_matches(['.', ' ']);
    if matches!(
        stem.to_ascii_uppercase().as_str(),
        "CON"
            | "PRN"
            | "AUX"
            | "NUL"
            | "COM1"
            | "COM2"
            | "COM3"
            | "COM4"
            | "COM5"
            | "COM6"
            | "COM7"
            | "COM8"
            | "COM9"
            | "LPT1"
            | "LPT2"
            | "LPT3"
            | "LPT4"
            | "LPT5"
            | "LPT6"
            | "LPT7"
            | "LPT8"
            | "LPT9"
    ) {
        bail!("remote name is a reserved Windows device name");
    }
    Ok(())
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RemoteEntry {
    pub name: String,
    pub location: LocationDescriptor,
    pub kind: RemoteEntryKind,
    pub size: Option<u64>,
    pub unix_mode: Option<u32>,
}

/// Authoritative metadata for one remote descriptor, including the directory currently being
/// viewed. Optional fields remain absent when the provider cannot report them without a recursive
/// scan or following an unrelated target.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RemoteMetadata {
    pub location: LocationDescriptor,
    pub kind: RemoteEntryKind,
    pub size: Option<u64>,
    pub unix_mode: Option<u32>,
    pub modified_unix_seconds: Option<u64>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RemoteEntryKind {
    File,
    Directory,
    FileSymlink,
    DirectorySymlink,
    BrokenSymlink,
    CircularSymlink,
}

impl RemoteEntryKind {
    pub const fn is_container(self) -> bool {
        matches!(self, Self::Directory | Self::DirectorySymlink)
    }

    pub const fn type_display(self) -> &'static str {
        match self {
            Self::File => "Remote file",
            Self::Directory => "Remote folder",
            Self::FileSymlink => "Remote file link",
            Self::DirectorySymlink => "Remote folder link",
            Self::BrokenSymlink => "Broken remote link",
            Self::CircularSymlink => "Circular remote link",
        }
    }
}

/// Synchronous provider operations run only on the remote worker pool. Implementations must poll
/// cancellation during long transfers and must never call GPUI or Windows Shell APIs.
pub trait RemoteProvider: Send + Sync {
    fn provider_id(&self) -> &'static str;
    fn list(
        &self,
        location: &VirtualLocationDescriptor,
        cancellation: &CancellationToken,
    ) -> Result<Vec<RemoteEntry>>;
    fn download(
        &self,
        source: &VirtualLocationDescriptor,
        local_destination: &Path,
        cancellation: &CancellationToken,
    ) -> Result<()>;
    fn download_with_progress(
        &self,
        source: &VirtualLocationDescriptor,
        local_destination: &Path,
        cancellation: &CancellationToken,
        progress: &(dyn Fn(u64) + Send + Sync),
    ) -> Result<()> {
        self.download(source, local_destination, cancellation)?;
        if let Ok(metadata) = self.metadata(source, cancellation)
            && let Some(bytes) = metadata.size
        {
            progress(bytes);
        }
        Ok(())
    }
    fn upload(
        &self,
        local_source: &Path,
        destination: &VirtualLocationDescriptor,
        cancellation: &CancellationToken,
    ) -> Result<()>;
    fn upload_with_progress(
        &self,
        local_source: &Path,
        destination: &VirtualLocationDescriptor,
        cancellation: &CancellationToken,
        progress: &(dyn Fn(u64) + Send + Sync),
    ) -> Result<()> {
        self.upload(local_source, destination, cancellation)?;
        progress(crate::transfer::local_tree_bytes(local_source).unwrap_or(0));
        Ok(())
    }
    fn create_directory(
        &self,
        location: &VirtualLocationDescriptor,
        cancellation: &CancellationToken,
    ) -> Result<()>;
    fn create_symlink(
        &self,
        _location: &VirtualLocationDescriptor,
        _target: &str,
        _cancellation: &CancellationToken,
    ) -> Result<()> {
        bail!("remote provider does not support symbolic-link creation")
    }
    fn metadata(
        &self,
        _location: &VirtualLocationDescriptor,
        _cancellation: &CancellationToken,
    ) -> Result<RemoteMetadata> {
        bail!("remote provider does not support single-location metadata")
    }
    fn rename(
        &self,
        source: &VirtualLocationDescriptor,
        destination: &VirtualLocationDescriptor,
        cancellation: &CancellationToken,
    ) -> Result<()>;
    fn delete(
        &self,
        location: &VirtualLocationDescriptor,
        recursive: bool,
        cancellation: &CancellationToken,
    ) -> Result<()>;
    fn set_unix_mode(
        &self,
        _location: &VirtualLocationDescriptor,
        _mode: u32,
        _cancellation: &CancellationToken,
    ) -> Result<()> {
        bail!("remote provider does not support Unix permission changes")
    }
}

#[derive(Default)]
pub struct RemoteProviderRegistry {
    providers: HashMap<&'static str, Arc<dyn RemoteProvider>>,
}

impl RemoteProviderRegistry {
    pub fn register(&mut self, provider: Arc<dyn RemoteProvider>) -> Result<()> {
        let id = provider.provider_id();
        if id.is_empty() || self.providers.insert(id, provider).is_some() {
            bail!("remote provider id is empty or duplicated");
        }
        Ok(())
    }

    pub fn resolve(&self, location: &LocationDescriptor) -> Result<&Arc<dyn RemoteProvider>> {
        let LocationDescriptor::Virtual(location) = location else {
            bail!("location is not remote");
        };
        self.providers
            .get(location.provider_id.as_str())
            .ok_or_else(|| anyhow::anyhow!("remote provider is unavailable"))
    }
}

#[cfg(test)]
mod tests {
    use super::{
        MAX_OPERATION_STAGING_BYTES, MAX_TRANSFER_DEPTH, MAX_TRANSFER_FILE_BYTES,
        MAX_TRANSFER_NODES, RemoteEntryKind, RemoteMetadata, transfer_bytes_within_limits,
        transfer_tree_within_limits, validate_remote_location, validate_windows_component,
    };
    use explorer_model::VirtualLocationDescriptor;

    #[test]
    fn remote_entry_kinds_define_container_and_type_semantics() {
        let cases = [
            (RemoteEntryKind::File, false, "Remote file"),
            (RemoteEntryKind::Directory, true, "Remote folder"),
            (RemoteEntryKind::FileSymlink, false, "Remote file link"),
            (
                RemoteEntryKind::DirectorySymlink,
                true,
                "Remote folder link",
            ),
            (RemoteEntryKind::BrokenSymlink, false, "Broken remote link"),
            (
                RemoteEntryKind::CircularSymlink,
                false,
                "Circular remote link",
            ),
        ];

        for (kind, is_container, type_display) in cases {
            assert_eq!(kind.is_container(), is_container);
            assert_eq!(kind.type_display(), type_display);
        }
    }

    #[test]
    fn metadata_contract_preserves_optional_fields_and_opaque_location() {
        let location = VirtualLocationDescriptor {
            provider_id: "adb".to_owned(),
            public_authority: Some("device".to_owned()),
            container_identity: [3; 16],
            container_generation: 7,
            entry_id: None,
            components: vec!["sdcard".to_owned(), "Download".to_owned()],
        };
        let metadata = RemoteMetadata {
            location: explorer_model::LocationDescriptor::Virtual(location.clone()),
            kind: RemoteEntryKind::Directory,
            size: None,
            unix_mode: Some(0o040755),
            modified_unix_seconds: None,
        };
        assert_eq!(
            metadata.location,
            explorer_model::LocationDescriptor::Virtual(location)
        );
        assert!(metadata.kind.is_container());
        assert_eq!(metadata.size, None);
        assert_eq!(metadata.modified_unix_seconds, None);
    }

    #[test]
    fn mutation_validation_rejects_root_and_hostile_components() {
        let mut location = VirtualLocationDescriptor {
            provider_id: "adb".to_owned(),
            public_authority: Some("device".to_owned()),
            container_identity: [1; 16],
            container_generation: 1,
            entry_id: None,
            components: Vec::new(),
        };
        assert!(validate_remote_location(&location, "adb", false).is_err());
        assert!(validate_remote_location(&location, "adb", true).is_ok());
        location.components.push("..".to_owned());
        assert!(validate_remote_location(&location, "adb", true).is_err());
    }

    #[test]
    fn windows_staging_component_rejects_escape_ads_and_devices() {
        for hostile in [
            "", ".", "..", "a/b", "a\\b", "a:stream", "name. ", "CON", "lpt1.txt",
        ] {
            assert!(validate_windows_component(hostile).is_err(), "{hostile:?}");
        }
        assert!(validate_windows_component("正常 file.txt").is_ok());
    }

    #[test]
    fn traversal_and_byte_limits_accept_boundary_and_reject_n_plus_one() {
        assert!(transfer_tree_within_limits(
            MAX_TRANSFER_DEPTH,
            MAX_TRANSFER_NODES
        ));
        assert!(!transfer_tree_within_limits(
            MAX_TRANSFER_DEPTH + 1,
            MAX_TRANSFER_NODES
        ));
        assert!(!transfer_tree_within_limits(
            MAX_TRANSFER_DEPTH,
            MAX_TRANSFER_NODES + 1
        ));
        assert!(transfer_bytes_within_limits(
            MAX_TRANSFER_FILE_BYTES,
            MAX_OPERATION_STAGING_BYTES
        ));
        assert!(!transfer_bytes_within_limits(
            MAX_TRANSFER_FILE_BYTES + 1,
            MAX_OPERATION_STAGING_BYTES
        ));
    }
}
