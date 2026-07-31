//! Windows Shell location resolution and incremental child enumeration.
#![allow(
    unsafe_code,
    reason = "Shell PIDLs and COM interfaces require audited Win32 pointer calls"
)]

use std::{
    mem,
    mem::size_of,
    os::windows::ffi::OsStrExt as _,
    path::{Path, PathBuf},
    ptr,
    time::UNIX_EPOCH,
};

use explorer_common::{ExplorerError, ExplorerErrorKind};
use explorer_model::{
    BreadcrumbMenuItem, DriveAvailability, DriveKind, DriveMetadata, ExplorerEvent, FileEntry,
    FileEntryMetadata, LocationDescriptor, LocationMetadata, NamespaceCapabilities, PropertyValue,
    RequestContext, ShellItemId,
};
use windows::{
    Win32::{
        Foundation::{FILETIME, HWND, SYSTEMTIME},
        Globalization::{DATE_SHORTDATE, GetDateFormatEx, GetTimeFormatEx, TIME_NOSECONDS},
        Storage::FileSystem::{
            CreateFileW, FILE_FLAG_BACKUP_SEMANTICS, FILE_FLAGS_AND_ATTRIBUTES, FILE_ID_INFO,
            FILE_READ_ATTRIBUTES, FILE_SHARE_DELETE, FILE_SHARE_READ, FILE_SHARE_WRITE, FileIdInfo,
            GetDiskFreeSpaceExW, GetDriveTypeW, GetFileInformationByHandleEx,
            GetVolumeInformationW, OPEN_EXISTING,
        },
        System::{
            SystemServices::{
                SFGAO_BROWSABLE, SFGAO_CANCOPY, SFGAO_CANDELETE, SFGAO_CANMOVE, SFGAO_CANRENAME,
                SFGAO_DROPTARGET, SFGAO_FOLDER, SFGAO_HASPROPSHEET,
            },
            Time::{FileTimeToSystemTime, SystemTimeToTzSpecificLocalTime},
        },
        UI::{
            Shell::{
                Common::ITEMIDLIST, IEnumIDList, ILCombine, ILGetSize, IShellFolder, IShellItem,
                SHCONTF_FOLDERS, SHCONTF_INCLUDEHIDDEN, SHCONTF_INCLUDESUPERHIDDEN,
                SHCONTF_NONFOLDERS, SHCreateItemFromIDList, SHFILEINFOW, SHGFI_TYPENAME,
                SHGetDesktopFolder, SHGetFileInfoW, SHGetKnownFolderIDList, SHGetNameFromIDList,
                SHParseDisplayName, SIGDN_FILESYSPATH, SIGDN_NORMALDISPLAY, ShellExecuteW,
            },
            WindowsAndMessaging::SW_SHOWNORMAL,
        },
    },
    core::{GUID, HSTRING, PCWSTR},
};

/// Maximum number of rows delivered in one model mutation.
pub const DIRECTORY_BATCH_ITEM_CAP: usize = 64;
/// Approximate maximum owned payload delivered in one model mutation.
pub const DIRECTORY_BATCH_BYTE_CAP: usize = 256 * 1024;

struct DirectoryBatchAccumulator {
    entries: Vec<FileEntry>,
    estimated_bytes: usize,
}

impl DirectoryBatchAccumulator {
    fn new() -> Self {
        Self {
            entries: Vec::with_capacity(DIRECTORY_BATCH_ITEM_CAP),
            estimated_bytes: 0,
        }
    }

    fn push(&mut self, entry: FileEntry) -> Option<Vec<FileEntry>> {
        let size = estimate_entry_bytes(&entry);
        let flush_before = !self.entries.is_empty()
            && (self.entries.len() >= DIRECTORY_BATCH_ITEM_CAP
                || self.estimated_bytes.saturating_add(size) > DIRECTORY_BATCH_BYTE_CAP);
        let ready = flush_before.then(|| mem::take(&mut self.entries));
        if flush_before {
            self.estimated_bytes = 0;
        }
        self.estimated_bytes = self.estimated_bytes.saturating_add(size);
        self.entries.push(entry);
        ready
    }

    fn finish(self) -> Option<Vec<FileEntry>> {
        (!self.entries.is_empty()).then_some(self.entries)
    }
}

/// A Shell-allocated absolute or relative PIDL.
///
/// # Invariant
///
/// `raw` is either null or points to one complete `ITEMIDLIST` allocated by a Shell API with the
/// COM task allocator. Ownership is unique and never crosses the STA boundary.
pub(crate) struct OwnedPidl {
    raw: crate::native::CoTaskMem<ITEMIDLIST>,
}

impl OwnedPidl {
    fn from_raw(raw: *mut ITEMIDLIST, operation: &'static str) -> Result<Self, ExplorerError> {
        if raw.is_null() {
            Err(shell_error(operation, None, "Shell returned a null PIDL"))
        } else {
            // SAFETY: this constructor is called only at Shell API ownership-transfer sites and
            // the null case was rejected above.
            let Some(raw) = (unsafe { crate::native::CoTaskMem::from_raw(raw) }) else {
                return Err(shell_error(
                    operation,
                    None,
                    "Shell returned an invalid PIDL",
                ));
            };
            Ok(Self { raw })
        }
    }

    pub(crate) fn as_ptr(&self) -> *const ITEMIDLIST {
        self.raw.as_ptr()
    }

    fn bytes(&self) -> Result<Vec<u8>, ExplorerError> {
        // SAFETY: `self.raw` satisfies OwnedPidl's complete-PIDL invariant for this lifetime.
        let size = unsafe { ILGetSize(Some(self.as_ptr())) } as usize;
        if size < size_of::<u16>() {
            return Err(shell_error(
                "copy PIDL",
                None,
                "Shell returned an invalid PIDL size",
            ));
        }
        // SAFETY: ILGetSize returned the byte extent of the live PIDL allocation.
        Ok(unsafe { std::slice::from_raw_parts(self.raw.as_ptr().cast::<u8>(), size) }.to_vec())
    }
}

/// Reconstructs one absolute PIDL on the owning STA for adapters requiring relative children.
pub(crate) fn location_absolute_pidl(
    descriptor: &LocationDescriptor,
) -> Result<OwnedPidl, ExplorerError> {
    match descriptor {
        LocationDescriptor::FileSystem(path) => parse_display_name(&shell_path_text(path)),
        LocationDescriptor::ParsingName(name) => parse_display_name(name),
        LocationDescriptor::KnownFolder(bytes) => {
            let guid = GUID::from_u128(u128::from_be_bytes(*bytes));
            // SAFETY: initialized GUID; returned task allocation transfers to OwnedPidl.
            OwnedPidl::from_raw(
                unsafe { SHGetKnownFolderIDList(&raw const guid, 0, None) }
                    .map_err(|error| windows_error("resolve known folder PIDL", &error))?,
                "resolve known folder PIDL",
            )
        }
        LocationDescriptor::ShellNamespace(bytes) => {
            let aligned = AlignedPidl::from_bytes(bytes)?;
            // SAFETY: aligned is a validated PIDL and ILCombine returns an owned clone.
            OwnedPidl::from_raw(
                unsafe { ILCombine(None, Some(aligned.as_ptr())) },
                "copy namespace PIDL",
            )
        }
    }
}

/// Aligned borrowed storage for a PIDL reconstructed from owned protocol bytes.
struct AlignedPidl(Vec<u16>);

impl AlignedPidl {
    fn from_bytes(bytes: &[u8]) -> Result<Self, ExplorerError> {
        if bytes.len() < 2 || !bytes.ends_with(&[0, 0]) {
            return Err(shell_error(
                "resolve Shell namespace",
                None,
                "Opaque PIDL bytes are malformed",
            ));
        }
        // ITEMIDLIST starts at a two-byte-aligned address, but each SHITEMID's byte count may be
        // odd. Round storage up for alignment without rejecting a valid odd-sized PIDL payload.
        let mut words = vec![0_u16; bytes.len().div_ceil(2)];
        // SAFETY: both regions are valid for exactly bytes.len() bytes and cannot overlap.
        unsafe {
            ptr::copy_nonoverlapping(bytes.as_ptr(), words.as_mut_ptr().cast::<u8>(), bytes.len());
        }
        Ok(Self(words))
    }

