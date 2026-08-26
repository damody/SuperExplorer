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
}

impl RemoteProviderKind {
    pub const fn provider_id(self) -> &'static str {
        match self {
            Self::Adb => "adb",
            Self::Sftp => "sftp",
        }
    }
}

/// A parsed remote address whose authority is an ADB serial or non-secret SFTP profile alias.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RemoteAddress {
    pub provider: RemoteProviderKind,
    pub authority: String,
    pub components: Vec<String>,
}

/// A direct SFTP address submission. `username_hint` is transient and never becomes part of the
/// canonical location stored by tabs, history, bookmarks, or diagnostics.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SftpAddressInput {
    pub address: RemoteAddress,
    pub username_hint: Option<String>,
}

impl SftpAddressInput {
    pub fn parse(input: &str) -> Result<Self, RemoteAddressError> {
        let remainder = input
            .strip_prefix("sftp://")
            .or_else(|| input.strip_prefix("SFTP://"))
            .ok_or(RemoteAddressError::UnsupportedScheme)?;
        let (authority, path) = remainder.split_once('/').unwrap_or((remainder, ""));
        let (host, username_hint) = authority
            .split_once('@')
            .map_or((authority, None), |(host, username)| {
                (host, Some(username.to_owned()))
            });
        validate_authority(host)?;
        if username_hint.as_deref().is_some_and(|value| {
            value.is_empty() || value.len() > 255 || value.contains([':', '@', '\0'])
        }) {
            return Err(RemoteAddressError::InvalidAuthority);
        }
        let canonical = if path.is_empty() {
            format!("sftp://{host}/")
        } else {
            format!("sftp://{host}/{path}")
        };
        Ok(Self {
            address: RemoteAddress::parse(&canonical)?,
            username_hint,
        })
    }
}

/// Persistable SFTP connection metadata. Passwords intentionally have no field here;
/// callers store them in the platform credential vault under `credential_target()`.
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
        alias: String,
        host: String,
        port: u16,
        username: String,
        container_identity: [u8; 16],
    ) -> Result<Self, SftpProfileError> {
        let profile = Self {
            alias,
            host,
            port,
            username,
            container_identity,
            host_key_fingerprint: None,
        };
        profile.validate()?;
        Ok(profile)
    }

    pub fn validate(&self) -> Result<(), SftpProfileError> {
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
        format!("SuperExplorer/SFTP/{}", self.alias)
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

impl RemoteAddress {
    /// Parses the canonical remote forms `adb://<serial>/<path>` and
    /// `sftp://<profile>/<path>`. Authorities deliberately cannot contain user-info.
    pub fn parse(input: &str) -> Result<Self, RemoteAddressError> {
        let (scheme, remainder) = input
            .split_once("://")
            .ok_or(RemoteAddressError::MissingScheme)?;
        let provider = match scheme.to_ascii_lowercase().as_str() {
            "adb" => RemoteProviderKind::Adb,
            "sftp" => RemoteProviderKind::Sftp,
            _ => return Err(RemoteAddressError::UnsupportedScheme),
        };
        let (authority, raw_path) = remainder.split_once('/').unwrap_or((remainder, ""));
        validate_authority(authority)?;
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
            Self::MissingScheme => "remote address must start with adb:// or sftp://",
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
    fn sftp_address_keeps_password_and_host_out_of_location() {
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
    fn direct_sftp_username_hint_is_transient_and_canonicalized() {
        let input = SftpAddressInput::parse("sftp://45.32.49.125@root/").unwrap();
        assert_eq!(input.username_hint.as_deref(), Some("root"));
        assert_eq!(input.address.canonical(), "sftp://45.32.49.125");
        assert!(!format!("{:?}", input.address).contains("root"));
    }

    #[test]
    fn sftp_profile_serialization_has_no_password_field() {
        let profile = SftpProfile::new(
            "production".into(),
            "sftp.example.test".into(),
            22,
            "root".into(),
            [8; 16],
        )
        .unwrap();
        let encoded = serde_json::to_string(&profile).unwrap();
        assert!(!encoded.contains("password"));
        assert_eq!(profile.credential_target(), "SuperExplorer/SFTP/production");
    }
}
