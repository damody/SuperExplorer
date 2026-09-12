//! Snapshot Windows File Explorer pinned folders for first-launch Quick Access.

#![expect(
    unsafe_code,
    reason = "enumerating Explorer Home/Links pins uses STA COM Shell APIs"
)]

use std::{
    collections::HashSet,
    ffi::OsString,
    os::windows::ffi::{OsStrExt as _, OsStringExt as _},
    path::{Path, PathBuf},
    ptr,
    sync::mpsc,
    thread,
    time::Duration,
};

use explorer_model::{LocationDescriptor, PersistedQuickAccessPin};
use windows::{
    Win32::{
        Foundation::{HWND, PROPERTYKEY},
        Storage::FileSystem::WIN32_FIND_DATAW,
        System::Com::{CLSCTX_INPROC_SERVER, CoCreateInstance, IPersistFile, STGM_READ},
        UI::Shell::{
            FOLDERID_Links, IEnumIDList, ILCombine, IShellItem, IShellItem2, IShellLinkW,
            SHCONTF_FOLDERS, SHCONTF_INCLUDEHIDDEN, SHCreateItemFromIDList, ShellLink,
        },
    },
    core::{GUID, Interface as _, PCWSTR},
};

use crate::navigation::{OwnedPidl, ResolvedLocation, child_entry, resolve_location};

const SNAPSHOT_TIMEOUT: Duration = Duration::from_secs(8);
const MAX_PINS: usize = 32;
const PKEY_IS_PINNED_TO_NAMESPACE_TREE: PROPERTYKEY = PROPERTYKEY {
    fmtid: GUID::from_u128(0x5d76b67f_9b3d_44bb_b6ae_25da4f638a67),
    pid: 2,
};

/// Returns folders currently pinned in Windows File Explorer Quick Access / Home.
///
/// # Errors
///
/// Returns a string when the dedicated STA cannot start or the Shell snapshot times out.
pub fn snapshot_windows_explorer_pinned_folders() -> Result<Vec<PersistedQuickAccessPin>, String> {
    let (sender, receiver) = mpsc::channel();
    thread::Builder::new()
        .name("explorer-pin-snapshot".into())
        .spawn(move || {
            let apartment = match crate::sta::ApartmentGuard::initialize() {
                Ok(apartment) => apartment,
                Err(error) => {
                    let _ = sender.send(Err(format!("Explorer pin snapshot STA failed: {error}")));
                    return;
                }
            };
            let result = snapshot_pinned_folders_on_sta();
            drop(apartment);
            let _ = sender.send(result);
        })
        .map_err(|error| format!("Explorer pin snapshot thread failed: {error}"))?;
    receiver
        .recv_timeout(SNAPSHOT_TIMEOUT)
        .map_err(|_| "Explorer pin snapshot timed out".to_owned())?
}

fn snapshot_pinned_folders_on_sta() -> Result<Vec<PersistedQuickAccessPin>, String> {
    let mut pins = snapshot_named_pinned_folders("shell:HomeFolder");
    if pins.is_empty() {
        pins = snapshot_named_pinned_folders("shell:::{679f85cb-0220-4080-b29b-5540cc05aab6}");
    }
    if pins.is_empty() {
        pins = snapshot_links_folder_pins();
    }
    Ok(finalize_pins(pins))
}

fn snapshot_named_pinned_folders(parsing_name: &str) -> Vec<PersistedQuickAccessPin> {
    let Ok(resolved) = resolve_location(&LocationDescriptor::ParsingName(parsing_name.to_owned()))
    else {
        return Vec::new();
    };
    let children = enumerate_folder_children(&resolved);
    let mut pins = Vec::new();
    for relative in children {
        if !item_is_pinned(&resolved, &relative) {
            continue;
        }
        let Ok(entry) = child_entry(&resolved, &relative) else {
            continue;
        };
        if !entry.is_container {
            continue;
        }
        pins.push(PersistedQuickAccessPin {
            location: entry.location,
            display_name: entry.display_name,
            order: 0,
        });
        if pins.len() >= MAX_PINS {
            break;
        }
    }
    pins
}

fn snapshot_links_folder_pins() -> Vec<PersistedQuickAccessPin> {
    let links = LocationDescriptor::KnownFolder(FOLDERID_Links.to_u128().to_be_bytes());
    let Ok(resolved) = resolve_location(&links) else {
        return Vec::new();
    };
    let children = enumerate_folder_children(&resolved);
    let mut pins = Vec::new();
    for relative in children {
        let Ok(entry) = child_entry(&resolved, &relative) else {
            continue;
        };
        let Some((location, display_name)) =
            resolve_links_entry(&entry.location, &entry.display_name)
        else {
            continue;
        };
        pins.push(PersistedQuickAccessPin {
            location,
            display_name,
            order: 0,
        });
        if pins.len() >= MAX_PINS {
            break;
        }
    }
    pins
}

fn enumerate_folder_children(resolved: &ResolvedLocation) -> Vec<OwnedPidl> {
    let mut enumerator: Option<IEnumIDList> = None;
    let flags = (SHCONTF_FOLDERS.0 | SHCONTF_INCLUDEHIDDEN.0) as u32;
    // SAFETY: the folder remains on this STA; the output slot is writable for the call.
    if unsafe {
        resolved
            .folder
            .EnumObjects(HWND::default(), flags, &raw mut enumerator)
    }
    .is_err()
    {
        return Vec::new();
    }
    let Some(enumerator) = enumerator else {
        return Vec::new();
    };
    let mut children = Vec::new();
    loop {
        let mut raw = ptr::null_mut();
        let mut fetched = 0_u32;
        // SAFETY: output slots are writable; returned PIDL ownership transfers immediately.
        let result =
            unsafe { enumerator.Next(std::slice::from_mut(&mut raw), Some(&raw mut fetched)) };
        if fetched == 0 {
            let _ = result.ok();
            break;
        }
        match OwnedPidl::from_raw(raw, "enumerate Explorer pin") {
            Ok(relative) => children.push(relative),
            Err(_) => continue,
        }
        if children.len() >= MAX_PINS.saturating_mul(8) {
            break;
        }
    }
    children
}

