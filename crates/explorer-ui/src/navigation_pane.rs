//! Typed Windows Explorer navigation-pane presentation data.

use std::{
    path::PathBuf,
    sync::{OnceLock, RwLock},
};

use explorer_model::{LocationDescriptor, ShellIconKey, ShellIconTheme, SyntheticRoot};

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum NavigationIcon {
    Home,
    QuickAccess,
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
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AdbNavigationDevice {
    pub serial: String,
    pub label: String,
    pub available: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SftpNavigationProfile {
    pub alias: String,
    pub label: String,
    pub container_identity: [u8; 16],
    pub available: bool,
}

static ADB_NAVIGATION_DEVICES: OnceLock<RwLock<Vec<AdbNavigationDevice>>> = OnceLock::new();
static SFTP_NAVIGATION_PROFILES: OnceLock<RwLock<Vec<SftpNavigationProfile>>> = OnceLock::new();

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

    fn phone_root() -> Self {
        Self {
            id: "phones".to_owned(),
            label: "手機".to_owned(),
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

    fn sftp_root() -> Self {
        Self {
            id: "sftp".to_owned(),
            label: "SFTP".to_owned(),
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
    !parent_is_this_pc
        || (filesystem_drive_root(child).is_none()
            && drive_root_display_letter(display_name).is_none())
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

pub fn windows_navigation_items() -> Vec<NavigationItem> {
    windows_navigation_items_with_pins(std::iter::empty())
}

/// Builds the stable Explorer root tree plus the application-owned Quick Access pins.
/// Pin descriptors are already privacy-filtered and reconstructible at this boundary.
pub fn windows_navigation_items_with_pins(
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
            "常用",
            NavigationIcon::Home,
            LocationDescriptor::synthetic(SyntheticRoot::Home),
            0,
            false,
        ),
        NavigationItem::section(
            "quick-access",
            "Quick access",
            NavigationIcon::QuickAccess,
            LocationDescriptor::synthetic(SyntheticRoot::QuickAccess),
        )
        .with_availability(quick_access_availability),
        NavigationItem::location(
            "gallery",
            "圖庫",
            NavigationIcon::Gallery,
            LocationDescriptor::ParsingName(
                "shell:::{e88865ea-0e1c-4e20-9aa6-edcd0212c87c}".into(),
            ),
            0,
            false,
        ),
    ];
    if let Some(path) = std::env::var_os("OneDrive")
        .map(PathBuf::from)
        .filter(|path| path.is_dir())
    {
        items.push(
            NavigationItem::location(
                "onedrive",
                "OneDrive - Personal",
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
            "OneDrive - Personal",
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

    for (id, label, parsing_name, icon) in [
        ("desktop", "桌面", "shell:Desktop", NavigationIcon::Desktop),
        (
            "downloads",
            "下載",
            "shell:Downloads",
            NavigationIcon::Downloads,
        ),
        (
            "documents",
            "文件",
            "shell:Personal",
            NavigationIcon::Documents,
        ),
        (
            "pictures",
            "圖片",
            "shell:My Pictures",
            NavigationIcon::Pictures,
        ),
        ("music", "音樂", "shell:My Music", NavigationIcon::Music),
        ("videos", "影片", "shell:My Video", NavigationIcon::Videos),
    ] {
        items.push(NavigationItem::location(
            id,
            label,
            icon,
            LocationDescriptor::ParsingName(parsing_name.into()),
            0,
            true,
        ));
    }

    items.push(NavigationItem::separator("computer-separator"));
    items.push(NavigationItem::location(
        "libraries",
        "Libraries",
        NavigationIcon::Libraries,
        LocationDescriptor::ParsingName("shell:Libraries".into()),
        0,
        false,
    ));
    items.push(NavigationItem::section(
        "this-pc",
        "本機",
        NavigationIcon::Computer,
        LocationDescriptor::ParsingName("shell:MyComputerFolder".into()),
    ));
    for letter in b'C'..=b'Z' {
        let root = PathBuf::from(format!("{}:\\", char::from(letter)));
        if root.is_dir() {
            let label = if letter == b'C' {
                format!("本機磁碟 ({}:)", char::from(letter))
            } else {
                format!("新增磁碟區 ({}:)", char::from(letter))
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
        "網路",
        NavigationIcon::Network,
        LocationDescriptor::ParsingName("shell:NetworkPlacesFolder".into()),
        0,
        false,
    ));
    items.push(NavigationItem::separator("phones-separator"));
    items.push(NavigationItem::phone_root());
    let devices = ADB_NAVIGATION_DEVICES
        .get_or_init(|| RwLock::new(Vec::new()))
        .read()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .clone();
    for device in devices {
        let location = explorer_model::RemoteAddress::parse(&format!("adb://{}/", device.serial))
            .ok()
            .and_then(|address| address.to_deterministic_location(1).ok());
        items.push(NavigationItem {
            id: format!("phone-{}", device.serial),
            label: device.label,
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
    items.push(NavigationItem::sftp_root());
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
            label: profile.label,
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
    items.push(NavigationItem::location(
        "recycle-bin",
        "Recycle Bin",
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
            Some(LocationDescriptor::FileSystem(left)),
            Some(LocationDescriptor::FileSystem(right)),
        ) => {
            let left = left.to_string_lossy();
            let right = right.to_string_lossy();
            left.eq_ignore_ascii_case(&right)
                || (item.id.starts_with("drive-")
                    && right
                        .to_ascii_lowercase()
                        .starts_with(&left.to_ascii_lowercase()))
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

    #[test]
    fn navigation_contract_has_stable_unique_ids_and_explorer_section_order() {
        let items = windows_navigation_items();
        let mut ids = std::collections::HashSet::new();
        assert!(items.iter().all(|item| ids.insert(item.id.as_str())));
        let position = |id| items.iter().position(|item| item.id == id).unwrap();
        assert!(position("home") < position("gallery"));
        assert!(position("gallery") < position("this-pc"));
        assert!(position("this-pc") < position("network"));
    }

    #[test]
    fn optional_navigation_roots_are_truthful_and_gallery_uses_real_shell_identity() {
        let items = windows_navigation_items_with_pins(std::iter::empty());
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

        let pinned = windows_navigation_items_with_pins([(
            "fixture".to_owned(),
            LocationDescriptor::file_system(r"C:\fixture"),
        )]);
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
        }]);
        configure_sftp_navigation_profiles(vec![SftpNavigationProfile {
            alias: "production".to_owned(),
            label: "production".to_owned(),
            container_identity: [9; 16],
            available: true,
        }]);
        let items = windows_navigation_items();
        assert!(items.iter().any(|item| item.id == "phones"));
        assert!(items.iter().any(|item| item.id == "sftp"));
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
                .any(|item| item.id == "sftp-profile-production")
        );
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
}
