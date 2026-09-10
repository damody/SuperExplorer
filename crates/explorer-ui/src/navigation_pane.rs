//! Typed Windows Explorer navigation-pane presentation data.

use std::{
    path::PathBuf,
    sync::{OnceLock, RwLock},
};

use explorer_i18n::{Catalog, FluentArgs};
use explorer_model::{LocationDescriptor, ShellIconKey, ShellIconTheme, SyntheticRoot};

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum NavigationIcon {
    Home,
    QuickAccess,
    Favorites,
    Gallery,
    OneDrive,
    Desktop,
    Downloads,
    Documents,
    Pictures,
    Music,
    Videos,
    Folder,
    Archive,
    Computer,
    Drive,
    Network,
    Libraries,
    RecycleBin,
    Phone,
    Server,
    GoogleDrive,
    Linux,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AdbNavigationState {
    Ready,
    Offline,
    Unauthorized,
    Unavailable,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AdbNavigationDevice {
    pub serial: String,
    pub label: String,
    pub available: bool,
    pub state: AdbNavigationState,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SftpNavigationProfile {
    pub alias: String,
    pub label: String,
    pub container_identity: [u8; 16],
    pub available: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FtpNavigationProfile {
    pub alias: String,
    pub label: String,
    pub container_identity: [u8; 16],
    pub available: bool,
    pub encrypted: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GdriveNavigationProfile {
    pub alias: String,
    pub label: String,
    pub container_identity: [u8; 16],
    pub available: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WslNavigationDistribution {
    pub name: String,
    pub label: String,
    pub available: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NetworkNavigationPlace {
    pub host: String,
    pub label: String,
    pub location: LocationDescriptor,
}

pub use explorer_model::LINUX_NAMESPACE;

static ADB_NAVIGATION_DEVICES: OnceLock<RwLock<Vec<AdbNavigationDevice>>> = OnceLock::new();
static SFTP_NAVIGATION_PROFILES: OnceLock<RwLock<Vec<SftpNavigationProfile>>> = OnceLock::new();
static FTP_NAVIGATION_PROFILES: OnceLock<RwLock<Vec<FtpNavigationProfile>>> = OnceLock::new();
static GDRIVE_NAVIGATION_PROFILES: OnceLock<RwLock<Vec<GdriveNavigationProfile>>> = OnceLock::new();
static WSL_NAVIGATION_DISTRIBUTIONS: OnceLock<RwLock<Vec<WslNavigationDistribution>>> =
    OnceLock::new();
static NETWORK_NAVIGATION_PLACES: OnceLock<RwLock<Vec<NetworkNavigationPlace>>> = OnceLock::new();

pub fn configure_adb_navigation_devices(devices: Vec<AdbNavigationDevice>) {
    *ADB_NAVIGATION_DEVICES
        .get_or_init(|| RwLock::new(Vec::new()))
        .write()
        .unwrap_or_else(std::sync::PoisonError::into_inner) = devices;
}

pub fn configure_sftp_navigation_profiles(profiles: Vec<SftpNavigationProfile>) {
    *SFTP_NAVIGATION_PROFILES
        .get_or_init(|| RwLock::new(Vec::new()))
        .write()
        .unwrap_or_else(std::sync::PoisonError::into_inner) = profiles;
}

pub fn configure_ftp_navigation_profiles(profiles: Vec<FtpNavigationProfile>) {
    *FTP_NAVIGATION_PROFILES
        .get_or_init(|| RwLock::new(Vec::new()))
        .write()
        .unwrap_or_else(std::sync::PoisonError::into_inner) = profiles;
}

pub fn configure_gdrive_navigation_profiles(profiles: Vec<GdriveNavigationProfile>) {
    *GDRIVE_NAVIGATION_PROFILES
        .get_or_init(|| RwLock::new(Vec::new()))
        .write()
        .unwrap_or_else(std::sync::PoisonError::into_inner) = profiles;
}

pub fn configure_wsl_navigation_distributions(distributions: Vec<WslNavigationDistribution>) {
    *WSL_NAVIGATION_DISTRIBUTIONS
        .get_or_init(|| RwLock::new(Vec::new()))
        .write()
        .unwrap_or_else(std::sync::PoisonError::into_inner) = distributions;
}

pub fn configure_network_navigation_places(places: Vec<NetworkNavigationPlace>) {
    *NETWORK_NAVIGATION_PLACES
        .get_or_init(|| RwLock::new(Vec::new()))
        .write()
        .unwrap_or_else(std::sync::PoisonError::into_inner) = places;
}

fn network_navigation_places() -> Vec<NetworkNavigationPlace> {
    NETWORK_NAVIGATION_PLACES
        .get_or_init(|| RwLock::new(Vec::new()))
        .read()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .clone()
}

pub fn wsl_distribution_root_path(name: &str) -> PathBuf {
    PathBuf::from(format!(r"\\wsl.localhost\{name}\"))
}

pub fn wsl_distribution_root_name(location: &LocationDescriptor) -> Option<String> {
    location
        .path()
        .and_then(explorer_model::wsl_unc_distribution_name)
}

fn is_wsl_distribution_root(location: &LocationDescriptor) -> bool {
    location
        .path()
        .is_some_and(explorer_model::is_wsl_distribution_root_path)
}

fn linux_namespace_location() -> LocationDescriptor {
    LocationDescriptor::ParsingName(LINUX_NAMESPACE.to_owned())
}

fn wsl_navigation_distributions() -> Vec<WslNavigationDistribution> {
    WSL_NAVIGATION_DISTRIBUTIONS
        .get_or_init(|| RwLock::new(Vec::new()))
        .read()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .clone()
}

fn nav_status_label(catalog: Catalog, label: String, key: &str) -> String {
    let mut args = FluentArgs::new();
    args.set("label", label);
    catalog.t_args(key, &args)
}

fn connected_profile_label(catalog: Catalog, label: String, available: bool) -> String {
    if available {
        label
    } else {
        nav_status_label(catalog, label, "nav-not-connected")
    }
}

fn ftp_profile_label(catalog: Catalog, profile: &FtpNavigationProfile) -> String {
    let label = connected_profile_label(catalog, profile.label.clone(), profile.available);
    if profile.encrypted || !profile.available {
        label
    } else {
        nav_status_label(catalog, label, "nav-unencrypted")
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum NavigationItemKind {
    Location,
    Section,
    Separator,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum NavigationItemAvailability {
    Available,
    Unavailable,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NavigationItem {
    pub id: String,
    pub label: String,
    pub kind: NavigationItemKind,
    pub icon: Option<NavigationIcon>,
    pub location: Option<LocationDescriptor>,
    pub icon_location: Option<LocationDescriptor>,
    pub depth: u8,
    pub pinned: bool,
    pub expanded: bool,
    pub availability: NavigationItemAvailability,
}

impl NavigationItem {
    fn location(
        id: impl Into<String>,
        label: impl Into<String>,
        icon: NavigationIcon,
        location: LocationDescriptor,
        depth: u8,
        pinned: bool,
    ) -> Self {
        Self {
            id: id.into(),
            label: label.into(),
            kind: NavigationItemKind::Location,
            icon: Some(icon),
            icon_location: Some(location.clone()),
            location: Some(location),
            depth,
            pinned,
            expanded: false,
            availability: NavigationItemAvailability::Available,
        }
    }

    fn unavailable(id: impl Into<String>, label: impl Into<String>, icon: NavigationIcon) -> Self {
        Self {
            id: id.into(),
            label: label.into(),
            kind: NavigationItemKind::Location,
            icon: Some(icon),
            location: None,
            icon_location: None,
            depth: 0,
            pinned: false,
            expanded: false,
            availability: NavigationItemAvailability::Unavailable,
        }
    }

    fn with_availability(mut self, availability: NavigationItemAvailability) -> Self {
        self.availability = availability;
        self
    }

    fn with_navigation_emblem(mut self) -> Self {
        self.icon_location = None;
        self
    }

    pub(crate) fn child_container(
        label: impl Into<String>,
        location: LocationDescriptor,
        depth: u8,
        expanded: bool,
    ) -> Self {
        let label = label.into();
        let icon_location =
            (!matches!(location, LocationDescriptor::Virtual(_))).then(|| location.clone());
        Self {
            id: format!("tree-{:016x}", navigation_location_hash(&location)),
            label,
            kind: NavigationItemKind::Location,
            icon: Some(NavigationIcon::Folder),
            icon_location,
            location: Some(location),
            depth,
            pinned: false,
            expanded,
            availability: NavigationItemAvailability::Available,
        }
    }

    fn section(
        id: impl Into<String>,
        label: impl Into<String>,
        icon: NavigationIcon,
        location: LocationDescriptor,
    ) -> Self {
        Self {
            id: id.into(),
            label: label.into(),
            kind: NavigationItemKind::Section,
            icon: Some(icon),
            icon_location: Some(location.clone()),
            location: Some(location),
            depth: 0,
            pinned: false,
            expanded: true,
            availability: NavigationItemAvailability::Available,
        }
    }

    fn separator(id: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            label: String::new(),
            kind: NavigationItemKind::Separator,
            icon: None,
            location: None,
            icon_location: None,
            depth: 0,
            pinned: false,
            expanded: false,
            availability: NavigationItemAvailability::Available,
        }
    }

    fn phone_root(catalog: Catalog) -> Self {
        Self {
            id: "phones".to_owned(),
            label: catalog.t("nav-phones"),
            kind: NavigationItemKind::Section,
            icon: Some(NavigationIcon::Phone),
            location: None,
            icon_location: None,
            depth: 0,
            pinned: false,
            expanded: true,
            availability: NavigationItemAvailability::Available,
        }
    }

    fn sftp_root(catalog: Catalog) -> Self {
        Self {
            id: "sftp".to_owned(),
            label: catalog.t("nav-sftp"),
            kind: NavigationItemKind::Section,
            icon: Some(NavigationIcon::Server),
            location: None,
            icon_location: None,
            depth: 0,
            pinned: false,
            expanded: true,
            availability: NavigationItemAvailability::Available,
        }
    }

    fn ftp_root(catalog: Catalog) -> Self {
        Self {
            id: "ftp".to_owned(),
            label: catalog.t("nav-ftp"),
            kind: NavigationItemKind::Section,
            icon: Some(NavigationIcon::Network),
            location: None,
            icon_location: None,
            depth: 0,
            pinned: false,
            expanded: true,
            availability: NavigationItemAvailability::Available,
        }
    }

    fn gdrive_root(catalog: Catalog) -> Self {
        Self {
            id: "gdrive".to_owned(),
            label: catalog.t("nav-gdrive"),
            kind: NavigationItemKind::Section,
            icon: Some(NavigationIcon::GoogleDrive),
            location: None,
            icon_location: None,
            depth: 0,
            pinned: false,
            expanded: true,
            availability: NavigationItemAvailability::Available,
        }
    }

    fn linux_root(catalog: Catalog) -> Self {
        let location = linux_namespace_location();
        Self {
            id: "linux".to_owned(),
            label: catalog.t("nav-linux"),
            kind: NavigationItemKind::Section,
            icon: Some(NavigationIcon::Linux),
            icon_location: Some(location.clone()),
            location: Some(location),
            depth: 0,
            pinned: false,
            expanded: true,
            availability: NavigationItemAvailability::Available,
        }
    }
}

fn navigation_location_hash(location: &LocationDescriptor) -> u64 {
    use std::hash::{Hash as _, Hasher as _};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    location.hash(&mut hasher);
    hasher.finish()
}

/// Returns the canonical ASCII drive letter for a filesystem volume root.
/// Shell ancestry may rediscover these roots below This PC with a different display name; the
/// navigation pane uses this identity to keep the single richer static drive row.
pub(crate) fn filesystem_drive_root(location: &LocationDescriptor) -> Option<char> {
    let LocationDescriptor::FileSystem(path) = location else {
        return None;
    };
    let value = path.to_string_lossy();
    let bytes = value.as_bytes();
    (bytes.len() == 3
        && bytes[0].is_ascii_alphabetic()
        && bytes[1] == b':'
        && matches!(bytes[2], b'\\' | b'/'))
    .then(|| char::from(bytes[0]).to_ascii_uppercase())
}

pub(crate) fn should_render_discovered_child(
    parent: &LocationDescriptor,
    child: &LocationDescriptor,
    display_name: &str,
) -> bool {
    let parent_is_this_pc = matches!(
        parent,
        LocationDescriptor::ParsingName(value)
            if value.eq_ignore_ascii_case("shell:MyComputerFolder")
    );
    if parent_is_this_pc {
        return filesystem_drive_root(child).is_none()
            && drive_root_display_letter(display_name).is_none()
            && !is_wsl_distribution_root(child)
            && !display_name_is_wsl_unc(display_name);
    }
    let parent_is_linux = matches!(
        parent,
        LocationDescriptor::ParsingName(value)
            if value.eq_ignore_ascii_case(LINUX_NAMESPACE)
    );
    !(parent_is_linux && is_wsl_distribution_root(child))
}

fn display_name_is_wsl_unc(display_name: &str) -> bool {
    let normalized = display_name.trim().replace('/', r"\");
    let rest = match normalized
        .strip_prefix(r"\\")
        .or_else(|| normalized.strip_prefix(r"//"))
    {
        Some(rest) => rest,
        None => return false,
    };
    rest.split('\\')
        .find(|part| !part.is_empty())
        .is_some_and(|server| {
            server.eq_ignore_ascii_case("wsl.localhost") || server.eq_ignore_ascii_case("wsl$")
        })
}

fn drive_root_display_letter(display_name: &str) -> Option<char> {
    let value = display_name.trim();
    let bytes = value.as_bytes();
    if bytes.len() == 2 && bytes[0].is_ascii_alphabetic() && bytes[1] == b':' {
        return Some(char::from(bytes[0]).to_ascii_uppercase());
    }
    let suffix = bytes.get(bytes.len().saturating_sub(4)..)?;
    (suffix[0] == b'(' && suffix[1].is_ascii_alphabetic() && suffix[2] == b':' && suffix[3] == b')')
        .then(|| char::from(suffix[1]).to_ascii_uppercase())
}

pub fn windows_navigation_items(catalog: Catalog) -> Vec<NavigationItem> {
    windows_navigation_items_with_pins(catalog, std::iter::empty())
}

/// Builds the stable Explorer root tree plus the application-owned Quick Access pins.
/// Pin descriptors are already privacy-filtered and reconstructible at this boundary.
pub fn windows_navigation_items_with_pins(
    catalog: Catalog,
    pins: impl IntoIterator<Item = (String, LocationDescriptor)>,
) -> Vec<NavigationItem> {
    let pins = pins.into_iter().collect::<Vec<_>>();
    let quick_access_availability = if pins.is_empty() {
        NavigationItemAvailability::Unavailable
    } else {
        NavigationItemAvailability::Available
    };
    let mut items = vec![
        NavigationItem::location(
            "home",
            catalog.t("nav-home"),
            NavigationIcon::Home,
            LocationDescriptor::synthetic(SyntheticRoot::Home),
            0,
            false,
        ),
        NavigationItem::section(
            "quick-access",
            catalog.t("nav-quick-access"),
            NavigationIcon::QuickAccess,
            LocationDescriptor::synthetic(SyntheticRoot::QuickAccess),
        )
        .with_availability(quick_access_availability),
        NavigationItem::location(
            "gallery",
            catalog.t("nav-gallery"),
            NavigationIcon::Gallery,
            LocationDescriptor::ParsingName(
                "shell:::{e88865ea-0e1c-4e20-9aa6-edcd0212c87c}".into(),
            ),
            0,
            false,
        ),
    ];
    let onedrive_label = catalog.t("nav-onedrive");
    if let Some(path) = std::env::var_os("OneDrive")
        .map(PathBuf::from)
        .filter(|path| path.is_dir())
    {
        items.push(
            NavigationItem::location(
                "onedrive",
                onedrive_label,
                NavigationIcon::OneDrive,
                LocationDescriptor::file_system(path),
                0,
                false,
            )
            .with_navigation_emblem(),
        );
    } else {
        items.push(NavigationItem::unavailable(
            "onedrive",
            onedrive_label,
            NavigationIcon::OneDrive,
        ));
    }
    items.push(NavigationItem::separator("favorites-separator"));

    let mut pinned_locations = pins
        .into_iter()
        .enumerate()
        .map(|(index, (label, location))| {
            NavigationItem::location(
                format!("quick-access-pin-{index}"),
                label,
                NavigationIcon::Folder,
                location,
                1,
                true,
            )
        })
        .collect::<Vec<_>>();
    items.append(&mut pinned_locations);

    for (id, key, parsing_name, icon) in [
        (
            "desktop",
            "nav-desktop",
            "shell:Desktop",
            NavigationIcon::Desktop,
        ),
        (
            "downloads",
            "nav-downloads",
            "shell:Downloads",
            NavigationIcon::Downloads,
        ),
        (
            "documents",
            "nav-documents",
            "shell:Personal",
            NavigationIcon::Documents,
        ),
        (
            "pictures",
            "nav-pictures",
            "shell:My Pictures",
            NavigationIcon::Pictures,
        ),
        (
            "music",
            "nav-music",
            "shell:My Music",
            NavigationIcon::Music,
        ),
        (
            "videos",
            "nav-videos",
            "shell:My Video",
            NavigationIcon::Videos,
        ),
    ] {
        items.push(NavigationItem::location(
            id,
            catalog.t(key),
            icon,
            LocationDescriptor::ParsingName(parsing_name.into()),
            0,
            true,
        ));
    }

    items.push(NavigationItem::separator("computer-separator"));
    items.push(NavigationItem::location(
        "libraries",
        catalog.t("nav-libraries"),
        NavigationIcon::Libraries,
        LocationDescriptor::ParsingName("shell:Libraries".into()),
        0,
        false,
    ));
    items.push(NavigationItem::section(
        "this-pc",
        catalog.t("nav-this-pc"),
        NavigationIcon::Computer,
        LocationDescriptor::ParsingName("shell:MyComputerFolder".into()),
    ));
    for letter in b'C'..=b'Z' {
        let root = PathBuf::from(format!("{}:\\", char::from(letter)));
        if root.is_dir() {
            let mut args = FluentArgs::new();
            args.set("letter", char::from(letter).to_string());
            let label = if letter == b'C' {
                catalog.t_args("nav-local-disk", &args)
            } else {
                catalog.t_args("nav-new-volume", &args)
            };
            items.push(NavigationItem::location(
                format!("drive-{}", char::from(letter).to_ascii_lowercase()),
                label,
                NavigationIcon::Drive,
                LocationDescriptor::file_system(root),
                1,
                false,
            ));
        }
    }
    items.push(NavigationItem::location(
        "network",
        catalog.t("nav-network"),
        NavigationIcon::Network,
        LocationDescriptor::ParsingName("shell:NetworkPlacesFolder".into()),
        0,
        false,
    ));
    for place in network_navigation_places() {
        items.push(NavigationItem::location(
            format!("network-host-{}", place.host),
            place.label,
            NavigationIcon::Network,
            place.location,
            1,
            false,
        ));
    }
    let wsl_distributions = wsl_navigation_distributions();
    if !wsl_distributions.is_empty() {
        items.push(NavigationItem::linux_root(catalog));
        for distro in wsl_distributions {
            items.push(NavigationItem {
                id: format!("linux-distro-{}", distro.name),
                label: distro.label,
                kind: NavigationItemKind::Location,
                icon: Some(NavigationIcon::Folder),
                icon_location: None,
                location: distro.available.then(|| {
                    LocationDescriptor::file_system(wsl_distribution_root_path(&distro.name))
                }),
                depth: 1,
                pinned: false,
                expanded: false,
                availability: if distro.available {
                    NavigationItemAvailability::Available
                } else {
                    NavigationItemAvailability::Unavailable
                },
            });
        }
    }
    items.push(NavigationItem::separator("phones-separator"));
    items.push(NavigationItem::phone_root(catalog));
    let devices = ADB_NAVIGATION_DEVICES
        .get_or_init(|| RwLock::new(Vec::new()))
        .read()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .clone();
    for device in devices {
        let location = explorer_model::RemoteAddress::parse(&format!("adb://{}/", device.serial))
            .ok()
            .and_then(|address| address.to_deterministic_location(1).ok());
        let label = match device.state {
            AdbNavigationState::Ready => device.label,
            AdbNavigationState::Offline => nav_status_label(catalog, device.label, "nav-offline"),
            AdbNavigationState::Unauthorized => {
                nav_status_label(catalog, device.label, "nav-unauthorized")
            }
            AdbNavigationState::Unavailable => {
                nav_status_label(catalog, device.label, "nav-unavailable")
            }
        };
        items.push(NavigationItem {
            id: format!("phone-{}", device.serial),
            label,
            kind: NavigationItemKind::Location,
            icon: Some(NavigationIcon::Phone),
            icon_location: None,
            location: device.available.then_some(location).flatten(),
            depth: 1,
            pinned: false,
            expanded: false,
            availability: if device.available {
                NavigationItemAvailability::Available
            } else {
                NavigationItemAvailability::Unavailable
            },
        });
    }
    items.push(NavigationItem::sftp_root(catalog));
    let profiles = SFTP_NAVIGATION_PROFILES
        .get_or_init(|| RwLock::new(Vec::new()))
        .read()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .clone();
    for profile in profiles {
        let location = explorer_model::RemoteAddress::parse(&format!("sftp://{}/", profile.alias))
            .ok()
            .and_then(|address| address.to_location(profile.container_identity, 1).ok());
        items.push(NavigationItem {
            id: format!("sftp-profile-{}", profile.alias),
            label: connected_profile_label(catalog, profile.label, profile.available),
            kind: NavigationItemKind::Location,
            icon: Some(NavigationIcon::Server),
            icon_location: None,
            location: profile.available.then_some(location).flatten(),
            depth: 1,
            pinned: false,
            expanded: false,
            availability: if profile.available {
                NavigationItemAvailability::Available
            } else {
                NavigationItemAvailability::Unavailable
            },
        });
    }
    items.push(NavigationItem::ftp_root(catalog));
    let ftp_profiles = FTP_NAVIGATION_PROFILES
        .get_or_init(|| RwLock::new(Vec::new()))
        .read()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .clone();
    for profile in ftp_profiles {
        let location = explorer_model::RemoteAddress::parse(&format!("ftp://{}/", profile.alias))
            .ok()
            .and_then(|address| address.to_location(profile.container_identity, 1).ok());
        let label = ftp_profile_label(catalog, &profile);
        items.push(NavigationItem {
            id: format!("ftp-profile-{}", profile.alias),
            label,
            kind: NavigationItemKind::Location,
            icon: Some(NavigationIcon::Network),
            icon_location: None,
            location: profile.available.then_some(location).flatten(),
            depth: 1,
            pinned: false,
            expanded: false,
            availability: if profile.available {
                NavigationItemAvailability::Available
            } else {
                NavigationItemAvailability::Unavailable
            },
        });
    }
    items.push(NavigationItem::gdrive_root(catalog));
    items.push(NavigationItem {
        id: "gdrive-connect".to_owned(),
        label: catalog.t("nav-connect-gdrive"),
        kind: NavigationItemKind::Location,
        icon: Some(NavigationIcon::GoogleDrive),
        icon_location: None,
        location: Some(LocationDescriptor::ParsingName(
            explorer_model::GDRIVE_CONNECT_LOCATION.to_owned(),
        )),
        depth: 1,
        pinned: false,
        expanded: false,
        availability: NavigationItemAvailability::Available,
    });
    let gdrive_profiles = GDRIVE_NAVIGATION_PROFILES
        .get_or_init(|| RwLock::new(Vec::new()))
        .read()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .clone();
    for profile in gdrive_profiles {
        let location =
            explorer_model::RemoteAddress::parse(&format!("gdrive://{}/", profile.alias))
                .ok()
                .and_then(|address| address.to_location(profile.container_identity, 1).ok());
        items.push(NavigationItem {
            id: format!("gdrive-profile-{}", profile.alias),
            label: connected_profile_label(catalog, profile.label, profile.available),
            kind: NavigationItemKind::Location,
            icon: Some(NavigationIcon::GoogleDrive),
            icon_location: None,
            location: profile.available.then_some(location).flatten(),
            depth: 1,
            pinned: false,
            expanded: false,
            availability: if profile.available {
                NavigationItemAvailability::Available
            } else {
                NavigationItemAvailability::Unavailable
            },
        });
    }
    items.push(NavigationItem::location(
        "recycle-bin",
        catalog.t("nav-recycle-bin"),
        NavigationIcon::RecycleBin,
        LocationDescriptor::ParsingName("shell:RecycleBinFolder".into()),
        0,
        false,
    ));
    items
}

pub fn is_selected(item: &NavigationItem, current: Option<&LocationDescriptor>) -> bool {
    match (item.location.as_ref(), current) {
        (
            Some(LocationDescriptor::FileSystem(left_path)),
            Some(LocationDescriptor::FileSystem(right_path)),
        ) => {
            let left = left_path.to_string_lossy();
            let right = right_path.to_string_lossy();
            left.eq_ignore_ascii_case(&right)
                || (item.id.starts_with("drive-")
                    && right
                        .to_ascii_lowercase()
                        .starts_with(&left.to_ascii_lowercase()))
                || (item.id.starts_with("linux-distro-")
                    && wsl_distribution_root_name(&LocationDescriptor::FileSystem(
                        left_path.clone(),
                    ))
                    .zip(wsl_distribution_root_name(&LocationDescriptor::FileSystem(
                        right_path.clone(),
                    )))
                    .is_some_and(|(item_distro, current_distro)| {
                        item_distro.eq_ignore_ascii_case(&current_distro)
                    }))
                || (item.id.starts_with("network-host-")
                    && explorer_model::NetworkPlace::from_unc_path(left_path).is_some_and(
                        |place| {
                            place.matches_location(&LocationDescriptor::FileSystem(
                                right_path.clone(),
                            ))
                        },
                    ))
        }
        (Some(LocationDescriptor::Virtual(left)), Some(LocationDescriptor::Virtual(right))) => {
            left.provider_id == right.provider_id
                && left.container_identity == right.container_identity
                && right.components.starts_with(&left.components)
        }
        (Some(left), Some(right)) => left == right,
        _ => false,
    }
}

pub fn shell_icon_key(
    location: &LocationDescriptor,
    theme: ShellIconTheme,
    dpi: u16,
) -> ShellIconKey {
    let size_bucket = ((u32::from(dpi) * 20 + 48) / 96).clamp(16, 256) as u16;
    ShellIconKey {
        item_id: None,
        location: location.clone(),
        // A 32px Shell bitmap remains crisp when displayed in the Explorer 20 logical-pixel slot
        // at common 150-175% Windows DPI settings.
        size_bucket,
        dpi,
        theme,
        association_generation: 0,
        overlay_generation: 0,
    }
}

pub(crate) const GENERIC_SHELL_FOLDER_ICON_PATH: &str = r"C:\__super_explorer_folder_base__";

pub(crate) fn generic_breadcrumb_folder_icon_key(
    theme: ShellIconTheme,
    dpi: u16,
    association_generation: u64,
) -> ShellIconKey {
    let mut key = shell_icon_key(
        &LocationDescriptor::file_system(GENERIC_SHELL_FOLDER_ICON_PATH),
        theme,
        dpi,
    );
    key.association_generation = association_generation.max(1);
    key.overlay_generation = 0;
    key
}

pub(crate) fn is_generic_breadcrumb_folder_icon_key(key: &ShellIconKey) -> bool {
    key.item_id.is_none()
        && key.association_generation > 0
        && key.overlay_generation == 0
        && key.location.path().is_some_and(|path| {
            path.as_os_str()
                .to_string_lossy()
                .eq_ignore_ascii_case(GENERIC_SHELL_FOLDER_ICON_PATH)
        })
}

pub fn file_icon_key(
    entry: &explorer_model::FileEntry,
    theme: ShellIconTheme,
    dpi: u16,
) -> ShellIconKey {
    file_icon_key_for_size(entry, theme, dpi, 20)
}

pub fn file_icon_key_for_size(
    entry: &explorer_model::FileEntry,
    theme: ShellIconTheme,
    dpi: u16,
    logical_size: u16,
) -> ShellIconKey {
    let mut key = shell_icon_key(&entry.location, theme, dpi);
    key.item_id = Some(entry.id.clone());
    key.size_bucket = file_icon_physical_size(dpi, logical_size);
    key
}

fn file_icon_physical_size(dpi: u16, logical_size: u16) -> u16 {
    // The zoom ladder reaches 512 logical pixels and Windows supports 200% DPI. Preserve that
    // actual raster demand in the cache key so the Shell image factory can return source pixels
    // instead of forcing GPUI to enlarge the old 256px ceiling.
    ((u32::from(dpi) * u32::from(logical_size) + 48) / 96).clamp(16, 1_024) as u16
}

pub const fn view_icon_logical_size(mode: explorer_model::ViewMode) -> u16 {
    explorer_model::default_icon_size_for_mode(mode)
}

pub fn view_icon_logical_size_for_settings(settings: &explorer_model::ViewSettings) -> u16 {
    explorer_model::effective_icon_size(settings)
}

#[cfg(test)]
mod tests {
    use super::*;
    use explorer_i18n::AppLocale;

    fn zh_tw_catalog() -> Catalog {
        Catalog::new(AppLocale::ZhTw)
    }

    #[test]
    fn navigation_contract_has_stable_unique_ids_and_explorer_section_order() {
        configure_network_navigation_places(Vec::new());
        let items = windows_navigation_items(zh_tw_catalog());
        let mut ids = std::collections::HashSet::new();
        assert!(items.iter().all(|item| ids.insert(item.id.as_str())));
        let position = |id| items.iter().position(|item| item.id == id).unwrap();
        assert!(position("home") < position("gallery"));
        assert!(position("gallery") < position("this-pc"));
        assert!(position("this-pc") < position("network"));
    }

    #[test]
    fn optional_navigation_roots_are_truthful_and_gallery_uses_real_shell_identity() {
        let items = windows_navigation_items_with_pins(zh_tw_catalog(), std::iter::empty());
        let quick_access = items
            .iter()
            .find(|item| item.id == "quick-access")
            .expect("Quick Access row");
        assert_eq!(
            quick_access.availability,
            NavigationItemAvailability::Unavailable
        );
        let gallery = items
            .iter()
            .find(|item| item.id == "gallery")
            .expect("Gallery row");
        assert_eq!(gallery.availability, NavigationItemAvailability::Available);
        assert_eq!(
            gallery.location,
            Some(LocationDescriptor::ParsingName(
                "shell:::{e88865ea-0e1c-4e20-9aa6-edcd0212c87c}".to_owned()
            ))
        );

        let pinned = windows_navigation_items_with_pins(
            zh_tw_catalog(),
            [(
                "fixture".to_owned(),
                LocationDescriptor::file_system(r"C:\fixture"),
            )],
        );
        assert_eq!(
            pinned
                .iter()
                .find(|item| item.id == "quick-access")
                .map(|item| item.availability),
            Some(NavigationItemAvailability::Available)
        );
    }

    #[test]
    fn filesystem_selection_is_ascii_case_insensitive() {
        let item = NavigationItem::location(
            "fixture",
            "Fixture",
            NavigationIcon::Folder,
            LocationDescriptor::file_system(r"D:\Fixture"),
            0,
            false,
        );
        assert!(is_selected(
            &item,
            Some(&LocationDescriptor::file_system(r"d:\fixture"))
        ));
    }

    #[test]
    fn remote_roots_show_devices_profiles_and_select_nested_paths() {
        configure_adb_navigation_devices(vec![AdbNavigationDevice {
            serial: "phone-123".to_owned(),
            label: "Pixel (phone-123)".to_owned(),
            available: true,
            state: AdbNavigationState::Ready,
        }]);
        let sftp_host = "192.0.2.10";
        let ftp_host = "192.0.2.11";
        configure_sftp_navigation_profiles(vec![SftpNavigationProfile {
            alias: sftp_host.to_owned(),
            label: sftp_host.to_owned(),
            container_identity: [9; 16],
            available: true,
        }]);
        configure_ftp_navigation_profiles(vec![FtpNavigationProfile {
            alias: ftp_host.to_owned(),
            label: ftp_host.to_owned(),
            container_identity: [8; 16],
            available: true,
            encrypted: false,
        }]);
        let items = windows_navigation_items(zh_tw_catalog());
        assert!(items.iter().any(|item| item.id == "phones"));
        assert!(items.iter().any(|item| item.id == "sftp"));
        assert!(items.iter().any(|item| item.id == "ftp"));
        assert!(
            items
                .iter()
                .any(|item| item.id == format!("ftp-profile-{ftp_host}"))
        );
        assert!(items.iter().any(|item| item.id == "gdrive"));
        assert!(items.iter().any(|item| item.id == "gdrive-connect"));
        let phone = items
            .iter()
            .find(|item| item.id == "phone-phone-123")
            .expect("phone row");
        assert!(matches!(
            phone.location.as_ref(),
            Some(LocationDescriptor::Virtual(remote)) if remote.components.is_empty()
        ));
        let nested = explorer_model::RemoteAddress::parse("adb://phone-123/sdcard/Download")
            .unwrap()
            .to_deterministic_location(1)
            .unwrap();
        assert!(is_selected(phone, Some(&nested)));
        let remote_child = NavigationItem::child_container("Download", nested, 2, false);
        assert!(remote_child.icon_location.is_none());
        assert!(
            items
                .iter()
                .any(|item| item.id == format!("sftp-profile-{sftp_host}"))
        );
        let ftp_row = items
            .iter()
            .find(|item| item.id == format!("ftp-profile-{ftp_host}"))
            .expect("ftp row");
        assert!(ftp_row.label.contains("未加密"));
        configure_ftp_navigation_profiles(vec![FtpNavigationProfile {
            alias: ftp_host.to_owned(),
            label: ftp_host.to_owned(),
            container_identity: [8; 16],
            available: false,
            encrypted: false,
        }]);
        configure_adb_navigation_devices(vec![AdbNavigationDevice {
            serial: "phone-123".to_owned(),
            label: "Pixel (phone-123)".to_owned(),
            available: false,
            state: AdbNavigationState::Offline,
        }]);
        let disconnected = windows_navigation_items(zh_tw_catalog());
        let ftp_disconnected = disconnected
            .iter()
            .find(|item| item.id == format!("ftp-profile-{ftp_host}"))
            .expect("disconnected ftp");
        assert!(ftp_disconnected.label.contains("尚未連線"));
        assert!(!ftp_disconnected.label.contains("未加密"));
        let phone_offline = disconnected
            .iter()
            .find(|item| item.id == "phone-phone-123")
            .expect("offline phone");
        assert!(phone_offline.label.contains("離線"));
        let en = windows_navigation_items(Catalog::new(AppLocale::En));
        let ftp_en = en
            .iter()
            .find(|item| item.id == format!("ftp-profile-{ftp_host}"))
            .expect("ftp en");
        assert!(ftp_en.label.contains("Not connected"));
        assert!(!ftp_en.label.contains("尚未連線"));
    }

    #[test]
    fn volume_root_identity_is_display_name_independent_and_rejects_descendants() {
        assert_eq!(
            filesystem_drive_root(&LocationDescriptor::file_system(r"d:\")),
            Some('D')
        );
        assert_eq!(
            filesystem_drive_root(&LocationDescriptor::file_system("D:/")),
            Some('D')
        );
        assert_eq!(
            filesystem_drive_root(&LocationDescriptor::file_system(r"D:\folder")),
            None
        );
        assert_eq!(
            filesystem_drive_root(&LocationDescriptor::ParsingName(
                "shell:MyComputerFolder".to_owned()
            )),
            None
        );
        let this_pc = LocationDescriptor::ParsingName("shell:MyComputerFolder".to_owned());
        assert!(!should_render_discovered_child(
            &this_pc,
            &LocationDescriptor::file_system(r"C:\"),
            "Local Disk (C:)"
        ));
        assert!(!should_render_discovered_child(
            &this_pc,
            &LocationDescriptor::ParsingName("::{opaque-drive-pidl}".to_owned()),
            "D:"
        ));
        assert!(should_render_discovered_child(
            &this_pc,
            &LocationDescriptor::ParsingName("shell:ThirdPartyProvider".to_owned()),
            "Cloud Provider"
        ));
        assert!(
            !should_render_discovered_child(
                &this_pc,
                &LocationDescriptor::file_system(r"\\wsl.localhost\Ubuntu-24.04\"),
                r"\\wsl.localhost\Ubuntu-24.04"
            ),
            "WSL distro roots belong under Linux, not This PC"
        );
        assert!(!should_render_discovered_child(
            &this_pc,
            &LocationDescriptor::file_system(r"\\wsl$\Ubuntu-24.04"),
            r"\\wsl$\Ubuntu-24.04"
        ));
    }

    #[test]
    fn generic_breadcrumb_folder_key_is_shell_shared_and_environment_specific() {
        let light = generic_breadcrumb_folder_icon_key(ShellIconTheme::Light, 96, 7);
        assert!(is_generic_breadcrumb_folder_icon_key(&light));
        assert_eq!(light.association_generation, 7);
        assert_eq!(light.overlay_generation, 0);
        assert!(light.item_id.is_none());

        let concrete = shell_icon_key(
            &LocationDescriptor::file_system(r"D:\fixture"),
            ShellIconTheme::Light,
            96,
        );
        assert_ne!(light, concrete);
        assert_ne!(
            light,
            generic_breadcrumb_folder_icon_key(ShellIconTheme::Dark, 96, 7)
        );
        assert_ne!(
            light,
            generic_breadcrumb_folder_icon_key(ShellIconTheme::Light, 144, 7)
        );
        assert_ne!(
            light,
            generic_breadcrumb_folder_icon_key(ShellIconTheme::Light, 96, 8)
        );
    }

    #[test]
    fn file_icon_raster_size_tracks_zoom_and_dpi_without_a_256px_ceiling() {
        assert_eq!(file_icon_physical_size(96, 128), 128);
        assert_eq!(file_icon_physical_size(168, 128), 224);
        assert_eq!(file_icon_physical_size(192, 512), 1_024);
        assert_eq!(file_icon_physical_size(u16::MAX, u16::MAX), 1_024);
    }

    #[test]
    fn navigation_section_titles_follow_catalog_locale() {
        let label = |items: &[NavigationItem], id: &str| {
            items
                .iter()
                .find(|item| item.id == id)
                .map(|item| item.label.as_str())
                .expect(id)
                .to_owned()
        };
        let zh = windows_navigation_items(zh_tw_catalog());
        assert_eq!(label(&zh, "phones"), "手機");
        assert_eq!(label(&zh, "sftp"), "SFTP");
        assert_eq!(label(&zh, "ftp"), "FTP");
        assert_eq!(label(&zh, "gdrive"), "Google Drive");
        assert_eq!(label(&zh, "gdrive-connect"), "連線 Google Drive");

        let en = windows_navigation_items(Catalog::new(AppLocale::En));
        assert_eq!(label(&en, "phones"), "Phones");
        assert_eq!(label(&en, "sftp"), "SFTP");
        assert_eq!(label(&en, "ftp"), "FTP");
        assert_eq!(label(&en, "gdrive"), "Google Drive");
        assert_eq!(label(&en, "gdrive-connect"), "Connect Google Drive");
    }

    #[test]
    fn remembered_unc_hosts_appear_under_network_and_select_nested_shares() {
        configure_network_navigation_places(vec![NetworkNavigationPlace {
            host: "122.116.110.30".to_owned(),
            label: r"\\122.116.110.30\".to_owned(),
            location: LocationDescriptor::file_system(r"\\122.116.110.30\"),
        }]);
        let items = windows_navigation_items(zh_tw_catalog());
        let position = |id| items.iter().position(|item| item.id == id).unwrap();
        assert!(position("network") < position("network-host-122.116.110.30"));
        let host = items
            .iter()
            .find(|item| item.id == "network-host-122.116.110.30")
            .expect("network host row");
        assert_eq!(host.label, r"\\122.116.110.30\");
        assert_eq!(host.depth, 1);
        assert_eq!(host.icon, Some(NavigationIcon::Network));
        assert!(is_selected(
            host,
            Some(&LocationDescriptor::file_system(
                r"\\122.116.110.30\Multimedia"
            ))
        ));
        assert!(!is_selected(
            host,
            Some(&LocationDescriptor::file_system(r"\\other-host\share"))
        ));
        configure_network_navigation_places(Vec::new());
    }

    #[test]
    fn linux_row_is_hidden_until_a_wsl_distribution_exists() {
        configure_network_navigation_places(Vec::new());
        configure_wsl_navigation_distributions(Vec::new());
        let hidden = windows_navigation_items(zh_tw_catalog());
        assert!(hidden.iter().all(|item| item.id != "linux"));
        assert!(
            hidden
                .iter()
                .all(|item| !item.id.starts_with("linux-distro-"))
        );

        configure_wsl_navigation_distributions(vec![WslNavigationDistribution {
            name: "Ubuntu-24.04".to_owned(),
            label: "Ubuntu-24.04".to_owned(),
            available: true,
        }]);
        let items = windows_navigation_items(zh_tw_catalog());
        let mut ids = std::collections::HashSet::new();
        assert!(items.iter().all(|item| ids.insert(item.id.as_str())));
        let position = |id| items.iter().position(|item| item.id == id).unwrap();
        assert!(position("network") < position("linux"));
        assert!(position("linux") < position("linux-distro-Ubuntu-24.04"));
        assert!(position("linux-distro-Ubuntu-24.04") < position("phones"));

        let linux = items
            .iter()
            .find(|item| item.id == "linux")
            .expect("linux row");
        assert_eq!(linux.label, "Linux");
        assert_eq!(linux.kind, NavigationItemKind::Section);
        assert_eq!(linux.icon, Some(NavigationIcon::Linux));
        assert_eq!(
            linux.location,
            Some(LocationDescriptor::ParsingName(LINUX_NAMESPACE.to_owned()))
        );
        assert_eq!(linux.icon_location, linux.location);
        assert_eq!(linux.depth, 0);
        assert!(linux.expanded);
        assert_eq!(linux.availability, NavigationItemAvailability::Available);

        let distro = items
            .iter()
            .find(|item| item.id == "linux-distro-Ubuntu-24.04")
            .expect("distro row");
        assert_eq!(distro.label, "Ubuntu-24.04");
        assert_eq!(distro.icon, Some(NavigationIcon::Folder));
        assert_eq!(distro.depth, 1);
        assert_eq!(
            distro.location,
            Some(LocationDescriptor::file_system(
                r"\\wsl.localhost\Ubuntu-24.04\"
            ))
        );
        assert!(is_selected(
            distro,
            Some(&LocationDescriptor::file_system(
                r"\\wsl.localhost\Ubuntu-24.04\home\user"
            ))
        ));
        assert!(is_selected(
            distro,
            Some(&LocationDescriptor::file_system(r"\\wsl$\Ubuntu-24.04\etc"))
        ));
        assert!(!is_selected(
            distro,
            Some(&LocationDescriptor::file_system(
                r"\\wsl.localhost\Ubuntu-22.04\"
            ))
        ));

        let linux_parent = LocationDescriptor::ParsingName(LINUX_NAMESPACE.to_owned());
        assert!(!should_render_discovered_child(
            &linux_parent,
            &LocationDescriptor::file_system(r"\\wsl.localhost\Ubuntu-24.04\"),
            "Ubuntu-24.04"
        ));
        assert!(should_render_discovered_child(
            &linux_parent,
            &LocationDescriptor::file_system(r"\\wsl.localhost\Ubuntu-24.04\home"),
            "home"
        ));

        let en = windows_navigation_items(Catalog::new(AppLocale::En));
        assert_eq!(
            en.iter()
                .find(|item| item.id == "linux")
                .map(|item| item.label.as_str()),
            Some("Linux")
        );
        configure_wsl_navigation_distributions(Vec::new());
    }

    #[test]
    fn wsl_unc_identity_accepts_localhost_and_dollar_roots() {
        assert_eq!(
            wsl_distribution_root_name(&LocationDescriptor::file_system(
                r"\\wsl.localhost\Ubuntu-24.04\"
            ))
            .as_deref(),
            Some("Ubuntu-24.04")
        );
        assert_eq!(
            wsl_distribution_root_name(&LocationDescriptor::file_system(
                r"\\wsl$\Ubuntu-24.04\home"
            ))
            .as_deref(),
            Some("Ubuntu-24.04")
        );
        assert_eq!(
            wsl_distribution_root_path("Ubuntu-24.04"),
            PathBuf::from(r"\\wsl.localhost\Ubuntu-24.04\")
        );
        assert_eq!(
            wsl_distribution_root_name(&LocationDescriptor::file_system(r"D:\Ubuntu-24.04")),
            None
        );
    }
}
