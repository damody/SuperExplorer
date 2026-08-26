//! Scoped, restartable legacy cleanup and invalid-canonical quarantine.

use crate::mft_persistence::LifecycleBarrierV1;
use serde::{Deserialize, Serialize};
use std::fs::{self, File, OpenOptions};
use std::io::{Read as _, Seek as _, SeekFrom, Write as _};
#[cfg(windows)]
use std::os::windows::ffi::OsStrExt as _;
#[cfg(windows)]
use std::os::windows::{fs::OpenOptionsExt as _, io::AsRawHandle as _};
use std::path::{Path, PathBuf};
#[cfg(windows)]
use windows::{
    Win32::Storage::FileSystem::{MOVE_FILE_FLAGS, MoveFileExW},
    core::PCWSTR,
};

const MANIFEST_VERSION: u32 = 1;

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub(crate) struct InventoryEntryV1 {
    pub(crate) name: String,
    pub(crate) bytes: u64,
    pub(crate) sha256: String,
    #[serde(default)]
    pub(crate) volume_serial: u64,
    #[serde(default)]
    pub(crate) file_id: u64,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
struct MaintenanceManifestV1 {
    version: u32,
    operation: String,
    volume: char,
    entries: Vec<InventoryEntryV1>,
    completed: Vec<String>,
    complete: bool,
}

pub(crate) fn inventory_legacy(root: &Path, letter: char) -> Result<Vec<InventoryEntryV1>, String> {
    let root = resolved_directory(root)?;
    let mut entries = Vec::new();
    for entry in fs::read_dir(&root).map_err(|error| error.to_string())? {
        let entry = entry.map_err(|error| error.to_string())?;
        let name = entry.file_name().to_string_lossy().into_owned();
        if !recognized_legacy_name(letter, &name) {
            continue;
        }
        entries.push(inventory_file(&root, &entry.path())?);
    }
    entries.sort_by(|left, right| left.name.cmp(&right.name));
    Ok(entries)
}

pub(crate) fn cleanup_legacy_after_promotion(
    cache_root: &Path,
    audit_root: &Path,
    letter: char,
) -> Result<Vec<InventoryEntryV1>, String> {
    cleanup_legacy_after_promotion_guarded(cache_root, audit_root, letter, || true)
}

pub(crate) fn cleanup_legacy_after_promotion_guarded(
    cache_root: &Path,
    audit_root: &Path,
    letter: char,
    lifecycle_open: impl Fn() -> bool,
) -> Result<Vec<InventoryEntryV1>, String> {
    cleanup_legacy_after_promotion_impl(cache_root, audit_root, letter, None, lifecycle_open)
}

pub(crate) fn cleanup_legacy_after_promotion_linearized(
    cache_root: &Path,
    audit_root: &Path,
    letter: char,
    barrier: &LifecycleBarrierV1,
    lifecycle_open: impl Fn() -> bool,
) -> Result<Vec<InventoryEntryV1>, String> {
    cleanup_legacy_after_promotion_impl(
        cache_root,
        audit_root,
        letter,
        Some(barrier),
        lifecycle_open,
    )
}

fn cleanup_legacy_after_promotion_impl(
    cache_root: &Path,
    audit_root: &Path,
    letter: char,
    barrier: Option<&LifecycleBarrierV1>,
    lifecycle_open: impl Fn() -> bool,
) -> Result<Vec<InventoryEntryV1>, String> {
    if !lifecycle_open() {
        return Err("MFT lifecycle closed before legacy cleanup".to_owned());
    }
    let cache_root = resolved_directory(cache_root)?;
    maintenance_write(barrier, &lifecycle_open, "audit directory creation", || {
        fs::create_dir_all(audit_root).map_err(|error| error.to_string())
    })?;
    let audit_root = resolved_directory(audit_root)?;
    let entries = inventory_legacy(&cache_root, letter)?;
    let intent = audit_root.join(format!("{letter}.legacy-cleanup-intent.json"));
    let complete = audit_root.join(format!("{letter}.legacy-cleanup-complete.json"));
    let mut manifest = MaintenanceManifestV1 {
        version: MANIFEST_VERSION,
        operation: "legacy-cleanup".into(),
        volume: letter,
        entries: entries.clone(),
        completed: Vec::new(),
        complete: false,
    };
    maintenance_write(barrier, &lifecycle_open, "cleanup intent", || {
        write_synced_json(&intent, &manifest)
    })?;
    for expected in &entries {
        if !lifecycle_open() {
            return Err("MFT lifecycle closed during legacy cleanup".to_owned());
        }
        let target = cache_root.join(&expected.name);
        if !target.exists() {
            manifest.completed.push(expected.name.clone());
            continue;
        }
        #[cfg(windows)]
        let target_handle = open_verified_inventory_handle(&cache_root, &target, expected)?;
        #[cfg(not(windows))]
        verify_inventory_match(&cache_root, &target, expected)?;
        #[cfg(windows)]
        maintenance_write(barrier, &lifecycle_open, "legacy delete", || {
            delete_file_handle(&target_handle)
        })?;
        #[cfg(not(windows))]
        maintenance_write(barrier, &lifecycle_open, "legacy delete", || {
            fs::remove_file(&target).map_err(|error| error.to_string())
        })?;
        manifest.completed.push(expected.name.clone());
    }
    manifest.complete = true;
    maintenance_write(barrier, &lifecycle_open, "cleanup completion", || {
        write_synced_json(&complete, &manifest)
    })?;
    maintenance_write(barrier, &lifecycle_open, "cleanup intent removal", || {
        fs::remove_file(&intent).map_err(|error| error.to_string())
    })?;
    Ok(entries)
}

pub(crate) fn quarantine_canonical(
    cache_root: &Path,
    quarantine_parent: &Path,
    letter: char,
    nonce: u64,
) -> Result<PathBuf, String> {
    quarantine_canonical_guarded(cache_root, quarantine_parent, letter, nonce, || true)
}

pub(crate) fn quarantine_canonical_guarded(
    cache_root: &Path,
    quarantine_parent: &Path,
    letter: char,
    nonce: u64,
    lifecycle_open: impl Fn() -> bool,
) -> Result<PathBuf, String> {
    quarantine_canonical_impl(
        cache_root,
        quarantine_parent,
        letter,
        nonce,
        None,
        lifecycle_open,
    )
}

pub(crate) fn quarantine_canonical_linearized(
    cache_root: &Path,
    quarantine_parent: &Path,
    letter: char,
    nonce: u64,
    barrier: &LifecycleBarrierV1,
    lifecycle_open: impl Fn() -> bool,
) -> Result<PathBuf, String> {
    quarantine_canonical_impl(
        cache_root,
        quarantine_parent,
        letter,
        nonce,
        Some(barrier),
        lifecycle_open,
    )
}

fn quarantine_canonical_impl(
    cache_root: &Path,
    quarantine_parent: &Path,
    letter: char,
    nonce: u64,
    barrier: Option<&LifecycleBarrierV1>,
    lifecycle_open: impl Fn() -> bool,
) -> Result<PathBuf, String> {
    if !lifecycle_open() {
        return Err("MFT lifecycle closed before canonical quarantine".to_owned());
    }
    let cache_root = resolved_directory(cache_root)?;
    maintenance_write(barrier, &lifecycle_open, "quarantine root creation", || {
        fs::create_dir_all(quarantine_parent).map_err(|error| error.to_string())
    })?;
    let quarantine_parent = resolved_directory(quarantine_parent)?;
    let existing = find_incomplete_quarantine(&quarantine_parent, letter)?;
    let directory =
        existing.unwrap_or_else(|| quarantine_parent.join(format!("{letter}-{nonce:016x}")));
    if !directory.exists() {
        maintenance_write(
            barrier,
            &lifecycle_open,
            "quarantine directory creation",
            || fs::create_dir(&directory).map_err(|error| error.to_string()),
        )?;
    }
    let directory = resolved_directory(&directory)?;
    if directory.parent() != Some(quarantine_parent.as_path()) {
        return Err("quarantine directory escapes its fixed parent".to_owned());
    }
    let intent = directory.join("intent.json");
    let mut manifest = if intent.is_file() {
        read_manifest(&intent)?
    } else {
        let entries = inventory_canonical(&cache_root, letter)?;
        if entries.is_empty() {
            return Err("invalid canonical set is absent".to_owned());
        }
        let manifest = MaintenanceManifestV1 {
            version: MANIFEST_VERSION,
            operation: "canonical-quarantine".into(),
            volume: letter,
            entries,
            completed: Vec::new(),
            complete: false,
        };
        maintenance_write(barrier, &lifecycle_open, "quarantine intent", || {
            write_synced_json(&intent, &manifest)
        })?;
        manifest
    };
    validate_manifest(&manifest, "canonical-quarantine", letter)?;
    for expected in manifest.entries.clone() {
        if !lifecycle_open() {
            return Err("MFT lifecycle closed during canonical quarantine".to_owned());
        }
        if manifest.completed.contains(&expected.name) {
            continue;
        }
        let source = cache_root.join(&expected.name);
        let destination = directory.join(&expected.name);
        if destination.exists() && !source.exists() {
            verify_inventory_match(&directory, &destination, &expected)?;
        } else {
            if destination.exists() {
                verify_inventory_match(&directory, &destination, &expected)?;
            }
            #[cfg(windows)]
            let source_handle = open_verified_inventory_handle(&cache_root, &source, &expected)?;
            #[cfg(not(windows))]
            verify_inventory_match(&cache_root, &source, &expected)?;
            #[cfg(windows)]
            maintenance_write(
                barrier,
                &lifecycle_open,
                "canonical quarantine move",
                || {
                    if destination.exists() {
                        delete_file_handle(&source_handle)
                    } else {
                        rename_file_handle(&source_handle, &destination)
                            .or_else(|_| {
                                hardlink_and_unlink_exclusive_handle(
                                    &source_handle,
                                    &source,
                                    &destination,
                                )
                            })
                            .map_err(|error| {
                                format!("quarantine move failed for {}: {error}", expected.name)
                            })
                    }
                },
            )?;
            #[cfg(not(windows))]
            maintenance_write(
                barrier,
                &lifecycle_open,
                "canonical quarantine move",
                || fs::rename(&source, &destination).map_err(|error| error.to_string()),
            )?;
        }
        manifest.completed.push(expected.name);
        maintenance_write(barrier, &lifecycle_open, "quarantine progress", || {
            write_synced_json(&intent, &manifest)
        })?;
    }
    manifest.complete = true;
    maintenance_write(barrier, &lifecycle_open, "quarantine completion", || {
        write_synced_json(&directory.join("complete.json"), &manifest)
    })?;
    maintenance_write(
        barrier,
        &lifecycle_open,
        "quarantine intent removal",
        || fs::remove_file(&intent).map_err(|error| error.to_string()),
    )?;
    Ok(directory)
}

fn inventory_canonical(root: &Path, letter: char) -> Result<Vec<InventoryEntryV1>, String> {
    let base = format!("{letter}.mft.sqlite3");
    let temporary = format!("{base}.migration-tmp");
    let mut entries = Vec::new();
    for name in [
        base.clone(),
        format!("{base}-wal"),
        format!("{base}-shm"),
        format!("{base}.replacement-backup"),
        temporary.clone(),
        format!("{temporary}-journal"),
        format!("{temporary}-wal"),
        format!("{temporary}-shm"),
    ] {
        let path = root.join(&name);
        if path.exists() {
            entries.push(inventory_file(root, &path)?);
        }
    }
    Ok(entries)
}

fn find_incomplete_quarantine(parent: &Path, letter: char) -> Result<Option<PathBuf>, String> {
    for entry in fs::read_dir(parent).map_err(|error| error.to_string())? {
        let path = entry.map_err(|error| error.to_string())?.path();
        if !path.is_dir()
            || !path
                .file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.starts_with(&format!("{letter}-")))
        {
            continue;
        }
        if path.join("intent.json").is_file() {
            return Ok(Some(path));
        }
    }
    Ok(None)
}

