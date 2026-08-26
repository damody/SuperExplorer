//! Password-authenticated SFTP provider with pinned SSH host keys.

use std::{
    collections::{HashMap, HashSet},
    path::Path,
    sync::{Arc, Mutex},
    time::Duration,
};

use anyhow::{Context as _, Result, bail};
use async_trait::async_trait;
use explorer_model::{
    CancellationToken, LocationDescriptor, SftpProfile, VirtualLocationDescriptor,
};
use russh::{
    Disconnect,
    client::{self, Config, Handle, Handler},
    keys::ssh_key::{HashAlg, PublicKey},
};
use russh_sftp::client::{SftpSession, error::Error as SftpError};
use russh_sftp::protocol::StatusCode;
use tokio::{
    io::{AsyncReadExt as _, AsyncWriteExt as _},
    runtime::{Builder, Runtime},
};

use crate::{RemoteEntry, RemoteEntryKind, RemoteProvider};

const MAX_SYMLINK_HOPS: usize = 40;

struct ProfileSecret(String);

struct RegisteredProfile {
    profile: SftpProfile,
    password: ProfileSecret,
}

struct PinnedHostKey {
    expected: String,
}

struct CaptureHostKey {
    observed: Arc<Mutex<Option<String>>>,
}

#[async_trait]
impl Handler for PinnedHostKey {
    type Error = russh::Error;

    async fn check_server_key(&mut self, key: &PublicKey) -> Result<bool, Self::Error> {
        Ok(key.fingerprint(HashAlg::Sha256).to_string() == self.expected)
    }
}

#[async_trait]
impl Handler for CaptureHostKey {
    type Error = russh::Error;

    async fn check_server_key(&mut self, key: &PublicKey) -> Result<bool, Self::Error> {
        *self
            .observed
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) =
            Some(key.fingerprint(HashAlg::Sha256).to_string());
        Ok(true)
    }
}

pub struct SftpProvider {
    runtime: Runtime,
    profiles: Mutex<HashMap<[u8; 16], Arc<RegisteredProfile>>>,
}

impl SftpProvider {
    pub fn new() -> Result<Self> {
        Ok(Self {
            runtime: Builder::new_multi_thread()
                .worker_threads(2)
                .enable_all()
                .build()
                .context("create SFTP runtime")?,
            profiles: Mutex::new(HashMap::new()),
        })
    }

    pub fn register_profile(&self, profile: SftpProfile, password: String) -> Result<()> {
        profile.validate().context("invalid SFTP profile")?;
        if password.is_empty() {
            bail!("SFTP password is empty");
        }
        self.profiles
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .insert(
                profile.container_identity,
                Arc::new(RegisteredProfile {
                    profile,
                    password: ProfileSecret(password),
                }),
            );
        Ok(())
    }

    pub fn remove_profile(&self, identity: [u8; 16]) {
        self.profiles
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .remove(&identity);
    }

    /// Opens only the SSH transport long enough to obtain the presented host fingerprint.
    /// The caller must show/approve it before storing it in an SFTP profile.
    pub fn probe_host_key(&self, host: &str, port: u16) -> Result<String> {
        if host.is_empty() || port == 0 {
            bail!("SFTP host or port is invalid");
        }
        let observed = Arc::new(Mutex::new(None));
        let capture = Arc::clone(&observed);
        self.runtime.block_on(async {
            let config = Arc::new(Config {
                inactivity_timeout: Some(Duration::from_secs(15)),
                ..Default::default()
            });
            let session =
                client::connect(config, (host, port), CaptureHostKey { observed: capture })
                    .await
                    .context("probe SSH host key")?;
            let _ = session
                .disconnect(Disconnect::ByApplication, "", "en")
                .await;
            Ok::<_, anyhow::Error>(())
        })?;
        let fingerprint = observed
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone()
            .context("SSH server did not present a host key")?;
        Ok(fingerprint)
    }

    fn profile(&self, location: &VirtualLocationDescriptor) -> Result<Arc<RegisteredProfile>> {
        self.profiles
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .get(&location.container_identity)
            .cloned()
            .context("SFTP profile is not registered")
    }

    async fn connect(profile: &RegisteredProfile) -> Result<(Handle<PinnedHostKey>, SftpSession)> {
        let expected = profile
            .profile
            .host_key_fingerprint
            .clone()
            .context("SFTP host key must be trusted before connecting")?;
        let config = Arc::new(Config {
            inactivity_timeout: Some(Duration::from_secs(300)),
            ..Default::default()
        });
        let address = (profile.profile.host.as_str(), profile.profile.port);
        let mut session = client::connect(config, address, PinnedHostKey { expected })
            .await
            .context("connect SSH transport")?;
        let authenticated = session
            .authenticate_password(&profile.profile.username, &profile.password.0)
            .await
            .context("authenticate SFTP password")?;
        if !authenticated {
            bail!("SFTP authentication failed");
        }
        let channel = session
            .channel_open_session()
            .await
            .context("open SSH session channel")?;
        channel
            .request_subsystem(true, "sftp")
            .await
            .context("request SFTP subsystem")?;
        let sftp = SftpSession::new(channel.into_stream())
            .await
            .context("start SFTP session")?;
        Ok((session, sftp))
    }

