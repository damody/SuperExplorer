//! Remote address parsing without credentials or platform I/O.

use std::fmt;

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{LocationDescriptor, LocationDescriptorValidationError};

/// Supported remote filesystem families.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RemoteProviderKind {
    Adb,
    Sftp,
    Ftp,
    Gdrive,
}

impl RemoteProviderKind {
    pub const fn provider_id(self) -> &'static str {
        match self {
            Self::Adb => "adb",
            Self::Sftp => "sftp",
            Self::Ftp => "ftp",
            Self::Gdrive => "gdrive",
        }
    }
}

pub fn is_remote_provider_id(provider_id: &str) -> bool {
    matches!(provider_id, "adb" | "sftp" | "ftp" | "gdrive")
}

/// Synthetic navigation target that starts Google Drive OAuth instead of listing a folder.
pub const GDRIVE_CONNECT_LOCATION: &str = "super-explorer:gdrive-connect";

pub fn is_gdrive_connect_location(location: &LocationDescriptor) -> bool {
    matches!(
        location,
        LocationDescriptor::ParsingName(value) if value == GDRIVE_CONNECT_LOCATION
    )
}

/// A parsed remote address whose authority is an ADB serial or non-secret SFTP profile alias.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RemoteAddress {
    pub provider: RemoteProviderKind,
    pub authority: String,
    pub components: Vec<String>,
}

/// A direct SFTP address submission. `username_hint` is transient and never becomes part of the
/// canonical credential-free location stored by tabs, history, bookmarks, or diagnostics.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SftpAddressInput {
    pub address: RemoteAddress,
    pub username_hint: Option<String>,
}

impl SftpAddressInput {
    pub fn parse(input: &str) -> Result<Self, RemoteAddressError> {
        let (address, username_hint) = parse_userinfo_address(input, "sftp")?;
        Ok(Self {
            address,
            username_hint,
        })
    }
}

/// A direct FTP address submission. Username is a transient login hint; the canonical address is
/// credential-free `ftp://host/path`.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FtpAddressInput {
    pub address: RemoteAddress,
    pub username_hint: Option<String>,
}

impl FtpAddressInput {
    pub fn parse(input: &str) -> Result<Self, RemoteAddressError> {
        let (address, username_hint) = parse_userinfo_address(input, "ftp")?;
        Ok(Self {
            address,
            username_hint,
        })
    }
}

fn parse_userinfo_address(
    input: &str,
    scheme: &str,
) -> Result<(RemoteAddress, Option<String>), RemoteAddressError> {
    let prefix = format!("{scheme}://");
    let remainder = input
        .strip_prefix(&prefix)
        .or_else(|| input.strip_prefix(&prefix.to_ascii_uppercase()))
        .ok_or(RemoteAddressError::UnsupportedScheme)?;
    let (authority, path) = remainder.split_once('/').unwrap_or((remainder, ""));
    let (host, username_hint) = authority
        .split_once('@')
        .map_or((authority, None), |(username, host)| {
            (host, Some(username.to_owned()))
        });
    validate_authority(host)?;
    if username_hint.as_deref().is_some_and(|value| {
        value.is_empty() || value.len() > 255 || value.contains([':', '@', '\0'])
    }) {
        return Err(RemoteAddressError::InvalidAuthority);
    }
    let canonical = if path.is_empty() {
        format!("{scheme}://{host}/")
    } else {
        format!("{scheme}://{host}/{path}")
    };
    Ok((RemoteAddress::parse(&canonical)?, username_hint))
}

/// Persistable SFTP connection metadata. Passwords intentionally have no field here;
/// callers store them in the platform credential vault under `credential_target()`.
///
/// Rule: the public identity is the connection host. `alias` is only a serialized
/// copy of `host` for older profile files; it is never a distinct nickname.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SftpProfile {
    pub alias: String,
    pub host: String,
    pub port: u16,
    pub username: String,
    pub container_identity: [u8; 16],
    pub host_key_fingerprint: Option<String>,
}

impl SftpProfile {
    pub fn new(
        host: String,
        port: u16,
        username: String,
        container_identity: [u8; 16],
    ) -> Result<Self, SftpProfileError> {
        let profile = Self {
            alias: host.clone(),
            host,
            port,
            username,
            container_identity,
            host_key_fingerprint: None,
        };
        profile.validate()?;
        Ok(profile)
    }