    fn as_ptr(&self) -> *const ITEMIDLIST {
        self.0.as_ptr().cast()
    }
}

pub(crate) struct ResolvedLocation {
    absolute: OwnedPidl,
    folder: IShellFolder,
    descriptor: LocationDescriptor,
    display_title: String,
}

impl ResolvedLocation {
    pub(crate) fn metadata(&self) -> LocationMetadata {
        LocationMetadata {
            descriptor: self.descriptor.clone(),
            display_title: self.display_title.clone(),
            can_go_up: true,
            can_write: true,
        }
    }
}

pub(crate) fn resolve_location(
    descriptor: &LocationDescriptor,
) -> Result<ResolvedLocation, ExplorerError> {
    let absolute = match descriptor {
        LocationDescriptor::FileSystem(path) => parse_display_name(&shell_path_text(path))?,
        LocationDescriptor::ParsingName(name) => parse_display_name(name)?,
        LocationDescriptor::KnownFolder(bytes) => {
            let guid = GUID::from_u128(u128::from_be_bytes(*bytes));
            // SAFETY: the GUID is initialized, no token is supplied, and ownership of the returned
            // COM-task allocation is immediately transferred to OwnedPidl.
            let raw = unsafe { SHGetKnownFolderIDList(&raw const guid, 0, None) }
                .map_err(|error| windows_error("resolve known folder", &error))?;
            OwnedPidl::from_raw(raw, "resolve known folder")?
        }
        LocationDescriptor::ShellNamespace(bytes) => {
            let aligned = AlignedPidl::from_bytes(bytes)?;
            // SAFETY: aligned contains one validated, terminated PIDL for the duration of the call.
            let item: IShellItem = unsafe { SHCreateItemFromIDList(aligned.as_ptr()) }
                .map_err(|error| windows_error("resolve Shell namespace", &error))?;
            drop(item);
            let raw = unsafe { ILCombine(None, Some(aligned.as_ptr())) };
            OwnedPidl::from_raw(raw, "copy Shell namespace PIDL")?
        }
    };

    // SAFETY: COM is initialized on the caller's STA and absolute is a complete PIDL.
    let desktop = unsafe { SHGetDesktopFolder() }
        .map_err(|error| windows_error("get desktop Shell folder", &error))?;
    // SAFETY: the absolute PIDL remains alive through the synchronous bind and no bind context is
    // required for a normal navigation bind.
    // The Desktop Known Folder resolves to the empty absolute PIDL. Binding the
    // desktop folder to that sentinel returns E_INVALIDARG; the desktop object
    // itself is the correct container.
    let folder: IShellFolder = if unsafe { ILGetSize(Some(absolute.as_ptr())) } <= 2 {
        desktop
    } else {
        unsafe { desktop.BindToObject(absolute.as_ptr(), None) }
            .map_err(|error| windows_error("bind Shell folder", &error))?
    };
    let display_title = name_from_pidl(absolute.as_ptr(), SIGDN_NORMALDISPLAY)
        .unwrap_or_else(|_| "Folder".to_owned());
    let filesystem_path = name_from_pidl(absolute.as_ptr(), SIGDN_FILESYSPATH).ok();
    let published_descriptor = canonical_location_descriptor(descriptor, filesystem_path);

    Ok(ResolvedLocation {
        absolute,
        folder,
        descriptor: published_descriptor,
        display_title,
    })
}

fn canonical_location_descriptor(
    requested: &LocationDescriptor,
    filesystem_path: Option<String>,
) -> LocationDescriptor {
    if let LocationDescriptor::FileSystem(path) = requested {
        let text = shell_path_text(path);
        if is_bare_drive_designator(&text) {
            return filesystem_path
                .filter(|path| !path.is_empty())
                .map_or_else(|| requested.clone(), LocationDescriptor::file_system);
        }
        return requested.clone();
    }
    if matches!(requested, LocationDescriptor::ShellNamespace(_)) {
        return requested.clone();
    }
    filesystem_path
        .filter(|path| !path.is_empty())
        .map_or_else(|| requested.clone(), LocationDescriptor::file_system)
}

fn is_bare_drive_designator(path: &str) -> bool {
    let bytes = path.as_bytes();
    bytes.len() == 2 && bytes[0].is_ascii_alphabetic() && bytes[1] == b':'
}

pub(crate) fn shell_item(descriptor: &LocationDescriptor) -> Result<IShellItem, ExplorerError> {
    match descriptor {
        LocationDescriptor::FileSystem(path) => {
            let text = HSTRING::from(shell_path_text(path));
            // SAFETY: text remains live for the synchronous Shell item creation call.
            unsafe { windows::Win32::UI::Shell::SHCreateItemFromParsingName(&text, None) }
                .map_err(|error| windows_error("resolve file-operation item", &error))
        }
        LocationDescriptor::ParsingName(name) => {
            let text = HSTRING::from(name);
            // SAFETY: text remains live for the synchronous Shell item creation call.
            unsafe { windows::Win32::UI::Shell::SHCreateItemFromParsingName(&text, None) }
                .map_err(|error| windows_error("resolve file-operation item", &error))
        }
        LocationDescriptor::KnownFolder(bytes) => {
            let guid = GUID::from_u128(u128::from_be_bytes(*bytes));
            // SAFETY: initialized GUID and transferred CoTaskMem PIDL ownership.
            let pidl = OwnedPidl::from_raw(
                unsafe { SHGetKnownFolderIDList(&raw const guid, 0, None) }
                    .map_err(|error| windows_error("resolve known folder item", &error))?,
                "resolve known folder item",
            )?;
            // SAFETY: pidl remains live throughout synchronous item creation.
            unsafe { SHCreateItemFromIDList(pidl.as_ptr()) }
                .map_err(|error| windows_error("create known folder item", &error))
        }
        LocationDescriptor::ShellNamespace(bytes) => {
            let pidl = AlignedPidl::from_bytes(bytes)?;
            // SAFETY: aligned PIDL remains live throughout synchronous item creation.
            unsafe { SHCreateItemFromIDList(pidl.as_ptr()) }
                .map_err(|error| windows_error("create namespace item", &error))
        }
    }
}

/// Resolves an owned Shell parent chain on the STA. Only strings/descriptors leave this helper;
/// no PIDL or COM interface crosses the protocol boundary.
pub(crate) fn shell_parent_chain(
    descriptor: &LocationDescriptor,
) -> Result<Vec<(LocationDescriptor, String)>, ExplorerError> {
    let this_pc = shell_item(&LocationDescriptor::ParsingName(
        "shell:MyComputerFolder".to_owned(),
    ))?;
    let this_pc_parsing_name = shell_item_name(
        &this_pc,
        windows::Win32::UI::Shell::SIGDN_DESKTOPABSOLUTEPARSING,
    )?;
    let mut item = shell_item(descriptor)?;
    let mut chain = Vec::new();
    for _ in 0..64 {
        let display_name = shell_item_name(&item, SIGDN_NORMALDISPLAY)?;
        let owned_descriptor = match shell_item_name(&item, SIGDN_FILESYSPATH) {
            Ok(path) if !path.is_empty() => LocationDescriptor::file_system(path),
            _ => LocationDescriptor::ParsingName(shell_item_name(
                &item,
                windows::Win32::UI::Shell::SIGDN_DESKTOPABSOLUTEPARSING,
            )?),
        };
        if chain
            .last()
            .is_some_and(|(existing, _)| *existing == owned_descriptor)
        {
            break;
        }
        chain.push((owned_descriptor, display_name));
        // SAFETY: item is used only on its owning Shell STA; the returned interface stays local.
        let Ok(parent) = (unsafe { item.GetParent() }) else {
            break;
        };
        item = parent;
    }
    chain.reverse();
    Ok(normalize_shell_parent_chain(chain, &this_pc_parsing_name))
}

