//! Cross-filesystem copy/move using bounded scoped staging.

use std::{
    fs,
    path::{Path, PathBuf},
};

use anyhow::{Context as _, Result, bail};
use explorer_model::{CancellationToken, LocationDescriptor};

use crate::RemoteProviderRegistry;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TransferMode {
    Copy,
    Move,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum TransferResult {
    Succeeded,
    Partial { diagnostic: String },
    Failed { diagnostic: String },
    Cancelled,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TransferItemOutcome {
    pub source: LocationDescriptor,
    pub destination: LocationDescriptor,
    pub result: TransferResult,
}

pub struct TransferEngine<'a> {
    providers: &'a RemoteProviderRegistry,
}

impl<'a> TransferEngine<'a> {
    pub const fn new(providers: &'a RemoteProviderRegistry) -> Self {
        Self { providers }
    }

    pub fn transfer(
        &self,
        source: LocationDescriptor,
        destination: LocationDescriptor,
        mode: TransferMode,
        cancellation: &CancellationToken,
    ) -> TransferItemOutcome {
        let result = if cancellation.is_cancelled() {
            TransferResult::Cancelled
        } else {
            match self.copy(&source, &destination, cancellation) {
                Ok(()) if mode == TransferMode::Copy => TransferResult::Succeeded,
                Ok(()) => match self.delete_source(&source, cancellation) {
                    Ok(()) => TransferResult::Succeeded,
                    Err(error) => TransferResult::Partial {
                        diagnostic: format!("copy completed but source deletion failed: {error}"),
                    },
                },
                Err(_error) if cancellation.is_cancelled() => TransferResult::Cancelled,
                Err(error) => TransferResult::Failed {
                    diagnostic: error.to_string(),
                },
            }
        };
        TransferItemOutcome {
            source,
            destination,
            result,
        }
    }

    fn copy(
        &self,
        source: &LocationDescriptor,
        destination: &LocationDescriptor,
        cancellation: &CancellationToken,
    ) -> Result<()> {
        match (source, destination) {
            (
                LocationDescriptor::FileSystem(source),
                LocationDescriptor::FileSystem(destination),
            ) => copy_local(source, destination),
            (LocationDescriptor::FileSystem(source), LocationDescriptor::Virtual(destination)) => {
                self.providers
                    .resolve(&LocationDescriptor::Virtual(destination.clone()))?
                    .upload(source, destination, cancellation)
            }
            (LocationDescriptor::Virtual(source), LocationDescriptor::FileSystem(destination)) => {
                let target = if destination.is_dir() {
                    destination.join(
                        source
                            .components
                            .last()
                            .context("remote source has no final component")?,
                    )
                } else {
                    destination.clone()
                };
                self.providers
                    .resolve(&LocationDescriptor::Virtual(source.clone()))?
                    .download(source, &target, cancellation)
            }
            (LocationDescriptor::Virtual(source), LocationDescriptor::Virtual(destination)) => {
                let staging = tempfile::Builder::new()
                    .prefix("superexplorer-remote-transfer-")
                    .tempdir()
                    .context("create scoped transfer staging")?;
                let name = source
                    .components
                    .last()
                    .context("remote source has no final component")?;
                let staged = staging.path().join(name);
                self.providers
                    .resolve(&LocationDescriptor::Virtual(source.clone()))?
                    .download(source, &staged, cancellation)?;
                if cancellation.is_cancelled() {
                    bail!("transfer cancelled");
                }
                self.providers
                    .resolve(&LocationDescriptor::Virtual(destination.clone()))?
                    .upload(&staged, destination, cancellation)
            }
            _ => bail!("unsupported Shell location in remote transfer"),
        }
    }

    fn delete_source(
        &self,
        source: &LocationDescriptor,
        cancellation: &CancellationToken,
    ) -> Result<()> {
        match source {
            LocationDescriptor::FileSystem(path) if path.is_dir() => fs::remove_dir_all(path)
                .with_context(|| format!("remove moved directory {}", path.display())),
            LocationDescriptor::FileSystem(path) => fs::remove_file(path)
                .with_context(|| format!("remove moved file {}", path.display())),
            LocationDescriptor::Virtual(location) => {
                self.providers
                    .resolve(source)?
                    .delete(location, true, cancellation)
            }
            _ => bail!("unsupported Shell source in remote transfer"),
        }
    }
}

fn copy_local(source: &Path, destination: &Path) -> Result<()> {
    if source.is_dir() {
        bail!("local directory copy remains owned by the Windows Shell");
    }
    let target = if destination.is_dir() {
        destination.join(source.file_name().context("source has no file name")?)
    } else {
        PathBuf::from(destination)
    };
    fs::copy(source, &target)
        .with_context(|| format!("copy {} to {}", source.display(), target.display()))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::*;
    use crate::{RemoteEntry, RemoteProvider};
    use explorer_model::VirtualLocationDescriptor;

    struct FakeProvider {
        fail_delete: bool,
    }

    impl RemoteProvider for FakeProvider {
        fn provider_id(&self) -> &'static str {
            "fake"
        }
        fn list(
            &self,
            _: &VirtualLocationDescriptor,
            _: &CancellationToken,
        ) -> Result<Vec<RemoteEntry>> {
            Ok(Vec::new())
        }
        fn download(
            &self,
            _: &VirtualLocationDescriptor,
            local: &Path,
            _: &CancellationToken,
        ) -> Result<()> {
            fs::write(local, b"remote")?;
            Ok(())
        }
        fn upload(
            &self,
            local: &Path,
            _: &VirtualLocationDescriptor,
            _: &CancellationToken,
        ) -> Result<()> {
            let _ = fs::read(local)?;
            Ok(())
        }
        fn create_directory(
            &self,
            _: &VirtualLocationDescriptor,
            _: &CancellationToken,
        ) -> Result<()> {
            Ok(())
        }
        fn rename(
            &self,
            _: &VirtualLocationDescriptor,
            _: &VirtualLocationDescriptor,
            _: &CancellationToken,
        ) -> Result<()> {
            Ok(())
        }
        fn delete(
            &self,
            _: &VirtualLocationDescriptor,
            _: bool,
            _: &CancellationToken,
        ) -> Result<()> {
            if self.fail_delete {
                bail!("fixture delete failure")
            } else {
                Ok(())
            }
        }
    }

    fn remote(name: &str) -> LocationDescriptor {
        LocationDescriptor::try_virtual("fake", [1; 16], 1, None, vec![name.into()]).unwrap()
    }

    #[test]
    fn remote_to_remote_copy_uses_scoped_staging() {
        let mut registry = RemoteProviderRegistry::default();
        registry
            .register(Arc::new(FakeProvider { fail_delete: false }))
            .unwrap();
        let outcome = TransferEngine::new(&registry).transfer(
            remote("a"),
            remote("b"),
            TransferMode::Copy,
            &CancellationToken::new(),
        );
        assert_eq!(outcome.result, TransferResult::Succeeded);
    }

    #[test]
    fn move_reports_partial_when_source_delete_fails() {
        let mut registry = RemoteProviderRegistry::default();
        registry
            .register(Arc::new(FakeProvider { fail_delete: true }))
            .unwrap();
        let outcome = TransferEngine::new(&registry).transfer(
            remote("a"),
            remote("b"),
            TransferMode::Move,
            &CancellationToken::new(),
        );
        assert!(matches!(outcome.result, TransferResult::Partial { .. }));
    }
}