    pub fn public_identity(&self) -> &str {
        &self.host
    }

    pub fn validate(&self) -> Result<(), SftpProfileError> {
        if self.alias != self.host {
            return Err(SftpProfileError::InvalidAlias);
        }
        validate_authority(&self.alias).map_err(|_| SftpProfileError::InvalidAlias)?;
        if self.host.is_empty()
            || self.host.len() > 255
            || self.host.contains(['/', '\\', '@', '\0'])
            || self.host.contains(char::is_whitespace)
        {
            return Err(SftpProfileError::InvalidHost);
        }
        if self.port == 0 {
            return Err(SftpProfileError::InvalidPort);
        }
        if self.username.is_empty()
            || self.username.len() > 255
            || self.username.contains([':', '\0'])
        {
            return Err(SftpProfileError::InvalidUsername);
        }
        if self.container_identity == [0; 16] {
            return Err(SftpProfileError::InvalidIdentity);
        }
        if self
            .host_key_fingerprint
            .as_deref()
            .is_some_and(|fingerprint| fingerprint.is_empty() || fingerprint.len() > 512)
        {
            return Err(SftpProfileError::InvalidFingerprint);
        }
        Ok(())
    }

    pub fn credential_target(&self) -> String {
        format!("SuperExplorer/SFTP/{}", self.public_identity())
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct HostIdentityCollapse<T> {
    pub profiles: Vec<T>,
    pub retired_aliases: Vec<(String, String)>,
}

pub fn collapse_sftp_profiles_to_host(
    profiles: Vec<SftpProfile>,
) -> HostIdentityCollapse<SftpProfile> {
    let mut grouped: Vec<((String, u16), Vec<SftpProfile>)> = Vec::new();
    for profile in profiles {
        let key = (profile.host.clone(), profile.port);
        if let Some((_, items)) = grouped.iter_mut().find(|(existing, _)| *existing == key) {
            items.push(profile);
        } else {
            grouped.push((key, vec![profile]));
        }
    }
    let mut collapsed = Vec::new();
    let mut retired_aliases = Vec::new();
    for ((host, _), mut items) in grouped {
        let winner_index = items
            .iter()
            .position(|item| item.alias == host)
            .unwrap_or(0);
        let mut winner = items.remove(winner_index);
        if winner.alias != host {
            retired_aliases.push((winner.alias.clone(), host.clone()));
            winner.alias = host.clone();
        }
        for dropped in items {
            if dropped.alias != host {
                retired_aliases.push((dropped.alias, host.clone()));
            }
        }
        collapsed.push(winner);
    }
    HostIdentityCollapse {
        profiles: collapsed,
        retired_aliases,
    }
}

pub fn collapse_ftp_profiles_to_host(
    profiles: Vec<FtpProfile>,
) -> HostIdentityCollapse<FtpProfile> {
    let mut grouped: Vec<((String, u16), Vec<FtpProfile>)> = Vec::new();
    for profile in profiles {
        let key = (profile.host.clone(), profile.port);
        if let Some((_, items)) = grouped.iter_mut().find(|(existing, _)| *existing == key) {
            items.push(profile);
        } else {
            grouped.push((key, vec![profile]));
        }
    }
    let mut collapsed = Vec::new();
    let mut retired_aliases = Vec::new();
    for ((host, _), mut items) in grouped {
        let winner_index = items
            .iter()
            .position(|item| item.alias == host)
            .unwrap_or(0);
        let mut winner = items.remove(winner_index);
        if winner.alias != host {
            retired_aliases.push((winner.alias.clone(), host.clone()));
            winner.alias = host.clone();
        }
        for dropped in items {
            if dropped.alias != host {
                retired_aliases.push((dropped.alias, host.clone()));
            }
        }
        collapsed.push(winner);
    }
    HostIdentityCollapse {
        profiles: collapsed,
        retired_aliases,
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SftpProfileError {
    InvalidAlias,
    InvalidHost,
    InvalidPort,
    InvalidUsername,
    InvalidIdentity,
    InvalidFingerprint,
}

impl fmt::Display for SftpProfileError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::InvalidAlias => "SFTP profile alias is invalid",
            Self::InvalidHost => "SFTP host is invalid",
            Self::InvalidPort => "SFTP port is invalid",
            Self::InvalidUsername => "SFTP username is invalid",
            Self::InvalidIdentity => "SFTP profile identity is invalid",
            Self::InvalidFingerprint => "SFTP host fingerprint is invalid",
        })
    }
}