fn normalize_shell_parent_chain(
    mut chain: Vec<(LocationDescriptor, String)>,
    this_pc_parsing_name: &str,
) -> Vec<(LocationDescriptor, String)> {
    let Some(this_pc_index) = chain.iter().position(|(location, _)| {
        matches!(
            location,
            LocationDescriptor::ParsingName(name)
                if name.eq_ignore_ascii_case(this_pc_parsing_name)
                    || name.eq_ignore_ascii_case("shell:MyComputerFolder")
        )
    }) else {
        return chain;
    };
    chain.drain(..this_pc_index);
    if let Some((location, _)) = chain.first_mut() {
        *location = LocationDescriptor::ParsingName("shell:MyComputerFolder".to_owned());
    }
    chain
}

pub(crate) fn shell_item_name(
    item: &IShellItem,
    format: windows::Win32::UI::Shell::SIGDN,
) -> Result<String, ExplorerError> {
    // SAFETY: item is live on its owning STA. The result is a CoTaskMem PWSTR.
    let value = unsafe { item.GetDisplayName(format) }
        .map_err(|error| windows_error("read Shell item name", &error))?;
    // SAFETY: GetDisplayName transferred one non-null CoTaskMem string.
    let owned = unsafe { crate::native::CoTaskMem::from_raw(value.0) }
        .ok_or_else(|| shell_error("read Shell item name", None, "Shell returned null text"))?;
    // SAFETY: the Shell result is NUL-terminated while owned remains alive.
    unsafe { PCWSTR(owned.as_ptr()).to_string() }
        .map_err(|error| shell_error("decode Shell item name", None, &error.to_string()))
}

fn parse_display_name(value: &str) -> Result<OwnedPidl, ExplorerError> {
    let value = HSTRING::from(value);
    let mut raw = ptr::null_mut();
    // SAFETY: the input is a live NUL-terminated HSTRING; output points to writable pointer
    // storage and ownership is transferred to OwnedPidl on success.
    unsafe { SHParseDisplayName(&value, None, &raw mut raw, 0, None) }
        .map_err(|error| windows_error("parse Shell location", &error))?;
    OwnedPidl::from_raw(raw, "parse Shell location")
}

pub(crate) fn shell_path_text(path: &Path) -> String {
    let text = path.as_os_str().to_string_lossy();
    if let Some(unc) = text.strip_prefix(r"\\?\UNC\") {
        format!(r"\\{unc}")
    } else if let Some(drive) = text.strip_prefix(r"\\?\") {
        drive.to_owned()
    } else {
        text.into_owned()
    }
}

pub(crate) fn enumerate_directory(
    context: &RequestContext,
    resolved: &ResolvedLocation,
    mut emit: impl FnMut(ExplorerEvent) -> bool,
) -> Result<bool, ExplorerError> {
    let mut enumerator: Option<IEnumIDList> = None;
    let flags = (SHCONTF_FOLDERS.0
        | SHCONTF_NONFOLDERS.0
        | SHCONTF_INCLUDEHIDDEN.0
        | SHCONTF_INCLUDESUPERHIDDEN.0) as u32;
    // SAFETY: the folder is apartment-affine and used only on its owning STA; the output slot is
    // valid for the duration of the call.
    unsafe {
        resolved
            .folder
            .EnumObjects(HWND::default(), flags, &raw mut enumerator)
    }
    .ok()
    .map_err(|error| windows_error("enumerate Shell folder", &error))?;
    let Some(enumerator) = enumerator else {
        return Ok(true);
    };

    let mut batch = DirectoryBatchAccumulator::new();
    loop {
        if context.cancellation.is_cancelled() {
            return Ok(false);
        }
        let mut raw = ptr::null_mut();
        let mut fetched = 0_u32;
        // SAFETY: the one-element output slice and fetched counter are writable. Any returned PIDL
        // becomes uniquely owned below before the next COM call.
        let result =
            unsafe { enumerator.Next(std::slice::from_mut(&mut raw), Some(&raw mut fetched)) };
        if fetched == 0 {
            result
                .ok()
                .map_err(|error| windows_error("enumerate next Shell item", &error))?;
            break;
        }
        let relative = OwnedPidl::from_raw(raw, "enumerate Shell item")?;
        let mut entry = child_entry(resolved, &relative)?;
        if matches!(
            &resolved.descriptor,
            LocationDescriptor::ParsingName(value)
                if value.eq_ignore_ascii_case("shell:RecycleBinFolder")
        ) {
            entry.metadata.namespace_capabilities = NamespaceCapabilities::from_public_bits(
                entry.metadata.namespace_capabilities.bits() | NamespaceCapabilities::RESTORE,
            );
        }
        if let Some(entries) = batch.push(entry) {
            if !emit(ExplorerEvent::DirectoryBatch {
                context: context.clone(),
                entries,
            }) {
                return Ok(false);
            }
        }
    }
    if let Some(entries) = batch.finish()
        && !emit(ExplorerEvent::DirectoryBatch {
            context: context.clone(),
            entries,
        })
    {
        return Ok(false);
    }
    Ok(!context.cancellation.is_cancelled())
}

/// Enumerates only direct Shell folder children in bounded batches.
pub(crate) fn enumerate_child_containers(
    context: &RequestContext,
    descriptor: &LocationDescriptor,
    mut emit: impl FnMut(Vec<BreadcrumbMenuItem>) -> Result<(), ExplorerError>,
) -> Result<bool, ExplorerError> {
    let resolved = resolve_location(descriptor)?;
    let mut enumerator: Option<IEnumIDList> = None;
    let flags = (SHCONTF_FOLDERS.0 | SHCONTF_INCLUDEHIDDEN.0 | SHCONTF_INCLUDESUPERHIDDEN.0) as u32;
    // SAFETY: the folder and enumerator remain on this STA; the output slot is writable.
    unsafe {
        resolved
            .folder
            .EnumObjects(HWND::default(), flags, &raw mut enumerator)
    }
    .ok()
    .map_err(|error| windows_error("enumerate breadcrumb children", &error))?;
    let Some(enumerator) = enumerator else {
        return Ok(true);
    };
    let mut batch = Vec::with_capacity(DIRECTORY_BATCH_ITEM_CAP);
    loop {
        if context.cancellation.is_cancelled() {
            return Ok(false);
        }
        let mut raw = ptr::null_mut();
        let mut fetched = 0_u32;
        // SAFETY: output slots are writable; returned PIDL ownership transfers immediately.
        let result =
            unsafe { enumerator.Next(std::slice::from_mut(&mut raw), Some(&raw mut fetched)) };
        if fetched == 0 {
            result
                .ok()
                .map_err(|error| windows_error("enumerate breadcrumb child", &error))?;
            break;
        }
        let relative = OwnedPidl::from_raw(raw, "enumerate breadcrumb child")?;
        let entry = child_entry(&resolved, &relative)?;
        if entry.is_container {
            batch.push(BreadcrumbMenuItem {
                display_name: entry.display_name,
                location: entry.location,
            });
        }
        if batch.len() == DIRECTORY_BATCH_ITEM_CAP {
            emit(mem::take(&mut batch))?;
        }
    }
    if !batch.is_empty() {
        emit(batch)?;
    }
    Ok(!context.cancellation.is_cancelled())
}

#[cfg(test)]
#[allow(
    clippy::items_after_test_module,
    reason = "batching tests sit beside the accumulator while Win32 conversion helpers follow"
)]
mod tests {
    use super::{
        DIRECTORY_BATCH_BYTE_CAP, DIRECTORY_BATCH_ITEM_CAP, DirectoryBatchAccumulator,
        canonical_location_descriptor, drive_metadata, entry_metadata, enumerate_child_containers,
        enumerate_directory, estimate_entry_bytes, is_bare_drive_designator,
        normalize_shell_parent_chain, resolve_location,
    };
    use explorer_model::{
        DriveAvailability, ExplorerEvent, FileEntry, Generation, LocationDescriptor,
        RequestContext, ShellItemId, TabId,
    };
    use explorer_test_support::OwnedTempFixture;