fn recognized_legacy_name(letter: char, name: &str) -> bool {
    if name == format!("{letter}.semftidx")
        || name == format!("{letter}.semftstatus")
        || name == format!("{letter}.persisted-partial")
    {
        return true;
    }
    let Some(rest) = name.strip_prefix(&format!("{letter}.")) else {
        return false;
    };
    let Some((generation, extension)) = rest.split_once('.') else {
        return false;
    };
    generation.len() == 20
        && generation.bytes().all(|byte| byte.is_ascii_digit())
        && matches!(extension, "semftcp" | "semftdelta")
}

fn inventory_file(root: &Path, path: &Path) -> Result<InventoryEntryV1, String> {
    let mut file = open_inventory_handle(path)?;
    inventory_file_from_handle(root, path, &mut file)
}

fn inventory_file_from_handle(
    root: &Path,
    path: &Path,
    file: &mut File,
) -> Result<InventoryEntryV1, String> {
    let metadata = fs::symlink_metadata(path).map_err(|error| error.to_string())?;
    if !metadata.file_type().is_file() || metadata.file_type().is_symlink() {
        return Err("maintenance target is not a regular file".to_owned());
    }
    let canonical = path.canonicalize().map_err(|error| error.to_string())?;
    if canonical.parent() != Some(root) {
        return Err("maintenance target escapes fixed root".to_owned());
    }
    let (volume_serial, file_id) = file_identity(file)?;
    Ok(InventoryEntryV1 {
        name: canonical
            .file_name()
            .and_then(|name| name.to_str())
            .ok_or_else(|| "maintenance filename is invalid".to_owned())?
            .to_owned(),
        bytes: metadata.len(),
        sha256: sha256_open_file(file)?,
        volume_serial,
        file_id,
    })
}