fn item_is_pinned(resolved: &ResolvedLocation, relative: &OwnedPidl) -> bool {
    // SAFETY: parent and relative PIDLs are live on this STA; ILCombine returns a new allocation.
    let raw_absolute =
        unsafe { ILCombine(Some(resolved.absolute.as_ptr()), Some(relative.as_ptr())) };
    let Ok(absolute) = OwnedPidl::from_raw(raw_absolute, "combine Explorer pin") else {
        return false;
    };
    // SAFETY: absolute is a complete PIDL owned on this STA.
    let item: IShellItem = match unsafe { SHCreateItemFromIDList(absolute.as_ptr()) } {
        Ok(item) => item,
        Err(_) => return false,
    };
    let Ok(item2): Result<IShellItem2, _> = item.cast() else {
        return false;
    };
    let key = PKEY_IS_PINNED_TO_NAMESPACE_TREE;
    // SAFETY: item2 is apartment-local and key is a live PROPERTYKEY for the call.
    unsafe { item2.GetBool(&raw const key) }
        .ok()
        .is_some_and(|value| value.as_bool())
}

fn resolve_links_entry(
    location: &LocationDescriptor,
    display_name: &str,
) -> Option<(LocationDescriptor, String)> {
    let LocationDescriptor::FileSystem(path) = location else {
        return None;
    };
    if path.is_dir() {
        return Some((location.clone(), display_name.to_owned()));
    }
    if path
        .extension()
        .is_none_or(|extension| !extension.eq_ignore_ascii_case("lnk"))
    {
        return None;
    }
    let target = resolve_shortcut_folder(path)?;
    let name = target
        .file_name()
        .map(|value| value.to_string_lossy().into_owned())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| display_name.to_owned());
    Some((LocationDescriptor::file_system(target), name))
}

fn resolve_shortcut_folder(path: &Path) -> Option<PathBuf> {
    // SAFETY: this runs on the initialized Shell STA and creates the registered in-process
    // ShellLink COM class without aggregation.
    let link: IShellLinkW =
        unsafe { CoCreateInstance(&ShellLink, None, CLSCTX_INPROC_SERVER).ok()? };
    let persist: IPersistFile = link.cast().ok()?;
    let wide = path
        .as_os_str()
        .encode_wide()
        .chain(Some(0))
        .collect::<Vec<_>>();
    // SAFETY: wide is NUL-terminated and persist is apartment-local.
    unsafe { persist.Load(PCWSTR(wide.as_ptr()), STGM_READ) }.ok()?;
    let mut buffer = [0_u16; 32768];
    // SAFETY: buffer is a writable UTF-16 path slot; no WIN32_FIND_DATA is requested.
    unsafe { link.GetPath(&mut buffer, ptr::null_mut::<WIN32_FIND_DATAW>(), 0) }.ok()?;
    let length = buffer
        .iter()
        .position(|unit| *unit == 0)
        .unwrap_or(buffer.len());
    if length == 0 {
        return None;
    }
    let target = PathBuf::from(OsString::from_wide(&buffer[..length]));
    target.is_dir().then_some(target)
}

fn finalize_pins(pins: Vec<PersistedQuickAccessPin>) -> Vec<PersistedQuickAccessPin> {
    let mut seen = HashSet::new();
    let mut unique = Vec::new();
    for pin in pins {
        if pin.display_name.trim().is_empty() || !seen.insert(pin.location.clone()) {
            continue;
        }
        unique.push(PersistedQuickAccessPin {
            order: u32::try_from(unique.len()).unwrap_or(u32::MAX),
            ..pin
        });
        if unique.len() >= MAX_PINS {
            break;
        }
    }
    unique
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finalize_pins_deduplicates_and_reassigns_order() {
        let pins = finalize_pins(vec![
            PersistedQuickAccessPin {
                location: LocationDescriptor::file_system(r"D:\Projects"),
                display_name: "Projects".to_owned(),
                order: 9,
            },
            PersistedQuickAccessPin {
                location: LocationDescriptor::file_system(r"D:\Projects"),
                display_name: "Duplicate".to_owned(),
                order: 8,
            },
            PersistedQuickAccessPin {
                location: LocationDescriptor::file_system(r"C:\Users\fixture\Downloads"),
                display_name: "Downloads".to_owned(),
                order: 3,
            },
        ]);
        assert_eq!(pins.len(), 2);
        assert_eq!(pins[0].display_name, "Projects");
        assert_eq!(pins[0].order, 0);
        assert_eq!(pins[1].display_name, "Downloads");
        assert_eq!(pins[1].order, 1);
    }

    #[test]
    fn live_windows_explorer_pinned_folders_are_unique_containers() {
        let _lock = crate::live_explorer_lock();
        let pins = snapshot_windows_explorer_pinned_folders().expect("snapshot");
        let mut orders = HashSet::new();
        let mut locations = HashSet::new();
        for pin in pins {
            assert!(orders.insert(pin.order), "duplicate pin order");
            assert!(
                locations.insert(pin.location.clone()),
                "duplicate pin location"
            );
            assert!(!pin.display_name.trim().is_empty());
        }
    }
}