impl std::error::Error for SftpProfileError {}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum FtpSecurityMode {
    Plain,
    ExplicitTls,
    ImplicitTls,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum FtpDataMode {
    Passive,
    Active,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum FtpEncoding {
    Auto,
    Utf8,
    System,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum FtpAuthKind {
    Password,
    Anonymous,
}

/// Persistable FTP connection metadata.
///
/// Rule: the public identity is the connection host. `alias` is only a serialized
/// copy of `host` for older profile files; it is never a distinct nickname.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FtpProfile {
    pub alias: String,
    pub host: String,
    pub port: u16,
    pub username: String,
    pub security_mode: FtpSecurityMode,
    pub data_mode: FtpDataMode,
    pub encoding: FtpEncoding,
    pub auth_kind: FtpAuthKind,
    pub initial_directory: Option<String>,
    pub tls_fingerprint: Option<String>,
    pub plain_warning_acknowledged: bool,
    pub container_identity: [u8; 16],
}

impl FtpProfile {
    pub fn new(
        host: String,
        port: u16,
        username: String,
        container_identity: [u8; 16],
    ) -> Result<Self, FtpProfileError> {
        let profile = Self {
            alias: host.clone(),
            host,
            port,
            username,
            security_mode: FtpSecurityMode::Plain,
            data_mode: FtpDataMode::Passive,
            encoding: FtpEncoding::Auto,
            auth_kind: FtpAuthKind::Password,
            initial_directory: None,
            tls_fingerprint: None,
            plain_warning_acknowledged: false,
            container_identity,
        };
        profile.validate()?;
        Ok(profile)
    }

    pub fn public_identity(&self) -> &str {
        &self.host
    }

    pub fn validate(&self) -> Result<(), FtpProfileError> {
        if self.alias != self.host {
            return Err(FtpProfileError::InvalidAlias);
        }
        validate_authority(&self.alias).map_err(|_| FtpProfileError::InvalidAlias)?;
        if self.host.is_empty()
            || self.host.len() > 255
            || self.host.contains(['/', '\\', '@', '\0'])
            || self.host.contains(char::is_whitespace)
        {
            return Err(FtpProfileError::InvalidHost);
        }
        if self.port == 0 {
            return Err(FtpProfileError::InvalidPort);
        }
        if self.auth_kind == FtpAuthKind::Password
            && (self.username.is_empty()
                || self.username.len() > 255
                || self.username.contains([':', '\0']))
        {
            return Err(FtpProfileError::InvalidUsername);
        }
        if self.container_identity == [0; 16] {
            return Err(FtpProfileError::InvalidIdentity);
        }
        if self
            .tls_fingerprint
            .as_deref()
            .is_some_and(|fingerprint| fingerprint.is_empty() || fingerprint.len() > 512)
        {
            return Err(FtpProfileError::InvalidFingerprint);
        }
        if let Some(directory) = &self.initial_directory {
            if directory.is_empty()
                || directory.contains(['\\', '\0', '\r', '\n'])
                || directory
                    .split('/')
                    .any(|component| component.is_empty() || matches!(component, "." | ".."))
            {
                return Err(FtpProfileError::InvalidDirectory);
            }
        }
        Ok(())
    }

    pub fn credential_target(&self) -> String {
        format!("SuperExplorer/FTP/{}", self.public_identity())
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FtpProfileError {
    InvalidAlias,
    InvalidHost,
    InvalidPort,
    InvalidUsername,
    InvalidIdentity,
    InvalidFingerprint,
    InvalidDirectory,
}

impl fmt::Display for FtpProfileError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::InvalidAlias => "FTP profile alias is invalid",
            Self::InvalidHost => "FTP host is invalid",
            Self::InvalidPort => "FTP port is invalid",
            Self::InvalidUsername => "FTP username is invalid",
            Self::InvalidIdentity => "FTP profile identity is invalid",
            Self::InvalidFingerprint => "FTP TLS fingerprint is invalid",
            Self::InvalidDirectory => "FTP initial directory is invalid",
        })
    }
}