fn verify_inventory_match(
    root: &Path,
    path: &Path,
    expected: &InventoryEntryV1,
) -> Result<(), String> {
    let actual = inventory_file(root, path)?;
    if &actual != expected {
        return Err("maintenance target identity changed".to_owned());
    }
    Ok(())
}

#[cfg(windows)]
fn open_inventory_handle(path: &Path) -> Result<File, String> {
    const GENERIC_READ_DELETE: u32 = 0x8001_0000;
    // Keep the verified object readable by diagnostics, but deny both content
    // mutation and pathname replacement until disposition completes.
    const SHARE_READ_ONLY: u32 = 0x0000_0001;
    const OPEN_REPARSE_POINT: u32 = 0x0020_0000;
    OpenOptions::new()
        .read(true)
        .access_mode(GENERIC_READ_DELETE)
        .share_mode(SHARE_READ_ONLY)
        .custom_flags(OPEN_REPARSE_POINT)
        .open(path)
        .map_err(|error| error.to_string())
}

#[cfg(not(windows))]
fn open_inventory_handle(path: &Path) -> Result<File, String> {
    File::open(path).map_err(|error| error.to_string())
}

#[cfg(windows)]
#[expect(
    unsafe_code,
    reason = "reading stable file identity requires Win32 handle metadata APIs"
)]
// SAFETY: The declaration matches kernel32 and the borrowed File keeps its
// handle live while writable metadata storage is passed synchronously.
fn file_identity(file: &File) -> Result<(u64, u64), String> {
    #[repr(C)]
    #[derive(Default)]
    struct ByHandleFileInformation {
        file_attributes: u32,
        creation_time: [u32; 2],
        last_access_time: [u32; 2],
        last_write_time: [u32; 2],
        volume_serial_number: u32,
        file_size_high: u32,
        file_size_low: u32,
        number_of_links: u32,
        file_index_high: u32,
        file_index_low: u32,
    }
    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn GetFileInformationByHandle(
            file: *mut core::ffi::c_void,
            information: *mut ByHandleFileInformation,
        ) -> i32;
    }
    let mut information = ByHandleFileInformation::default();
    if unsafe { GetFileInformationByHandle(file.as_raw_handle(), &raw mut information) } == 0 {
        return Err(std::io::Error::last_os_error().to_string());
    }
    Ok((
        u64::from(information.volume_serial_number),
        (u64::from(information.file_index_high) << 32) | u64::from(information.file_index_low),
    ))
}

