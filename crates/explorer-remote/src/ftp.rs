//! Native FTP / Explicit FTPS provider. Implicit FTPS is rejected.

use std::{
    collections::HashMap,
    io::{Read, Write},
    net::{Shutdown, SocketAddr, TcpStream, ToSocketAddrs},
    path::Path,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

use anyhow::{Context as _, Result, bail};
use explorer_model::{
    CancellationToken, FtpAuthKind, FtpDataMode, FtpProfile, FtpSecurityMode, LocationDescriptor,
    RemoteProviderCapabilities, VirtualLocationDescriptor,
};
use sha2::{Digest, Sha256};

use crate::{
    RemoteEntry, RemoteEntryKind, RemoteMetadata, RemoteProvider,
    ftp_protocol::{
        command_is_safe, ftp_path, parse_epsv, parse_list_line, parse_mlsd_line, parse_pasv,
        parse_reply_block, rewrite_pasv_peer, sanitize_ftp_display,
    },
    provider::validate_remote_location,
};

const CONNECT_TIMEOUT: Duration = Duration::from_secs(15);
const COMMAND_TIMEOUT: Duration = Duration::from_secs(30);
const DATA_TIMEOUT: Duration = Duration::from_secs(300);

struct ProfileSecret(String);

impl std::fmt::Debug for ProfileSecret {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("ProfileSecret(*)")
    }
}

struct RegisteredProfile {
    profile: FtpProfile,
    password: Option<ProfileSecret>,
}

enum ControlStream {
    Plain(TcpStream),
    Tls(native_tls::TlsStream<TcpStream>),
}

impl ControlStream {
    fn set_read_timeout(&self, timeout: Option<Duration>) -> std::io::Result<()> {
        match self {
            Self::Plain(stream) => stream.set_read_timeout(timeout),
            Self::Tls(stream) => stream.get_ref().set_read_timeout(timeout),
        }
    }

    fn set_write_timeout(&self, timeout: Option<Duration>) -> std::io::Result<()> {
        match self {
            Self::Plain(stream) => stream.set_write_timeout(timeout),
            Self::Tls(stream) => stream.get_ref().set_write_timeout(timeout),
        }
    }

    #[allow(dead_code)]
    fn peer_addr(&self) -> std::io::Result<SocketAddr> {
        match self {
            Self::Plain(stream) => stream.peer_addr(),
            Self::Tls(stream) => stream.get_ref().peer_addr(),
        }
    }

    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        match self {
            Self::Plain(stream) => stream.read(buf),
            Self::Tls(stream) => stream.read(buf),
        }
    }

    fn write_all(&mut self, buf: &[u8]) -> std::io::Result<()> {
        match self {
            Self::Plain(stream) => stream.write_all(buf),
            Self::Tls(stream) => stream.write_all(buf),
        }
    }
}

struct FtpControl {
    stream: ControlStream,
    leftover: Vec<u8>,
    peer: SocketAddr,
    supports_mlsd: bool,
    supports_utf8: bool,
    #[allow(dead_code)]
    atomic_rename: bool,
}

impl FtpControl {
    fn connect(profile: &RegisteredProfile) -> Result<Self> {
        if profile.profile.security_mode == FtpSecurityMode::ImplicitTls {
            bail!("Implicit FTPS is not supported");
        }
        if profile.profile.data_mode == FtpDataMode::Active {
            bail!("Active FTP mode is reserved for a later connection path");
        }
        let addr = (profile.profile.host.as_str(), profile.profile.port)
            .to_socket_addrs()
            .context("resolve FTP host")?
            .next()
            .context("FTP host did not resolve")?;
        let tcp =
            TcpStream::connect_timeout(&addr, CONNECT_TIMEOUT).context("connect FTP control")?;
        tcp.set_nodelay(true).ok();
        let mut control = Self {
            peer: tcp.peer_addr().context("FTP peer address")?,
            stream: ControlStream::Plain(tcp),
            leftover: Vec::new(),
            supports_mlsd: false,
            supports_utf8: false,
            atomic_rename: true,
        };
        control.stream.set_read_timeout(Some(COMMAND_TIMEOUT))?;
        control.stream.set_write_timeout(Some(COMMAND_TIMEOUT))?;
        let greeting = control.read_reply()?;
        if greeting.code / 100 != 2 {
            bail!("FTP server rejected the connection");
        }
        if profile.profile.security_mode == FtpSecurityMode::ExplicitTls {
            control.upgrade_explicit_tls(&profile.profile)?;
        }
        let username = if profile.profile.auth_kind == FtpAuthKind::Anonymous {
            "anonymous"
        } else {
            profile.profile.username.as_str()
        };
        let password = match profile.profile.auth_kind {
            FtpAuthKind::Anonymous => "ftp@localhost",
            FtpAuthKind::Password => profile
                .password
                .as_ref()
                .map(|secret| secret.0.as_str())
                .context("FTP password is missing")?,
        };
        control.login(username, password)?;
        if let Ok(feat) = control.command("FEAT")
            && feat.code / 100 == 2
        {
            let features = feat.text.to_ascii_uppercase();
            control.supports_mlsd = features.contains("MLSD");
            control.supports_utf8 = features.contains("UTF8");
        }
        if control.supports_utf8 {
            let _ = control.command("OPTS UTF8 ON");
        }
        let _ = control.command("TYPE I");
        if let Some(directory) = &profile.profile.initial_directory {
            control.command(&format!("CWD {directory}"))?;
        }
        Ok(control)
    }