impl std::error::Error for FtpProfileError {}

/// Persistable Google Drive account metadata. Refresh tokens live only in the
/// platform credential vault under `credential_target()`.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GdriveProfile {
    pub alias: String,
    pub container_identity: [u8; 16],
}

impl GdriveProfile {
    pub fn new(alias: String, container_identity: [u8; 16]) -> Result<Self, GdriveProfileError> {
        let profile = Self {
            alias,
            container_identity,
        };
        profile.validate()?;
        Ok(profile)
    }

    pub fn validate(&self) -> Result<(), GdriveProfileError> {
        validate_gdrive_authority(&self.alias).map_err(|_| GdriveProfileError::InvalidAlias)?;
        if self.container_identity == [0; 16] {
            return Err(GdriveProfileError::InvalidIdentity);
        }
        Ok(())
    }

    pub fn credential_target(&self) -> String {
        format!("SuperExplorer/GDRIVE/{}", self.alias)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum GdriveProfileError {
    InvalidAlias,
    InvalidIdentity,
}

impl fmt::Display for GdriveProfileError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::InvalidAlias => "Google Drive profile email is invalid",
            Self::InvalidIdentity => "Google Drive profile identity is invalid",
        })
    }
}

impl std::error::Error for GdriveProfileError {}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RemoteProviderCapabilities {
    pub metadata: bool,
    pub unix_permissions: bool,
    pub symlink_inspect: bool,
    pub symlink_create: bool,
    pub atomic_rename: bool,
    pub passive_mode: bool,
    pub active_mode: bool,
    pub server_side_rename: bool,
    pub modification_time: bool,
}

impl RemoteProviderCapabilities {
    pub const fn sftp_defaults() -> Self {
        Self {
            metadata: true,
            unix_permissions: true,
            symlink_inspect: true,
            symlink_create: true,
            atomic_rename: true,
            passive_mode: false,
            active_mode: false,
            server_side_rename: true,
            modification_time: true,
        }
    }

    pub const fn ftp_defaults() -> Self {
        Self {
            metadata: true,
            unix_permissions: false,
            symlink_inspect: false,
            symlink_create: false,
            atomic_rename: true,
            passive_mode: true,
            active_mode: true,
            server_side_rename: true,
            modification_time: true,
        }
    }

    pub const fn gdrive_defaults() -> Self {
        Self {
            metadata: true,
            unix_permissions: false,
            symlink_inspect: false,
            symlink_create: false,
            atomic_rename: true,
            passive_mode: false,
            active_mode: false,
            server_side_rename: true,
            modification_time: true,
        }
    }
}

impl RemoteAddress {
    /// Parses the canonical remote forms `adb://<serial>/<path>`,
    /// `sftp://<profile>/<path>`, `ftp://<profile>/<path>`, and
    /// `gdrive://<email>/<path>`.
    /// Authorities deliberately cannot contain a password.
    pub fn parse(input: &str) -> Result<Self, RemoteAddressError> {
        let (scheme, remainder) = input
            .split_once("://")
            .ok_or(RemoteAddressError::MissingScheme)?;
        let provider = match scheme.to_ascii_lowercase().as_str() {
            "adb" => RemoteProviderKind::Adb,
            "sftp" => RemoteProviderKind::Sftp,
            "ftp" => RemoteProviderKind::Ftp,
            "gdrive" => RemoteProviderKind::Gdrive,
            _ => return Err(RemoteAddressError::UnsupportedScheme),
        };
        let (authority, raw_path) = remainder.split_once('/').unwrap_or((remainder, ""));
        if provider == RemoteProviderKind::Gdrive {
            validate_gdrive_authority(authority)?;
        } else {
            validate_authority(authority)?;
        }
        let components = raw_path
            .split('/')
            .filter(|component| !component.is_empty())
            .map(normalize_component)
            .collect::<Result<Vec<_>, _>>()?;
        Ok(Self {
            provider,
            authority: authority.to_owned(),
            components,
        })
    }