    async fn disconnect(session: Handle<PinnedHostKey>) {
        let _ = session
            .disconnect(Disconnect::ByApplication, "", "en")
            .await;
    }
}

impl RemoteProvider for SftpProvider {
    fn provider_id(&self) -> &'static str {
        "sftp"
    }

    fn list(
        &self,
        location: &VirtualLocationDescriptor,
        cancellation: &CancellationToken,
    ) -> Result<Vec<RemoteEntry>> {
        let profile = self.profile(location)?;
        let remote = remote_path(location);
        self.runtime.block_on(async {
            if cancellation.is_cancelled() {
                bail!("SFTP operation cancelled");
            }
            let (session, sftp) = Self::connect(&profile).await?;
            let entries = sftp
                .read_dir(&remote)
                .await
                .context("list SFTP directory")?;
            let mut output = Vec::new();
            for entry in entries {
                if cancellation.is_cancelled() {
                    Self::disconnect(session).await;
                    bail!("SFTP operation cancelled");
                }
                let name = entry.file_name();
                if matches!(name.as_str(), "." | "..") {
                    continue;
                }
                let mut child = location.clone();
                child.components.push(name.clone());
                child.entry_id = None;
                let file_type = entry.file_type();
                let kind = if file_type.is_symlink() {
                    resolve_sftp_symlink(&sftp, &entry.path(), cancellation).await?
                } else if file_type.is_dir() {
                    RemoteEntryKind::Directory
                } else {
                    RemoteEntryKind::File
                };
                output.push(RemoteEntry {
                    name,
                    location: LocationDescriptor::Virtual(child),
                    kind,
                    size: entry.metadata().size,
                });
            }
            Self::disconnect(session).await;
            Ok(output)
        })
    }

    fn download(
        &self,
        source: &VirtualLocationDescriptor,
        local_destination: &Path,
        cancellation: &CancellationToken,
    ) -> Result<()> {
        let profile = self.profile(source)?;
        let remote = remote_path(source);
        let local = local_destination.to_path_buf();
        self.runtime.block_on(async {
            let (session, sftp) = Self::connect(&profile).await?;
            if let Some(parent) = local.parent() {
                tokio::fs::create_dir_all(parent).await?;
            }
            let mut input = sftp.open(&remote).await.context("open SFTP source")?;
            let mut output = tokio::fs::File::create(&local)
                .await
                .context("create local destination")?;
            let mut buffer = vec![0; 64 * 1024];
            loop {
                if cancellation.is_cancelled() {
                    Self::disconnect(session).await;
                    bail!("SFTP download cancelled");
                }
                let read = input.read(&mut buffer).await.context("read SFTP source")?;
                if read == 0 {
                    break;
                }
                output
                    .write_all(&buffer[..read])
                    .await
                    .context("write local destination")?;
            }
            output.flush().await?;
            Self::disconnect(session).await;
            Ok(())
        })
    }

    fn upload(
        &self,
        local_source: &Path,
        destination: &VirtualLocationDescriptor,
        cancellation: &CancellationToken,
    ) -> Result<()> {
        let profile = self.profile(destination)?;
        let mut remote = remote_path(destination);
        if let Some(name) = local_source.file_name().and_then(|name| name.to_str()) {
            if remote.ends_with('/') {
                remote.push_str(name);
            } else {
                remote.push('/');
                remote.push_str(name);
            }
        }
        let local = local_source.to_path_buf();
        self.runtime.block_on(async {
            let (session, sftp) = Self::connect(&profile).await?;
            let mut input = tokio::fs::File::open(&local)
                .await
                .context("open local upload source")?;
            let mut output = sftp
                .create(&remote)
                .await
                .context("create SFTP destination")?;
            let mut buffer = vec![0; 64 * 1024];
            loop {
                if cancellation.is_cancelled() {
                    Self::disconnect(session).await;
                    bail!("SFTP upload cancelled");
                }
                let read = input
                    .read(&mut buffer)
                    .await
                    .context("read local upload source")?;
                if read == 0 {
                    break;
                }
                output
                    .write_all(&buffer[..read])
                    .await
                    .context("write SFTP destination")?;
            }
            output.shutdown().await.context("flush SFTP destination")?;
            Self::disconnect(session).await;
            Ok(())
        })
    }

    fn create_directory(
        &self,
        location: &VirtualLocationDescriptor,
        cancellation: &CancellationToken,
    ) -> Result<()> {
        let profile = self.profile(location)?;
        let remote = remote_path(location);
        self.runtime.block_on(async {
            if cancellation.is_cancelled() {
                bail!("SFTP operation cancelled");
            }
            let (session, sftp) = Self::connect(&profile).await?;
            sftp.create_dir(&remote)
                .await
                .context("create SFTP directory")?;
            Self::disconnect(session).await;
            Ok(())
        })
    }

    fn rename(
        &self,
        source: &VirtualLocationDescriptor,
        destination: &VirtualLocationDescriptor,
        cancellation: &CancellationToken,
    ) -> Result<()> {
        if source.container_identity != destination.container_identity {
            bail!("SFTP rename cannot cross profiles");
        }
        let profile = self.profile(source)?;
        let old = remote_path(source);
        let new = remote_path(destination);
        self.runtime.block_on(async {
            if cancellation.is_cancelled() {
                bail!("SFTP operation cancelled");
            }
            let (session, sftp) = Self::connect(&profile).await?;
            sftp.rename(&old, &new).await.context("rename SFTP item")?;
            Self::disconnect(session).await;
            Ok(())
        })
    }

    fn delete(
        &self,
        location: &VirtualLocationDescriptor,
        recursive: bool,
        cancellation: &CancellationToken,
    ) -> Result<()> {
        let profile = self.profile(location)?;
        let remote = remote_path(location);
        self.runtime.block_on(async {
            if cancellation.is_cancelled() {
                bail!("SFTP operation cancelled");
            }
            let (session, sftp) = Self::connect(&profile).await?;
            let metadata = sftp.metadata(&remote).await.context("inspect SFTP item")?;
            if metadata.is_dir() {
                if recursive {
                    remove_tree(&sftp, &remote, cancellation).await?;
                } else {
                    sftp.remove_dir(&remote)
                        .await
                        .context("remove SFTP directory")?;
                }
            } else {
                sftp.remove_file(&remote)
                    .await
                    .context("remove SFTP file")?;
            }
            Self::disconnect(session).await;
            Ok(())
        })
    }
}

