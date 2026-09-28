//! Local drive media probe used only to decide whether Columns may be effective.
#![allow(
    unsafe_code,
    reason = "GetDriveTypeW is a bounded Win32 query with a NUL-terminated root"
)]

use std::path::{Component, Path, PathBuf};

use explorer_model::DriveKind;

pub fn probe_drive_kind(path: &Path) -> DriveKind {
    let mut root = PathBuf::new();
    for component in path.components() {
        match component {
            Component::Prefix(_) => root.push(component.as_os_str()),
            Component::RootDir => {
                root.push(component.as_os_str());
                break;
            }
            _ => break,
        }
    }
    if root.as_os_str().is_empty() {
        return DriveKind::Unknown;
    }
    drive_kind_from_root(&root)
}

fn drive_kind_from_root(path: &Path) -> DriveKind {
    let wide = std::os::windows::ffi::OsStrExt::encode_wide(path.as_os_str())
        .chain(std::iter::once(0))
        .collect::<Vec<_>>();
    let root = windows::core::PCWSTR(wide.as_ptr());
    // SAFETY: `wide` is a live NUL-terminated drive root for this call only.
    let raw = unsafe { windows::Win32::Storage::FileSystem::GetDriveTypeW(root) };
    match raw {
        2 => DriveKind::Removable,
        3 => DriveKind::Fixed,
        4 => DriveKind::Network,
        5 => DriveKind::Optical,
        6 => DriveKind::RamDisk,
        _ => DriveKind::Unknown,
    }
}