#[cfg(not(windows))]
fn file_identity(_: &File) -> Result<(u64, u64), String> {
    Ok((0, 0))
}

#[cfg(windows)]
fn open_verified_inventory_handle(
    root: &Path,
    path: &Path,
    expected: &InventoryEntryV1,
) -> Result<File, String> {
    let mut file = open_inventory_handle(path)?;
    if inventory_file_from_handle(root, path, &mut file)? != *expected {
        return Err("maintenance target identity changed".to_owned());
    }
    Ok(file)
}

#[cfg(windows)]
#[expect(
    unsafe_code,
    reason = "deleting the file owned by an exclusive handle requires Win32 disposition metadata"
)]
// SAFETY: The declaration matches kernel32; the borrowed File remains live and
// the disposition structure is passed with its exact size.
fn delete_file_handle(file: &File) -> Result<(), String> {
    #[repr(C)]
    struct FileDispositionInfo {
        delete_file: i32,
    }
    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn SetFileInformationByHandle(
            file: *mut core::ffi::c_void,
            information_class: u32,
            information: *const core::ffi::c_void,
            bytes: u32,
        ) -> i32;
    }
    let disposition = FileDispositionInfo { delete_file: 1 };
    if unsafe {
        SetFileInformationByHandle(
            file.as_raw_handle(),
            4,
            (&raw const disposition).cast(),
            std::mem::size_of::<FileDispositionInfo>() as u32,
        )
    } == 0
    {
        Err(std::io::Error::last_os_error().to_string())
    } else {
        Ok(())
    }
}