async fn resolve_sftp_symlink(
    sftp: &SftpSession,
    link_path: &str,
    cancellation: &CancellationToken,
) -> Result<RemoteEntryKind> {
    let mut current =
        normalize_sftp_path(link_path, None).context("SFTP symbolic-link path is invalid")?;
    let mut visited = HashSet::new();

    for hop in 0..=MAX_SYMLINK_HOPS {
        ensure_sftp_not_cancelled(cancellation)?;
        if !visited.insert(current.clone()) {
            return Ok(RemoteEntryKind::CircularSymlink);
        }
        let metadata = match sftp.symlink_metadata(&current).await {
            Ok(metadata) => metadata,
            Err(error) if unresolved_sftp_target(&error) => {
                return Ok(RemoteEntryKind::BrokenSymlink);
            }
            Err(error) => return Err(error).context("inspect SFTP symbolic link"),
        };
        if metadata.is_dir() {
            return Ok(RemoteEntryKind::DirectorySymlink);
        }
        if !metadata.is_symlink() {
            return Ok(RemoteEntryKind::FileSymlink);
        }
        if hop == MAX_SYMLINK_HOPS {
            return Ok(RemoteEntryKind::CircularSymlink);
        }
        let target = match sftp.read_link(&current).await {
            Ok(target) => target,
            Err(error) if unresolved_sftp_target(&error) => {
                return Ok(RemoteEntryKind::BrokenSymlink);
            }
            Err(error) => return Err(error).context("read SFTP symbolic link"),
        };
        current = match next_sftp_link_path(&current, &target, &visited) {
            Ok(next) => next,
            Err(kind) => return Ok(kind),
        };
    }

    unreachable!("bounded SFTP symbolic-link loop always returns")
}

fn ensure_sftp_not_cancelled(cancellation: &CancellationToken) -> Result<()> {
    if cancellation.is_cancelled() {
        bail!("SFTP operation cancelled");
    }
    Ok(())
}

fn unresolved_sftp_target(error: &SftpError) -> bool {
    matches!(
        error,
        SftpError::Status(status)
            if matches!(
                status.status_code,
                StatusCode::NoSuchFile | StatusCode::PermissionDenied | StatusCode::Failure
            )
    )
}

fn next_sftp_link_path(
    current: &str,
    target: &str,
    visited: &HashSet<String>,
) -> std::result::Result<String, RemoteEntryKind> {
    let Some(next) = normalize_sftp_path(current, Some(target)) else {
        return Err(RemoteEntryKind::BrokenSymlink);
    };
    if visited.contains(&next) {
        Err(RemoteEntryKind::CircularSymlink)
    } else {
        Ok(next)
    }
}

