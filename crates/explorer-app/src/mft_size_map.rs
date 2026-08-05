//! Fast NTFS metadata index used by Size Map.

#![cfg(windows)]

use std::{
    collections::HashMap,
    ffi::c_void,
    io::{BufReader, BufWriter, Read as _, Write as _},
    mem::size_of,
    os::windows::ffi::OsStrExt,
    path::Path,
};

use windows::{
    Win32::{
        Foundation::{CloseHandle, HANDLE},
        Storage::FileSystem::{
            BY_HANDLE_FILE_INFORMATION, CreateFileW, FILE_ATTRIBUTE_DIRECTORY, FILE_GENERIC_READ,
            FILE_SHARE_DELETE, FILE_SHARE_READ, FILE_SHARE_WRITE, GetFileInformationByHandle,
            OPEN_EXISTING,
        },
        System::{
            IO::DeviceIoControl,
            Ioctl::{FSCTL_ENUM_USN_DATA, FSCTL_GET_NTFS_FILE_RECORD, MFT_ENUM_DATA_V0},
            Threading::{INFINITE, WaitForSingleObject},
        },
        UI::Shell::{SEE_MASK_NOCLOSEPROCESS, SHELLEXECUTEINFOW, ShellExecuteExW},
    },
    core::PCWSTR,
};

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct MftEntryV1 {
    pub(crate) reference: u64,
    pub(crate) parent_reference: u64,
    pub(crate) name: String,
    pub(crate) logical_bytes: u64,
    pub(crate) allocated_bytes: u64,
    pub(crate) is_directory: bool,
}

