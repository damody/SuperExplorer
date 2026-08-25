//! Platform-neutral remote provider boundary.

use std::{collections::HashMap, path::Path, sync::Arc};

use anyhow::{Result, bail};
use explorer_model::{CancellationToken, LocationDescriptor, VirtualLocationDescriptor};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RemoteEntry {
    pub name: String,
    pub location: LocationDescriptor,
    pub is_directory: bool,
    pub size: Option<u64>,
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