#[cfg(windows)]
#[expect(
    unsafe_code,
    reason = "renaming the file owned by an exclusive handle requires Win32 rename metadata"
)]
// SAFETY: Both rename buffers include their inline UTF-16 storage and exact
// byte size; the borrowed source handle remains live through each call.
fn rename_file_handle(file: &File, destination: &Path) -> Result<(), String> {
    #[repr(C)]
    struct FileRenameInfoLayout {
        flags: u32,
        root_directory: *mut core::ffi::c_void,
        file_name_length: u32,
        file_name: [u16; 1],
    }
    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn SetFileInformationByHandle(
            file: *mut core::ffi::c_void,
            information_class: u32,
            information: *const core::ffi::c_void,
            bytes: u32,
        ) -> i32;
    }
    let destination_text = destination.to_string_lossy();
    let destination_text = destination_text
        .strip_prefix(r"\\?\")
        .unwrap_or(&destination_text);
    let native_destination = if destination_text.as_bytes().get(1) == Some(&b':') {
        format!(r"\??\{destination_text}")
    } else {
        destination_text.to_string()
    };
    let name = std::ffi::OsStr::new(&native_destination)
        .encode_wide()
        .collect::<Vec<_>>();
    let name_bytes = name
        .len()
        .checked_mul(std::mem::size_of::<u16>())
        .ok_or_else(|| "quarantine destination is too long".to_owned())?;
    let prefix = std::mem::offset_of!(FileRenameInfoLayout, file_name);
    let mut buffer = vec![0_u8; prefix.saturating_add(name_bytes)];
    unsafe {
        std::ptr::write_unaligned(
            buffer
                .as_mut_ptr()
                .add(std::mem::offset_of!(FileRenameInfoLayout, root_directory))
                .cast::<*mut core::ffi::c_void>(),
            std::ptr::null_mut(),
        );
        std::ptr::write_unaligned(
            buffer
                .as_mut_ptr()
                .add(std::mem::offset_of!(FileRenameInfoLayout, file_name_length))
                .cast::<u32>(),
            u32::try_from(name_bytes).map_err(|_| "quarantine destination is too long")?,
        );
        std::ptr::copy_nonoverlapping(
            name.as_ptr().cast::<u8>(),
            buffer.as_mut_ptr().add(prefix),
            name_bytes,
        );
    }
    if unsafe {
        SetFileInformationByHandle(
            file.as_raw_handle(),
            22,
            buffer.as_ptr().cast(),
            u32::try_from(buffer.len()).map_err(|_| "quarantine destination is too long")?,
        )
    } == 0
    {
        Err(std::io::Error::last_os_error().to_string())
    } else {
        Ok(())
    }
}