    #[test]
    fn batching_enforces_count_and_estimated_byte_caps() {
        let mut accumulator = DirectoryBatchAccumulator::new();
        let mut batches = Vec::new();
        for index in 0..130_u16 {
            let name = if index < 3 {
                "x".repeat(140_000)
            } else {
                format!("entry-{index}")
            };
            let entry = FileEntry {
                id: ShellItemId::from_provider_bytes(index.to_le_bytes()).expect("identity"),
                display_name: name,
                location: LocationDescriptor::file_system(format!(r"C:\fixture\{index}")),
                is_container: false,
                metadata: explorer_model::FileEntryMetadata::default(),
            };
            if let Some(batch) = accumulator.push(entry) {
                batches.push(batch);
            }
        }
        if let Some(batch) = accumulator.finish() {
            batches.push(batch);
        }
        assert!(
            batches
                .iter()
                .all(|batch| batch.len() <= DIRECTORY_BATCH_ITEM_CAP)
        );
        assert!(batches.iter().all(|batch| {
            let bytes = batch.iter().map(estimate_entry_bytes).sum::<usize>();
            bytes <= DIRECTORY_BATCH_BYTE_CAP || batch.len() == 1
        }));
        assert_eq!(batches.iter().map(Vec::len).sum::<usize>(), 130);
    }

    #[test]
    fn real_file_metadata_has_local_date_size_and_shell_type() {
        let path = std::path::Path::new(r"D:\test\Cargo.toml");
        let expected_size = std::fs::metadata(path).expect("fixture metadata").len();
        let metadata = entry_metadata(&LocationDescriptor::file_system(path), false, 0);
        assert_eq!(metadata.size_bytes, Some(expected_size));
        assert!(
            metadata
                .modified_display
                .as_deref()
                .is_some_and(|value| !value.is_empty())
        );
        assert!(
            metadata
                .type_display
                .as_deref()
                .is_some_and(|value| !value.is_empty())
        );
        assert_eq!(metadata.unavailable_reason, None);
    }

    #[test]
    fn namespace_capabilities_never_leak_into_filesystem_visibility_attributes() {
        let shell_attributes = windows::Win32::System::SystemServices::SFGAO_CANMOVE.0
            | windows::Win32::System::SystemServices::SFGAO_CANLINK.0;
        let metadata = entry_metadata(
            &LocationDescriptor::ParsingName("shell:fixture-child".to_owned()),
            true,
            shell_attributes,
        );

        assert_eq!(metadata.filesystem_attributes, 0);
        assert!(
            metadata
                .namespace_capabilities
                .contains(explorer_model::NamespaceCapabilities::DROP)
        );
    }