#[derive(Debug)]
pub(crate) struct MftIndexV1 {
    pub(crate) entries: HashMap<u64, MftEntryV1>,
    children: HashMap<u64, Vec<u64>>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct MftProjectedNodeV1 {
    pub(crate) reference: u64,
    pub(crate) parent_reference: Option<u64>,
    pub(crate) name: String,
    pub(crate) logical_bytes: u64,
    pub(crate) allocated_bytes: u64,
    pub(crate) is_directory: bool,
}

impl MftIndexV1 {
    pub(crate) fn project_subtree(
        &self,
        root_reference: u64,
        visible_limit: usize,
        mut cancelled: impl FnMut() -> bool,
    ) -> Result<Vec<MftProjectedNodeV1>, String> {
        if !self.entries.contains_key(&root_reference) {
            return Err("MFT root record is unavailable".to_owned());
        }
        let mut traversal = Vec::new();
        let mut pending = vec![root_reference];
        while let Some(reference) = pending.pop() {
            if cancelled() {
                return Err("MFT projection cancelled".to_owned());
            }
            traversal.push(reference);
            if let Some(children) = self.children.get(&reference) {
                pending.extend(children.iter().copied());
            }
        }
        let mut logical_totals = HashMap::<u64, u64>::new();
        let mut allocated_totals = HashMap::<u64, u64>::new();
        for reference in traversal.iter().rev().copied() {
            let Some(entry) = self.entries.get(&reference) else {
                continue;
            };
            let logical = entry.logical_bytes.saturating_add(
                self.children
                    .get(&reference)
                    .into_iter()
                    .flatten()
                    .fold(0_u64, |sum, child| {
                        sum.saturating_add(logical_totals.get(child).copied().unwrap_or_default())
                    }),
            );
            let allocated = entry.allocated_bytes.saturating_add(
                self.children
                    .get(&reference)
                    .into_iter()
                    .flatten()
                    .fold(0_u64, |sum, child| {
                        sum.saturating_add(allocated_totals.get(child).copied().unwrap_or_default())
                    }),
            );
            logical_totals.insert(reference, logical);
            allocated_totals.insert(reference, allocated);
        }
        let mut projected = Vec::with_capacity(visible_limit.min(traversal.len()));
        let mut breadth = std::collections::VecDeque::from([(root_reference, None)]);
        while let Some((reference, parent_reference)) = breadth.pop_front() {
            if projected.len() >= visible_limit {
                break;
            }
            let Some(entry) = self.entries.get(&reference) else {
                continue;
            };
            projected.push(MftProjectedNodeV1 {
                reference,
                parent_reference,
                name: entry.name.clone(),
                logical_bytes: logical_totals.get(&reference).copied().unwrap_or_default(),
                allocated_bytes: allocated_totals
                    .get(&reference)
                    .copied()
                    .unwrap_or_default(),
                is_directory: entry.is_directory,
            });
            if let Some(children) = self.children.get(&reference) {
                let mut children = children.clone();
                children.sort_unstable_by_key(|child| {
                    std::cmp::Reverse(logical_totals.get(child).copied().unwrap_or_default())
                });
                breadth.extend(children.into_iter().map(|child| (child, Some(reference))));
            }
        }
        Ok(projected)
    }
}

pub(crate) fn write_index(path: &Path, index: &MftIndexV1) -> Result<(), String> {
    validate_helper_output_path(path)?;
    let file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .map_err(|error| error.to_string())?;
    let mut writer = BufWriter::new(file);
    writer
        .write_all(b"SEMFTIDX\x01")
        .map_err(|error| error.to_string())?;
    writer
        .write_all(&(index.entries.len() as u64).to_le_bytes())
        .map_err(|error| error.to_string())?;
    for entry in index.entries.values() {
        let name = entry.name.as_bytes();
        let name_length = u32::try_from(name.len()).map_err(|_| "MFT name is too long")?;
        writer
            .write_all(&entry.reference.to_le_bytes())
            .map_err(|error| error.to_string())?;
        writer
            .write_all(&entry.parent_reference.to_le_bytes())
            .map_err(|error| error.to_string())?;
        writer
            .write_all(&entry.logical_bytes.to_le_bytes())
            .map_err(|error| error.to_string())?;
        writer
            .write_all(&entry.allocated_bytes.to_le_bytes())
            .map_err(|error| error.to_string())?;
        writer
            .write_all(&[u8::from(entry.is_directory)])
            .map_err(|error| error.to_string())?;
        writer
            .write_all(&name_length.to_le_bytes())
            .map_err(|error| error.to_string())?;
        writer.write_all(name).map_err(|error| error.to_string())?;
    }
    writer.flush().map_err(|error| error.to_string())
}

fn validate_helper_output_path(path: &Path) -> Result<(), String> {
    let temporary = std::env::temp_dir();
    if path.parent() != Some(temporary.as_path()) {
        return Err("MFT helper output must be directly inside the user Temp directory".to_owned());
    }
    let name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or_default();
    if !name.starts_with("superexplorer-mft-") || !name.ends_with(".idx") {
        return Err("MFT helper output has an invalid name".to_owned());
    }
    Ok(())
}

pub(crate) fn read_index(path: &Path) -> Result<MftIndexV1, String> {
    let file = std::fs::File::open(path).map_err(|error| error.to_string())?;
    let mut reader = BufReader::new(file);
    let mut magic = [0_u8; 9];
    reader
        .read_exact(&mut magic)
        .map_err(|error| error.to_string())?;
    if &magic != b"SEMFTIDX\x01" {
        return Err("unsupported MFT index format".to_owned());
    }
    let count = read_stream_u64(&mut reader)?;
    let capacity = usize::try_from(count).map_err(|_| "MFT index is too large")?;
    let mut entries = HashMap::with_capacity(capacity);
    for _ in 0..count {
        let reference = read_stream_u64(&mut reader)?;
        let parent_reference = read_stream_u64(&mut reader)?;
        let logical_bytes = read_stream_u64(&mut reader)?;
        let allocated_bytes = read_stream_u64(&mut reader)?;
        let mut directory = [0_u8; 1];
        reader
            .read_exact(&mut directory)
            .map_err(|error| error.to_string())?;
        let mut name_length = [0_u8; 4];
        reader
            .read_exact(&mut name_length)
            .map_err(|error| error.to_string())?;
        let name_length = u32::from_le_bytes(name_length) as usize;
        if name_length > 64 * 1024 {
            return Err("MFT index name exceeds the safety limit".to_owned());
        }
        let mut name = vec![0_u8; name_length];
        reader
            .read_exact(&mut name)
            .map_err(|error| error.to_string())?;
        entries.insert(
            reference,
            MftEntryV1 {
                reference,
                parent_reference,
                name: String::from_utf8(name).map_err(|error| error.to_string())?,
                logical_bytes,
                allocated_bytes,
                is_directory: directory[0] != 0,
            },
        );
    }
    let mut children = HashMap::<u64, Vec<u64>>::new();
    for entry in entries.values() {
        if entry.reference != entry.parent_reference {
            children
                .entry(entry.parent_reference)
                .or_default()
                .push(entry.reference);
        }
    }
    Ok(MftIndexV1 { entries, children })
}

fn read_stream_u64(reader: &mut impl std::io::Read) -> Result<u64, String> {
    let mut bytes = [0_u8; 8];
    reader
        .read_exact(&mut bytes)
        .map_err(|error| error.to_string())?;
    Ok(u64::from_le_bytes(bytes))
}

struct HandleGuard(HANDLE);

impl Drop for HandleGuard {
    fn drop(&mut self) {
        // SAFETY: the guard owns the handle returned by CreateFileW.
        let _ = unsafe { CloseHandle(self.0) };
    }
}

pub(crate) fn file_reference_number(path: &Path) -> Result<u64, String> {
    let wide = path
        .as_os_str()
        .encode_wide()
        .chain([0])
        .collect::<Vec<_>>();
    // SAFETY: the UTF-16 path is terminated and remains alive for the call.
    let handle = unsafe {
        CreateFileW(
            PCWSTR(wide.as_ptr()),
            0,
            FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE,
            None,
            OPEN_EXISTING,
            windows::Win32::Storage::FileSystem::FILE_FLAG_BACKUP_SEMANTICS,
            None,
        )
    }
    .map_err(|error| error.to_string())?;
    let handle = HandleGuard(handle);
    let mut info = BY_HANDLE_FILE_INFORMATION::default();
    // SAFETY: `info` is valid writable storage and the handle remains owned.
    unsafe { GetFileInformationByHandle(handle.0, &mut info) }
        .map_err(|error| error.to_string())?;
    Ok((u64::from(info.nFileIndexHigh) << 32) | u64::from(info.nFileIndexLow))
}

pub(crate) fn read_volume_index(
    path: &Path,
    mut cancelled: impl FnMut() -> bool,
) -> Result<MftIndexV1, String> {
    let volume = volume_device_path(path)?;
    let wide = std::ffi::OsStr::new(&volume)
        .encode_wide()
        .chain([0])
        .collect::<Vec<_>>();
    // SAFETY: the device path is terminated and remains alive for the call.
    let handle = unsafe {
        CreateFileW(
            PCWSTR(wide.as_ptr()),
            FILE_GENERIC_READ.0,
            FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE,
            None,
            OPEN_EXISTING,
            Default::default(),
            None,
        )
    }
    .map_err(|error| error.to_string())?;
    let handle = HandleGuard(handle);
    let mut cursor = MFT_ENUM_DATA_V0 {
        StartFileReferenceNumber: 0,
        LowUsn: 0,
        HighUsn: i64::MAX,
    };
    let mut output = vec![0_u8; 1024 * 1024];
    let mut entries = HashMap::new();
    loop {
        if cancelled() {
            return Err("MFT scan cancelled".to_owned());
        }
        let mut returned = 0_u32;
        // SAFETY: both buffers are valid for their advertised sizes and the
        // synchronous call receives no OVERLAPPED pointer.
        let result = unsafe {
            DeviceIoControl(
                handle.0,
                FSCTL_ENUM_USN_DATA,
                Some((&raw const cursor).cast::<c_void>()),
                size_of::<MFT_ENUM_DATA_V0>() as u32,
                Some(output.as_mut_ptr().cast::<c_void>()),
                output.len() as u32,
                Some(&mut returned),
                None,
            )
        };
        if result.is_err() {
            if entries.is_empty() {
                return Err(result.unwrap_err().to_string());
            }
            break;
        }
        let returned = returned as usize;
        if returned <= 8 {
            break;
        }
        cursor.StartFileReferenceNumber = read_u64(&output, 0).ok_or("invalid MFT cursor")?;
        let mut offset = 8_usize;
        while offset + 60 <= returned {
            let record_length = read_u32(&output, offset).unwrap_or_default() as usize;
            if record_length < 60 || offset.saturating_add(record_length) > returned {
                break;
            }
            let major = read_u16(&output, offset + 4).unwrap_or_default();
            if major == 2 {
                let reference = read_u64(&output, offset + 8).unwrap_or_default();
                let parent_reference = read_u64(&output, offset + 16).unwrap_or_default();
                let attributes = read_u32(&output, offset + 52).unwrap_or_default();
                let name_len = read_u16(&output, offset + 56).unwrap_or_default() as usize;
                let name_offset = read_u16(&output, offset + 58).unwrap_or_default() as usize;
                let name_start = offset.saturating_add(name_offset);
                let name_end = name_start.saturating_add(name_len);
                if name_end <= offset + record_length && name_len.is_multiple_of(2) {
                    let name = output[name_start..name_end]
                        .chunks_exact(2)
                        .map(|pair| u16::from_le_bytes([pair[0], pair[1]]))
                        .collect::<Vec<_>>();
                    let (logical_bytes, allocated_bytes) =
                        if attributes & FILE_ATTRIBUTE_DIRECTORY.0 == 0 {
                            read_file_sizes(handle.0, reference).unwrap_or_default()
                        } else {
                            (0, 0)
                        };
                    entries.insert(
                        reference,
                        MftEntryV1 {
                            reference,
                            parent_reference,
                            name: String::from_utf16_lossy(&name),
                            logical_bytes,
                            allocated_bytes,
                            is_directory: attributes & FILE_ATTRIBUTE_DIRECTORY.0 != 0,
                        },
                    );
                }
            }
            offset += record_length;
        }
    }
    let mut children = HashMap::<u64, Vec<u64>>::new();
    for entry in entries.values() {
        if entry.reference != entry.parent_reference {
            children
                .entry(entry.parent_reference)
                .or_default()
                .push(entry.reference);
        }
    }
    Ok(MftIndexV1 { entries, children })
}

pub(crate) fn read_volume_index_with_helper(
    path: &Path,
    mut cancelled: impl FnMut() -> bool,
) -> Result<MftIndexV1, String> {
    match read_volume_index(path, &mut cancelled) {
        Ok(index) => return Ok(index),
        Err(direct_error) if cancelled() => return Err(direct_error),
        Err(_) => {}
    }
    let executable = std::env::current_exe().map_err(|error| error.to_string())?;
    let helper = executable
        .parent()
        .ok_or("application executable has no parent directory")?
        .join("superexplorer-mft-helper.exe");
    if !helper.is_file() {
        return Err(format!("MFT helper is missing: {}", helper.display()));
    }
    let output = std::env::temp_dir().join(format!(
        "superexplorer-mft-{}-{}.idx",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(|error| error.to_string())?
            .as_nanos()
    ));
    let parameters = format!("\"{}\" \"{}\"", path.display(), output.display());
    let verb = "runas\0".encode_utf16().collect::<Vec<_>>();
    let helper_wide = helper
        .as_os_str()
        .encode_wide()
        .chain([0])
        .collect::<Vec<_>>();
    let parameters_wide = parameters.encode_utf16().chain([0]).collect::<Vec<_>>();
    let mut execute = SHELLEXECUTEINFOW {
        cbSize: size_of::<SHELLEXECUTEINFOW>() as u32,
        fMask: SEE_MASK_NOCLOSEPROCESS,
        lpVerb: PCWSTR(verb.as_ptr()),
        lpFile: PCWSTR(helper_wide.as_ptr()),
        lpParameters: PCWSTR(parameters_wide.as_ptr()),
        nShow: 0,
        ..Default::default()
    };
    // SAFETY: all UTF-16 buffers outlive the synchronous launch call and the
    // structure size matches this target architecture.
    unsafe { ShellExecuteExW(&raw mut execute) }.map_err(|error| error.to_string())?;
    if execute.hProcess.is_invalid() {
        return Err("elevated MFT helper returned no process handle".to_owned());
    }
    let process = HandleGuard(execute.hProcess);
    // SAFETY: the helper process handle remains owned by `process`.
    let _ = unsafe { WaitForSingleObject(process.0, INFINITE) };
    let result = read_index(&output);
    let _ = std::fs::remove_file(&output);
    result
}

fn read_file_sizes(handle: HANDLE, reference: u64) -> Option<(u64, u64)> {
    let input = reference as i64;
    let mut output = vec![0_u8; 64 * 1024];
    let mut returned = 0_u32;
    // SAFETY: buffers are valid and the operation is synchronous.
    unsafe {
        DeviceIoControl(
            handle,
            FSCTL_GET_NTFS_FILE_RECORD,
            Some((&raw const input).cast::<c_void>()),
            size_of::<i64>() as u32,
            Some(output.as_mut_ptr().cast::<c_void>()),
            output.len() as u32,
            Some(&mut returned),
            None,
        )
    }
    .ok()?;
    if returned < 16 {
        return None;
    }
    let record_len = read_u32(&output, 8)? as usize;
    let record = output.get(12..12 + record_len)?;
    parse_unnamed_data_sizes(record)
}

fn parse_unnamed_data_sizes(record: &[u8]) -> Option<(u64, u64)> {
    if record.get(0..4)? != b"FILE" {
        return None;
    }
    let mut offset = read_u16(record, 20)? as usize;
    while offset + 16 <= record.len() {
        let kind = read_u32(record, offset)?;
        if kind == u32::MAX {
            break;
        }
        let length = read_u32(record, offset + 4)? as usize;
        if length < 16 || offset + length > record.len() {
            break;
        }
        let non_resident = *record.get(offset + 8)? != 0;
        let name_length = *record.get(offset + 9)?;
        if kind == 0x80 && name_length == 0 {
            if non_resident {
                return Some((
                    read_u64(record, offset + 48)?,
                    read_u64(record, offset + 40)?,
                ));
            }
            let logical = u64::from(read_u32(record, offset + 16)?);
            return Some((logical, logical));
        }
        offset += length;
    }
    Some((0, 0))
}

fn volume_device_path(path: &Path) -> Result<String, String> {
    let text = path.to_string_lossy();
    let bytes = text.as_bytes();
    if bytes.len() < 2 || bytes[1] != b':' || !bytes[0].is_ascii_alphabetic() {
        return Err("MFT fast path requires a local drive-letter path".to_owned());
    }
    Ok(format!(r"\\.\{}:", (bytes[0] as char).to_ascii_uppercase()))
}

fn read_u16(bytes: &[u8], offset: usize) -> Option<u16> {
    Some(u16::from_le_bytes(
        bytes.get(offset..offset + 2)?.try_into().ok()?,
    ))
}

fn read_u32(bytes: &[u8], offset: usize) -> Option<u32> {
    Some(u32::from_le_bytes(
        bytes.get(offset..offset + 4)?.try_into().ok()?,
    ))
}

fn read_u64(bytes: &[u8], offset: usize) -> Option<u64> {
    Some(u64::from_le_bytes(
        bytes.get(offset..offset + 8)?.try_into().ok()?,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_resident_unnamed_data_size() {
        let mut record = vec![0_u8; 96];
        record[0..4].copy_from_slice(b"FILE");
        record[20..22].copy_from_slice(&48_u16.to_le_bytes());
        record[48..52].copy_from_slice(&0x80_u32.to_le_bytes());
        record[52..56].copy_from_slice(&32_u32.to_le_bytes());
        record[64..68].copy_from_slice(&1234_u32.to_le_bytes());
        record[80..84].copy_from_slice(&u32::MAX.to_le_bytes());
        assert_eq!(parse_unnamed_data_sizes(&record), Some((1234, 1234)));
    }

    #[test]
    fn parses_nonresident_unnamed_data_sizes() {
        let mut record = vec![0_u8; 128];
        record[0..4].copy_from_slice(b"FILE");
        record[20..22].copy_from_slice(&48_u16.to_le_bytes());
        record[48..52].copy_from_slice(&0x80_u32.to_le_bytes());
        record[52..56].copy_from_slice(&72_u32.to_le_bytes());
        record[56] = 1;
        record[88..96].copy_from_slice(&8192_u64.to_le_bytes());
        record[96..104].copy_from_slice(&5000_u64.to_le_bytes());
        record[120..124].copy_from_slice(&u32::MAX.to_le_bytes());
        assert_eq!(parse_unnamed_data_sizes(&record), Some((5000, 8192)));
    }

    #[test]
    fn opt_in_volume_smoke_reads_mft_index() {
        let Ok(root) = std::env::var("SUPEREXPLORER_MFT_TEST_ROOT") else {
            return;
        };
        let index = read_volume_index(Path::new(&root), || false)
            .unwrap_or_else(|error| panic!("MFT smoke failed for {root}: {error}"));
        assert!(!index.entries.is_empty());
        let reference = file_reference_number(Path::new(&root)).unwrap();
        assert!(index.entries.contains_key(&reference));
    }

    #[test]
    fn helper_index_round_trips_without_overwriting() {
        let path = std::env::temp_dir().join(format!(
            "superexplorer-mft-test-{}-{}.idx",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let entry = MftEntryV1 {
            reference: 42,
            parent_reference: 5,
            name: "資料夾".to_owned(),
            logical_bytes: 123,
            allocated_bytes: 4096,
            is_directory: false,
        };
        let index = MftIndexV1 {
            entries: HashMap::from([(entry.reference, entry.clone())]),
            children: HashMap::from([(entry.parent_reference, vec![entry.reference])]),
        };
        write_index(&path, &index).unwrap();
        assert!(
            write_index(&path, &index).is_err(),
            "helper output cannot overwrite a file"
        );
        let restored = read_index(&path).unwrap();
        std::fs::remove_file(path).unwrap();
        assert_eq!(restored.entries.get(&42), Some(&entry));
    }
}