#[cfg(windows)]
#[expect(
    unsafe_code,
    reason = "creating a recovery hard link requires the Win32 CreateHardLinkW API"
)]
// SAFETY: Both paths are live NUL-terminated UTF-16 buffers for the synchronous
// call and no security-attribute pointer is supplied.
fn hardlink_and_unlink_exclusive_handle(
    source_handle: &File,
    source: &Path,
    destination: &Path,
) -> Result<(), String> {
    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn CreateHardLinkW(
            new_file_name: *const u16,
            existing_file_name: *const u16,
            security_attributes: *const core::ffi::c_void,
        ) -> i32;
    }
    // The verified source handle denies SHARE_WRITE and SHARE_DELETE, so this
    // compatibility fallback cannot be redirected to changed/replaced data.
    let destination = destination
        .as_os_str()
        .encode_wide()
        .chain([0])
        .collect::<Vec<_>>();
    let source = source
        .as_os_str()
        .encode_wide()
        .chain([0])
        .collect::<Vec<_>>();
    if unsafe { CreateHardLinkW(destination.as_ptr(), source.as_ptr(), std::ptr::null()) } == 0 {
        return Err(std::io::Error::last_os_error().to_string());
    }
    delete_file_handle(source_handle)
}

fn maintenance_write<T>(
    barrier: Option<&LifecycleBarrierV1>,
    lifecycle_open: &impl Fn() -> bool,
    boundary: &str,
    operation: impl FnOnce() -> Result<T, String>,
) -> Result<T, String> {
    let run = || {
        if !lifecycle_open() {
            return Err(format!("MFT lifecycle closed before {boundary}"));
        }
        operation()
    };
    match barrier {
        Some(barrier) => barrier.invoke(run),
        None => run(),
    }
}

fn resolved_directory(path: &Path) -> Result<PathBuf, String> {
    let resolved = path.canonicalize().map_err(|error| error.to_string())?;
    if !resolved.is_dir() {
        return Err("maintenance root is not a directory".to_owned());
    }
    Ok(resolved)
}

fn validate_manifest(
    manifest: &MaintenanceManifestV1,
    operation: &str,
    letter: char,
) -> Result<(), String> {
    if manifest.version != MANIFEST_VERSION
        || manifest.operation != operation
        || manifest.volume != letter
        || manifest.complete
    {
        return Err("maintenance intent is incompatible".to_owned());
    }
    if manifest
        .completed
        .iter()
        .any(|name| !manifest.entries.iter().any(|entry| &entry.name == name))
    {
        return Err("maintenance intent completion is invalid".to_owned());
    }
    Ok(())
}

fn read_manifest(path: &Path) -> Result<MaintenanceManifestV1, String> {
    let bytes = fs::read(path).map_err(|error| error.to_string())?;
    serde_json::from_slice(&bytes).map_err(|error| error.to_string())
}

fn write_synced_json(path: &Path, value: &MaintenanceManifestV1) -> Result<(), String> {
    let bytes = serde_json::to_vec_pretty(value).map_err(|error| error.to_string())?;
    let temporary = path.with_extension(format!("json.tmp-{}", std::process::id()));
    let mut file = OpenOptions::new()
        .create(true)
        .truncate(true)
        .write(true)
        .open(&temporary)
        .map_err(|error| error.to_string())?;
    file.write_all(&bytes)
        .and_then(|()| file.sync_all())
        .map_err(|error| error.to_string())?;
    drop(file);
    #[cfg(windows)]
    #[expect(
        unsafe_code,
        reason = "durable manifest publication requires flushing the owned Win32 file handle"
    )]
    // SAFETY: `file` exclusively owns a live handle for this synchronous flush;
    // the handle is not closed until the call returns.
    {
        let source = temporary
            .as_os_str()
            .encode_wide()
            .chain([0])
            .collect::<Vec<_>>();
        let destination = path
            .as_os_str()
            .encode_wide()
            .chain([0])
            .collect::<Vec<_>>();
        unsafe {
            MoveFileExW(
                PCWSTR(source.as_ptr()),
                PCWSTR(destination.as_ptr()),
                MOVE_FILE_FLAGS(0x1 | 0x8),
            )
        }
        .map_err(|error| error.to_string())?;
    }
    #[cfg(not(windows))]
    std::fs::rename(&temporary, path).map_err(|error| error.to_string())?;
    Ok(())
}

fn sha256_file(path: &Path) -> Result<String, String> {
    let mut file = File::open(path).map_err(|error| error.to_string())?;
    sha256_open_file(&mut file)
}

fn sha256_open_file(file: &mut File) -> Result<String, String> {
    file.seek(SeekFrom::Start(0))
        .map_err(|error| error.to_string())?;
    let mut state = Sha256::new();
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        let read = file.read(&mut buffer).map_err(|error| error.to_string())?;
        if read == 0 {
            break;
        }
        state.update(&buffer[..read]);
    }
    Ok(state
        .finish()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect())
}