    #[test]
    fn real_d_drive_enumeration_preserves_file_metadata_in_emitted_rows() {
        let _apartment = crate::sta::ApartmentGuard::initialize().expect("STA initializes");
        let location = LocationDescriptor::file_system(r"D:\");
        let resolved = resolve_location(&location).expect("D drive resolves");
        let context = RequestContext::new(TabId::new(), Generation::new(1));
        let mut rows = Vec::new();
        enumerate_directory(&context, &resolved, |event| {
            if let ExplorerEvent::DirectoryBatch { entries, .. } = event {
                rows.extend(entries);
            }
            true
        })
        .expect("D drive enumerates");
        let files = rows
            .into_iter()
            .filter(|entry| entry.metadata.size_bytes.is_some())
            .collect::<Vec<_>>();
        assert!(
            !files.is_empty(),
            "D drive fixture contains filesystem files"
        );
        for entry in files {
            assert!(
                entry.metadata.modified_display.is_some(),
                "missing modified date: {entry:?}"
            );
            assert!(
                entry.metadata.type_display.is_some(),
                "missing type: {entry:?}"
            );
        }
    }

    #[test]
    fn system_drive_metadata_exposes_capacity_and_filesystem_name() {
        let root = std::env::var_os("SystemDrive").map_or_else(
            || std::path::PathBuf::from(r"C:\"),
            |drive| std::path::PathBuf::from(format!("{}\\", drive.to_string_lossy())),
        );
        let metadata = drive_metadata(&root).expect("system drive metadata");

        assert_eq!(metadata.availability, DriveAvailability::Available);
        assert!(metadata.total_bytes.is_some_and(|total| total > 0));
        assert!(metadata.available_bytes.is_some());
        assert!(
            metadata
                .filesystem_name
                .as_deref()
                .is_some_and(|filesystem| !filesystem.is_empty()),
            "system drive should expose the Shell filesystem name"
        );
    }

    #[test]
    fn real_breadcrumb_children_are_bounded_and_cancel_between_batches() {
        let fixture = OwnedTempFixture::new().expect("breadcrumb fixture");
        for index in 0..130 {
            fixture
                .create_dir(format!("folder-{index:03}"))
                .expect("child folder");
        }
        let _apartment = crate::sta::ApartmentGuard::initialize().expect("STA initializes");
        let context = RequestContext::new(TabId::new(), Generation::new(1));
        let started = std::time::Instant::now();
        let mut batches = Vec::new();
        let completed = enumerate_child_containers(
            &context,
            &LocationDescriptor::file_system(fixture.root()),
            |items| {
                batches.push((started.elapsed(), items.len()));
                context.cancellation.cancel();
                Ok(())
            },
        )
        .expect("real child enumeration");

        assert!(!completed, "cancellation must stop before a second batch");
        assert_eq!(batches.len(), 1);
        assert_eq!(batches[0].1, DIRECTORY_BATCH_ITEM_CAP);
        assert!(
            batches[0].0 < std::time::Duration::from_secs(5),
            "first menu batch latency regressed: {:?}",
            batches[0].0
        );
        assert!(
            started.elapsed() < std::time::Duration::from_secs(5),
            "cancel latency regressed: {:?}",
            started.elapsed()
        );
    }

    #[test]
    fn canonical_descriptor_repairs_bare_drive_roots_and_preserves_other_identities() {
        let bare_drive = LocationDescriptor::file_system("D:");
        assert_eq!(
            canonical_location_descriptor(&bare_drive, Some(r"D:\".to_owned())),
            LocationDescriptor::file_system(r"D:\")
        );
        assert_eq!(
            canonical_location_descriptor(&bare_drive, None),
            bare_drive,
            "an unresolved drive must not be rewritten speculatively"
        );
        assert!(!is_bare_drive_designator(r"D:\"));
        assert!(!is_bare_drive_designator(r"\\server\share"));

        let explicit = LocationDescriptor::file_system(r"C:\Users\fixture\Documents");
        assert_eq!(
            canonical_location_descriptor(
                &explicit,
                Some(r"C:\Users\different\Documents".to_owned())
            ),
            explicit
        );

        for parsing_name in [
            "shell:HomeFolder",
            "shell:MyComputerFolder",
            "shell:RecycleBinFolder",
            "shell:NetworkPlacesFolder",
            "shell:Libraries",
        ] {
            let descriptor = LocationDescriptor::ParsingName(parsing_name.to_owned());
            assert_eq!(
                canonical_location_descriptor(&descriptor, None),
                descriptor,
                "{parsing_name} must not receive a fabricated filesystem path"
            );
        }

        let namespace = LocationDescriptor::ShellNamespace(vec![1, 0, 0, 0]);
        assert_eq!(
            canonical_location_descriptor(&namespace, Some(r"D:\misleading-path".to_owned())),
            namespace,
            "opaque namespace identity must not collapse into a coincidental filesystem path"
        );
    }

    #[test]
    fn shell_ancestry_starts_once_at_this_pc() {
        let this_pc_absolute = "::{20D04FE0-3AEA-1069-A2D8-08002B30309D}";
        let chain = vec![
            (
                LocationDescriptor::ParsingName("::{desktop-root}".to_owned()),
                "本機".to_owned(),
            ),
            (
                LocationDescriptor::file_system(r"C:\Users\fixture\Desktop"),
                "桌面".to_owned(),
            ),
            (
                LocationDescriptor::ParsingName(this_pc_absolute.to_owned()),
                "本機".to_owned(),
            ),
            (LocationDescriptor::file_system(r"D:\"), "D:".to_owned()),
            (
                LocationDescriptor::ShellNamespace(vec![1, 0, 0]),
                "archive.zip".to_owned(),
            ),
        ];

        let normalized = normalize_shell_parent_chain(chain, this_pc_absolute);
        assert_eq!(normalized.len(), 3);
        assert!(matches!(
            &normalized[0].0,
            LocationDescriptor::ParsingName(name)
                if name.eq_ignore_ascii_case("shell:MyComputerFolder")
        ));
        assert_eq!(normalized[1].1, "D:");
        assert_eq!(normalized[2].1, "archive.zip");
    }

    #[test]
    fn real_filesystem_backed_shell_shortcuts_publish_complete_paths() {
        let _apartment = crate::sta::ApartmentGuard::initialize().expect("STA");
        for parsing_name in [
            "shell:Desktop",
            "shell:Downloads",
            "shell:Personal",
            "shell:My Pictures",
            "shell:My Music",
            "shell:My Video",
        ] {
            let requested = LocationDescriptor::ParsingName(parsing_name.to_owned());
            let resolved = resolve_location(&requested)
                .unwrap_or_else(|error| panic!("resolve {parsing_name}: {error:?}"));
            let metadata = resolved.metadata();
            let LocationDescriptor::FileSystem(path) = metadata.descriptor else {
                panic!("{parsing_name} did not publish a filesystem descriptor")
            };
            assert!(path.is_absolute(), "{parsing_name}: {}", path.display());
            assert!(!path.as_os_str().is_empty(), "{parsing_name}");

            let rebound = resolve_location(&LocationDescriptor::file_system(&path))
                .unwrap_or_else(|error| panic!("rebind {}: {error:?}", path.display()));
            assert_eq!(
                rebound.metadata().descriptor,
                LocationDescriptor::file_system(&path),
                "the copied path must navigate back to the same filesystem location"
            );
        }
    }

    #[test]
    fn real_namespace_root_fixture_matrix_resolves_without_path_assumptions() {
        let _apartment = crate::sta::ApartmentGuard::initialize().expect("STA");
        let roots = [
            (
                "Desktop",
                true,
                LocationDescriptor::KnownFolder(
                    windows::Win32::UI::Shell::FOLDERID_Desktop
                        .to_u128()
                        .to_be_bytes(),
                ),
            ),
            (
                "Home",
                false,
                LocationDescriptor::ParsingName("shell:HomeFolder".to_owned()),
            ),
            (
                "This PC",
                true,
                LocationDescriptor::ParsingName("shell:MyComputerFolder".to_owned()),
            ),
            (
                "Libraries",
                true,
                LocationDescriptor::ParsingName("shell:Libraries".to_owned()),
            ),
            (
                "Recycle Bin",
                true,
                LocationDescriptor::ParsingName("shell:RecycleBinFolder".to_owned()),
            ),
            (
                "Network",
                false,
                LocationDescriptor::ParsingName("shell:NetworkPlacesFolder".to_owned()),
            ),
        ];
        for (name, required, root) in roots {
            match resolve_location(&root) {
                Ok(resolved) => {
                    assert!(!resolved.display_title.is_empty(), "{name}");
                    assert!(resolved.absolute.bytes().is_ok(), "{name}");
                    if name != "Desktop" {
                        assert_eq!(
                            resolved.metadata().descriptor,
                            root,
                            "{name} must preserve its Shell namespace identity"
                        );
                    }
                }
                Err(error) if !required => {
                    eprintln!("SKIP {name}: {}", error.technical_detail);
                }
                Err(error) => panic!("public Shell namespace root {name}: {error:?}"),
            }
        }

        let fixture = OwnedTempFixture::new().expect("ZIP fixture root");
        let archive = fixture.root().join("fixture.zip");
        // Empty ZIP end-of-central-directory record; Windows owns all archive parsing.
        std::fs::write(
            &archive,
            [
                0x50, 0x4b, 0x05, 0x06, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
            ],
        )
        .expect("empty ZIP");
        let resolved = resolve_location(&LocationDescriptor::file_system(&archive))
            .expect("Windows compressed-folder namespace");
        assert!(!resolved.display_title.is_empty());

        let archive_with_file = fixture.root().join("with-file.zip");
        std::fs::write(
            &archive_with_file,
            [
                0x50, 0x4b, 0x03, 0x04, 0x14, 0x00, 0x00, 0x00, 0x08, 0x00, 0x17, 0xb7, 0xfc, 0x5c,
                0x01, 0xa1, 0xc7, 0x86, 0x12, 0x00, 0x00, 0x00, 0x10, 0x00, 0x00, 0x00, 0x0a, 0x00,
                0x00, 0x00, 0x69, 0x6e, 0x73, 0x69, 0x64, 0x65, 0x2e, 0x74, 0x78, 0x74, 0xab, 0xca,
                0x2c, 0x50, 0x28, 0x4a, 0x4d, 0x2f, 0x4a, 0x2d, 0x2e, 0xce, 0xcc, 0xcf, 0xe3, 0xe5,
                0x02, 0x00, 0x50, 0x4b, 0x01, 0x02, 0x14, 0x00, 0x14, 0x00, 0x00, 0x00, 0x08, 0x00,
                0x17, 0xb7, 0xfc, 0x5c, 0x01, 0xa1, 0xc7, 0x86, 0x12, 0x00, 0x00, 0x00, 0x10, 0x00,
                0x00, 0x00, 0x0a, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
                0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x69, 0x6e, 0x73, 0x69, 0x64, 0x65, 0x2e, 0x74,
                0x78, 0x74, 0x50, 0x4b, 0x05, 0x06, 0x00, 0x00, 0x00, 0x00, 0x01, 0x00, 0x01, 0x00,
                0x38, 0x00, 0x00, 0x00, 0x3a, 0x00, 0x00, 0x00, 0x00, 0x00,
            ],
        )
        .expect("ZIP with one file");
        let resolved = resolve_location(&LocationDescriptor::file_system(&archive_with_file))
            .expect("non-empty Windows compressed-folder namespace");
        let context = RequestContext::new(TabId::new(), Generation::new(1));
        let mut entries = Vec::new();
        let completed = enumerate_directory(&context, &resolved, |event| {
            if let ExplorerEvent::DirectoryBatch { entries: batch, .. } = event {
                entries.extend(batch);
            }
            true
        })
        .expect("enumerate non-empty ZIP");
        assert!(completed);
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].display_name, "inside.txt");
        assert_eq!(
            entries[0].metadata.filesystem_attributes & 0x6,
            0,
            "Shell capability bits must not hide namespace children as filesystem items"
        );
    }

    #[test]
    fn configured_real_zip_enumerates_through_windows_compressed_folders() {
        let Some(path) = std::env::var_os("EXPLORER_REAL_ZIP_FIXTURE") else {
            eprintln!("SKIP: EXPLORER_REAL_ZIP_FIXTURE is not configured");
            return;
        };
        let path = std::path::PathBuf::from(path);
        assert!(path.is_file(), "configured ZIP fixture must exist");
        let _apartment = crate::sta::ApartmentGuard::initialize().expect("STA");
        let resolved = resolve_location(&LocationDescriptor::file_system(&path))
            .expect("configured Windows compressed-folder namespace");
        let context = RequestContext::new(TabId::new(), Generation::new(1));
        let mut entries = Vec::new();
        let completed = enumerate_directory(&context, &resolved, |event| {
            if let ExplorerEvent::DirectoryBatch { entries: batch, .. } = event {
                entries.extend(batch);
            }
            true
        })
        .expect("enumerate configured ZIP");
        assert!(completed);
        assert!(
            !entries.is_empty(),
            "configured ZIP must expose its contents"
        );
        assert!(entries.iter().all(|entry| !entry.display_name.is_empty()));
        assert!(
            entries
                .iter()
                .all(|entry| matches!(&entry.location, LocationDescriptor::ShellNamespace(_))),
            "configured ZIP children must retain Shell namespace identity"
        );
        if let Some(folder) = entries.iter().find(|entry| entry.is_container) {
            let child = resolve_location(&folder.location)
                .expect("configured ZIP child folder must resolve through Shell identity");
            assert!(matches!(
                child.metadata().descriptor,
                LocationDescriptor::ShellNamespace(_)
            ));
            let child_context = RequestContext::new(TabId::new(), Generation::new(1));
            let mut child_entries = Vec::new();
            enumerate_directory(&child_context, &child, |event| {
                if let ExplorerEvent::DirectoryBatch { entries, .. } = event {
                    child_entries.extend(entries);
                }
                true
            })
            .expect("configured ZIP child folder must enumerate");
            assert!(
                !child_entries.is_empty(),
                "configured ZIP child folder must expose its contents"
            );
            assert!(
                child_entries
                    .iter()
                    .all(|entry| entry.metadata.filesystem_attributes & 0x6 == 0),
                "configured ZIP nested children must remain visible"
            );
        }
    }

    #[test]
    fn windows_zip_child_folder_enumerates_its_files() {
        let fixture = OwnedTempFixture::new().expect("nested ZIP fixture root");
        let nested = fixture.create_dir("nested").expect("nested source folder");
        std::fs::write(nested.join("child.txt"), b"nested ZIP child").expect("nested source file");
        let archive = fixture.root().join("nested.zip");
        let output = std::process::Command::new("tar.exe")
            .current_dir(fixture.root())
            .args(["-a", "-c", "-f"])
            .arg(&archive)
            .arg("nested")
            .output()
            .expect("create nested ZIP fixture");
        assert!(
            output.status.success(),
            "tar.exe failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        std::fs::remove_dir_all(&nested).expect("remove ZIP source folder after archiving");

        let _apartment = crate::sta::ApartmentGuard::initialize().expect("STA");
        let root =
            resolve_location(&LocationDescriptor::file_system(&archive)).expect("resolve ZIP root");
        let root_context = RequestContext::new(TabId::new(), Generation::new(1));
        let mut root_entries = Vec::new();
        enumerate_directory(&root_context, &root, |event| {
            if let ExplorerEvent::DirectoryBatch { entries, .. } = event {
                root_entries.extend(entries);
            }
            true
        })
        .expect("enumerate ZIP root");
        let folder = root_entries
            .into_iter()
            .find(|entry| entry.is_container && entry.display_name == "nested")
            .expect("ZIP root exposes nested folder");
        assert!(matches!(
            &folder.location,
            LocationDescriptor::ShellNamespace(_)
        ));

        let child = resolve_location(&folder.location).expect("resolve ZIP child folder");
        assert!(matches!(
            child.metadata().descriptor,
            LocationDescriptor::ShellNamespace(_)
        ));
        let child_context = RequestContext::new(TabId::new(), Generation::new(1));
        let mut child_entries = Vec::new();
        enumerate_directory(&child_context, &child, |event| {
            if let ExplorerEvent::DirectoryBatch { entries, .. } = event {
                child_entries.extend(entries);
            }
            true
        })
        .expect("enumerate ZIP child folder");

        assert!(
            child_entries
                .iter()
                .any(|entry| entry.display_name == "child.txt"),
            "ZIP child folder must display its contained file"
        );
        assert!(
            child_entries
                .iter()
                .all(|entry| entry.metadata.filesystem_attributes & 0x6 == 0),
            "nested namespace items must remain visible"
        );
    }
}

pub(crate) fn open_default(descriptor: &LocationDescriptor) -> Result<(), ExplorerError> {
    let target = match descriptor {
        LocationDescriptor::FileSystem(path) => path.as_os_str().to_string_lossy().into_owned(),
        LocationDescriptor::ParsingName(name) => name.clone(),
        LocationDescriptor::KnownFolder(_) | LocationDescriptor::ShellNamespace(_) => {
            let absolute = match descriptor {
                LocationDescriptor::KnownFolder(bytes) => {
                    let guid = GUID::from_u128(u128::from_be_bytes(*bytes));
                    // SAFETY: initialized GUID and transferred CoTaskMem PIDL ownership.
                    OwnedPidl::from_raw(
                        unsafe { SHGetKnownFolderIDList(&raw const guid, 0, None) }
                            .map_err(|error| windows_error("resolve item to open", &error))?,
                        "resolve item to open",
                    )?
                }
                LocationDescriptor::ShellNamespace(bytes) => {
                    let aligned = AlignedPidl::from_bytes(bytes)?;
                    OwnedPidl::from_raw(
                        unsafe { ILCombine(None, Some(aligned.as_ptr())) },
                        "copy item to open",
                    )?
                }
                LocationDescriptor::FileSystem(_) | LocationDescriptor::ParsingName(_) => {
                    unreachable!("path and parsing-name cases returned above")
                }
            };
            name_from_pidl(
                absolute.as_ptr(),
                windows::Win32::UI::Shell::SIGDN_DESKTOPABSOLUTEPARSING,
            )?
        }
    };
    let target = HSTRING::from(target);
    // SAFETY: all text parameters are live NUL-terminated HSTRING/None values. ShellExecuteW does
    // not retain them and requests the user's registered default verb without a process handle.
    let result = unsafe { ShellExecuteW(None, None, &target, None, None, SW_SHOWNORMAL) };
    let code = result.0 as isize;
    if code > 32 {
        Ok(())
    } else {
        Err(shell_error(
            "open Shell item",
            Some(i32::try_from(code).unwrap_or(i32::MIN)),
            "ShellExecuteW returned an execution error code",
        ))
    }
}

fn child_entry(
    resolved: &ResolvedLocation,
    relative: &OwnedPidl,
) -> Result<FileEntry, ExplorerError> {
    // SAFETY: both complete parent and relative child PIDLs are live; ILCombine returns a new
    // COM-task allocation independent of both inputs.
    let raw_absolute =
        unsafe { ILCombine(Some(resolved.absolute.as_ptr()), Some(relative.as_ptr())) };
    let absolute = OwnedPidl::from_raw(raw_absolute, "combine Shell item identity")?;
    // Some virtual providers (including Windows Compressed Folders) return a STRRET_OFFSET whose
    // relative-PIDL conversion can produce shifted or unterminated text. We already own the
    // combined absolute PIDL, so ask the Shell for its normal display name directly.
    let display_name = name_from_pidl(absolute.as_ptr(), SIGDN_NORMALDISPLAY)?;
    let mut attributes = (SFGAO_BROWSABLE
        | SFGAO_CANCOPY
        | SFGAO_CANDELETE
        | SFGAO_CANMOVE
        | SFGAO_CANRENAME
        | SFGAO_DROPTARGET
        | SFGAO_FOLDER
        | SFGAO_HASPROPSHEET)
        .0;
    // SAFETY: the relative PIDL is owned and live; the attribute mask is writable.
    unsafe {
        resolved
            .folder
            .GetAttributesOf(&[relative.as_ptr()], &raw mut attributes)
    }
    .map_err(|error| windows_error("read Shell item attributes", &error))?;
    let preserve_namespace = matches!(&resolved.descriptor, LocationDescriptor::ShellNamespace(_))
        || matches!(
            &resolved.descriptor,
            LocationDescriptor::FileSystem(path) if !path.is_dir()
        );
    let location = if preserve_namespace {
        LocationDescriptor::ShellNamespace(absolute.bytes()?)
    } else {
        match name_from_pidl(absolute.as_ptr(), SIGDN_FILESYSPATH) {
            Ok(path) => LocationDescriptor::FileSystem(PathBuf::from(path)),
            Err(_) => LocationDescriptor::ShellNamespace(absolute.bytes()?),
        }
    };
    let identity_bytes = match &location {
        LocationDescriptor::FileSystem(path) => {
            filesystem_identity(path, attributes & 0x2000_0000 != 0)
                .unwrap_or_else(|_| fallback_filesystem_identity(path))
        }
        LocationDescriptor::ShellNamespace(_) => absolute.bytes()?,
        LocationDescriptor::ParsingName(name) => {
            let mut bytes = vec![b'P'];
            bytes.extend_from_slice(name.as_bytes());
            bytes
        }
        LocationDescriptor::KnownFolder(bytes) => {
            let mut identity = vec![b'K'];
            identity.extend_from_slice(bytes);
            identity
        }
    };
    let id = ShellItemId::from_provider_bytes(identity_bytes)
        .ok_or_else(|| shell_error("create Shell item identity", None, "identity was empty"))?;
    let is_container = attributes & 0x2000_0000 != 0;
    let metadata = entry_metadata(&location, is_container, attributes);
    Ok(FileEntry {
        id,
        display_name,
        location,
        is_container,
        metadata,
    })
}

fn entry_metadata(
    location: &LocationDescriptor,
    is_container: bool,
    shell_attributes: u32,
) -> FileEntryMetadata {
    let namespace_capabilities = namespace_capabilities(shell_attributes, is_container);
    let Some(path) = location.path() else {
        let mut metadata = FileEntryMetadata {
            type_display: is_container.then(|| "檔案資料夾".to_owned()),
            // SFGAO_* and FILE_ATTRIBUTE_* reuse low bits for unrelated meanings. For example,
            // SFGAO_CANMOVE/SFGAO_CANLINK overlap FILE_ATTRIBUTE_HIDDEN/SYSTEM. Namespace items
            // therefore must not publish Shell capability bits as filesystem attributes.
            filesystem_attributes: 0,
            unavailable_reason: Some("Shell namespace metadata is unavailable".to_owned()),
            namespace_capabilities,
            ..Default::default()
        };
        if let Ok(item) = crate::namespace::inspect_namespace_item(location) {
            metadata.namespace_capabilities = item.capabilities;
            metadata.unavailable_reason = item
                .unavailable_reason
                .map(|reason| format!("Shell namespace item unavailable: {reason:?}"));
            for (key, value) in item.properties {
                match (key.property_id, value) {
                    (4, PropertyValue::Text(value)) => metadata.type_display = Some(value),
                    (12, PropertyValue::Unsigned(value)) => metadata.size_bytes = Some(value),
                    (14, PropertyValue::FileTime(value)) => {
                        metadata.modified_sort_key = Some(value);
                        metadata.modified_display = format_filetime_ticks(value);
                    }
                    _ => {}
                }
            }
        }
        return metadata;
    };
    let type_display = shell_type_name(path).or_else(|| {
        Some(if is_container {
            "檔案資料夾".to_owned()
        } else {
            "檔案".to_owned()
        })
    });
    match std::fs::metadata(path) {
        Ok(metadata) => {
            #[cfg(windows)]
            use std::os::windows::fs::MetadataExt as _;
            FileEntryMetadata {
                modified_display: metadata.modified().ok().and_then(format_windows_time),
                modified_sort_key: Some(metadata.last_write_time()),
                created_display: metadata.created().ok().and_then(format_windows_time),
                created_sort_key: Some(metadata.creation_time()),
                // Shell archives can advertise SFGAO_FOLDER because they are browsable
                // containers; Explorer still shows their on-disk file size in Details view.
                size_bytes: (!metadata.is_dir()).then_some(metadata.len()),
                type_display,
                filesystem_attributes: metadata.file_attributes(),
                unavailable_reason: None,
                namespace_capabilities,
                authors_display: None,
                tags_display: None,
                title_display: None,
                drive: drive_metadata(path),
            }
        }
        Err(error) => FileEntryMetadata {
            type_display,
            filesystem_attributes: 0,
            unavailable_reason: Some(format!(
                "filesystem metadata unavailable: {:?}",
                error.kind()
            )),
            namespace_capabilities,
            ..Default::default()
        },
    }
}

fn drive_metadata(path: &Path) -> Option<DriveMetadata> {
    if path.parent().is_some() {
        return None;
    }
    let wide = path
        .as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect::<Vec<_>>();
    let path = PCWSTR(wide.as_ptr());
    // SAFETY: `wide` is a live NUL-terminated root path for the duration of each call.
    let raw_kind = unsafe { GetDriveTypeW(path) };
    let kind = match raw_kind {
        2 => DriveKind::Removable,
        3 => DriveKind::Fixed,
        4 => DriveKind::Network,
        5 => DriveKind::Optical,
        6 => DriveKind::RamDisk,
        _ => DriveKind::Unknown,
    };
    let mut volume_name = [0u16; 261];
    let mut filesystem_name = [0u16; 64];
    // SAFETY: the mutable label and filesystem-name buffers are valid for the call.
    let volume_information = unsafe {
        GetVolumeInformationW(
            path,
            Some(&mut volume_name),
            None,
            None,
            None,
            Some(&mut filesystem_name),
        )
    };
    let decode_buffer = |buffer: &[u16]| {
        let length = buffer
            .iter()
            .position(|value| *value == 0)
            .unwrap_or(buffer.len());
        (length > 0).then(|| String::from_utf16_lossy(&buffer[..length]))
    };
    let volume_label = volume_information
        .as_ref()
        .ok()
        .and_then(|()| decode_buffer(&volume_name));
    let filesystem_name = volume_information
        .as_ref()
        .ok()
        .and_then(|()| decode_buffer(&filesystem_name));
    let mut available = 0u64;
    let mut total = 0u64;
    // SAFETY: the root path and both writable u64 outputs are valid for the call.
    let capacity =
        unsafe { GetDiskFreeSpaceExW(path, Some(&raw mut available), Some(&raw mut total), None) };
    let (availability, total_bytes, available_bytes) = match capacity {
        Ok(()) => (DriveAvailability::Available, Some(total), Some(available)),
        Err(error) => {
            let code = u32::from_ne_bytes(error.code().0.to_ne_bytes());
            let availability = match code {
                0x8007_0015 => DriveAvailability::NoMedia,
                0x8007_0005 => DriveAvailability::AccessDenied,
                _ if kind == DriveKind::Network => DriveAvailability::Disconnected,
                _ => DriveAvailability::Unknown,
            };
            (availability, None, None)
        }
    };
    Some(DriveMetadata {
        kind,
        availability,
        volume_label,
        filesystem_name,
        total_bytes,
        available_bytes,
    })
}

fn namespace_capabilities(shell_attributes: u32, is_container: bool) -> NamespaceCapabilities {
    let has =
        |flag: windows::Win32::System::SystemServices::SFGAO_FLAGS| shell_attributes & flag.0 != 0;
    let mut bits = NamespaceCapabilities::OPEN | NamespaceCapabilities::CONTEXT_MENU;
    if is_container {
        bits |= NamespaceCapabilities::ENUMERATE | NamespaceCapabilities::SEARCH;
    } else {
        bits |= NamespaceCapabilities::THUMBNAIL | NamespaceCapabilities::PREVIEW;
    }
    if has(SFGAO_CANCOPY) {
        bits |= NamespaceCapabilities::COPY;
    }
    if has(SFGAO_CANMOVE) || has(SFGAO_DROPTARGET) {
        bits |= NamespaceCapabilities::DROP;
    }
    if is_container && has(SFGAO_DROPTARGET) {
        bits |= NamespaceCapabilities::PASTE;
    }
    if has(SFGAO_CANRENAME) {
        bits |= NamespaceCapabilities::RENAME;
    }
    if has(SFGAO_CANDELETE) {
        bits |= NamespaceCapabilities::DELETE;
    }
    if has(SFGAO_HASPROPSHEET) {
        bits |= NamespaceCapabilities::PROPERTIES;
    }
    bits |= NamespaceCapabilities::PIN;
    NamespaceCapabilities::from_public_bits(bits)
}

fn shell_type_name(path: &Path) -> Option<String> {
    let path = HSTRING::from(path.as_os_str().to_string_lossy().as_ref());
    let mut info = SHFILEINFOW::default();
    // SAFETY: path is a live NUL-terminated HSTRING and info is correctly sized writable storage.
    let info_size = u32::try_from(size_of::<SHFILEINFOW>()).ok()?;
    let result = unsafe {
        SHGetFileInfoW(
            &path,
            FILE_FLAGS_AND_ATTRIBUTES::default(),
            Some(&raw mut info),
            info_size,
            SHGFI_TYPENAME,
        )
    };
    if result == 0 {
        return None;
    }
    let length = info
        .szTypeName
        .iter()
        .position(|value| *value == 0)
        .unwrap_or(info.szTypeName.len());
    (length != 0).then(|| String::from_utf16_lossy(&info.szTypeName[..length]))
}

fn format_windows_time(value: std::time::SystemTime) -> Option<String> {
    let unix_100ns = match value.duration_since(UNIX_EPOCH) {
        Ok(duration) => {
            u128::from(duration.as_secs()) * 10_000_000 + u128::from(duration.subsec_nanos() / 100)
        }
        Err(_) => return None,
    };
    let windows_100ns = unix_100ns.checked_add(116_444_736_000_000_000)?;
    let ticks = u64::try_from(windows_100ns).ok()?;
    format_filetime_ticks(ticks)
}

fn format_filetime_ticks(ticks: u64) -> Option<String> {
    let file_time = FILETIME {
        dwLowDateTime: u32::try_from(ticks & u64::from(u32::MAX)).ok()?,
        dwHighDateTime: u32::try_from(ticks >> 32).ok()?,
    };
    let mut utc = SYSTEMTIME::default();
    let mut local = SYSTEMTIME::default();
    // SAFETY: all pointers reference live, correctly sized value/output storage.
    unsafe { FileTimeToSystemTime(&raw const file_time, &raw mut utc) }.ok()?;
    // SAFETY: null time-zone information requests the current system time zone; both system-time
    // pointers are live for the synchronous conversion.
    unsafe { SystemTimeToTzSpecificLocalTime(None, &raw const utc, &raw mut local) }.ok()?;
    let mut date = [0_u16; 96];
    let mut time = [0_u16; 96];
    // SAFETY: null locale selects the user's default locale and the output slices are writable.
    let date_len = unsafe {
        GetDateFormatEx(
            PCWSTR::null(),
            DATE_SHORTDATE,
            Some(&raw const local),
            PCWSTR::null(),
            Some(&mut date),
            PCWSTR::null(),
        )
    };
    // SAFETY: same live local SYSTEMTIME and writable output contract as above.
    let time_len = unsafe {
        GetTimeFormatEx(
            PCWSTR::null(),
            TIME_NOSECONDS,
            Some(&raw const local),
            PCWSTR::null(),
            Some(&mut time),
        )
    };
    if date_len <= 1 || time_len <= 1 {
        return None;
    }
    let date = String::from_utf16_lossy(&date[..usize::try_from(date_len - 1).ok()?]);
    let time = String::from_utf16_lossy(&time[..usize::try_from(time_len - 1).ok()?]);
    Some(format!("{date} {time}"))
}

pub(crate) fn filesystem_identity(
    path: &Path,
    is_directory: bool,
) -> Result<Vec<u8>, ExplorerError> {
    let path = HSTRING::from(path.as_os_str().to_string_lossy().as_ref());
    let flags = if is_directory {
        FILE_FLAG_BACKUP_SEMANTICS
    } else {
        FILE_FLAGS_AND_ATTRIBUTES::default()
    };
    // SAFETY: the path is a live NUL-terminated HSTRING. The returned handle ownership transfers
    // to OwnedHandle and broad sharing prevents metadata reads from blocking normal file changes.
    let raw = unsafe {
        CreateFileW(
            &path,
            FILE_READ_ATTRIBUTES.0,
            FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE,
            None,
            OPEN_EXISTING,
            flags,
            None,
        )
    }
    .map_err(|error| windows_error("open item identity", &error))?;
    // SAFETY: CreateFileW transferred one valid uniquely owned kernel handle.
    let handle = unsafe { crate::native::OwnedHandle::from_raw(raw) }
        .ok_or_else(|| shell_error("open item identity", None, "invalid file handle"))?;
    let mut info = FILE_ID_INFO::default();
    // SAFETY: handle permits attribute reads and info is correctly sized writable storage.
    let info_size = u32::try_from(size_of::<FILE_ID_INFO>()).map_err(|_| {
        shell_error(
            "read item identity",
            None,
            "FILE_ID_INFO size exceeds the Win32 API limit",
        )
    })?;
    unsafe {
        GetFileInformationByHandleEx(handle.get(), FileIdInfo, (&raw mut info).cast(), info_size)
    }
    .map_err(|error| windows_error("read item identity", &error))?;
    let mut bytes = Vec::with_capacity(25);
    bytes.push(b'F');
    bytes.extend_from_slice(&info.VolumeSerialNumber.to_le_bytes());
    bytes.extend_from_slice(&info.FileId.Identifier);
    Ok(bytes)
}

pub(crate) fn fallback_filesystem_identity(path: &Path) -> Vec<u8> {
    let mut bytes = vec![b'P'];
    bytes.extend_from_slice(path.as_os_str().to_string_lossy().to_lowercase().as_bytes());
    bytes
}

fn name_from_pidl(
    pidl: *const ITEMIDLIST,
    format: windows::Win32::UI::Shell::SIGDN,
) -> Result<String, ExplorerError> {
    // SAFETY: pidl points to one live complete ITEMIDLIST; the returned PWSTR uses CoTaskMem.
    let value = unsafe { SHGetNameFromIDList(pidl, format) }
        .map_err(|error| windows_error("read Shell item name", &error))?;
    // SAFETY: SHGetNameFromIDList transfers one non-null CoTaskMem string on success.
    let owned = unsafe { crate::native::CoTaskMem::from_raw(value.0) }
        .ok_or_else(|| shell_error("read Shell item name", None, "Shell returned null text"))?;
    // SAFETY: SHGetNameFromIDList returns a valid NUL-terminated string on success.
    unsafe { PCWSTR(owned.as_ptr()).to_string() }
        .map_err(|error| shell_error("decode Shell item name", None, &error.to_string()))
}

fn estimate_entry_bytes(entry: &FileEntry) -> usize {
    size_of::<FileEntry>()
        .saturating_add(entry.id.provider_bytes().len())
        .saturating_add(entry.display_name.len())
        .saturating_add(
            entry
                .metadata
                .modified_display
                .as_ref()
                .map_or(0, String::len),
        )
        .saturating_add(entry.metadata.type_display.as_ref().map_or(0, String::len))
        .saturating_add(match &entry.location {
            LocationDescriptor::FileSystem(path) => path.as_os_str().len(),
            LocationDescriptor::ShellNamespace(bytes) => bytes.len(),
            LocationDescriptor::ParsingName(name) => name.len(),
            LocationDescriptor::KnownFolder(bytes) => bytes.len(),
        })
}

fn windows_error(operation: &'static str, error: &windows::core::Error) -> ExplorerError {
    shell_error(operation, Some(error.code().0), &error.to_string())
}

fn shell_error(operation: &'static str, code: Option<i32>, detail: &str) -> ExplorerError {
    let kind = match code {
        Some(-2_147_024_891) => ExplorerErrorKind::Authorization, // E_ACCESSDENIED
        _ => ExplorerErrorKind::Availability,
    };
    let user_message = if kind == ExplorerErrorKind::Authorization {
        "您沒有存取這個位置的權限。"
    } else {
        "Windows 無法讀取這個位置。"
    };
    let error = ExplorerError::new(kind, operation, true, user_message, detail);
    match code {
        Some(code) => error.with_native_code(code),
        None => error,
    }
}