    fn upgrade_explicit_tls(&mut self, profile: &FtpProfile) -> Result<()> {
        let reply = self.command("AUTH TLS")?;
        if reply.code / 100 != 2 {
            bail!("FTP server does not support Explicit FTPS");
        }
        let ControlStream::Plain(stream) = &self.stream else {
            bail!("FTP control stream is already encrypted");
        };
        let stream = stream.try_clone().context("clone FTP stream for TLS")?;
        let connector = native_tls::TlsConnector::builder()
            .danger_accept_invalid_certs(profile.tls_fingerprint.is_some())
            .build()
            .context("build TLS connector")?;
        let tls = connector
            .connect(&profile.host, stream)
            .context("upgrade FTP control to TLS")?;
        if let Some(expected) = &profile.tls_fingerprint {
            let der = tls
                .peer_certificate()
                .context("read FTPS certificate")?
                .context("FTPS server presented no certificate")?
                .to_der()
                .context("encode FTPS certificate")?;
            let actual = format!("{:x}", Sha256::digest(&der));
            if actual != expected.to_ascii_lowercase() {
                bail!("The FTPS certificate changed; login was blocked.");
            }
        }
        self.stream = ControlStream::Tls(tls);
        self.stream.set_read_timeout(Some(COMMAND_TIMEOUT))?;
        self.stream.set_write_timeout(Some(COMMAND_TIMEOUT))?;
        let pbsz = self.command("PBSZ 0")?;
        if pbsz.code / 100 != 2 {
            bail!("FTP PBSZ failed");
        }
        let prot = self.command("PROT P")?;
        if prot.code / 100 != 2 {
            bail!("FTP PROT failed");
        }
        Ok(())
    }

    fn login(&mut self, username: &str, password: &str) -> Result<()> {
        let user = self.command(&format!("USER {username}"))?;
        if user.code == 230 {
            return Ok(());
        }
        if user.code != 331 {
            bail!("FTP authentication failed");
        }
        let pass = self.command(&format!("PASS {password}"))?;
        if pass.code / 100 != 2 {
            bail!("FTP authentication failed");
        }
        Ok(())
    }

    fn command(&mut self, command: &str) -> Result<crate::ftp_protocol::FtpReply> {
        if !command_is_safe(command) {
            bail!("FTP command contains a forbidden newline");
        }
        self.stream.write_all(format!("{command}\r\n").as_bytes())?;
        self.read_reply()
    }

    fn read_reply(&mut self) -> Result<crate::ftp_protocol::FtpReply> {
        let mut collected = String::new();
        let mut buf = [0_u8; 1024];
        let started = Instant::now();
        loop {
            if started.elapsed() > COMMAND_TIMEOUT {
                bail!("FTP command timed out");
            }
            if let Some(index) = find_complete_reply(&self.leftover) {
                let block = self.leftover.drain(..=index).collect::<Vec<_>>();
                collected.push_str(&String::from_utf8_lossy(&block));
                return parse_reply_block(&collected);
            }
            let read = self.stream.read(&mut buf)?;
            if read == 0 {
                bail!("FTP control connection closed");
            }
            self.leftover.extend_from_slice(&buf[..read]);
            if self.leftover.len() > 64 * 1024 {
                bail!("FTP reply is too large");
            }
        }
    }