fn normalize_sftp_path(link_path: &str, target: Option<&str>) -> Option<String> {
    if link_path.contains(['\0', '\r', '\n']) || !link_path.starts_with('/') {
        return None;
    }
    let candidate = match target {
        None => link_path.to_owned(),
        Some(target) if target.contains(['\0', '\r', '\n']) || target.is_empty() => return None,
        Some(target) if target.starts_with('/') => target.to_owned(),
        Some(target) => {
            let parent = link_path.rsplit_once('/').map_or("/", |(parent, _)| parent);
            format!("{parent}/{target}")
        }
    };
    let mut components = Vec::new();
    for component in candidate.split('/') {
        match component {
            "" | "." => {}
            ".." => {
                components.pop();
            }
            component => components.push(component),
        }
    }
    Some(format!("/{}", components.join("/")))
}

async fn remove_tree(
    sftp: &SftpSession,
    root: &str,
    cancellation: &CancellationToken,
) -> Result<()> {
    let mut directories = vec![root.to_owned()];
    let mut index = 0;
    while index < directories.len() {
        if cancellation.is_cancelled() {
            bail!("SFTP delete cancelled");
        }
        let current = directories[index].clone();
        index += 1;
        for entry in sftp
            .read_dir(&current)
            .await
            .context("enumerate SFTP delete tree")?
        {
            let name = entry.file_name();
            if matches!(name.as_str(), "." | "..") {
                continue;
            }
            let child = format!("{}/{}", current.trim_end_matches('/'), name);
            if entry.file_type().is_dir() {
                directories.push(child);
            } else {
                sftp.remove_file(&child).await.context("remove SFTP file")?;
            }
        }
    }
    for directory in directories.into_iter().rev() {
        sftp.remove_dir(&directory)
            .await
            .context("remove SFTP directory")?;
    }
    Ok(())
}

fn remote_path(location: &VirtualLocationDescriptor) -> String {
    if location.components.is_empty() {
        "/".to_owned()
    } else {
        format!("/{}", location.components.join("/"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use russh_sftp::protocol::Status;

    #[test]
    fn profile_secret_has_no_debug_representation() {
        assert!(!std::any::type_name::<ProfileSecret>().is_empty());
    }

    #[test]
    fn normalizes_relative_and_absolute_sftp_link_targets() {
        assert_eq!(
            normalize_sftp_path("/root/links/photos", Some("../media/./photos")),
            Some("/root/media/photos".to_owned())
        );
        assert_eq!(
            normalize_sftp_path("/root/links/photos", Some("/srv/photos")),
            Some("/srv/photos".to_owned())
        );
        assert_eq!(
            normalize_sftp_path("/root/links/photos", Some("../../../srv")),
            Some("/srv".to_owned())
        );
    }

    #[test]
    fn rejects_invalid_sftp_link_targets() {
        assert_eq!(normalize_sftp_path("relative/link", Some("target")), None);
        assert_eq!(normalize_sftp_path("/root/link", Some("")), None);
        assert_eq!(normalize_sftp_path("/root/link", Some("bad\nname")), None);
    }

    #[test]
    fn repeated_normalized_sftp_paths_are_detectable_as_cycles() {
        let start = normalize_sftp_path("/root/a", None).unwrap();
        let mut visited = HashSet::new();
        assert!(visited.insert(start.clone()));
        assert_eq!(
            next_sftp_link_path(&start, "../root/a", &visited),
            Err(RemoteEntryKind::CircularSymlink)
        );
        assert_eq!(MAX_SYMLINK_HOPS, 40);
    }

    #[test]
    fn sftp_resolution_distinguishes_broken_targets_from_transport_failures() {
        let status = |status_code| {
            SftpError::Status(Status {
                id: 1,
                status_code,
                error_message: String::new(),
                language_tag: String::new(),
            })
        };
        assert!(unresolved_sftp_target(&status(StatusCode::NoSuchFile)));
        assert!(unresolved_sftp_target(&status(
            StatusCode::PermissionDenied
        )));
        assert!(!unresolved_sftp_target(&status(StatusCode::ConnectionLost)));
        assert!(!unresolved_sftp_target(&SftpError::Timeout));
        assert_eq!(
            next_sftp_link_path("/root/link", "bad\nname", &HashSet::new()),
            Err(RemoteEntryKind::BrokenSymlink)
        );
    }

    #[test]
    fn sftp_resolution_honours_cancellation() {
        let cancellation = CancellationToken::new();
        assert!(ensure_sftp_not_cancelled(&cancellation).is_ok());
        cancellation.cancel();
        assert!(ensure_sftp_not_cancelled(&cancellation).is_err());
    }
}