struct Sha256 {
    state: [u32; 8],
    length: u64,
    buffer: Vec<u8>,
}
impl Sha256 {
    fn new() -> Self {
        Self {
            state: [
                0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a, 0x510e527f, 0x9b05688c, 0x1f83d9ab,
                0x5be0cd19,
            ],
            length: 0,
            buffer: Vec::new(),
        }
    }
    fn update(&mut self, bytes: &[u8]) {
        self.length = self.length.wrapping_add(bytes.len() as u64);
        self.buffer.extend_from_slice(bytes);
        while self.buffer.len() >= 64 {
            let mut block = [0_u8; 64];
            block.copy_from_slice(&self.buffer[..64]);
            self.compress(&block);
            self.buffer.drain(..64);
        }
    }
    fn finish(mut self) -> [u8; 32] {
        let bits = self.length.wrapping_mul(8);
        self.buffer.push(0x80);
        while self.buffer.len() % 64 != 56 {
            self.buffer.push(0);
        }
        self.buffer.extend_from_slice(&bits.to_be_bytes());
        while !self.buffer.is_empty() {
            let mut block = [0_u8; 64];
            block.copy_from_slice(&self.buffer[..64]);
            self.compress(&block);
            self.buffer.drain(..64);
        }
        let mut out = [0; 32];
        for (i, v) in self.state.iter().enumerate() {
            out[i * 4..i * 4 + 4].copy_from_slice(&v.to_be_bytes());
        }
        out
    }
    fn compress(&mut self, b: &[u8; 64]) {
        const K: [u32; 64] = [
            0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4,
            0xab1c5ed5, 0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe,
            0x9bdc06a7, 0xc19bf174, 0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc, 0x2de92c6f,
            0x4a7484aa, 0x5cb0a9dc, 0x76f988da, 0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7,
            0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967, 0x27b70a85, 0x2e1b2138, 0x4d2c6dfc,
            0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85, 0xa2bfe8a1, 0xa81a664b,
            0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070, 0x19a4c116,
            0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3,
            0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7,
            0xc67178f2,
        ];
        let mut w = [0u32; 64];
        for i in 0..16 {
            let offset = i * 4;
            w[i] = u32::from_be_bytes([b[offset], b[offset + 1], b[offset + 2], b[offset + 3]]);
        }
        for i in 16..64 {
            let s0 = w[i - 15].rotate_right(7) ^ w[i - 15].rotate_right(18) ^ (w[i - 15] >> 3);
            let s1 = w[i - 2].rotate_right(17) ^ w[i - 2].rotate_right(19) ^ (w[i - 2] >> 10);
            w[i] = w[i - 16]
                .wrapping_add(s0)
                .wrapping_add(w[i - 7])
                .wrapping_add(s1);
        }
        let [a0, b0, c0, d0, e0, f0, g0, h0] = self.state;
        let (mut a, mut b, mut c, mut d, mut e, mut f, mut g, mut h) =
            (a0, b0, c0, d0, e0, f0, g0, h0);
        for i in 0..64 {
            let s1 = e.rotate_right(6) ^ e.rotate_right(11) ^ e.rotate_right(25);
            let ch = (e & f) ^ (!e & g);
            let t1 = h
                .wrapping_add(s1)
                .wrapping_add(ch)
                .wrapping_add(K[i])
                .wrapping_add(w[i]);
            let s0 = a.rotate_right(2) ^ a.rotate_right(13) ^ a.rotate_right(22);
            let maj = (a & b) ^ (a & c) ^ (b & c);
            let t2 = s0.wrapping_add(maj);
            h = g;
            g = f;
            f = e;
            e = d.wrapping_add(t1);
            d = c;
            c = b;
            b = a;
            a = t1.wrapping_add(t2);
        }
        for (i, v) in [a, b, c, d, e, f, g, h].iter().enumerate() {
            self.state[i] = self.state[i].wrapping_add(*v);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn sha256_matches_known_vector() {
        let root = TempDir::new().unwrap();
        let path = root.path().join("v");
        fs::write(&path, b"abc").unwrap();
        assert_eq!(
            sha256_file(&path).unwrap(),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }

    #[test]
    fn cleanup_only_removes_recognized_identity_checked_legacy_files() {
        let root = TempDir::new().unwrap();
        let audit = TempDir::new().unwrap();
        for name in [
            "C.semftidx",
            "C.00000000000000000001.semftcp",
            "C.00000000000000000001.semftdelta",
            "C.semftstatus",
        ] {
            fs::write(root.path().join(name), name).unwrap();
        }
        fs::write(root.path().join("unrelated.txt"), b"keep").unwrap();
        let removed = cleanup_legacy_after_promotion(root.path(), audit.path(), 'C').unwrap();
        assert_eq!(removed.len(), 4);
        assert!(root.path().join("unrelated.txt").is_file());
        assert!(
            audit
                .path()
                .join("C.legacy-cleanup-complete.json")
                .is_file()
        );
    }

    #[test]
    fn quarantine_moves_only_fixed_members_and_is_idempotently_resumable() {
        let root = TempDir::new().unwrap();
        let quarantine = TempDir::new().unwrap();
        for name in [
            "D.mft.sqlite3",
            "D.mft.sqlite3-wal",
            "D.mft.sqlite3-shm",
            "D.mft.sqlite3.replacement-backup",
            "D.mft.sqlite3.migration-tmp",
            "D.mft.sqlite3.migration-tmp-journal",
        ] {
            fs::write(root.path().join(name), name).unwrap();
        }
        fs::write(root.path().join("D.mft.sqlite3-other"), b"keep").unwrap();
        let destination = quarantine_canonical(root.path(), quarantine.path(), 'D', 1).unwrap();
        assert!(destination.join("complete.json").is_file());
        assert!(root.path().join("D.mft.sqlite3-other").is_file());
        assert!(!root.path().join("D.mft.sqlite3").exists());
        assert!(
            !root
                .path()
                .join("D.mft.sqlite3.replacement-backup")
                .exists()
        );
        assert!(!root.path().join("D.mft.sqlite3.migration-tmp").exists());
    }

    #[test]
    fn quarantine_resumes_crash_between_move_and_intent_update() {
        let root = TempDir::new().unwrap();
        let quarantine = TempDir::new().unwrap();
        for name in ["F.mft.sqlite3", "F.mft.sqlite3-wal"] {
            fs::write(root.path().join(name), name).unwrap();
        }
        let root_resolved = root.path().canonicalize().unwrap();
        let directory = quarantine.path().join("F-0000000000000001");
        fs::create_dir(&directory).unwrap();
        let entries = inventory_canonical(&root_resolved, 'F').unwrap();
        let manifest = MaintenanceManifestV1 {
            version: 1,
            operation: "canonical-quarantine".into(),
            volume: 'F',
            entries: entries.clone(),
            completed: vec![],
            complete: false,
        };
        write_synced_json(&directory.join("intent.json"), &manifest).unwrap();
        fs::rename(
            root.path().join(&entries[0].name),
            directory.join(&entries[0].name),
        )
        .unwrap();
        let resumed = quarantine_canonical(root.path(), quarantine.path(), 'F', 2).unwrap();
        assert_eq!(resumed, directory.canonicalize().unwrap());
        assert!(resumed.join("complete.json").is_file());
        assert!(!root.path().join("F.mft.sqlite3-wal").exists());
    }

    #[test]
    fn identity_change_and_path_escape_are_rejected() {
        let root = TempDir::new().unwrap();
        let audit = TempDir::new().unwrap();
        fs::write(root.path().join("E.semftidx"), b"old").unwrap();
        let mut inventory = inventory_legacy(root.path(), 'E').unwrap();
        fs::write(root.path().join("E.semftidx"), b"new").unwrap();
        assert!(
            verify_inventory_match(
                &root.path().canonicalize().unwrap(),
                &root.path().join("E.semftidx"),
                &inventory.remove(0)
            )
            .is_err()
        );
        fs::write(
            audit.path().join("E.00000000000000000001.semftcp"),
            b"outside",
        )
        .unwrap();
        assert!(
            inventory_file(
                &root.path().canonicalize().unwrap(),
                &audit.path().join("E.00000000000000000001.semftcp")
            )
            .is_err()
        );
    }
}