    fn open_data(&mut self, cancellation: &CancellationToken) -> Result<TcpStream> {
        ensure_not_cancelled(cancellation)?;
        if let Ok(reply) = self.command("EPSV")
            && reply.code == 229
        {
            let port = parse_epsv(&reply.text)?;
            let addr = SocketAddr::new(self.peer.ip(), port);
            let data = TcpStream::connect_timeout(&addr, CONNECT_TIMEOUT)?;
            data.set_read_timeout(Some(DATA_TIMEOUT))?;
            data.set_write_timeout(Some(DATA_TIMEOUT))?;
            return Ok(data);
        }
        let reply = self.command("PASV")?;
        if reply.code != 227 {
            bail!("FTP server did not enter passive mode");
        }
        let (ip, port) = parse_pasv(&reply.text)?;
        let addr = rewrite_pasv_peer(self.peer, ip, port);
        let data = TcpStream::connect_timeout(&addr, CONNECT_TIMEOUT)?;
        data.set_read_timeout(Some(DATA_TIMEOUT))?;
        data.set_write_timeout(Some(DATA_TIMEOUT))?;
        Ok(data)
    }
}

fn find_complete_reply(buffer: &[u8]) -> Option<usize> {
    let text = std::str::from_utf8(buffer).ok()?;
    let mut offset = 0;
    for line in text.split_inclusive(['\n']) {
        offset += line.len();
        let trimmed = line.trim_end_matches(['\r', '\n']);
        if trimmed.len() >= 4
            && trimmed.as_bytes()[0].is_ascii_digit()
            && trimmed.as_bytes()[3] == b' '
        {
            return Some(offset - 1);
        }
    }
    None
}

fn ensure_not_cancelled(cancellation: &CancellationToken) -> Result<()> {
    if cancellation.is_cancelled() {
        bail!("FTP operation cancelled");
    }
    Ok(())
}

pub struct FtpProvider {
    profiles: Mutex<HashMap<[u8; 16], Arc<RegisteredProfile>>>,
}

impl FtpProvider {
    pub fn new() -> Result<Self> {
        Ok(Self {
            profiles: Mutex::new(HashMap::new()),
        })
    }