    /// Creates the opaque descriptor used by tabs/history. The caller owns a stable,
    /// persisted container identity for the device serial or SFTP profile.
    pub fn to_location(
        &self,
        container_identity: [u8; 16],
        generation: u64,
    ) -> Result<LocationDescriptor, LocationDescriptorValidationError> {
        let mut location = LocationDescriptor::try_virtual(
            self.provider.provider_id(),
            container_identity,
            generation,
            None,
            self.components.clone(),
        )?;
        if let LocationDescriptor::Virtual(descriptor) = &mut location {
            descriptor.public_authority = Some(self.authority.clone());
        }
        location.validate()?;
        Ok(location)
    }

    /// Creates a stable non-secret identity from provider kind and authority. This lets direct
    /// address entry resolve before the runtime looks up a device serial or profile alias.
    pub fn to_deterministic_location(
        &self,
        generation: u64,
    ) -> Result<LocationDescriptor, LocationDescriptorValidationError> {
        self.to_location(
            remote_container_identity(self.provider, &self.authority),
            generation,
        )
    }

    /// Returns an address safe for history and UI. It never contains a password or SFTP host.
    pub fn canonical(&self) -> String {
        let path = self.components.join("/");
        if path.is_empty() {
            format!("{}://{}", self.provider.provider_id(), self.authority)
        } else {
            format!(
                "{}://{}/{path}",
                self.provider.provider_id(),
                self.authority
            )
        }
    }
}

fn validate_authority(value: &str) -> Result<(), RemoteAddressError> {
    if value.is_empty() {
        return Err(RemoteAddressError::EmptyAuthority);
    }
    if value.len() > 255
        || value.contains(['@', ':', '\\', '\0'])
        || value.contains(char::is_whitespace)
    {
        return Err(RemoteAddressError::InvalidAuthority);
    }
    Ok(())
}

fn validate_gdrive_authority(value: &str) -> Result<(), RemoteAddressError> {
    if value.is_empty() {
        return Err(RemoteAddressError::EmptyAuthority);
    }
    if value.len() > 255
        || value.contains([':', '/', '\\', '\0'])
        || value.contains(char::is_whitespace)
    {
        return Err(RemoteAddressError::InvalidAuthority);
    }
    let Some((local, domain)) = value.split_once('@') else {
        return Err(RemoteAddressError::InvalidAuthority);
    };
    if local.is_empty()
        || domain.is_empty()
        || domain.contains('@')
        || !domain.contains('.')
        || local.starts_with('.')
        || local.ends_with('.')
        || domain.starts_with('.')
        || domain.ends_with('.')
    {
        return Err(RemoteAddressError::InvalidAuthority);
    }
    Ok(())
}

fn normalize_component(value: &str) -> Result<String, RemoteAddressError> {
    if value.is_empty() || matches!(value, "." | "..") || value.contains(['\\', '\0']) {
        return Err(RemoteAddressError::InvalidComponent);
    }
    Ok(value.to_owned())
}

/// Remote input was not a safe canonical address.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RemoteAddressError {
    MissingScheme,
    UnsupportedScheme,
    EmptyAuthority,
    InvalidAuthority,
    InvalidComponent,
}

impl fmt::Display for RemoteAddressError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::MissingScheme => "remote address must start with a supported remote scheme",
            Self::UnsupportedScheme => "remote address scheme is not supported",
            Self::EmptyAuthority => "remote address requires a device serial or profile alias",
            Self::InvalidAuthority => "remote address authority is invalid",
            Self::InvalidComponent => "remote address path contains an invalid component",
        })
    }
}

impl std::error::Error for RemoteAddressError {}

/// Allocates a persistent container identity without deriving it from a server address.
pub fn new_remote_container_identity() -> [u8; 16] {
    *Uuid::new_v4().as_bytes()
}

