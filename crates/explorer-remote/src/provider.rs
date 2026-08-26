//! Platform-neutral remote provider boundary.

use std::{collections::HashMap, path::Path, sync::Arc};

use anyhow::{Result, bail};
use explorer_model::{CancellationToken, LocationDescriptor, VirtualLocationDescriptor};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RemoteEntry {
    pub name: String,
    pub location: LocationDescriptor,
    pub kind: RemoteEntryKind,
    pub size: Option<u64>,
    pub unix_mode: Option<u32>,
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
    fn upload(
        &self,
        local_source: &Path,
        destination: &VirtualLocationDescriptor,
        cancellation: &CancellationToken,
    ) -> Result<()>;
    fn create_directory(
        &self,
        location: &VirtualLocationDescriptor,
        cancellation: &CancellationToken,
    ) -> Result<()>;
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
    use super::RemoteEntryKind;

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
}