    pub fn register_profile(&self, profile: FtpProfile, password: Option<String>) -> Result<()> {
        profile.validate().context("invalid FTP profile")?;
        if profile.auth_kind == FtpAuthKind::Password
            && password.as_ref().is_none_or(String::is_empty)
        {
            bail!("FTP password is empty");
        }
        self.profiles
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .insert(
                profile.container_identity,
                Arc::new(RegisteredProfile {
                    profile,
                    password: password.map(ProfileSecret),
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

    fn profile(&self, location: &VirtualLocationDescriptor) -> Result<Arc<RegisteredProfile>> {
        let profile = self
            .profiles
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .get(&location.container_identity)
            .cloned()
            .context("FTP profile is not registered")?;
        if location.public_authority.as_deref() != Some(profile.profile.public_identity()) {
            bail!("FTP location authority does not match the registered profile");
        }
        Ok(profile)
    }

    fn with_control<T>(
        &self,
        location: &VirtualLocationDescriptor,
        cancellation: &CancellationToken,
        body: impl FnOnce(&mut FtpControl) -> Result<T>,
    ) -> Result<T> {
        ensure_not_cancelled(cancellation)?;
        let profile = self.profile(location)?;
        let mut control = FtpControl::connect(&profile)?;
        let result = body(&mut control);
        let _ = control.command("QUIT");
        result
    }
}

impl RemoteProvider for FtpProvider {
    fn provider_id(&self) -> &'static str {
        "ftp"
    }

    fn capabilities(&self) -> RemoteProviderCapabilities {
        RemoteProviderCapabilities::ftp_defaults()
    }

    fn list(
        &self,
        location: &VirtualLocationDescriptor,
        cancellation: &CancellationToken,
    ) -> Result<Vec<RemoteEntry>> {
        validate_remote_location(location, "ftp", true)?;
        let path = ftp_path(&location.components)?;
        self.with_control(location, cancellation, |control| {
            if path != "/" {
                control.command(&format!("CWD {path}"))?;
            }
            let mut data = control.open_data(cancellation)?;
            let listing_command = if control.supports_mlsd {
                "MLSD"
            } else {
                "LIST"
            };
            let start = control.command(listing_command)?;
            if start.code / 100 != 1 {
                bail!("{}", sanitize_ftp_display(&start.text));
            }
            let mut bytes = Vec::new();
            let mut buf = [0_u8; 8192];
            loop {
                ensure_not_cancelled(cancellation)?;
                let read = data.read(&mut buf)?;
                if read == 0 {
                    break;
                }
                bytes.extend_from_slice(&buf[..read]);
                if bytes.len() > 8 * 1024 * 1024 {
                    bail!("FTP listing is too large");
                }
            }
            let done = control.read_reply()?;
            if done.code / 100 != 2 {
                bail!("FTP listing failed");
            }
            let text = String::from_utf8_lossy(&bytes);
            let mut output = Vec::new();
            for line in text.lines() {
                let parsed = if control.supports_mlsd {
                    parse_mlsd_line(line)?
                } else {
                    parse_list_line(line)
                };
                let Some(entry) = parsed else {
                    continue;
                };
                let mut child = location.clone();
                child.components.push(entry.name.clone());
                child.entry_id = None;
                output.push(RemoteEntry {
                    name: entry.name,
                    location: LocationDescriptor::Virtual(child),
                    kind: if entry.is_directory {
                        RemoteEntryKind::Directory
                    } else if entry.is_symlink {
                        RemoteEntryKind::FileSymlink
                    } else {
                        RemoteEntryKind::File
                    },
                    size: entry.size,
                    unix_mode: entry.unix_mode,
                });
            }
            Ok(output)
        })
    }

    fn download(
        &self,
        source: &VirtualLocationDescriptor,
        local_destination: &Path,
        cancellation: &CancellationToken,
    ) -> Result<()> {
        validate_remote_location(source, "ftp", false)?;
        let remote = ftp_path(&source.components)?;
        self.with_control(source, cancellation, |control| {
            download_path(control, &remote, local_destination, cancellation)
        })
    }

    fn upload(
        &self,
        local_source: &Path,
        destination: &VirtualLocationDescriptor,
        cancellation: &CancellationToken,
    ) -> Result<()> {
        let remote = ftp_upload_remote_path(destination, local_source)?;
        self.with_control(destination, cancellation, |control| {
            upload_path(control, local_source, &remote, cancellation)
        })
    }

    fn create_directory(
        &self,
        location: &VirtualLocationDescriptor,
        cancellation: &CancellationToken,
    ) -> Result<()> {
        validate_remote_location(location, "ftp", false)?;
        let remote = ftp_path(&location.components)?;
        self.with_control(location, cancellation, |control| {
            let reply = control.command(&format!("MKD {remote}"))?;
            if reply.code / 100 != 2 {
                bail!("{}", sanitize_ftp_display(&reply.text));
            }
            Ok(())
        })
    }

    fn metadata(
        &self,
        location: &VirtualLocationDescriptor,
        cancellation: &CancellationToken,
    ) -> Result<RemoteMetadata> {
        validate_remote_location(location, "ftp", true)?;
        if location.components.is_empty() {
            return Ok(RemoteMetadata {
                location: LocationDescriptor::Virtual(location.clone()),
                kind: RemoteEntryKind::Directory,
                size: None,
                unix_mode: None,
                modified_unix_seconds: None,
            });
        }
        let mut parent = location.clone();
        let name = parent.components.pop().unwrap_or_default();
        let entries = self.list(&parent, cancellation)?;
        entries
            .into_iter()
            .find(|entry| entry.name == name)
            .map(|entry| RemoteMetadata {
                location: entry.location,
                kind: entry.kind,
                size: entry.size,
                unix_mode: entry.unix_mode,
                modified_unix_seconds: None,
            })
            .context("FTP item metadata is unavailable")
    }

    fn rename(
        &self,
        source: &VirtualLocationDescriptor,
        destination: &VirtualLocationDescriptor,
        cancellation: &CancellationToken,
    ) -> Result<()> {
        validate_remote_location(source, "ftp", false)?;
        validate_remote_location(destination, "ftp", false)?;
        let from = ftp_path(&source.components)?;
        let to = ftp_path(&destination.components)?;
        self.with_control(source, cancellation, |control| {
            let ready = control.command(&format!("RNFR {from}"))?;
            if ready.code != 350 {
                bail!("{}", sanitize_ftp_display(&ready.text));
            }
            let done = control.command(&format!("RNTO {to}"))?;
            if done.code / 100 != 2 {
                bail!("{}", sanitize_ftp_display(&done.text));
            }
            Ok(())
        })
    }

    fn delete(
        &self,
        location: &VirtualLocationDescriptor,
        recursive: bool,
        cancellation: &CancellationToken,
    ) -> Result<()> {
        validate_remote_location(location, "ftp", false)?;
        let remote = ftp_path(&location.components)?;
        self.with_control(location, cancellation, |control| {
            delete_path(control, &remote, recursive, cancellation)
        })
    }
}

fn ftp_upload_remote_path(
    destination: &VirtualLocationDescriptor,
    local_source: &Path,
) -> Result<String> {
    // The transfer engine passes the destination folder, including the FTP account
    // home (`ftp://alias/` with no components). That home is the writable login
    // root, not an OS filesystem root, so uploads into it are allowed.
    validate_remote_location(destination, "ftp", true)?;
    let mut remote = ftp_path(&destination.components)?;
    if let Some(name) = local_source.file_name().and_then(|name| name.to_str()) {
        if remote.ends_with('/') {
            remote.push_str(name);
        } else {
            remote.push('/');
            remote.push_str(name);
        }
    }
    Ok(remote)
}

fn download_path(
    control: &mut FtpControl,
    remote: &str,
    local: &Path,
    cancellation: &CancellationToken,
) -> Result<()> {
    ensure_not_cancelled(cancellation)?;
    let mut data = control.open_data(cancellation)?;
    let start = control.command(&format!("RETR {remote}"))?;
    if start.code / 100 != 1 {
        let _ = data.shutdown(Shutdown::Both);
        bail!("{}", sanitize_ftp_display(&start.text));
    }
    if let Some(parent) = local.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let mut file = std::fs::File::create(local)?;
    let mut buf = [0_u8; 64 * 1024];
    loop {
        ensure_not_cancelled(cancellation)?;
        let read = data.read(&mut buf)?;
        if read == 0 {
            break;
        }
        file.write_all(&buf[..read])?;
    }
    let done = control.read_reply()?;
    if done.code / 100 != 2 {
        bail!("FTP download failed");
    }
    Ok(())
}

fn upload_path(
    control: &mut FtpControl,
    local: &Path,
    remote: &str,
    cancellation: &CancellationToken,
) -> Result<()> {
    ensure_not_cancelled(cancellation)?;
    if local.is_dir() {
        let mkdir = control.command(&format!("MKD {remote}"))?;
        if mkdir.code / 100 != 2 && mkdir.code != 550 {
            bail!("{}", sanitize_ftp_display(&mkdir.text));
        }
        for child in std::fs::read_dir(local)? {
            let child = child?;
            let name = child.file_name().to_string_lossy().into_owned();
            crate::provider::validate_windows_component(&name)?;
            upload_path(
                control,
                &child.path(),
                &format!("{remote}/{name}"),
                cancellation,
            )?;
        }
        return Ok(());
    }
    let file_name = Path::new(remote)
        .file_name()
        .and_then(|name| name.to_str())
        .context("FTP upload name is invalid")?;
    let parent = remote.rsplit_once('/').map_or("", |(parent, _)| parent);
    let temp = if parent.is_empty() {
        format!("/.{file_name}.superexplorer-upload")
    } else {
        format!("{parent}/.{file_name}.superexplorer-upload")
    };
    let mut input = std::fs::File::open(local)?;
    let mut data = control.open_data(cancellation)?;
    let start = control.command(&format!("STOR {temp}"))?;
    if start.code / 100 != 1 {
        let _ = data.shutdown(Shutdown::Both);
        bail!("{}", sanitize_ftp_display(&start.text));
    }
    let mut buf = [0_u8; 64 * 1024];
    loop {
        ensure_not_cancelled(cancellation)?;
        let read = input.read(&mut buf)?;
        if read == 0 {
            break;
        }
        data.write_all(&buf[..read])?;
    }
    let _ = data.shutdown(Shutdown::Both);
    let stored = control.read_reply()?;
    if stored.code / 100 != 2 {
        let _ = control.command(&format!("DELE {temp}"));
        bail!("FTP upload failed");
    }
    let ready = control.command(&format!("RNFR {temp}"))?;
    if ready.code == 350 {
        let renamed = control.command(&format!("RNTO {remote}"))?;
        if renamed.code / 100 != 2 {
            let _ = control.command(&format!("DELE {temp}"));
            bail!("FTP upload rename failed");
        }
        return Ok(());
    }
    let _ = control.command(&format!("DELE {temp}"));
    let mut data = control.open_data(cancellation)?;
    let mut input = std::fs::File::open(local)?;
    let start = control.command(&format!("STOR {remote}"))?;
    if start.code / 100 != 1 {
        bail!("FTP non-atomic upload failed");
    }
    loop {
        ensure_not_cancelled(cancellation)?;
        let read = input.read(&mut buf)?;
        if read == 0 {
            break;
        }
        data.write_all(&buf[..read])?;
    }
    let _ = data.shutdown(Shutdown::Both);
    let done = control.read_reply()?;
    if done.code / 100 != 2 {
        bail!("FTP non-atomic upload failed");
    }
    Ok(())
}

fn delete_path(
    control: &mut FtpControl,
    remote: &str,
    recursive: bool,
    cancellation: &CancellationToken,
) -> Result<()> {
    ensure_not_cancelled(cancellation)?;
    let deleted = control.command(&format!("DELE {remote}"))?;
    if deleted.code / 100 == 2 {
        return Ok(());
    }
    if !recursive {
        bail!("{}", sanitize_ftp_display(&deleted.text));
    }
    let removed = control.command(&format!("RMD {remote}"))?;
    if removed.code / 100 != 2 {
        bail!("{}", sanitize_ftp_display(&removed.text));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ftp_upload_into_account_root_targets_child_path() {
        let destination = VirtualLocationDescriptor {
            provider_id: "ftp".to_owned(),
            public_authority: Some("45.32.49.125".to_owned()),
            container_identity: [4; 16],
            container_generation: 1,
            entry_id: None,
            provider_entry_key: None,
            components: Vec::new(),
        };
        assert!(
            validate_remote_location(&destination, "ftp", false).is_err(),
            "mutating the FTP account root itself stays forbidden"
        );
        assert_eq!(
            ftp_upload_remote_path(&destination, Path::new(r"D:\SuperExplorer\README.md")).unwrap(),
            "/README.md"
        );
        let mut nested = destination.clone();
        nested.components.push("uploads".to_owned());
        assert_eq!(
            ftp_upload_remote_path(&nested, Path::new(r"D:\SuperExplorer\an.txt")).unwrap(),
            "/uploads/an.txt"
        );
    }

    #[test]
    fn secret_debug_does_not_contain_password() {
        let secret = ProfileSecret("hunter2".into());
        assert!(!format!("{secret:?}").contains("hunter2"));
    }

    #[test]
    fn implicit_ftps_is_rejected() {
        let profile = FtpProfile::new("127.0.0.1".into(), 21, "test".into(), [3; 16]).unwrap();
        let mut profile = profile;
        profile.security_mode = FtpSecurityMode::ImplicitTls;
        let registered = RegisteredProfile {
            profile,
            password: Some(ProfileSecret("secret".into())),
        };
        let error = FtpControl::connect(&registered)
            .err()
            .expect("implicit FTPS must fail")
            .to_string();
        assert!(error.contains("Implicit FTPS"));
        assert!(!error.contains("secret"));
    }

    #[test]
    fn live_vsftpd_password_login_can_list_home() {
        let Ok(password) = std::env::var("SUPEREXPLORER_FTP_SMOKE_PASSWORD") else {
            return;
        };
        let identity = [4; 16];
        let profile = FtpProfile::new("45.32.49.125".into(), 21, "test".into(), identity).unwrap();
        let provider = FtpProvider::new().unwrap();
        provider.register_profile(profile, Some(password)).unwrap();
        let location = explorer_model::RemoteAddress::parse("ftp://45.32.49.125/")
            .unwrap()
            .to_location(identity, 1)
            .unwrap();
        let LocationDescriptor::Virtual(remote) = location else {
            panic!("expected virtual FTP location");
        };
        let entries = provider
            .list(&remote, &CancellationToken::new())
            .expect("list after password login");
        assert!(
            entries
                .iter()
                .all(|entry| !entry.name.contains("secret") && !entry.name.contains("test7777"))
        );
    }
}