pub fn remote_container_identity(provider: RemoteProviderKind, authority: &str) -> [u8; 16] {
    *Uuid::new_v5(
        &Uuid::NAMESPACE_URL,
        format!("superexplorer:{}:{authority}", provider.provider_id()).as_bytes(),
    )
    .as_bytes()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn adb_phone_storage_path_is_canonical_and_virtual() {
        let address = RemoteAddress::parse("adb://device-123/sdcard/Download").unwrap();
        assert_eq!(address.provider, RemoteProviderKind::Adb);
        assert_eq!(address.canonical(), "adb://device-123/sdcard/Download");
        let location = address.to_location([7; 16], 1).unwrap();
        assert!(matches!(location, LocationDescriptor::Virtual(_)));
        assert!(address.to_location([7; 16], 0).is_err());
        assert_eq!(
            address.to_deterministic_location(1).unwrap(),
            address.to_deterministic_location(1).unwrap()
        );
    }

    #[test]
    fn sftp_address_keeps_user_info_out_of_canonical_location() {
        let address = RemoteAddress::parse("sftp://production/root").unwrap();
        assert_eq!(address.canonical(), "sftp://production/root");
        for unsafe_address in [
            "sftp://root@45.32.49.125/root",
            "sftp://production:22/root",
            "sftp://production/../../etc",
        ] {
            assert!(RemoteAddress::parse(unsafe_address).is_err());
        }
    }

    #[test]
    fn direct_sftp_username_hint_uses_standard_userinfo_order() {
        let input = SftpAddressInput::parse("sftp://root@45.32.49.125/").unwrap();
        assert_eq!(input.username_hint.as_deref(), Some("root"));
        assert_eq!(input.address.canonical(), "sftp://45.32.49.125");
        assert!(!format!("{:?}", input.address).contains("root"));
    }

    #[test]
    fn direct_sftp_host_only_address_remains_compatible() {
        let input = SftpAddressInput::parse("sftp://45.32.49.125/home/linuxuser").unwrap();
        assert_eq!(input.username_hint, None);
        assert_eq!(
            input.address.canonical(),
            "sftp://45.32.49.125/home/linuxuser"
        );
    }

    #[test]
    fn direct_sftp_reversed_legacy_order_is_not_inferred() {
        let input = SftpAddressInput::parse("sftp://45.32.49.125@root/").unwrap();
        assert_eq!(input.username_hint.as_deref(), Some("45.32.49.125"));
        assert_eq!(input.address.canonical(), "sftp://root");
    }

    #[test]
    fn direct_sftp_rejects_password_bearing_or_malformed_user_info() {
        for input in [
            "sftp://root:secret@45.32.49.125/",
            "sftp://@45.32.49.125/",
            "sftp://root@@45.32.49.125/",
        ] {
            assert_eq!(
                SftpAddressInput::parse(input),
                Err(RemoteAddressError::InvalidAuthority)
            );
        }
    }

    #[test]
    fn direct_sftp_password_is_absent_from_error_diagnostics() {
        let error = SftpAddressInput::parse("sftp://root:secret@45.32.49.125/").unwrap_err();
        assert_eq!(error, RemoteAddressError::InvalidAuthority);
        let display = error.to_string();
        let debug = format!("{error:?}");
        assert!(!display.contains("secret"));
        assert!(!debug.contains("secret"));
        assert!(!display.contains("root:secret"));
        assert!(!debug.contains("root:secret"));
    }

    #[test]
    fn sftp_profile_serialization_has_no_password_field() {
        let profile =
            SftpProfile::new("sftp.example.test".into(), 22, "root".into(), [8; 16]).unwrap();
        let encoded = serde_json::to_string(&profile).unwrap();
        assert!(!encoded.contains("password"));
        assert_eq!(
            profile.credential_target(),
            "SuperExplorer/SFTP/sftp.example.test"
        );
        assert_eq!(profile.alias, profile.host);
    }

    #[test]
    fn sftp_profiles_collapse_nicknames_to_host() {
        let nicknamed = SftpProfile {
            alias: "nickname".into(),
            host: "192.0.2.10".into(),
            port: 22,
            username: "root".into(),
            container_identity: [1; 16],
            host_key_fingerprint: Some("SHA256:old".into()),
        };
        let canonical = SftpProfile {
            alias: "192.0.2.10".into(),
            host: "192.0.2.10".into(),
            port: 22,
            username: "root".into(),
            container_identity: [2; 16],
            host_key_fingerprint: Some("SHA256:new".into()),
        };
        assert_eq!(nicknamed.public_identity(), "192.0.2.10");
        let collapsed = collapse_sftp_profiles_to_host(vec![nicknamed, canonical.clone()]);
        assert_eq!(collapsed.profiles.len(), 1);
        assert_eq!(collapsed.profiles[0].public_identity(), "192.0.2.10");
        assert_eq!(collapsed.profiles[0].alias, collapsed.profiles[0].host);
        assert_eq!(collapsed.profiles[0].container_identity, [2; 16]);
        assert_eq!(
            collapsed.retired_aliases,
            vec![("nickname".into(), "192.0.2.10".into())]
        );
    }

    #[test]
    fn direct_ftp_username_hint_uses_standard_userinfo_order() {
        let input = FtpAddressInput::parse("ftp://test@45.32.49.125/").unwrap();
        assert_eq!(input.username_hint.as_deref(), Some("test"));
        assert_eq!(input.address.canonical(), "ftp://45.32.49.125");
        assert_eq!(input.address.provider, RemoteProviderKind::Ftp);
    }

    #[test]
    fn direct_ftp_rejects_password_bearing_uri() {
        let error = FtpAddressInput::parse("ftp://test:secret@45.32.49.125/").unwrap_err();
        assert_eq!(error, RemoteAddressError::InvalidAuthority);
        assert!(!error.to_string().contains("secret"));
        assert!(!format!("{error:?}").contains("secret"));
    }

    #[test]
    fn ftp_profile_serialization_has_no_password_field() {
        let profile = FtpProfile::new("192.0.2.10".into(), 21, "test".into(), [9; 16]).unwrap();
        let encoded = serde_json::to_string(&profile).unwrap();
        assert!(
            !encoded.contains("\"password\":"),
            "FTP profile JSON must not store a password field: {encoded}"
        );
        assert_eq!(profile.credential_target(), "SuperExplorer/FTP/192.0.2.10");
        assert_eq!(profile.security_mode, FtpSecurityMode::Plain);
        assert_eq!(profile.data_mode, FtpDataMode::Passive);
    }

    #[test]
    fn gdrive_email_address_is_canonical_and_virtual() {
        let address = RemoteAddress::parse("gdrive://you@gmail.com/Work/Notes").unwrap();
        assert_eq!(address.provider, RemoteProviderKind::Gdrive);
        assert_eq!(address.authority, "you@gmail.com");
        assert_eq!(
            address.components,
            vec!["Work".to_owned(), "Notes".to_owned()]
        );
        assert_eq!(address.canonical(), "gdrive://you@gmail.com/Work/Notes");
        let location = address.to_location([7; 16], 1).unwrap();
        let LocationDescriptor::Virtual(descriptor) = location else {
            panic!("expected virtual Google Drive location");
        };
        assert_eq!(descriptor.provider_id, "gdrive");
        assert_eq!(
            descriptor.public_authority.as_deref(),
            Some("you@gmail.com")
        );
        assert_eq!(descriptor.provider_entry_key, None);
    }

    #[test]
    fn gdrive_rejects_password_bearing_or_malformed_email_authority() {
        for input in [
            "gdrive://you:secret@gmail.com/",
            "gdrive://you@@gmail.com/",
            "gdrive://you@",
            "gdrive://@gmail.com/",
            "gdrive://you gmail.com/",
        ] {
            assert_eq!(
                RemoteAddress::parse(input),
                Err(RemoteAddressError::InvalidAuthority),
                "{input}"
            );
        }
        let error = RemoteAddress::parse("gdrive://you:secret@gmail.com/").unwrap_err();
        assert!(!error.to_string().contains("secret"));
        assert!(!format!("{error:?}").contains("secret"));
    }

    #[test]
    fn gdrive_profile_serialization_has_no_token_fields() {
        let profile = GdriveProfile::new("you@gmail.com".into(), [4; 16]).unwrap();
        let encoded = serde_json::to_string(&profile).unwrap();
        for forbidden in ["refresh_token", "access_token", "password", "secret"] {
            assert!(
                !encoded.contains(forbidden),
                "Google Drive profile JSON must not contain {forbidden}: {encoded}"
            );
        }
        assert_eq!(
            profile.credential_target(),
            "SuperExplorer/GDRIVE/you@gmail.com"
        );
    }

    #[test]
    fn gdrive_connect_location_is_a_stable_synthetic_parsing_name() {
        assert_eq!(GDRIVE_CONNECT_LOCATION, "super-explorer:gdrive-connect");
    }
}
